package api

import (
	"context"
	"encoding/json"
	"fmt"
	"html/template"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/iceice666/sunflower/server/internal/adminui"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/cookies"
	"github.com/iceice666/sunflower/server/internal/jobs"
)

type adminViewData struct {
	Title         string
	Authenticated bool
	CSRFToken     string
	Flash         string
	Error         string

	Status     adminStatusPayload
	Devices    []adminDevice
	Pairing    *auth.PairingCodeResponse
	Counts     libraryCounts
	Jobs       []*jobs.Job
	Cookie     cookieStatusResponse
	NowPlaying any
	Events     []adminAuditEvent
}

func (d *Deps) renderAdmin(w http.ResponseWriter, r *http.Request, page string, data adminViewData) {
	data.Authenticated = data.Authenticated || adminSessionFromCtx(r.Context()) != nil
	if data.CSRFToken == "" {
		data.CSRFToken = adminCSRFTokenFromCtx(r.Context())
	}
	tpl, err := adminui.Parse(page, template.FuncMap{
		"fmtTime":  formatAdminTime,
		"metadata": redactedMetadataString,
	})
	if err != nil {
		d.Log.Error().Err(err).Str("page", page).Msg("admin template parse")
		http.Error(w, "template error", http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	if err := tpl.ExecuteTemplate(w, "layout", data); err != nil {
		d.Log.Error().Err(err).Str("page", page).Msg("admin template execute")
	}
}

func (d *Deps) renderAdminError(w http.ResponseWriter, r *http.Request, status int, msg string) {
	w.WriteHeader(status)
	d.renderAdmin(w, r, "error.html", adminViewData{
		Title: "Error",
		Error: msg,
	})
}

func (d *Deps) adminOverviewPage(w http.ResponseWriter, r *http.Request) {
	d.renderAdmin(w, r, "overview.html", adminViewData{
		Title:  "Overview",
		Status: d.buildAdminStatus(r.Context()),
		Flash:  r.URL.Query().Get("flash"),
	})
}

func (d *Deps) adminDevicesPage(w http.ResponseWriter, r *http.Request) {
	d.renderAdmin(w, r, "devices.html", adminViewData{
		Title:   "Devices",
		Devices: d.listAdminDevices(r.Context()),
		Flash:   r.URL.Query().Get("flash"),
	})
}

func (d *Deps) adminRevokeDeviceForm(w http.ResponseWriter, r *http.Request) {
	if !d.checkAdminFormCSRF(w, r) {
		return
	}
	if err := d.revokeDevice(r.Context(), adminSessionFromCtx(r.Context()), chi.URLParam(r, "id"), r.FormValue("reason")); err != nil {
		d.renderAdminError(w, r, http.StatusBadRequest, "Could not revoke device")
		return
	}
	http.Redirect(w, r, "/admin/devices?flash=device_revoked", http.StatusFound)
}

func (d *Deps) adminPairingNewPage(w http.ResponseWriter, r *http.Request) {
	d.renderAdmin(w, r, "pairing_new.html", adminViewData{
		Title: "Pairing",
		Flash: r.URL.Query().Get("flash"),
	})
}

func (d *Deps) adminCreatePairingForm(w http.ResponseWriter, r *http.Request) {
	if !d.checkAdminFormCSRF(w, r) {
		return
	}
	ttl, _ := strconv.Atoi(r.FormValue("ttl_seconds"))
	sess := adminSessionFromCtx(r.Context())
	resp, err := auth.CreatePairingCode(
		r.Context(), d.DB, sess.UserID, sess.ID, r.FormValue("label"),
		time.Duration(ttl)*time.Second, d.serverBaseURL(r),
	)
	if err != nil {
		d.renderAdminError(w, r, http.StatusInternalServerError, "Could not create pairing code")
		return
	}
	d.renderAdmin(w, r, "pairing_new.html", adminViewData{
		Title:   "Pairing",
		Pairing: resp,
	})
}

func (d *Deps) adminLibraryPage(w http.ResponseWriter, r *http.Request) {
	d.renderAdmin(w, r, "library.html", adminViewData{
		Title:  "Library",
		Counts: d.libraryCounts(r.Context()),
		Jobs:   d.recentJobs(),
		Flash:  r.URL.Query().Get("flash"),
	})
}

func (d *Deps) adminStartScanForm(w http.ResponseWriter, r *http.Request) {
	if !d.checkAdminFormCSRF(w, r) {
		return
	}
	roots := splitRoots(r.FormValue("roots"))
	if len(roots) == 0 {
		d.renderAdminError(w, r, http.StatusBadRequest, "Enter at least one root")
		return
	}
	job := d.Jobs.Create()
	go jobs.RunScanJob(context.Background(), d.Jobs, d.Scanner, job.ID, roots)
	sess := adminSessionFromCtx(r.Context())
	_ = auth.WriteAudit(r.Context(), d.DB, auth.AuditEvent{
		UserID:     sess.UserID,
		ActorType:  "admin_session",
		ActorID:    sess.ID.String(),
		Event:      "library_scan_started",
		TargetType: "job",
		TargetID:   job.ID,
		Metadata:   map[string]any{"root_count": len(roots)},
	})
	http.Redirect(w, r, "/admin/library?flash=scan_started", http.StatusFound)
}

func (d *Deps) adminCookiesPage(w http.ResponseWriter, r *http.Request) {
	d.renderAdmin(w, r, "cookies_youtube.html", adminViewData{
		Title:  "YouTube Cookies",
		Cookie: d.cookieStatus(r.Context()),
		Flash:  r.URL.Query().Get("flash"),
	})
}

func (d *Deps) adminUploadCookiesForm(w http.ResponseWriter, r *http.Request) {
	r.Body = http.MaxBytesReader(w, r.Body, 1<<20)
	if !d.checkAdminFormCSRF(w, r) {
		return
	}
	if d.CookieKey == [32]byte{} {
		d.renderAdminError(w, r, http.StatusServiceUnavailable, "Cookie encryption is not configured")
		return
	}
	if err := r.ParseForm(); err != nil {
		d.renderAdminError(w, r, http.StatusBadRequest, "Invalid cookie form")
		return
	}
	raw := strings.TrimSpace(r.FormValue("cookies"))
	if raw == "" {
		d.renderAdminError(w, r, http.StatusBadRequest, "Cookie export is empty")
		return
	}
	sess := adminSessionFromCtx(r.Context())
	if err := cookies.Store(r.Context(), d.DB, d.CookieKey, sess.UserID, "youtube", []byte(raw)); err != nil {
		d.renderAdminError(w, r, http.StatusInternalServerError, "Could not store cookies")
		return
	}
	_ = auth.WriteAudit(r.Context(), d.DB, auth.AuditEvent{
		UserID:     sess.UserID,
		ActorType:  "admin_session",
		ActorID:    sess.ID.String(),
		Event:      "youtube_cookies_updated",
		TargetType: "cookie_store",
		TargetID:   "youtube",
		Metadata:   map[string]any{"bytes": len(raw)},
	})
	http.Redirect(w, r, "/admin/cookies/youtube?flash=cookies_updated", http.StatusFound)
}

func (d *Deps) adminProbeCookiesForm(w http.ResponseWriter, r *http.Request) {
	if !d.checkAdminFormCSRF(w, r) {
		return
	}
	_, _ = d.DB.Exec(r.Context(), `
		INSERT INTO cookie_health (provider, status, checked_at, detail)
		VALUES ('youtube', 'unknown', now(), 'manual probe requested')
		ON CONFLICT (provider) DO UPDATE
		SET status = 'unknown', checked_at = now(), detail = 'manual probe requested'
	`)
	http.Redirect(w, r, "/admin/cookies/youtube?flash=probe_requested", http.StatusFound)
}

func (d *Deps) adminClearCookiesForm(w http.ResponseWriter, r *http.Request) {
	if !d.checkAdminFormCSRF(w, r) {
		return
	}
	sess := adminSessionFromCtx(r.Context())
	_, _ = d.DB.Exec(r.Context(), `DELETE FROM encrypted_cookies WHERE user_id = $1 AND provider = 'youtube'`, sess.UserID)
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
	http.Redirect(w, r, "/admin/cookies/youtube?flash=cookies_cleared", http.StatusFound)
}

func (d *Deps) adminNowPlayingPage(w http.ResponseWriter, r *http.Request) {
	var snapshot any = []any{}
	if d.Hub != nil {
		snapshot = d.Hub.Snapshot()
	}
	d.renderAdmin(w, r, "now_playing.html", adminViewData{
		Title:      "Now Playing",
		NowPlaying: snapshot,
		Flash:      r.URL.Query().Get("flash"),
	})
}

func (d *Deps) adminNowPlayingCommandForm(w http.ResponseWriter, r *http.Request) {
	if !d.checkAdminFormCSRF(w, r) {
		return
	}
	req := wsCommandRequest{
		DeviceID: r.FormValue("device_id"),
		Command:  r.FormValue("command"),
	}
	if d.Hub != nil {
		_ = d.Hub.SendCommand(req.DeviceID, req.Command)
	}
	http.Redirect(w, r, "/admin/now-playing?flash=command_sent", http.StatusFound)
}

func (d *Deps) adminAuditPage(w http.ResponseWriter, r *http.Request) {
	d.renderAdmin(w, r, "audit.html", adminViewData{
		Title:  "Audit",
		Events: d.recentAudit(r.Context(), 200),
	})
}

func splitRoots(raw string) []string {
	var roots []string
	for _, line := range strings.Split(raw, "\n") {
		line = strings.TrimSpace(line)
		if line != "" {
			roots = append(roots, line)
		}
	}
	return roots
}

func formatAdminTime(v any) string {
	switch t := v.(type) {
	case time.Time:
		if t.IsZero() {
			return ""
		}
		return t.Local().Format("2006-01-02 15:04")
	case *time.Time:
		if t == nil || t.IsZero() {
			return "never"
		}
		return t.Local().Format("2006-01-02 15:04")
	default:
		return fmt.Sprint(v)
	}
}

func (ev adminAuditEvent) MetadataString() string {
	b, _ := json.Marshal(ev.Metadata)
	return string(b)
}
