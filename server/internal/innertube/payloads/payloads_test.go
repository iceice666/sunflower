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
