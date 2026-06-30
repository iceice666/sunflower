package auth

import (
	"context"
	"crypto/rand"
	"encoding/base32"
	"net/url"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"
)

const (
	DefaultPairingTTL = 10 * time.Minute
	MaxPairingTTL     = time.Hour
)

// PairingCodeResponse is returned once when an admin creates a code.
type PairingCodeResponse struct {
	PairingCode string    `json:"pairing_code"`
	PairingURL  string    `json:"pairing_url"`
	ExpiresAt   time.Time `json:"expires_at"`
}

func NormalizePairingCode(code string) string {
	code = strings.ToUpper(strings.TrimSpace(code))
	code = strings.ReplaceAll(code, "-", "")
	code = strings.ReplaceAll(code, " ", "")
	if len(code) == 8 {
		return code[:4] + "-" + code[4:]
	}
	return code
}

// CreatePairingCode creates a short-lived one-time code and stores only its
// verifier. The raw code is returned once.
func CreatePairingCode(ctx context.Context, pool *pgxpool.Pool, userID, sessionID uuid.UUID, label string, ttl time.Duration, serverURL string) (*PairingCodeResponse, error) {
	if ttl <= 0 {
		ttl = DefaultPairingTTL
	}
	if ttl > MaxPairingTTL {
		ttl = MaxPairingTTL
	}
	code, err := generatePairingCode()
	if err != nil {
		return nil, err
	}
	expiresAt := time.Now().UTC().Add(ttl)
	var pairingID uuid.UUID
	err = pool.QueryRow(ctx, `
		INSERT INTO pairing_codes
			(user_id, code_hash, label, expires_at, created_by_session_id)
		VALUES
			($1, $2, nullif($3,''), $4, $5)
		RETURNING id
	`, userID, HashToken(code), strings.TrimSpace(label), expiresAt, sessionID).Scan(&pairingID)
	if err != nil {
		return nil, err
	}
	_ = WriteAudit(ctx, pool, AuditEvent{
		UserID:     userID,
		ActorType:  "admin_session",
		ActorID:    sessionID.String(),
		Event:      "pairing_code_created",
		TargetType: "pairing_code",
		TargetID:   pairingID.String(),
		Metadata: map[string]any{
			"label":       strings.TrimSpace(label),
			"ttl_seconds": int(ttl.Seconds()),
		},
	})
	return &PairingCodeResponse{
		PairingCode: code,
		PairingURL:  pairingURL(serverURL, code),
		ExpiresAt:   expiresAt,
	}, nil
}

func generatePairingCode() (string, error) {
	// 5 random bytes = 40 bits. Base32 encodes to exactly 8 characters.
	raw := make([]byte, 5)
	if _, err := rand.Read(raw); err != nil {
		return "", err
	}
	encoded := base32.StdEncoding.WithPadding(base32.NoPadding).EncodeToString(raw)
	return encoded[:4] + "-" + encoded[4:8], nil
}

func pairingURL(serverURL, code string) string {
	u := url.URL{Scheme: "sunflower", Host: "pair"}
	q := u.Query()
	q.Set("server", strings.TrimRight(serverURL, "/"))
	q.Set("code", code)
	u.RawQuery = q.Encode()
	return u.String()
}
