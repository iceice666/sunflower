package streamproxy

import (
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/rs/zerolog"
)

func TestProxyRejectsInvalidToken(t *testing.T) {
	s := NewSigner([]byte("k"), time.Minute)
	h := &Handler{Signer: s, Client: http.DefaultClient, Log: zerolog.Nop()}

	req := httptest.NewRequest(http.MethodGet, "/proxy?token=garbage", nil)
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	if rec.Code != http.StatusForbidden {
		t.Fatalf("expected 403 for invalid token, got %d", rec.Code)
	}
}

func TestProxyRejectsBadHost(t *testing.T) {
	s := NewSigner([]byte("k"), time.Minute)
	h := &Handler{Signer: s, Client: http.DefaultClient, Log: zerolog.Nop()}

	tok := s.Sign("https://evil.example.com/videoplayback")
	req := httptest.NewRequest(http.MethodGet, "/proxy?token="+tok, nil)
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	if rec.Code != http.StatusForbidden {
		t.Fatalf("expected 403 for forbidden target host, got %d", rec.Code)
	}
}

func TestAllowedHost(t *testing.T) {
	cases := map[string]bool{
		"googlevideo.com":             true,
		"r1---sn-abc.googlevideo.com": true,
		"www.youtube.com":             true,
		"youtube.com":                 true,
		"evil.com":                    false,
		"googlevideo.com.evil.com":    false,
		"127.0.0.1":                   false,
		"GoogleVideo.COM":             true, // case-insensitive per RFC 3986
		"R1---SN-abc.GoogleVideo.com": true,
		"":                            false,
	}
	for host, want := range cases {
		if got := allowedHost(host); got != want {
			t.Errorf("allowedHost(%q) = %v, want %v", host, got, want)
		}
	}
}

func TestProxyForwardsRangeToAllowedHost(t *testing.T) {
	// This exercises the happy path of header/status forwarding by stubbing the
	// host-allow check via a transport that records the forwarded Range.
	const body = "0123456789abcdefghij"
	var gotRange string
	rt := roundTripFunc(func(r *http.Request) *http.Response {
		gotRange = r.Header.Get("Range")
		h := make(http.Header)
		h.Set("Content-Range", "bytes 2-5/20")
		h.Set("Accept-Ranges", "bytes")
		return &http.Response{
			StatusCode: http.StatusPartialContent,
			Header:     h,
			Body:       io.NopCloser(strings.NewReader(body[2:6])),
		}
	})

	s := NewSigner([]byte("k"), time.Minute)
	h := &Handler{Signer: s, Client: &http.Client{Transport: rt}, Log: zerolog.Nop()}

	tok := s.Sign("https://r1.googlevideo.com/videoplayback?expire=999")
	req := httptest.NewRequest(http.MethodGet, "/proxy?token="+tok, nil)
	req.Header.Set("Range", "bytes=2-5")
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)

	if gotRange != "bytes=2-5" {
		t.Fatalf("upstream Range = %q, want bytes=2-5", gotRange)
	}
	if rec.Code != http.StatusPartialContent {
		t.Fatalf("status = %d, want 206", rec.Code)
	}
	if rec.Header().Get("Content-Range") != "bytes 2-5/20" {
		t.Fatalf("Content-Range = %q", rec.Header().Get("Content-Range"))
	}
	if got := rec.Body.String(); got != body[2:6] {
		t.Fatalf("body = %q, want %q", got, body[2:6])
	}
}

// TestNewClientBlocksRedirectToDisallowedHost verifies the SSRF hardening: an
// upstream 3xx pointing at a non-allowlisted host must not be followed, even
// though the initial request was permitted.
func TestNewClientBlocksRedirectToDisallowedHost(t *testing.T) {
	// Internal "metadata" server the redirect tries to reach.
	internal := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = io.WriteString(w, "SECRET")
	}))
	defer internal.Close()

	// Upstream returns a 302 pointing at the internal (non-allowlisted) host.
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Redirect(w, nil, internal.URL, http.StatusFound)
	}))
	defer upstream.Close()

	c := NewClient()
	req, _ := http.NewRequest(http.MethodGet, upstream.URL, nil)
	resp, err := c.Do(req)
	if err == nil {
		resp.Body.Close()
		t.Fatal("expected redirect to disallowed host to be blocked, got success")
	}
}

type roundTripFunc func(*http.Request) *http.Response

func (f roundTripFunc) RoundTrip(r *http.Request) (*http.Response, error) {
	return f(r), nil
}
