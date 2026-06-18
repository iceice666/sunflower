package parser_test

import (
	"os"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/parser"
)

func TestParseNextPage_NormalShape(t *testing.T) {
	raw, err := os.ReadFile("testdata/next_response.json")
	if err != nil {
		t.Skipf("fixture not yet captured: %v", err)
	}
	page := parser.ParseNextPage(raw)
	if page.Current.VideoID == "" {
		t.Error("Current.VideoID should not be empty")
	}
	// Related items are optional; just verify no panic.
	t.Logf("related items: %d", len(page.Related))
}

func TestParseNextPage_NoContinuation(t *testing.T) {
	raw, err := os.ReadFile("testdata/next_no_continuation.json")
	if err != nil {
		t.Skipf("fixture not yet captured: %v", err)
	}
	page := parser.ParseNextPage(raw)
	if !page.Continuation.IsZero() {
		t.Error("continuation should be zero when absent in fixture")
	}
}

func TestParseNextPage_EmptyJSON(t *testing.T) {
	page := parser.ParseNextPage([]byte("{}"))
	// Must not panic; all fields zero.
	if page.Current.VideoID != "" {
		t.Errorf("unexpected VideoID: %q", page.Current.VideoID)
	}
}
