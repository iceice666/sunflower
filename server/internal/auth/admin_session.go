package auth

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"errors"
	"net/http"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

const AdminCookieName = "sf_admin"
const AdminCSRFCookieName = "sf_admin_csrf"

// AdminSession is an authenticated browser/admin API session.
type AdminSession struct {
	ID             uuid.UUID
	UserID         uuid.UUID
	CSRFSecretHash string
	ExpiresAt      time.Time
}

// Login verifies the singleton owner password and creates an admin session.
func Login(ctx context.Context, pool *pgxpool.Pool, password string) (token string, csrf string, expiresAt time.Time, err error) {
	userID, _, hash, err := FirstOwner(ctx, pool)
	if err != nil {
		return "", "", time.Time{}, err
	}
	ok, err := VerifyPassword(hash, password)
	if err != nil {
		return "", "", time.Time{}, err
	}
	if !ok {
		_ = WriteAudit(ctx, pool, AuditEvent{
			UserID:    userID,
			ActorType: "admin",
			Event:     "admin_login_failed",
			Metadata:  map[string]any{"reason": "bad_password"},
		})
		return "", "", time.Time{}, apiError("invalid_password", 401)
	}
	token, err = randomSecret("sf_adm_")
	if err != nil {
		return "", "", time.Time{}, err
	}
	csrf, err = randomSecret("sf_csrf_")
	if err != nil {
		return "", "", time.Time{}, err
	}
	expiresAt = time.Now().UTC().Add(AdminSessionTTL())
	var sessionID uuid.UUID
	err = pool.QueryRow(ctx, `
		INSERT INTO admin_sessions (user_id, token_hash, csrf_secret_hash, expires_at, last_seen_at)
		VALUES ($1, $2, $3, $4, now())
		RETURNING id
	`, userID, HashVerifier(token), HashVerifier(csrf), expiresAt).Scan(&sessionID)
	if err != nil {
		return "", "", time.Time{}, err
	}
	_ = WriteAudit(ctx, pool, AuditEvent{
		UserID:     userID,
		ActorType:  "admin_session",
		ActorID:    sessionID.String(),
		Event:      "admin_login_succeeded",
		TargetType: "admin_session",
		TargetID:   sessionID.String(),
		Metadata:   map[string]any{},
	})
	return token, csrf, expiresAt, nil
}

// VerifyAdminSession loads an active, unexpired session by raw cookie token.
func VerifyAdminSession(ctx context.Context, pool *pgxpool.Pool, token string) (*AdminSession, error) {
	if token == "" {
		return nil, apiError("missing_admin_session", 401)
	}
	var sess AdminSession
	err := pool.QueryRow(ctx, `
		SELECT id, user_id, csrf_secret_hash, expires_at
		FROM admin_sessions
		WHERE token_hash = $1
		  AND revoked_at IS NULL
		  AND expires_at > now()
	`, HashVerifier(token)).Scan(&sess.ID, &sess.UserID, &sess.CSRFSecretHash, &sess.ExpiresAt)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, apiError("invalid_admin_session", 401)
	}
	if err != nil {
		return nil, err
	}
	_, _ = pool.Exec(context.Background(), `UPDATE admin_sessions SET last_seen_at = now() WHERE id = $1`, sess.ID)
	return &sess, nil
}

// RevokeAdminSession revokes a session by raw cookie token.
func RevokeAdminSession(ctx context.Context, pool *pgxpool.Pool, token string) error {
	if token == "" {
		return nil
	}
	_, err := pool.Exec(ctx, `
		UPDATE admin_sessions
		SET revoked_at = now()
		WHERE token_hash = $1 AND revoked_at IS NULL
	`, HashVerifier(token))
	return err
}

func VerifyCSRF(sess *AdminSession, token string) bool {
	return token != "" && HashVerifier(token) == sess.CSRFSecretHash
}

func AdminSessionCookie(r *http.Request, token string, expiresAt time.Time) *http.Cookie {
	return &http.Cookie{
		Name:     AdminCookieName,
		Value:    token,
		Path:     "/",
		Expires:  expiresAt,
		HttpOnly: true,
		SameSite: http.SameSiteLaxMode,
		Secure:   isHTTPS(r),
	}
}

func AdminCSRFCookie(r *http.Request, token string, expiresAt time.Time) *http.Cookie {
	return &http.Cookie{
		Name:     AdminCSRFCookieName,
		Value:    token,
		Path:     "/",
		Expires:  expiresAt,
		HttpOnly: false,
		SameSite: http.SameSiteLaxMode,
		Secure:   isHTTPS(r),
	}
}

func ClearAdminSessionCookie(r *http.Request) *http.Cookie {
	return &http.Cookie{
		Name:     AdminCookieName,
		Value:    "",
		Path:     "/",
		Expires:  time.Unix(0, 0),
		MaxAge:   -1,
		HttpOnly: true,
		SameSite: http.SameSiteLaxMode,
		Secure:   isHTTPS(r),
	}
}

func ClearAdminCSRFCookie(r *http.Request) *http.Cookie {
	return &http.Cookie{
		Name:     AdminCSRFCookieName,
		Value:    "",
		Path:     "/",
		Expires:  time.Unix(0, 0),
		MaxAge:   -1,
		HttpOnly: false,
		SameSite: http.SameSiteLaxMode,
		Secure:   isHTTPS(r),
	}
}

func randomSecret(prefix string) (string, error) {
	raw := make([]byte, 32)
	if _, err := rand.Read(raw); err != nil {
		return "", err
	}
	return prefix + base64.RawURLEncoding.EncodeToString(raw), nil
}

func isHTTPS(r *http.Request) bool {
	return r.TLS != nil || r.Header.Get("X-Forwarded-Proto") == "https"
}
