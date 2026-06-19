package sig

import (
	"errors"
	"testing"
)

// TestPlayerHashReEscapedSlashes locks in the iframe_api drift fix: the body
// JSON-escapes slashes as `\/s\/player\/<hash>\/…`, and the older legacy form
// uses bare slashes. Both must yield the same player-JS hash.
func TestPlayerHashReEscapedSlashes(t *testing.T) {
	cases := []struct {
		name string
		body string
		want string
	}{
		{
			name: "escaped slashes (current iframe_api)",
			body: `g.url="https:\/\/www.youtube.com\/s\/player\/ac678d18\/www-widgetapi.vflset\/www-widgetapi.js"`,
			want: "ac678d18",
		},
		{
			name: "bare slashes (legacy)",
			body: `src="https://www.youtube.com/s/player/deadbeef/player_ias.vflset/en_US/base.js"`,
			want: "deadbeef",
		},
		{
			name: "hash with hyphen and underscore",
			body: `"\/s\/player\/a1_b-2c\/"`,
			want: "a1_b-2c",
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			m := playerHashRe.FindStringSubmatch(tc.body)
			if m == nil {
				t.Fatalf("no match in %q", tc.body)
			}
			if m[1] != tc.want {
				t.Fatalf("hash = %q, want %q", m[1], tc.want)
			}
		})
	}
}

// TestParseAndReplaceNBestEffort verifies the throttle-mitigation contract:
// a nil descrambler (stale base.js pattern) must not error out the stream —
// the URL is returned unchanged with ErrNsigUnavailable so callers can surface
// the degradation while still playing (throttled).
func TestParseAndReplaceNBestEffort(t *testing.T) {
	const urlWithN = "https://example.googlevideo.com/videoplayback?itag=251&n=ABC123&mime=audio"

	got, err := parseAndReplaceN(urlWithN, nil)
	if !errors.Is(err, ErrNsigUnavailable) {
		t.Fatalf("err = %v, want ErrNsigUnavailable", err)
	}
	if got != urlWithN {
		t.Fatalf("url = %q, want unchanged %q", got, urlWithN)
	}

	// No n-param → no work, no error, even with a nil descrambler.
	const urlNoN = "https://example.googlevideo.com/videoplayback?itag=251&mime=audio"
	got, err = parseAndReplaceN(urlNoN, nil)
	if err != nil {
		t.Fatalf("unexpected err for n-less URL: %v", err)
	}
	if got != urlNoN {
		t.Fatalf("url = %q, want unchanged %q", got, urlNoN)
	}
}
