package api

import (
	"context"
	"encoding/json"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/cookies"
	"github.com/iceice666/sunflower/server/internal/jobs"
	"github.com/iceice666/sunflower/server/internal/ws"
)

func (d *Deps) adminStatusJSON(w http.ResponseWriter, r *http.Request) {
	jsonOK(w, d.buildAdminStatus(r.Context()))
}

func (d *Deps) adminDevicesJSON(w http.ResponseWriter, r *http.Request) {
	jsonOK(w, map[string]any{"devices": d.listAdminDevices(r.Context())})
}

func (d *Deps) adminRevokeDeviceJSON(w http.ResponseWriter, r *http.Request) {
	sess := adminSessionFromCtx(r.Context())
	var req struct {
		Reason string `json:"reason"`
	}
	_ = json.NewDecoder(r.Body).Decode(&req)
	if err := d.revokeDevice(r.Context(), sess, chi.URLParam(r, "id"), req.Reason); err != nil {
		jsonError(w, "invalid_id", http.StatusBadRequest)
		return
	}
	jsonOK(w, map[string]bool{"ok": true})
}

func (d *Deps) adminCreatePairingJSON(w http.ResponseWriter, r *http.Request) {
	sess := adminSessionFromCtx(r.Context())
	var req struct {
		Label      string `json:"label"`
		TTLSeconds int    `json:"ttl_seconds"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	resp, err := auth.CreatePairingCode(
		r.Context(),
		d.DB,
		sess.UserID,
		sess.ID,
		req.Label,
		time.Duration(req.TTLSeconds)*time.Second,
		d.serverBaseURL(r),
	)
	if err != nil {
		d.Log.Error().Err(err).Msg("admin create pairing")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	jsonOK(w, resp)
}

func (d *Deps) adminLibraryStatusJSON(w http.ResponseWriter, r *http.Request) {
	jsonOK(w, map[string]any{
		"counts": d.libraryCounts(r.Context()),
		"jobs":   d.recentJobs(),
	})
}

func (d *Deps) adminStartScanJSON(w http.ResponseWriter, r *http.Request) {
	sess := adminSessionFromCtx(r.Context())
	var req struct {
		Roots []string `json:"roots"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || len(req.Roots) == 0 {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	job := d.Jobs.Create()
	go jobs.RunScanJob(context.Background(), d.Jobs, d.Scanner, job.ID, req.Roots)
	_ = auth.WriteAudit(r.Context(), d.DB, auth.AuditEvent{
		UserID:     sess.UserID,
		ActorType:  "admin_session",
		ActorID:    sess.ID.String(),
		Event:      "library_scan_started",
		TargetType: "job",
		TargetID:   job.ID,
		Metadata:   map[string]any{"root_count": len(req.Roots)},
	})
	jsonOK(w, map[string]string{"job_id": job.ID})
}

func (d *Deps) adminCookiesStatusJSON(w http.ResponseWriter, r *http.Request) {
	jsonOK(w, d.cookieStatus(r.Context()))
}

func (d *Deps) adminUploadCookiesJSON(w http.ResponseWriter, r *http.Request) {
	sess := adminSessionFromCtx(r.Context())
	if d.CookieKey == [32]byte{} {
		jsonError(w, "cookies_disabled", http.StatusServiceUnavailable)
		return
	}
	r.Body = http.MaxBytesReader(w, r.Body, 1<<20)
	var req uploadCookiesRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || strings.TrimSpace(req.Cookies) == "" {
		jsonError(w, "invalid_format", http.StatusBadRequest)
		return
	}
	if err := cookies.Store(r.Context(), d.DB, d.CookieKey, sess.UserID, "youtube", []byte(req.Cookies)); err != nil {
		d.Log.Error().Err(err).Msg("admin cookies store")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	_ = auth.WriteAudit(r.Context(), d.DB, auth.AuditEvent{
		UserID:     sess.UserID,
		ActorType:  "admin_session",
		ActorID:    sess.ID.String(),
		Event:      "youtube_cookies_updated",
		TargetType: "cookie_store",
		TargetID:   "youtube",
		Metadata:   map[string]any{"bytes": len(req.Cookies)},
	})
	jsonOK(w, map[string]bool{"ok": true})
}

func (d *Deps) adminProbeCookiesJSON(w http.ResponseWriter, r *http.Request) {
	sess := adminSessionFromCtx(r.Context())
	_, _ = d.DB.Exec(r.Context(), `
		INSERT INTO cookie_health (provider, status, checked_at, detail)
		VALUES ('youtube', 'unknown', now(), 'manual probe requested')
		ON CONFLICT (provider) DO UPDATE
		SET status = 'unknown', checked_at = now(), detail = 'manual probe requested'
	`)
	_ = auth.WriteAudit(r.Context(), d.DB, auth.AuditEvent{
		UserID:     sess.UserID,
		ActorType:  "admin_session",
		ActorID:    sess.ID.String(),
		Event:      "youtube_cookies_probe_requested",
		TargetType: "cookie_store",
		TargetID:   "youtube",
		Metadata:   map[string]any{},
	})
	jsonOK(w, d.cookieStatus(r.Context()))
}

func (d *Deps) adminClearCookiesJSON(w http.ResponseWriter, r *http.Request) {
	sess := adminSessionFromCtx(r.Context())
	_, err := d.DB.Exec(r.Context(), `
		DELETE FROM encrypted_cookies WHERE user_id = $1 AND provider = 'youtube'
	`, sess.UserID)
	if err != nil {
		d.Log.Error().Err(err).Msg("admin cookies clear")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	_, _ = d.DB.Exec(r.Context(), `DELETE FROM cookie_health WHERE provider = 'youtube'`)
	_ = auth.WriteAudit(r.Context(), d.DB, auth.AuditEvent{
		UserID:     sess.UserID,
		ActorType:  "admin_session",
		ActorID:    sess.ID.String(),
		Event:      "youtube_cookies_cleared",
		TargetType: "cookie_store",
		TargetID:   "youtube",
		Metadata:   map[string]any{},
	})
	jsonOK(w, map[string]bool{"ok": true})
}

func (d *Deps) adminNowPlayingJSON(w http.ResponseWriter, r *http.Request) {
	if d.Hub == nil {
		jsonOK(w, map[string]any{"now_playing": []any{}})
		return
	}
	jsonOK(w, map[string]any{"now_playing": d.Hub.Snapshot()})
}

func (d *Deps) adminNowPlayingCommandJSON(w http.ResponseWriter, r *http.Request) {
	var req wsCommandRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	d.sendNowPlayingCommand(w, r, req)
}

func (d *Deps) adminAuditJSON(w http.ResponseWriter, r *http.Request) {
	limit := 100
	if raw := r.URL.Query().Get("limit"); raw != "" {
		if n, err := strconv.Atoi(raw); err == nil && n > 0 && n <= 500 {
			limit = n
		}
	}
	jsonOK(w, map[string]any{"events": d.recentAudit(r.Context(), limit)})
}

func (d *Deps) sendNowPlayingCommand(w http.ResponseWriter, _ *http.Request, req wsCommandRequest) {
	if d.Hub == nil {
		jsonError(w, "ws_unavailable", http.StatusServiceUnavailable)
		return
	}
	if req.DeviceID == "" || req.Command == "" {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	switch req.Command {
	case ws.CmdPause, ws.CmdPlay, ws.CmdSkipNext, ws.CmdSkipPrev:
	default:
		jsonError(w, "invalid_command", http.StatusBadRequest)
		return
	}
	n := d.Hub.SendCommand(req.DeviceID, req.Command)
	jsonOK(w, map[string]int{"delivered": n})
}

func (d *Deps) recentJobs() []*jobs.Job {
	if d.Jobs == nil {
		return nil
	}
	return d.Jobs.ListRecent(25)
}

func (d *Deps) serverBaseURL(r *http.Request) string {
	if d.PublicBaseURL != "" {
		return strings.TrimRight(d.PublicBaseURL, "/")
	}
	scheme := "http"
	if r.TLS != nil || r.Header.Get("X-Forwarded-Proto") == "https" {
		scheme = "https"
	}
	host := r.Host
	if host == "" {
		host = "localhost"
	}
	return scheme + "://" + host
}
