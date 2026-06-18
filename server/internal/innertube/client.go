// server/internal/innertube/client.go
package innertube

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"

	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/innertube/sig"
)

const defaultBaseURL = "https://music.youtube.com"

// ClientOpts configures a Client.
type ClientOpts struct {
	HTTPClient *http.Client
	SigCache   *sig.Cache
	Cookies    func() []*http.Cookie // nil = guest mode
	CookieSink func([]*http.Cookie)  // captures Set-Cookie rotations; may be nil
	Locale     models.Locale
	BaseURL    string // override for testing; defaults to https://music.youtube.com
}

// Client posts requests to the InnerTube API and returns raw JSON responses.
// Parsing is handled by the parser package, not here.
type Client struct {
	http       *http.Client
	sig        *sig.Cache
	cookies    func() []*http.Cookie
	cookieSink func([]*http.Cookie)
	locale     models.Locale
	baseURL    string
}

// NewClient creates a Client from opts. SigCache must not be nil.
func NewClient(opts ClientOpts) *Client {
	if opts.HTTPClient == nil {
		opts.HTTPClient = http.DefaultClient
	}
	if opts.BaseURL == "" {
		opts.BaseURL = defaultBaseURL
	}
	return &Client{
		http:       opts.HTTPClient,
		sig:        opts.SigCache,
		cookies:    opts.Cookies,
		cookieSink: opts.CookieSink,
		locale:     opts.Locale,
		baseURL:    opts.BaseURL,
	}
}

// Player calls /youtubei/v1/player and returns a parsed PlayerResponse.
func (c *Client) Player(ctx context.Context, videoID string) (models.PlayerResponse, error) {
	body := BuildAndroidMusicContext(c.locale)
	body["videoId"] = videoID
	body["params"] = "CgIQBg==" // audio-only formats
	raw, err := c.post(ctx, "/youtubei/v1/player", AndroidMusicAPIKey(), body)
	if err != nil {
		return models.PlayerResponse{}, err
	}
	return parsePlayerResponseRaw(ctx, raw, c.sig)
}

// Next calls /youtubei/v1/next and returns raw JSON for the parser to consume.
func (c *Client) Next(ctx context.Context, videoID string, cont continuation.Cursor) (json.RawMessage, error) {
	body := BuildAndroidMusicContext(c.locale)
	body["videoId"] = videoID
	if !cont.IsZero() {
		body["continuation"] = string(cont)
	}
	return c.post(ctx, "/youtubei/v1/next", AndroidMusicAPIKey(), body)
}

// Browse calls /youtubei/v1/browse with WEB_REMIX context.
func (c *Client) Browse(ctx context.Context, browseID string, cont continuation.Cursor) (json.RawMessage, error) {
	body := BuildWebRemixContext(c.locale)
	body["browseId"] = browseID
	if !cont.IsZero() {
		body["continuation"] = string(cont)
	}
	return c.post(ctx, "/youtubei/v1/browse", WebRemixAPIKey(), body)
}

// Search calls /youtubei/v1/search with WEB_REMIX context.
func (c *Client) Search(ctx context.Context, query string) (json.RawMessage, error) {
	body := BuildWebRemixContext(c.locale)
	body["query"] = query
	return c.post(ctx, "/youtubei/v1/search", WebRemixAPIKey(), body)
}

func (c *Client) post(ctx context.Context, path, apiKey string, payload map[string]any) (json.RawMessage, error) {
	encoded, err := json.Marshal(payload)
	if err != nil {
		return nil, fmt.Errorf("innertube post: marshal: %w", err)
	}

	url := c.baseURL + path + "?key=" + apiKey
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(encoded))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	if apiKey == webRemixAPIKey {
		req.Header.Set("User-Agent", webRemixUserAgent)
		req.Header.Set("X-YouTube-Client-Name", webRemixClientID)
		req.Header.Set("X-YouTube-Client-Version", webRemixClientVersion)
	} else {
		req.Header.Set("User-Agent", androidMusicUserAgent)
		req.Header.Set("X-YouTube-Client-Name", androidMusicClientID)
		req.Header.Set("X-YouTube-Client-Version", androidMusicClientVersion)
	}

	if c.cookies != nil {
		for _, ck := range c.cookies() {
			req.AddCookie(ck)
		}
	}

	resp, err := c.http.Do(req)
	if err != nil {
		return nil, fmt.Errorf("innertube post %s: %w", path, err)
	}
	// No defer here yet — may reassign resp below.

	if resp.StatusCode >= 500 {
		// Retry once on 5xx; explicit close before reassigning resp.
		resp.Body.Close()
		req2, _ := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(encoded))
		req2.Header = req.Header.Clone()
		resp, err = c.http.Do(req2)
		if err != nil {
			return nil, fmt.Errorf("innertube post %s retry: %w", path, err)
		}
	}
	// Single defer, always for the final response.
	defer resp.Body.Close()

	// Capture cookies from the final response.
	if c.cookieSink != nil && len(resp.Cookies()) > 0 {
		c.cookieSink(resp.Cookies())
	}

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("innertube post %s: status %d", path, resp.StatusCode)
	}

	return io.ReadAll(resp.Body)
}

// parsePlayerResponseRaw extracts stream URLs from the raw player JSON,
// applies n-param decryption, and picks the best audio stream.
func parsePlayerResponseRaw(ctx context.Context, raw json.RawMessage, cache *sig.Cache) (models.PlayerResponse, error) {
	var pr struct {
		VideoDetails struct {
			VideoID     string `json:"videoId"`
			PlayerJsUrl string `json:"playerJsUrl"` // add this
		} `json:"videoDetails"`
		StreamingData struct {
			AdaptiveFormats []struct {
				Itag            int     `json:"itag"`
				MimeType        string  `json:"mimeType"`
				Bitrate         int     `json:"bitrate"`
				URL             string  `json:"url"`
				SignatureCipher string  `json:"signatureCipher"`
				LoudnessDB      float64 `json:"loudnessDb"`
			} `json:"adaptiveFormats"`
		} `json:"streamingData"`
	}
	if err := json.Unmarshal(raw, &pr); err != nil {
		return models.PlayerResponse{}, fmt.Errorf("parse player: %w", err)
	}

	var streams []models.StreamURL
	var firstNsigErr error
	for _, f := range pr.StreamingData.AdaptiveFormats {
		if !strings.HasPrefix(f.MimeType, "audio/") {
			continue
		}
		rawURL := f.URL
		if rawURL == "" {
			continue // signatureCipher path not yet supported
		}
		decoded, decErr := cache.DecodeN(ctx, rawURL, pr.VideoDetails.PlayerJsUrl)
		if decErr != nil {
			if firstNsigErr == nil {
				firstNsigErr = fmt.Errorf("nsig decode itag %d: %w", f.Itag, decErr)
			}
			decoded = rawURL // best-effort: may be throttled
		}
		streams = append(streams, models.StreamURL{
			URL:      decoded,
			Itag:     f.Itag,
			MimeType: f.MimeType,
			Bitrate:  f.Bitrate,
			Loudness: f.LoudnessDB,
		})
	}

	var best models.StreamURL
	for _, s := range streams {
		if s.Bitrate > best.Bitrate {
			best = s
		}
	}

	return models.PlayerResponse{
		VideoID:     pr.VideoDetails.VideoID,
		PlayerJsURL: pr.VideoDetails.PlayerJsUrl,
		Stream:      best,
		AllStreams:   streams,
		NsigErr:     firstNsigErr,
	}, nil
}
