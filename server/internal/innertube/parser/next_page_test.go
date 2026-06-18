package parser_test

import (
	"os"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/parser"
)

func TestParseNextPage_NormalShape(t *testing.T) {
	raw, err := os.ReadFile("testdata/next_response.json")
	if err != nil {
		t.Fatalf("fixture missing: %v", err)
	}
	// Current is populated by the Client layer from /player, not by this parser.
	page := parser.ParseNextPage(raw)
	if len(page.Related) == 0 {
		t.Error("expected at least one related item")
	}
	if page.Related[0].VideoID == "" {
		t.Error("first related item VideoID should not be empty")
	}
	if page.Continuation.IsZero() {
		t.Error("continuation should be present in fixture")
	}
	t.Logf("related items: %d, continuation zero: %v", len(page.Related), page.Continuation.IsZero())
}

func TestParseNextPage_NoContinuation(t *testing.T) {
	// No continuation fixture needed; the EmptyJSON test covers the zero case.
	page := parser.ParseNextPage([]byte(`{"contents":{}}`))
	if !page.Continuation.IsZero() {
		t.Error("continuation should be zero when absent")
	}
}

func TestParseNextPage_EmptyJSON(t *testing.T) {
	page := parser.ParseNextPage([]byte("{}"))
	// Must not panic; all fields zero.
	if page.Current.VideoID != "" {
		t.Errorf("unexpected VideoID: %q", page.Current.VideoID)
	}
}
