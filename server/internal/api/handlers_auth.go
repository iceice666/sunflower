package api

import (
	"encoding/json"
	"errors"
	"net/http"

	"github.com/iceice666/sunflower/server/internal/auth"
)

func (d *Deps) registerDevice(w http.ResponseWriter, r *http.Request) {
	var req auth.RegisterDeviceRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	if !d.PairingLimiter.Allow(r.RemoteAddr) {
		jsonError(w, "rate_limited", http.StatusTooManyRequests)
		return
	}
	resp, err := auth.RegisterDeviceWithOptions(r.Context(), d.DB, req, auth.RegisterDeviceOptions{
		DevOpenRegistration: d.DevOpenRegistration,
	})
	if err != nil {
		var ae *auth.Error
		if errors.As(err, &ae) {
			jsonError(w, ae.Code, ae.Status)
			return
		}
		d.Log.Error().Err(err).Msg("register-device")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	d.PairingLimiter.Reset(r.RemoteAddr)
	jsonOK(w, resp)
}
