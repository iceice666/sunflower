package auth

import (
	"context"
	"errors"
	"net/http"
	"strings"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

type ctxKey int

const (
	ctxKeyUserID ctxKey = iota
	ctxKeyDeviceID
)

// UserIDFromCtx returns the authenticated user's UUID from context.
func UserIDFromCtx(ctx context.Context) uuid.UUID {
	v, _ := ctx.Value(ctxKeyUserID).(uuid.UUID)
	return v
}

// DeviceIDFromCtx returns the authenticated device's UUID from context.
func DeviceIDFromCtx(ctx context.Context) uuid.UUID {
	v, _ := ctx.Value(ctxKeyDeviceID).(uuid.UUID)
	return v
}

// Middleware validates the Bearer token on every request and injects
// user_id and device_id into the request context.
func Middleware(pool *pgxpool.Pool) func(http.Handler) http.Handler {
	q := gen.New(pool)
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			hdr := r.Header.Get("Authorization")
			token, ok := strings.CutPrefix(hdr, "Bearer ")
			if !ok || token == "" {
				http.Error(w, `{"error":"missing_token"}`, http.StatusUnauthorized)
				return
			}

			device, err := q.GetDeviceByTokenHash(r.Context(), HashToken(token))
			if errors.Is(err, pgx.ErrNoRows) {
				http.Error(w, `{"error":"invalid_token"}`, http.StatusUnauthorized)
				return
			}
			if err != nil {
				http.Error(w, `{"error":"internal"}`, http.StatusInternalServerError)
				return
			}

			go func() { _ = q.UpdateDeviceLastSeen(context.Background(), device.ID) }()

			ctx := context.WithValue(r.Context(), ctxKeyUserID, uuid.UUID(device.UserID.Bytes))
			ctx = context.WithValue(ctx, ctxKeyDeviceID, uuid.UUID(device.ID.Bytes))
			next.ServeHTTP(w, r.WithContext(ctx))
		})
	}
}
