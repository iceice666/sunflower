package api_test

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/iceice666/sunflower/server/internal/api"
	"github.com/rs/zerolog"
)

func TestM9M10SecureEnrollmentAndAdmin(t *testing.T) {
	ctx := context.Background()
	pool := testPool(t, ctx)
	defer pool.Close()

	handler := api.NewRouter(api.Deps{Log: zerolog.Nop(), DB: pool})
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)

	statusResp := doJSON(t, srv, http.MethodGet, "/api/v1/setup/status", nil, "")
	var status struct {
		Configured      bool `json:"configured"`
		PairingRequired bool `json:"pairing_required"`
	}
	mustDecode(t, statusResp.Body, &status)
	if status.Configured || !status.PairingRequired {
		t.Fatalf("fresh setup status = %+v, want configured=false pairing_required=true", status)
	}

	noCode := doJSON(t, srv, http.MethodPost, "/api/v1/auth/register-device",
		map[string]string{"device_name": "rogue", "platform": "test"}, "")
	assertJSONError(t, noCode, http.StatusForbidden, "pairing_required")

	setupResp := doJSON(t, srv, http.MethodPost, "/api/v1/setup/owner", map[string]string{
		"setup_token":  "sunflower-test-setup-token",
		"display_name": "Owner",
		"password":     "sunflower owner password",
	}, "")
	if setupResp.StatusCode != http.StatusOK {
		t.Fatalf("setup owner: want 200, got %d", setupResp.StatusCode)
	}
	setupResp.Body.Close()

	setupAgain := doJSON(t, srv, http.MethodPost, "/api/v1/setup/owner", map[string]string{
		"setup_token":  "sunflower-test-setup-token",
		"display_name": "Owner",
		"password":     "sunflower owner password",
	}, "")
	assertJSONError(t, setupAgain, http.StatusForbidden, "setup_disabled")

	adminNoCookie := doJSON(t, srv, http.MethodGet, "/api/v1/admin/status", nil, "")
	assertJSONError(t, adminNoCookie, http.StatusUnauthorized, "missing_admin_session")

	loginResp := doJSON(t, srv, http.MethodPost, "/api/v1/admin/auth/login",
		map[string]string{"password": "sunflower owner password"}, "")
	if loginResp.StatusCode != http.StatusOK {
		t.Fatalf("admin login: want 200, got %d", loginResp.StatusCode)
	}
	var login struct {
		CSRFToken string `json:"csrf_token"`
	}
	mustDecode(t, loginResp.Body, &login)
	cookies := loginResp.Cookies()

	adminStatus := doAdminJSON(t, srv, http.MethodGet, "/api/v1/admin/status", nil, cookies, "")
	if adminStatus.StatusCode != http.StatusOK {
		t.Fatalf("admin status: want 200, got %d", adminStatus.StatusCode)
	}
	adminStatus.Body.Close()

	for _, path := range []string{
		"/admin/",
		"/admin/devices",
		"/admin/pairing/new",
		"/admin/library",
		"/admin/cookies/youtube",
		"/admin/now-playing",
		"/admin/audit",
	} {
		resp := doAdminGET(t, srv, path, cookies)
		if resp.StatusCode != http.StatusOK {
			t.Fatalf("GET %s: want 200, got %d", path, resp.StatusCode)
		}
		resp.Body.Close()
	}

	pairNoCSRF := doAdminJSON(t, srv, http.MethodPost, "/api/v1/admin/pairing-codes",
		map[string]any{"label": "Pixel", "ttl_seconds": 600}, cookies, "")
	assertJSONError(t, pairNoCSRF, http.StatusForbidden, "invalid_csrf")

	pairResp := doAdminJSON(t, srv, http.MethodPost, "/api/v1/admin/pairing-codes",
		map[string]any{"label": "Pixel", "ttl_seconds": 600}, cookies, login.CSRFToken)
	if pairResp.StatusCode != http.StatusOK {
		t.Fatalf("pairing code: want 200, got %d", pairResp.StatusCode)
	}
	var pair struct {
		PairingCode string `json:"pairing_code"`
		PairingURL  string `json:"pairing_url"`
	}
	mustDecode(t, pairResp.Body, &pair)
	if pair.PairingCode == "" || !strings.HasPrefix(pair.PairingURL, "sunflower://pair?") {
		t.Fatalf("bad pairing response: %+v", pair)
	}

	regResp := doJSON(t, srv, http.MethodPost, "/api/v1/auth/register-device",
		map[string]string{
			"device_name":    "Pixel",
			"platform":       "android",
			"client_version": "0.3.0",
			"pairing_code":   pair.PairingCode,
		}, "")
	if regResp.StatusCode != http.StatusOK {
		t.Fatalf("register paired device: want 200, got %d", regResp.StatusCode)
	}
	var dev pairedDevice
	mustDecode(t, regResp.Body, &dev)

	reuse := doJSON(t, srv, http.MethodPost, "/api/v1/auth/register-device",
		map[string]string{
			"device_name":  "Reuse",
			"platform":     "android",
			"pairing_code": pair.PairingCode,
		}, "")
	assertJSONError(t, reuse, http.StatusUnauthorized, "invalid_pairing_code")

	authed := doJSON(t, srv, http.MethodGet, "/api/v1/library/songs", nil, dev.Token)
	if authed.StatusCode != http.StatusOK {
		t.Fatalf("valid device token: want 200, got %d", authed.StatusCode)
	}
	authed.Body.Close()

	revoke := doAdminJSON(t, srv, http.MethodPost, "/api/v1/admin/devices/"+dev.DeviceID+"/revoke",
		map[string]string{"reason": "Lost phone"}, cookies, login.CSRFToken)
	if revoke.StatusCode != http.StatusOK {
		t.Fatalf("revoke device: want 200, got %d", revoke.StatusCode)
	}
	revoke.Body.Close()

	revoked := doJSON(t, srv, http.MethodGet, "/api/v1/library/songs", nil, dev.Token)
	assertJSONError(t, revoked, http.StatusUnauthorized, "device_revoked")

	noRedirect := &http.Client{
		CheckRedirect: func(*http.Request, []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
	resp, err := noRedirect.Get(srv.URL + "/admin/")
	if err != nil && !errors.Is(err, http.ErrUseLastResponse) {
		t.Fatalf("GET /admin/: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusFound || resp.Header.Get("Location") != "/admin/login" {
		t.Fatalf("GET /admin/ unauth: got %d Location=%q", resp.StatusCode, resp.Header.Get("Location"))
	}
}

func assertJSONError(t *testing.T, resp *http.Response, status int, code string) {
	t.Helper()
	defer resp.Body.Close()
	if resp.StatusCode != status {
		t.Fatalf("want status %d, got %d", status, resp.StatusCode)
	}
	var body struct {
		Error string `json:"error"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		t.Fatalf("decode error body: %v", err)
	}
	if body.Error != code {
		t.Fatalf("want error %q, got %q", code, body.Error)
	}
}

func doAdminGET(t *testing.T, srv *httptest.Server, path string, cookies []*http.Cookie) *http.Response {
	t.Helper()
	req, err := http.NewRequest(http.MethodGet, srv.URL+path, nil)
	if err != nil {
		t.Fatal(err)
	}
	for _, c := range cookies {
		req.AddCookie(c)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	return resp
}
