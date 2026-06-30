package api

import (
	"context"
	"net/http"

	"github.com/iceice666/sunflower/server/internal/auth"
)

type adminCtxKey int

const (
	adminCtxSession adminCtxKey = iota
	adminCtxCSRFToken
)

func (d *Deps) adminHTMLMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		sess, csrf, ok := d.adminSessionFromRequest(w, r, true)
		if !ok {
			return
		}
		ctx := context.WithValue(r.Context(), adminCtxSession, sess)
		ctx = context.WithValue(ctx, adminCtxCSRFToken, csrf)
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

func (d *Deps) adminJSONMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		sess, csrf, ok := d.adminSessionFromRequest(w, r, false)
		if !ok {
			return
		}
		ctx := context.WithValue(r.Context(), adminCtxSession, sess)
		ctx = context.WithValue(ctx, adminCtxCSRFToken, csrf)
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

func (d *Deps) adminCSRFMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		sess := adminSessionFromCtx(r.Context())
		token := r.Header.Get("X-CSRF-Token")
		if token == "" {
			token = r.FormValue("csrf_token")
		}
		if sess == nil || !auth.VerifyCSRF(sess, token) {
			jsonError(w, "invalid_csrf", http.StatusForbidden)
			return
		}
		next.ServeHTTP(w, r)
	})
}

func (d *Deps) adminSessionFromRequest(w http.ResponseWriter, r *http.Request, redirect bool) (*auth.AdminSession, string, bool) {
	cookie, err := r.Cookie(auth.AdminCookieName)
	if err != nil || cookie.Value == "" {
		if redirect {
			http.Redirect(w, r, "/admin/login", http.StatusFound)
		} else {
			jsonError(w, "missing_admin_session", http.StatusUnauthorized)
		}
		return nil, "", false
	}
	sess, err := auth.VerifyAdminSession(r.Context(), d.DB, cookie.Value)
	if err != nil {
		if redirect {
			http.Redirect(w, r, "/admin/login", http.StatusFound)
		} else {
			jsonError(w, "invalid_admin_session", http.StatusUnauthorized)
		}
		return nil, "", false
	}
	csrf := ""
	if c, err := r.Cookie(auth.AdminCSRFCookieName); err == nil {
		csrf = c.Value
	}
	return sess, csrf, true
}

func adminSessionFromCtx(ctx context.Context) *auth.AdminSession {
	sess, _ := ctx.Value(adminCtxSession).(*auth.AdminSession)
	return sess
}

func adminCSRFTokenFromCtx(ctx context.Context) string {
	token, _ := ctx.Value(adminCtxCSRFToken).(string)
	return token
}
