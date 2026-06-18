package continuation_test

import (
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"testing"
)

func TestCursorIsZero(t *testing.T) {
	var zero continuation.Cursor
	if !zero.IsZero() {
		t.Fatal("nil cursor should be zero")
	}
	nonZero := continuation.Cursor([]byte("token"))
	if nonZero.IsZero() {
		t.Fatal("non-empty cursor should not be zero")
	}
}
