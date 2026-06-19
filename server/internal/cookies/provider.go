package cookies

import (
	"context"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"
)

// providerTTL bounds how long a loaded cookie set is reused before reloading,
// so API uploads and file edits take effect without a server restart.
const providerTTL = time.Minute

// ParseCookies turns stored cookie bytes into request-injectable cookies. It
// accepts, in order of preference:
//
//  1. the labeled export ("***INNERTUBE COOKIE*** =<header>", as exported by
//     common browser extensions),
//  2. a raw Cookie-header string ("name=value; name2=value2"),
//  3. Netscape cookies.txt (tab-separated, one cookie per line).
//
// It returns nil when nothing parseable is found, which the InnerTube client
// treats as guest mode.
func ParseCookies(raw []byte) []*http.Cookie {
	if header := extractCookieHeader(raw); header != "" {
		if cs, err := http.ParseCookie(header); err == nil && len(cs) > 0 {
			return cs
		}
	}
	return parseNetscape(raw)
}

// extractCookieHeader returns a Cookie-header string from raw, or "" when raw
// is a Netscape file (which ParseCookies handles via parseNetscape instead).
func extractCookieHeader(raw []byte) string {
	s := string(raw)
	for _, line := range strings.Split(s, "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "***INNERTUBE COOKIE***") {
			if i := strings.IndexByte(line, '='); i >= 0 {
				return strings.TrimSpace(line[i+1:])
			}
		}
	}
	// Netscape files are tab-delimited; never treat them as a header.
	if strings.Contains(s, "\t") {
		return ""
	}
	if t := strings.TrimSpace(s); strings.Contains(t, "=") && !strings.Contains(t, "\n") {
		return t
	}
	return ""
}

// parseNetscape parses Netscape cookies.txt bytes into cookies.
// Per-line format: domain\tincludeSubdomains\tpath\tsecure\texpiry\tname\tvalue
func parseNetscape(raw []byte) []*http.Cookie {
	var out []*http.Cookie
	for _, line := range strings.Split(string(raw), "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		parts := strings.SplitN(line, "\t", 7)
		if len(parts) < 7 {
			continue
		}
		c := &http.Cookie{
			Name:   parts[5],
			Value:  parts[6],
			Path:   parts[2],
			Secure: parts[3] == "TRUE",
		}
		if exp, err := strconv.ParseInt(parts[4], 10, 64); err == nil && exp > 0 {
			c.Expires = time.Unix(exp, 0)
		}
		out = append(out, c)
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

// Provider is the single source of truth for the YouTube cookies attached to
// outgoing InnerTube requests. It reads the encrypted store first (cookies
// uploaded via the API) and falls back to a cookie file on disk (self-host
// bootstrap). Loaded cookies are cached for providerTTL. A nil result means
// guest mode. Safe for concurrent use.
type Provider struct {
	db   *pgxpool.Pool
	key  [32]byte
	file string

	mu        sync.Mutex
	cached    []*http.Cookie
	fetchedAt time.Time
}

// NewProvider builds a Provider. db/key may be zero (no encrypted store); file
// may be "" (no disk fallback). With both unset, Cookies always returns nil.
func NewProvider(db *pgxpool.Pool, key [32]byte, file string) *Provider {
	return &Provider{db: db, key: key, file: file}
}

// Cookies returns the current YouTube cookies, or nil for guest mode. It
// matches the innertube.ClientOpts.Cookies signature.
func (p *Provider) Cookies() []*http.Cookie {
	p.mu.Lock()
	defer p.mu.Unlock()
	if !p.fetchedAt.IsZero() && time.Since(p.fetchedAt) < providerTTL {
		return p.cached
	}
	p.cached = p.load()
	p.fetchedAt = time.Now()
	return p.cached
}

func (p *Provider) load() []*http.Cookie {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	// Encrypted store first — requires a cookie key and a registered user.
	if p.db != nil && p.key != ([32]byte{}) {
		var userID uuid.UUID
		if err := p.db.QueryRow(ctx, `SELECT id FROM users LIMIT 1`).Scan(&userID); err == nil {
			if raw, err := Load(ctx, p.db, p.key, userID, "youtube"); err == nil {
				if cs := ParseCookies(raw); len(cs) > 0 {
					return cs
				}
			}
		}
	}

	// File fallback (self-host bootstrap).
	if p.file != "" {
		if raw, err := os.ReadFile(p.file); err == nil {
			if cs := ParseCookies(raw); len(cs) > 0 {
				return cs
			}
		}
	}

	return nil
}
