// Package streamproxy provides a Range-aware reverse proxy for upstream
// (googlevideo) audio streams, gated by short-lived HMAC-signed tokens so the
// endpoint cannot be used as an open proxy.
package streamproxy

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"strings"
	"time"
)

// ErrInvalidToken is returned when a proxy token is malformed, has a bad
// signature, or has expired.
var ErrInvalidToken = errors.New("streamproxy: invalid or expired token")

// Signer mints and verifies short-lived tokens that authorize proxying a
// specific upstream URL. Tokens are opaque to clients and posted back verbatim.
type Signer struct {
	key []byte
	ttl time.Duration
	now func() time.Time // injectable for tests; defaults to time.Now
}

// tokenPayload is the signed portion of a proxy token.
type tokenPayload struct {
	URL string `json:"u"`
	Exp int64  `json:"e"` // unix seconds
}

// NewSigner returns a Signer using key for HMAC-SHA256 and ttl as the token
// lifetime. key must be non-empty.
func NewSigner(key []byte, ttl time.Duration) *Signer {
	return &Signer{key: key, ttl: ttl, now: time.Now}
}

// Sign returns a token authorizing the proxy to fetch target until the signer's
// configured ttl elapses.
func (s *Signer) Sign(target string) string {
	return s.SignUntil(target, s.now().Add(s.ttl))
}

// SignUntil returns a token authorizing the proxy to fetch target until exp.
// Callers align the token lifetime with an upstream URL's own expiry so a single
// token covers a whole listening session instead of dying after the default ttl.
func (s *Signer) SignUntil(target string, exp time.Time) string {
	payload := tokenPayload{URL: target, Exp: exp.Unix()}
	raw, _ := json.Marshal(payload)
	body := base64.RawURLEncoding.EncodeToString(raw)
	return body + "." + s.mac(body)
}

// Verify checks the token's signature and expiry and returns the target URL.
func (s *Signer) Verify(token string) (string, error) {
	body, sig, ok := strings.Cut(token, ".")
	if !ok || body == "" || sig == "" {
		return "", ErrInvalidToken
	}
	if !hmac.Equal([]byte(sig), []byte(s.mac(body))) {
		return "", ErrInvalidToken
	}
	raw, err := base64.RawURLEncoding.DecodeString(body)
	if err != nil {
		return "", ErrInvalidToken
	}
	var payload tokenPayload
	if err := json.Unmarshal(raw, &payload); err != nil {
		return "", ErrInvalidToken
	}
	if s.now().Unix() > payload.Exp {
		return "", ErrInvalidToken
	}
	return payload.URL, nil
}

// mac returns the base64url HMAC-SHA256 of body under the signer's key.
func (s *Signer) mac(body string) string {
	h := hmac.New(sha256.New, s.key)
	h.Write([]byte(body))
	return base64.RawURLEncoding.EncodeToString(h.Sum(nil))
}
