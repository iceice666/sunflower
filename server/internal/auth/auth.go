// Package auth handles device registration and Bearer token validation.
package auth

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"golang.org/x/crypto/argon2"
)

// argon2id parameters (OWASP minimum recommendation)
const (
	argonTime    uint32 = 1
	argonMemory  uint32 = 64 * 1024 // 64 MiB
	argonThreads uint8  = 4
	argonKeyLen  uint32 = 32
)

// argonSalt is a fixed per-server salt. Random tokens (256-bit entropy) make
// per-token salts unnecessary for brute-force resistance; a fixed salt enables
// deterministic O(1) DB lookup without an additional salt column.
var argonSalt = []byte("sunflower-tok-v1") // 16 bytes

// HashToken computes the argon2id hash of a token string for storage/lookup.
func HashToken(token string) string {
	h := argon2.IDKey([]byte(token), argonSalt, argonTime, argonMemory, argonThreads, argonKeyLen)
	return hex.EncodeToString(h)
}

// HashVerifier computes a fast deterministic verifier for high-entropy random
// secrets such as admin sessions and CSRF tokens.
func HashVerifier(secret string) string {
	sum := sha256.Sum256([]byte(secret))
	return hex.EncodeToString(sum[:])
}

// Error is a public auth failure with a stable API error code and HTTP status.
type Error struct {
	Code   string
	Status int
}

func (e *Error) Error() string { return e.Code }

func apiError(code string, status int) error {
	return &Error{Code: code, Status: status}
}

// RegisterDeviceRequest is the JSON body for POST /api/v1/auth/register-device.
type RegisterDeviceRequest struct {
	DeviceName    string `json:"device_name"`
	Platform      string `json:"platform"`
	ClientVersion string `json:"client_version"`
	PairingCode   string `json:"pairing_code"`
}

// RegisterDeviceResponse is the JSON response.
type RegisterDeviceResponse struct {
	DeviceID           string   `json:"device_id"`
	Token              string   `json:"token"`
	ServerCapabilities []string `json:"server_capabilities"`
}

// RegisterDeviceOptions tune the M9 enrollment boundary.
type RegisterDeviceOptions struct {
	DevOpenRegistration bool
}

// RegisterDevice creates a device after consuming a valid one-time pairing code
// and returns a one-time Bearer token. The raw token is returned only once.
func RegisterDevice(ctx context.Context, pool *pgxpool.Pool, req RegisterDeviceRequest) (*RegisterDeviceResponse, error) {
	return RegisterDeviceWithOptions(ctx, pool, req, RegisterDeviceOptions{})
}

