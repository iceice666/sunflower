package sig

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"regexp"
	"sync"
	"time"

	"github.com/dop251/goja"
)

var (
	ErrNoPlayerJs     = errors.New("sig: no player js available")
	ErrSigInvalidated = errors.New("sig: invalidated after sustained 403s")

	// iframeAPIURL is fetched to discover the current player JS hash.
	iframeAPIURL = "https://www.youtube.com/iframe_api"

	// playerHashRe extracts the player hash from the iframe_api JS body.
	// The charset is widened to [a-zA-Z0-9_-] because real hashes are not
	// strictly lowercase hex.
	playerHashRe = regexp.MustCompile(`/s/player/([a-zA-Z0-9_-]+)/`)

	cacheTTL      = 6 * time.Hour
	failThreshold = 3
	failWindow    = 60 * time.Second
)

type entry struct {
	nsig     *nsigEntry
	loadedAt time.Time
}

// Cache fetches, parses, and caches base.js per player-JS hash.
// It is safe for concurrent use.
type Cache struct {
	mu      sync.RWMutex
	entries map[string]*entry // key = playerJsHash
	current string            // most recently bootstrapped hash
	http    *http.Client

	failMu sync.Mutex
	fails  map[string][]time.Time // recent 403 timestamps per hash
}

// NewCache creates a Cache. Pass nil to use http.DefaultClient.
func NewCache(httpClient *http.Client) *Cache {
	if httpClient == nil {
		httpClient = http.DefaultClient
	}
	return &Cache{
		entries: make(map[string]*entry),
		fails:   make(map[string][]time.Time),
		http:    httpClient,
	}
}

// Bootstrap fetches https://www.youtube.com/iframe_api, derives the current
// player JS hash, fetches and parses base.js, and pre-warms the cache.
// Call once at startup; DecodeN will also call it lazily if needed.
func (c *Cache) Bootstrap(ctx context.Context) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, iframeAPIURL, nil)
	if err != nil {
		return err
	}
	resp, err := c.http.Do(req)
	if err != nil {
		return fmt.Errorf("sig bootstrap: %w", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("sig bootstrap: read: %w", err)
	}
	m := playerHashRe.FindSubmatch(body)
	if m == nil {
		return fmt.Errorf("sig bootstrap: player hash not found in iframe_api response")
	}
	hash := string(m[1])
	baseJsURL := fmt.Sprintf("https://www.youtube.com/s/player/%s/player_ias.vflset/en_US/base.js", hash)
	return c.load(ctx, hash, baseJsURL)
}

func (c *Cache) load(ctx context.Context, hash, baseJsURL string) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, baseJsURL, nil)
	if err != nil {
		return err
	}
	resp, err := c.http.Do(req)
	if err != nil {
		return fmt.Errorf("sig load base.js: %w", err)
	}
	defer resp.Body.Close()
	js, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("sig load base.js read: %w", err)
	}
	nsig, err := extractNsig(string(js))
	if err != nil {
		return fmt.Errorf("sig load %s: %w", hash, err)
	}
	c.mu.Lock()
	c.entries[hash] = &entry{nsig: nsig, loadedAt: time.Now()}
	c.current = hash
	c.mu.Unlock()
	return nil
}

func (c *Cache) getEntry(ctx context.Context, playerJsURL string) (*entry, error) {
	hash := playerJsURL
	if m := playerHashRe.FindStringSubmatch(playerJsURL); m != nil {
		hash = m[1]
	}

	c.mu.RLock()
	e, ok := c.entries[hash]
	c.mu.RUnlock()
	if ok && time.Since(e.loadedAt) < cacheTTL {
		return e, nil
	}

	// Not cached or stale — fetch.
	if playerJsURL == "" {
		if err := c.Bootstrap(ctx); err != nil {
			return nil, fmt.Errorf("%w: bootstrap failed: %v", ErrNoPlayerJs, err)
		}
		c.mu.RLock()
		e = c.entries[c.current]
		c.mu.RUnlock()
		return e, nil
	}
	if err := c.load(ctx, hash, playerJsURL); err != nil {
		return nil, err
	}
	c.mu.RLock()
	e = c.entries[hash]
	c.mu.RUnlock()
	return e, nil
}

// DecodeN replaces the ?n= query parameter in rawURL with the decoded value.
// playerJsURL is the absolute base.js URL from PlayerResponse.PlayerJsURL;
// if empty, the most recently bootstrapped entry is used.
func (c *Cache) DecodeN(ctx context.Context, rawURL, playerJsURL string) (string, error) {
	e, err := c.getEntry(ctx, playerJsURL)
	if err != nil {
		return rawURL, err
	}

	parsed, err := parseAndReplaceN(rawURL, e.nsig)
	if err != nil {
		return rawURL, err
	}
	return parsed, nil
}

// LoadNsigForTest loads a pre-compiled nsig entry for unit testing.
// Do not call in production code.
func (c *Cache) LoadNsigForTest(funcName, funcBody string) error {
	src := "var " + funcName + "=" + funcBody
	prog, err := goja.Compile("nsig_test", src, false)
	if err != nil {
		return err
	}
	e := &entry{
		nsig:     &nsigEntry{prog: prog, funcName: funcName},
		loadedAt: time.Now(),
	}
	c.mu.Lock()
	c.entries["__test__"] = e
	c.current = "__test__"
	c.mu.Unlock()
	return nil
}

// DecodeNRaw decodes a raw n-token (not a full URL) using the currently loaded entry.
// Only for testing.
func (c *Cache) DecodeNRaw(ctx context.Context, token string) (string, error) {
	c.mu.RLock()
	e, ok := c.entries[c.current]
	c.mu.RUnlock()
	if !ok {
		return "", ErrNoPlayerJs
	}
	return e.nsig.decode(token)
}
