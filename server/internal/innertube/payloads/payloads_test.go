package payloads_test

import (
	"encoding/json"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/innertube/payloads"
)

func TestPlayerPayload(t *testing.T) {
	p := payloads.Player("dQw4w9WgXcQ", models.Locale{HL: "en", GL: "US"})
	if p["videoId"] != "dQw4w9WgXcQ" {
		t.Errorf("videoId = %v", p["videoId"])
	}
	b, _ := json.Marshal(p)
	if len(b) == 0 {
		t.Fatal("empty payload")
	}
}

func TestNextPayload_NoContinuation(t *testing.T) {
	p := payloads.Next("dQw4w9WgXcQ", nil, models.Locale{HL: "en", GL: "US"})
	if p["videoId"] != "dQw4w9WgXcQ" {
		t.Errorf("videoId = %v", p["videoId"])
	}
	if _, hasCont := p["continuation"]; hasCont {
		t.Error("continuation should be absent when cursor is zero")
	}
}

func TestNextPayload_WithContinuation(t *testing.T) {
	p := payloads.Next("dQw4w9WgXcQ", continuation.Cursor("tok"), models.Locale{HL: "en", GL: "US"})
	if p["continuation"] != "tok" {
		t.Errorf("continuation = %v, want tok", p["continuation"])
	}
}

func TestBrowsePayload(t *testing.T) {
	p := payloads.Browse("FEmusic_home", nil, models.Locale{HL: "en", GL: "US"})
	if p["browseId"] != "FEmusic_home" {
		t.Errorf("browseId = %v", p["browseId"])
	}
	ctx := p["context"].(map[string]any)["client"].(map[string]any)
	if ctx["clientName"] != "WEB_REMIX" {
		t.Errorf("context should use WEB_REMIX, got %v", ctx["clientName"])
	}
}

func TestSearchPayload(t *testing.T) {
	p := payloads.Search("Beatles", models.Locale{HL: "en", GL: "US"})
	if p["query"] != "Beatles" {
		t.Errorf("query = %v", p["query"])
	}
}
