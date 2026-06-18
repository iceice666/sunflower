package streams

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/streamproxy"
)

// Source identifies how a stream URL should be consumed by the client.
type Source string

const (
	SourceLocal   Source = "local"
	SourceYouTube Source = "youtube"
	SourceProxy   Source = "proxy"
)

// ErrUnavailable indicates the media could not be resolved to a playable
// stream (deleted video, region block, no audio formats). Handlers map this to
// HTTP 410.
var ErrUnavailable = errors.New("streams: media unavailable")

// Resolved is the outcome of resolving a media_id to a playable stream.
type Resolved struct {
	MediaID   string
	Source    Source
	StreamURL string
	ExpiresAt time.Time // zero value for local sources (no expiry)
	Itag      int
	MimeType  string
	Loudness  float64
}

// YTResolver resolves a YouTube video ID to a player response. *innertube.Client
// satisfies it; tests substitute a fake.
type YTResolver interface {
	Player(ctx context.Context, videoID string) (models.PlayerResponse, error)
}

// Options tunes a single Resolve call.
type Options struct {
	// PreferProxy routes a YouTube stream through the server proxy instead of
	// handing the client a direct googlevideo URL. Used as the 403/CORS
	// fallback. Ignored when no proxy Signer is configured.
	PreferProxy bool
}

// Resolver dispatches a media_id to the correct stream source.
type Resolver struct {
	YT     YTResolver          // may be nil if only local media is served
	Signer *streamproxy.Signer // nil disables proxy fallback
	// ProxyPath is the server path that serves proxied streams, e.g.
	// "/api/v1/streams/proxy". The token is appended as a query parameter.
	ProxyPath string
}

// Resolve turns a media_id ("<source>:<external_id>") into a playable stream.
func (r *Resolver) Resolve(ctx context.Context, mediaID string, opts Options) (Resolved, error) {
	source, externalID, ok := strings.Cut(mediaID, ":")
	if !ok || externalID == "" {
		return Resolved{}, fmt.Errorf("streams: malformed media_id %q", mediaID)
	}

	switch source {
	case "local":
		return Resolved{
			MediaID:   mediaID,
			Source:    SourceLocal,
			StreamURL: "/api/v1/library/songs/" + mediaID + "/stream",
			// ExpiresAt deliberately left zero: local files never expire.
		}, nil

	case "yt":
		return r.resolveYouTube(ctx, mediaID, externalID, opts)

	default:
		return Resolved{}, fmt.Errorf("streams: unknown source %q in media_id", source)
	}
}

func (r *Resolver) resolveYouTube(ctx context.Context, mediaID, videoID string, opts Options) (Resolved, error) {
	if r.YT == nil {
		return Resolved{}, ErrUnavailable
	}
	pr, err := r.YT.Player(ctx, videoID)
	if err != nil {
		return Resolved{}, fmt.Errorf("streams: yt player: %w", err)
	}
	if pr.Stream.URL == "" {
		return Resolved{}, ErrUnavailable
	}

	res := Resolved{
		MediaID:   mediaID,
		Source:    SourceYouTube,
		StreamURL: pr.Stream.URL,
		ExpiresAt: ExpiryFromURL(pr.Stream.URL),
		Itag:      pr.Stream.Itag,
		MimeType:  pr.Stream.MimeType,
		Loudness:  pr.Stream.Loudness,
	}

	if opts.PreferProxy && r.Signer != nil {
		token := r.Signer.Sign(pr.Stream.URL)
		res.Source = SourceProxy
		res.StreamURL = r.ProxyPath + "?token=" + token
	}

	return res, nil
}
