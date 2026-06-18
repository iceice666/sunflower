package streamproxy

import (
	"errors"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/rs/zerolog"
)

// allowedHost restricts the proxy to YouTube media origins. A signed token
// already authorizes the URL, but this is defense-in-depth against a leaked key
// being used to reach arbitrary internal hosts (SSRF). Host comparison is
// case-insensitive per RFC 3986.
func allowedHost(host string) bool {
	host = strings.ToLower(host)
	return host == "googlevideo.com" ||
		strings.HasSuffix(host, ".googlevideo.com") ||
		host == "youtube.com" ||
		strings.HasSuffix(host, ".youtube.com")
}

// NewClient returns an *http.Client suitable for proxying upstream media.
//
// It deliberately sets no Client.Timeout: that deadline bounds the entire
// request including the streaming body read, which would truncate a long
// ranged-audio response. Cancellation instead rides on the request context
// (the inbound client's connection). A per-hop CheckRedirect re-validates the
// host on every redirect so an upstream 3xx cannot bounce the proxy to an
// internal/non-allowlisted host (SSRF hardening).
func NewClient() *http.Client {
	return &http.Client{
		Transport: &http.Transport{
			Proxy:                 http.ProxyFromEnvironment,
			ForceAttemptHTTP2:     true,
			MaxIdleConns:          100,
			IdleConnTimeout:       90 * time.Second,
			TLSHandshakeTimeout:   10 * time.Second,
			ExpectContinueTimeout: time.Second,
			ResponseHeaderTimeout: 30 * time.Second,
		},
		CheckRedirect: func(req *http.Request, _ []*http.Request) error {
			if !allowedHost(req.URL.Hostname()) {
				return errors.New("streamproxy: redirect to disallowed host blocked")
			}
			return nil
		},
	}
}

// Handler is the HTTP handler for GET /api/v1/streams/proxy?token=…
//
// It verifies the token, then streams the upstream response back to the client,
// forwarding the Range request header and the upstream status (including 206
// Partial Content) so seeking works through the proxy.
type Handler struct {
	Signer *Signer
	Client *http.Client
	Log    zerolog.Logger
}

func (h *Handler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	token := r.URL.Query().Get("token")
	target, err := h.Signer.Verify(token)
	if err != nil {
		http.Error(w, `{"error":"invalid_token"}`, http.StatusForbidden)
		return
	}

	u, err := url.Parse(target)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || !allowedHost(u.Hostname()) {
		http.Error(w, `{"error":"forbidden_target"}`, http.StatusForbidden)
		return
	}

	req, err := http.NewRequestWithContext(r.Context(), http.MethodGet, target, nil)
	if err != nil {
		http.Error(w, `{"error":"bad_target"}`, http.StatusBadGateway)
		return
	}
	// Forward Range so the upstream answers with 206 + the requested window.
	if rng := r.Header.Get("Range"); rng != "" {
		req.Header.Set("Range", rng)
	}

	resp, err := h.Client.Do(req)
	if err != nil {
		h.Log.Warn().Err(err).Msg("streamproxy: upstream fetch failed")
		http.Error(w, `{"error":"upstream_error"}`, http.StatusBadGateway)
		return
	}
	defer resp.Body.Close()

	// Mirror the headers a media player needs for ranged playback.
	copyHeader(w.Header(), resp.Header, "Content-Type")
	copyHeader(w.Header(), resp.Header, "Content-Length")
	copyHeader(w.Header(), resp.Header, "Content-Range")
	copyHeader(w.Header(), resp.Header, "Accept-Ranges")
	copyHeader(w.Header(), resp.Header, "Last-Modified")

	w.WriteHeader(resp.StatusCode)
	_, _ = io.Copy(w, resp.Body)
}

func copyHeader(dst, src http.Header, key string) {
	if v := src.Get(key); v != "" {
		dst.Set(key, v)
	}
}
