package auth

import (
	"context"
	"crypto/subtle"
	"errors"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// SetupStatus is the public first-launch state.
type SetupStatus struct {
	Configured         bool     `json:"configured"`
	PairingRequired    bool     `json:"pairing_required"`
	ServerVersion      string   `json:"server_version"`
	ServerCapabilities []string `json:"server_capabilities"`
}

// OwnerSetupRequest is POST /api/v1/setup/owner.
type OwnerSetupRequest struct {
	SetupToken  string `json:"setup_token"`
	DisplayName string `json:"display_name"`
	Password    string `json:"password"`
}

// OwnerConfigured reports whether the single owner password exists.
func OwnerConfigured(ctx context.Context, pool *pgxpool.Pool) (bool, error) {
	var ok bool
	err := pool.QueryRow(ctx, `
		SELECT EXISTS (
			SELECT 1 FROM users WHERE admin_password_hash IS NOT NULL
		)
	`).Scan(&ok)
	return ok, err
}

// SetupOwner creates or updates the singleton owner with an admin password.
func SetupOwner(ctx context.Context, pool *pgxpool.Pool, setupToken string, req OwnerSetupRequest) error {
	configured, err := OwnerConfigured(ctx, pool)
	if err != nil {
		return err
	}
	if configured {
		return apiError("setup_disabled", 403)
	}
	if subtle.ConstantTimeCompare([]byte(strings.TrimSpace(req.SetupToken)), []byte(strings.TrimSpace(setupToken))) != 1 {
		_ = WriteAudit(ctx, pool, AuditEvent{
			ActorType: "setup",
			Event:     "owner_setup_failed",
			Metadata:  map[string]any{"reason": "invalid_setup_token"},
		})
		return apiError("invalid_setup_token", 401)
	}
	if err := ValidateOwnerPassword(req.Password, setupToken); err != nil {
		return apiError("weak_password", 400)
	}
	hash, err := HashPassword(req.Password)
	if err != nil {
		return err
	}
	displayName := strings.TrimSpace(req.DisplayName)
	if displayName == "" {
		displayName = "Owner"
	}
	userID, err := ensureOwnerUser(ctx, pool, displayName)
	if err != nil {
		return err
	}
	_, err = pool.Exec(ctx, `
		UPDATE users
		SET display_name = $2,
		    admin_password_hash = $3,
		    admin_password_updated_at = now()
		WHERE id = $1
	`, userID, displayName, hash)
	if err != nil {
		return err
	}
	return WriteAudit(ctx, pool, AuditEvent{
		UserID:     userID,
		ActorType:  "setup",
		Event:      "owner_setup_completed",
		TargetType: "user",
		TargetID:   userID.String(),
		Metadata:   map[string]any{},
	})
}

// FirstOwner returns the singleton user and optional password hash.
func FirstOwner(ctx context.Context, pool *pgxpool.Pool) (uuid.UUID, string, string, error) {
	var userID uuid.UUID
	var displayName string
	var hash *string
	err := pool.QueryRow(ctx, `
		SELECT id, display_name, admin_password_hash
		FROM users
		ORDER BY created_at
		LIMIT 1
	`).Scan(&userID, &displayName, &hash)
	if errors.Is(err, pgx.ErrNoRows) {
		return uuid.Nil, "", "", apiError("setup_required", 403)
	}
	if err != nil {
		return uuid.Nil, "", "", err
	}
	if hash == nil || *hash == "" {
		return uuid.Nil, "", "", apiError("setup_required", 403)
	}
	return userID, displayName, *hash, nil
}

// Capabilities advertised by setup/status.
func SetupCapabilities() []string {
	return []string{"auth.pairing.v1", "admin.sessions.v1", "device.revoke.v1"}
}

func AdminSessionTTL() time.Duration { return 14 * 24 * time.Hour }
