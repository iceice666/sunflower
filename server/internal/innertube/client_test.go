// server/internal/innertube/client_test.go
package innertube_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/innertube/sig"
)

func TestClientPlayer_MockServer(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Minimal player response: one audio format with a plain URL.
		json.NewEncoder(w).Encode(map[string]any{
			"videoDetails": map[string]any{"videoId": "dQw4w9WgXcQ"},
			"streamingData": map[string]any{
				"formats": []any{},
				"adaptiveFormats": []any{
					map[string]any{
						"itag":     251,
						"mimeType": "audio/webm; codecs=\"opus\"",
						"bitrate":  129000,
						"url":      "https://example.com/stream?n=test",
					},
				},
			},
		})
	}))
	defer srv.Close()

	cache := sig.NewCache(srv.Client())
	cache.LoadNsigForTest("decodeN", "function decodeN(a){return a}")

	client := innertube.NewClient(innertube.ClientOpts{
		HTTPClient: srv.Client(),
		SigCache:   cache,
		Locale:     models.Locale{HL: "en", GL: "US"},
		BaseURL:    srv.URL,
	})

	resp, err := client.Player(context.Background(), "dQw4w9WgXcQ")
	if err != nil {
		t.Fatalf("Player: %v", err)
	}
	if resp.VideoID != "dQw4w9WgXcQ" {
		t.Errorf("VideoID = %q, want dQw4w9WgXcQ", resp.VideoID)
	}
	if resp.Stream.Itag != 251 {
		t.Errorf("Stream.Itag = %d, want 251", resp.Stream.Itag)
	}
}
