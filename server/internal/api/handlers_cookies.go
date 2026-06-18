// server/internal/api/handlers_cookies.go
package api

import (
	"context"
	"encoding/json"
	"net/http"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/cookies"
)

type uploadCookiesRequest struct {
	Cookies string `json:"cookies"` // Netscape-format cookie file contents
}

// uploadYTCookies handles POST /api/v1/cookies/youtube.
func (d *Deps) uploadYTCookies(w http.ResponseWriter, r *http.Request) {
	if d.CookieKey == [32]byte{} {
		jsonError(w, "cookies_disabled", http.StatusServiceUnavailable)
		return
	}
	deviceID := auth.DeviceIDFromCtx(r.Context())

	var req uploadCookiesRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.Cookies == "" {
		jsonError(w, "invalid_format", http.StatusBadRequest)
		return
	}

	userID, err := d.userIDForDevice(r.Context(), deviceID)
	if err != nil {
		d.Log.Error().Err(err).Str("device", deviceID.String()).Msg("uploadYTCookies: lookup user")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}

	if err := cookies.Store(r.Context(), d.DB, d.CookieKey, userID, "youtube", []byte(req.Cookies)); err != nil {
		d.Log.Error().Err(err).Msg("uploadYTCookies: store")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}

	w.WriteHeader(http.StatusNoContent)
}

type cookieStatusResponse struct {
	Status    string  `json:"status"`
	CheckedAt *string `json:"checked_at"`
	Detail    *string `json:"detail"`
}

// ytCookieStatus handles GET /api/v1/cookies/youtube/status.
func (d *Deps) ytCookieStatus(w http.ResponseWriter, r *http.Request) {
	if d.CookieKey == [32]byte{} {
		jsonError(w, "cookies_disabled", http.StatusServiceUnavailable)
		return
	}
	var status string
	var checkedAt *time.Time
	var detail *string

	err := d.DB.QueryRow(r.Context(),
		`SELECT status, checked_at, detail FROM cookie_health WHERE provider='youtube'`,
	).Scan(&status, &checkedAt, &detail)
	if err != nil {
		// No row yet.
		jsonOK(w, cookieStatusResponse{Status: "unknown"})
		return
	}

	resp := cookieStatusResponse{Status: status}
	if checkedAt != nil {
		s := checkedAt.Format(time.RFC3339)
		resp.CheckedAt = &s
	}
	resp.Detail = detail
	jsonOK(w, resp)
}

// userIDForDevice looks up the user_id for a device_id.
func (d *Deps) userIDForDevice(ctx context.Context, deviceID uuid.UUID) (uuid.UUID, error) {
	var userID uuid.UUID
	err := d.DB.QueryRow(ctx,
		`SELECT user_id FROM devices WHERE id=$1`, deviceID,
	).Scan(&userID)
	return userID, err
}
