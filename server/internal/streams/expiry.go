// Package streams resolves a media_id to a playable stream URL, choosing
// between a local file, a direct YouTube (googlevideo) URL, or the server's
// Range-aware proxy as a fallback.
package streams

import (
	"net/url"
	"strconv"
	"time"
)

// ExpiryFromURL extracts the expiry time encoded in a googlevideo stream URL.
//
// googlevideo URLs carry an "expire" query parameter holding a unix timestamp
// (seconds). Returns the zero time if the parameter is absent or unparseable —
// callers treat a zero expiry as "unknown", not "already expired".
func ExpiryFromURL(streamURL string) time.Time {
	u, err := url.Parse(streamURL)
	if err != nil {
		return time.Time{}
	}
	exp := u.Query().Get("expire")
	if exp == "" {
		return time.Time{}
	}
	secs, err := strconv.ParseInt(exp, 10, 64)
	if err != nil {
		return time.Time{}
	}
	return time.Unix(secs, 0).UTC()
}
