package api

import (
	"encoding/json"
	"errors"
	"net/http"

	"github.com/iceice666/sunflower/server/internal/auth"
)

func (d *Deps) setupStatus(w http.ResponseWriter, r *http.Request) {
	configured := false
	if d.DB != nil {
		var err error
		configured, err = auth.OwnerConfigured(r.Context(), d.DB)
		if err != nil {
			d.Log.Error().Err(err).Msg("setup status")
			jsonError(w, "internal", http.StatusInternalServerError)
			return
		}
	}
	jsonOK(w, auth.SetupStatus{
		Configured:         configured,
		PairingRequired:    true,
		ServerVersion:      d.ServerVersion,
		ServerCapabilities: auth.SetupCapabilities(),
	})
}

func (d *Deps) setupOwner(w http.ResponseWriter, r *http.Request) {
	if !d.SetupLimiter.Allow(r.RemoteAddr) {
		jsonError(w, "rate_limited", http.StatusTooManyRequests)
		return
	}
	var req auth.OwnerSetupRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	if err := auth.SetupOwner(r.Context(), d.DB, d.SetupToken, req); err != nil {
		var ae *auth.Error
		if errors.As(err, &ae) {
			jsonError(w, ae.Code, ae.Status)
			return
		}
		d.Log.Error().Err(err).Msg("setup owner")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	d.SetupLimiter.Reset(r.RemoteAddr)
	jsonOK(w, map[string]bool{"ok": true})
}
