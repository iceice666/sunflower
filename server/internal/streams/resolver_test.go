package streams

import (
	"context"
	"errors"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/streamproxy"
)

type fakeYT struct {
	resp models.PlayerResponse
	err  error
}

func (f fakeYT) Player(_ context.Context, _ string) (models.PlayerResponse, error) {
	return f.resp, f.err
}

func TestResolveLocal(t *testing.T) {
	r := &Resolver{}
	got, err := r.Resolve(context.Background(), "local:01HZABC", Options{})
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if got.Source != SourceLocal {
		t.Fatalf("source = %q, want local", got.Source)
	}
	if !strings.Contains(got.StreamURL, "local:01HZABC") {
		t.Fatalf("stream url = %q, want it to reference the media id", got.StreamURL)
	}
	if !got.ExpiresAt.IsZero() {
		t.Fatalf("local ExpiresAt = %v, want zero (never expires)", got.ExpiresAt)
	}
}

func TestResolveYouTubeDirect(t *testing.T) {
	exp := time.Now().Add(2 * time.Hour).Unix()
	url := "https://r1.googlevideo.com/videoplayback?expire=" + itoa(exp)
	r := &Resolver{YT: fakeYT{resp: models.PlayerResponse{
		Stream: models.StreamURL{URL: url, Itag: 251, MimeType: "audio/webm"},
	}}}

	got, err := r.Resolve(context.Background(), "yt:abc123", Options{})
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if got.Source != SourceYouTube {
		t.Fatalf("source = %q, want youtube", got.Source)
	}
	if got.StreamURL != url {
		t.Fatalf("stream url = %q, want %q", got.StreamURL, url)
	}
	if got.ExpiresAt.Unix() != exp {
		t.Fatalf("ExpiresAt = %d, want %d", got.ExpiresAt.Unix(), exp)
	}
}

func TestResolveYouTubePreferProxy(t *testing.T) {
	url := "https://r1.googlevideo.com/videoplayback?expire=999999999999"
	signer := streamproxy.NewSigner([]byte("k"), time.Minute)
	r := &Resolver{
		YT:        fakeYT{resp: models.PlayerResponse{Stream: models.StreamURL{URL: url}}},
		Signer:    signer,
		ProxyPath: "/api/v1/streams/proxy",
	}

	got, err := r.Resolve(context.Background(), "yt:abc123", Options{PreferProxy: true})
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if got.Source != SourceProxy {
		t.Fatalf("source = %q, want proxy", got.Source)
	}
	if !strings.HasPrefix(got.StreamURL, "/api/v1/streams/proxy?token=") {
		t.Fatalf("stream url = %q, want a proxy url with token", got.StreamURL)
	}
	// The proxy token must round-trip back to the original upstream URL.
	tok := strings.TrimPrefix(got.StreamURL, "/api/v1/streams/proxy?token=")
	back, err := signer.Verify(tok)
	if err != nil || back != url {
		t.Fatalf("proxy token did not round-trip: back=%q err=%v", back, err)
	}
}

func TestResolveYouTubeUnavailable(t *testing.T) {
	// Player returns an empty stream URL → ErrUnavailable.
	r := &Resolver{YT: fakeYT{resp: models.PlayerResponse{}}}
	_, err := r.Resolve(context.Background(), "yt:gone", Options{})
	if !errors.Is(err, ErrUnavailable) {
		t.Fatalf("err = %v, want ErrUnavailable", err)
	}
}

func TestResolveYouTubePlayerError(t *testing.T) {
	r := &Resolver{YT: fakeYT{err: errors.New("network")}}
	_, err := r.Resolve(context.Background(), "yt:abc", Options{})
	if err == nil || errors.Is(err, ErrUnavailable) {
		t.Fatalf("err = %v, want a wrapped player error (not ErrUnavailable)", err)
	}
}

func TestResolveNoYTClient(t *testing.T) {
	r := &Resolver{} // YT nil
	_, err := r.Resolve(context.Background(), "yt:abc", Options{})
	if !errors.Is(err, ErrUnavailable) {
		t.Fatalf("err = %v, want ErrUnavailable when YT client is nil", err)
	}
}

func TestResolveMalformedMediaID(t *testing.T) {
	r := &Resolver{}
	for _, id := range []string{"", "noseparator", "yt:", ":id"} {
		if _, err := r.Resolve(context.Background(), id, Options{}); err == nil {
			t.Fatalf("expected error for malformed media_id %q", id)
		}
	}
}

func TestResolveUnknownSource(t *testing.T) {
	r := &Resolver{}
	if _, err := r.Resolve(context.Background(), "spotify:track", Options{}); err == nil {
		t.Fatal("expected error for unknown source")
	}
}

func itoa(n int64) string {
	return strconv.FormatInt(n, 10)
}
