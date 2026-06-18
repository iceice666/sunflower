package streams

import (
	"testing"
	"time"
)

func TestExpiryFromURL(t *testing.T) {
	t.Run("valid expire param", func(t *testing.T) {
		got := ExpiryFromURL("https://r1.googlevideo.com/videoplayback?expire=1700000000&itag=251")
		want := time.Unix(1700000000, 0).UTC()
		if !got.Equal(want) {
			t.Fatalf("got %v, want %v", got, want)
		}
	})

	t.Run("missing expire param", func(t *testing.T) {
		if got := ExpiryFromURL("https://r1.googlevideo.com/videoplayback?itag=251"); !got.IsZero() {
			t.Fatalf("got %v, want zero time", got)
		}
	})

	t.Run("unparseable expire", func(t *testing.T) {
		if got := ExpiryFromURL("https://x/y?expire=notanumber"); !got.IsZero() {
			t.Fatalf("got %v, want zero time", got)
		}
	})

	t.Run("malformed url", func(t *testing.T) {
		if got := ExpiryFromURL("://bad-url"); !got.IsZero() {
			t.Fatalf("got %v, want zero time", got)
		}
	})
}
