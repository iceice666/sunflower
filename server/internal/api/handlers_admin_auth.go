package api

import (
	"encoding/json"
	"errors"
	"net/http"
	"strings"
	"time"

	"github.com/iceice666/sunflower/server/internal/auth"
)

type adminLoginRequest struct {
	Password string `json:"password"`
}

type adminLoginResponse struct {
	CSRFToken string `json:"csrf_token"`
	ExpiresAt string `json:"expires_at"`
}

func (d *Deps) adminLoginJSON(w http.ResponseWriter, r *http.Request) {
	if !d.AdminLoginLimiter.Allow(r.RemoteAddr) {
		jsonError(w, "rate_limited", http.StatusTooManyRequests)
		return
	}
	var req adminLoginRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	token, csrf, expires, err := auth.Login(r.Context(), d.DB, req.Password)
	if err != nil {
		var ae *auth.Error
		if errors.As(err, &ae) {
			jsonError(w, ae.Code, ae.Status)
			return
		}
		d.Log.Error().Err(err).Msg("admin login")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	d.AdminLoginLimiter.Reset(r.RemoteAddr)
	http.SetCookie(w, auth.AdminSessionCookie(r, token, expires))
	http.SetCookie(w, auth.AdminCSRFCookie(r, csrf, expires))
	jsonOK(w, adminLoginResponse{CSRFToken: csrf, ExpiresAt: expires.Format(time.RFC3339)})
}

func (d *Deps) adminLogoutJSON(w http.ResponseWriter, r *http.Request) {
	if c, err := r.Cookie(auth.AdminCookieName); err == nil {
		_ = auth.RevokeAdminSession(r.Context(), d.DB, c.Value)
	}
	http.SetCookie(w, auth.ClearAdminSessionCookie(r))
	http.SetCookie(w, auth.ClearAdminCSRFCookie(r))
	jsonOK(w, map[string]bool{"ok": true})
}

func (d *Deps) adminMeJSON(w http.ResponseWriter, r *http.Request) {
	sess, _, ok := d.adminSessionFromRequest(w, r, false)
	if !ok {
		return
	}
	var displayName string
	if err := d.DB.QueryRow(r.Context(), `SELECT display_name FROM users WHERE id = $1`, sess.UserID).Scan(&displayName); err != nil {
		d.Log.Error().Err(err).Msg("admin me")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}
	csrf := ""
	if c, err := r.Cookie(auth.AdminCSRFCookieName); err == nil {
		csrf = c.Value
	}
	jsonOK(w, map[string]any{
		"user_id":      sess.UserID.String(),
		"display_name": displayName,
		"csrf_token":   csrf,
		"expires_at":   sess.ExpiresAt.Format(time.RFC3339),
	})
}

func (d *Deps) adminLoginPage(w http.ResponseWriter, r *http.Request) {
	if c, err := r.Cookie(auth.AdminCookieName); err == nil && c.Value != "" {
		if _, err := auth.VerifyAdminSession(r.Context(), d.DB, c.Value); err == nil {
			http.Redirect(w, r, "/admin/", http.StatusFound)
			return
		}
	}
	d.renderAdmin(w, r, "login.html", adminViewData{
		Title: "Admin Login",
		Error: r.URL.Query().Get("error"),
	})
}

func (d *Deps) adminLoginForm(w http.ResponseWriter, r *http.Request) {
	if !d.AdminLoginLimiter.Allow(r.RemoteAddr) {
		http.Redirect(w, r, "/admin/login?error=rate_limited", http.StatusFound)
		return
	}
	if err := r.ParseForm(); err != nil {
		http.Redirect(w, r, "/admin/login?error=invalid_request", http.StatusFound)
		return
	}
	token, csrf, expires, err := auth.Login(r.Context(), d.DB, r.FormValue("password"))
	if err != nil {
		code := "invalid_password"
		var ae *auth.Error
		if errors.As(err, &ae) {
			code = ae.Code
		}
		http.Redirect(w, r, "/admin/login?error="+code, http.StatusFound)
		return
	}
	d.AdminLoginLimiter.Reset(r.RemoteAddr)
	http.SetCookie(w, auth.AdminSessionCookie(r, token, expires))
	http.SetCookie(w, auth.AdminCSRFCookie(r, csrf, expires))
	http.Redirect(w, r, "/admin/", http.StatusFound)
}

func (d *Deps) adminLogoutForm(w http.ResponseWriter, r *http.Request) {
	if !d.checkAdminFormCSRF(w, r) {
		return
	}
	if c, err := r.Cookie(auth.AdminCookieName); err == nil {
		_ = auth.RevokeAdminSession(r.Context(), d.DB, c.Value)
	}
	http.SetCookie(w, auth.ClearAdminSessionCookie(r))
	http.SetCookie(w, auth.ClearAdminCSRFCookie(r))
	http.Redirect(w, r, "/admin/login", http.StatusFound)
}

func (d *Deps) checkAdminFormCSRF(w http.ResponseWriter, r *http.Request) bool {
	sess := adminSessionFromCtx(r.Context())
	if sess == nil {
		http.Redirect(w, r, "/admin/login", http.StatusFound)
		return false
	}
	if err := r.ParseForm(); err != nil {
		d.renderAdminError(w, r, http.StatusBadRequest, "Invalid form")
		return false
	}
	if !auth.VerifyCSRF(sess, strings.TrimSpace(r.FormValue("csrf_token"))) {
		d.renderAdminError(w, r, http.StatusForbidden, "Invalid CSRF token")
		return false
	}
	return true
}
