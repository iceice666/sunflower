package api

import (
	"encoding/json"
	"net/http"

	"github.com/iceice666/sunflower/server/internal/auth"
)

func (d *Deps) registerDevice(w http.ResponseWriter, r *http.Request) {
	var req auth.RegisterDeviceRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	resp, err := auth.RegisterDevice(r.Context(), d.DB, req)
	if err != nil {
		d.Log.Error().Err(err).Msg("register-device")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	jsonOK(w, resp)
}