// RegisterDeviceWithOptions is the implementation used by the HTTP layer.
func RegisterDeviceWithOptions(ctx context.Context, pool *pgxpool.Pool, req RegisterDeviceRequest, opts RegisterDeviceOptions) (*RegisterDeviceResponse, error) {
	code := NormalizePairingCode(req.PairingCode)
	if code == "" {
		if !opts.DevOpenRegistration {
			return nil, apiError("pairing_required", 403)
		}
		return registerDeviceOpen(ctx, pool, req)
	}

	tx, err := pool.Begin(ctx)
	if err != nil {
		return nil, err
	}
	defer tx.Rollback(ctx) //nolint:errcheck

	var pairID uuid.UUID
	var userID uuid.UUID
	var label *string
	var expiresAt time.Time
	var usedAt *time.Time
	err = tx.QueryRow(ctx, `
		SELECT id, user_id, label, expires_at, used_at
		FROM pairing_codes
		WHERE code_hash = $1
		FOR UPDATE
	`, HashToken(code)).Scan(&pairID, &userID, &label, &expiresAt, &usedAt)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, apiError("invalid_pairing_code", 401)
	}
	if err != nil {
		return nil, err
	}
	if usedAt != nil || time.Now().After(expiresAt) {
		return nil, apiError("invalid_pairing_code", 401)
	}

	tokenStr, err := GenerateDeviceToken()
	if err != nil {
		return nil, err
	}
	name := strings.TrimSpace(req.DeviceName)
	if name == "" && label != nil {
		name = *label
	}
	platform := strings.TrimSpace(req.Platform)
	var deviceID uuid.UUID
	err = tx.QueryRow(ctx, `
		INSERT INTO devices (user_id, name, platform, token_hash, token_label)
		VALUES ($1, nullif($2,''), nullif($3,''), $4, nullif($5,''))
		RETURNING id
	`, userID, name, platform, HashToken(tokenStr), name).Scan(&deviceID)
	if err != nil {
		return nil, err
	}
	if _, err := tx.Exec(ctx, `
		UPDATE pairing_codes
		SET used_at = now(), used_by_device_id = $1
		WHERE id = $2
	`, deviceID, pairID); err != nil {
		return nil, err
	}
	if err := writeAuditTx(ctx, tx, AuditEvent{
		UserID:     userID,
		ActorType:  "pairing_code",
		ActorID:    pairID.String(),
		Event:      "device_paired",
		TargetType: "device",
		TargetID:   deviceID.String(),
		Metadata: map[string]any{
			"platform":       platform,
			"client_version": req.ClientVersion,
		},
	}); err != nil {
		return nil, err
	}
	if err := tx.Commit(ctx); err != nil {
		return nil, err
	}

	return &RegisterDeviceResponse{
		DeviceID:           deviceID.String(),
		Token:              tokenStr,
		ServerCapabilities: DeviceCapabilities(),
	}, nil
}

// GenerateDeviceToken returns a 256-bit opaque device bearer token.
func GenerateDeviceToken() (string, error) {
	raw := make([]byte, 32)
	if _, err := rand.Read(raw); err != nil {
		return "", err
	}
	return "sf_dev_" + base64.RawURLEncoding.EncodeToString(raw), nil
}

// DeviceCapabilities are advertised to newly paired clients.
func DeviceCapabilities() []string {
	return []string{
		"auth.pairing.v1",
		"library.v1",
		"recs.v1",
		"stream.proxy",
		"ws.now_playing",
	}
}

func registerDeviceOpen(ctx context.Context, pool *pgxpool.Pool, req RegisterDeviceRequest) (*RegisterDeviceResponse, error) {
	userID, err := ensureOwnerUser(ctx, pool, "owner")
	if err != nil {
		return nil, err
	}
	tokenStr, err := GenerateDeviceToken()
	if err != nil {
		return nil, err
	}
	var deviceID uuid.UUID
	err = pool.QueryRow(ctx, `
		INSERT INTO devices (user_id, name, platform, token_hash, token_label)
		VALUES ($1, nullif($2,''), nullif($3,''), $4, nullif($2,''))
		RETURNING id
	`, userID, strings.TrimSpace(req.DeviceName), strings.TrimSpace(req.Platform), HashToken(tokenStr)).Scan(&deviceID)
	if err != nil {
		return nil, err
	}
	return &RegisterDeviceResponse{
		DeviceID:           deviceID.String(),
		Token:              tokenStr,
		ServerCapabilities: DeviceCapabilities(),
	}, nil
}

func ensureOwnerUser(ctx context.Context, pool *pgxpool.Pool, displayName string) (uuid.UUID, error) {
	var userID uuid.UUID
	err := pool.QueryRow(ctx, `SELECT id FROM users ORDER BY created_at LIMIT 1`).Scan(&userID)
	if errors.Is(err, pgx.ErrNoRows) {
		err = pool.QueryRow(ctx, `
			INSERT INTO users (display_name)
			VALUES ($1)
			RETURNING id
		`, strings.TrimSpace(displayName)).Scan(&userID)
	}
	if err != nil {
		return uuid.Nil, fmt.Errorf("ensure owner user: %w", err)
	}
	return userID, nil
}
