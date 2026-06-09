// Package auth handles device registration and Bearer token validation.
package auth

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"encoding/hex"
	"errors"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgtype"
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

// RegisterDeviceRequest is the JSON body for POST /api/v1/auth/register-device.
type RegisterDeviceRequest struct {
	DeviceName    string `json:"device_name"`
	Platform      string `json:"platform"`
	ClientVersion string `json:"client_version"`
}

// RegisterDeviceResponse is the JSON response.
type RegisterDeviceResponse struct {
	DeviceID           string   `json:"device_id"`
	Token              string   `json:"token"`
	ServerCapabilities []string `json:"server_capabilities"`
}

// RegisterDevice creates or reuses the singleton user, then creates a device
// and returns a one-time Bearer token. The raw token is returned only once.
func RegisterDevice(ctx context.Context, pool *pgxpool.Pool, req RegisterDeviceRequest) (*RegisterDeviceResponse, error) {
	q := gen.New(pool)

	user, err := q.GetFirstUser(ctx)
	if errors.Is(err, pgx.ErrNoRows) {
		user, err = q.InsertUser(ctx, "owner")
	}
	if err != nil {
		return nil, err
	}

	// 32-byte cryptographically random token.
	raw := make([]byte, 32)
	if _, err := rand.Read(raw); err != nil {
		return nil, err
	}
	tokenStr := "sf_dev_" + base64.RawURLEncoding.EncodeToString(raw)

	device, err := q.InsertDevice(ctx, gen.InsertDeviceParams{
		UserID:    pgtype.UUID{Bytes: user.ID.Bytes, Valid: true},
		Name:      pgtype.Text{String: req.DeviceName, Valid: req.DeviceName != ""},
		Platform:  pgtype.Text{String: req.Platform, Valid: req.Platform != ""},
		TokenHash: HashToken(tokenStr),
	})
	if err != nil {
		return nil, err
	}

	return &RegisterDeviceResponse{
		DeviceID: uuid.UUID(device.ID.Bytes).String(),
		Token:    tokenStr,
		ServerCapabilities: []string{"auth.v1", "library.v1"},
	}, nil
}
