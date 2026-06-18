// Package recs builds the server-side recommendation surface: the Home feed and
// its sections. It mirrors Metrolist's HomeViewModel fan-out but in Go — a
// candidate pipeline (generate → filter → rank → diversify) over candidates
// drawn from the local library (play history, likes) and InnerTube (home feed,
// related, artist/song radio).
//
// Design references: plans/architecture.md §internal/recs and
// plans/milestones/m5-recommendation-pipeline.md.
package recs

import (
	"context"
	"encoding/json"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/innertube/continuation"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/rs/zerolog"
)

// Candidate is one item flowing through the pipeline before it becomes a
// rendered Item. It carries the raw signal fields the ranker consumes.
type Candidate struct {
	MediaID    string
	Title      string
	Artists    []string
	AlbumID    string
	DurationMs int
	Source     string // "local" | "yt"
	Explicit   bool
	VideoOnly  bool
	IsShort    bool
	ThumbURL   string

	// Signal inputs (populated by the section builder from local stats).
	PlayCount        int // raw play count for this candidate's seed/self
	LastPlayed       time.Time
	RemoteConfidence float64 // 0..1, how strongly the remote source vouched
}

// Item is the rendered form returned to the client.
type Item struct {
	MediaID    string   `json:"media_id"`
	Title      string   `json:"title"`
	Artists    []string `json:"artists,omitempty"`
	AlbumID    string   `json:"album_id,omitempty"`
	DurationMs int      `json:"duration_ms,omitempty"`
	Source     string   `json:"source"`
	ThumbURL   string   `json:"thumbnail_url,omitempty"`
	Score      float64  `json:"score"`
}

// Section is a titled, ordered row in the Home feed.
type Section struct {
	ID    string `json:"id"`
	Title string `json:"title"`
	Kind  string `json:"kind"` // quick_picks | daily_discover | similar_artist | similar_song | community_playlists | yt_home
	Seed  string `json:"seed,omitempty"`
	Items []Item `json:"items"`
}

// Home is the assembled feed.
type Home struct {
	Sections []Section `json:"sections"`
	Chips    []string  `json:"chips,omitempty"`
	Stale    bool      `json:"stale"` // true when served from an expired cache (cold start)
}

// Prefs are the per-user filter toggles wired from the settings screen.
type Prefs struct {
	HideExplicit bool
	HideVideo    bool
	HideShorts   bool
}

// YTClient is the InnerTube surface the recs engine needs. *innertube.Client
// satisfies it; tests substitute a fake. Signatures mirror the client exactly
// (json.RawMessage, not []byte) so the concrete client satisfies the interface.
type YTClient interface {
	Browse(ctx context.Context, browseID string, cont continuation.Cursor) (json.RawMessage, error)
	Next(ctx context.Context, videoID string, cont continuation.Cursor) (json.RawMessage, error)
	Search(ctx context.Context, query string) (json.RawMessage, error)
	Player(ctx context.Context, videoID string) (models.PlayerResponse, error)
}

// contextWithTimeout is a thin wrapper so section builders read uniformly; the
// per-call InnerTube timeout (architecture: 8s) is applied around each fan-out
// call.
func contextWithTimeout(ctx context.Context, d time.Duration) (context.Context, context.CancelFunc) {
	return context.WithTimeout(ctx, d)
}

// Engine assembles recommendation sections. It is safe for concurrent use.
type Engine struct {
	db    *pgxpool.Pool
	yt    YTClient
	log   zerolog.Logger
	cache *Cache
	clock func() time.Time

	// maxFanout caps concurrent InnerTube calls per home build (architecture
	// §internal/recs: capped at 5).
	maxFanout int
	// callTimeout bounds each InnerTube call (architecture: 8s per call).
	callTimeout time.Duration
}

// buildBudget caps the total BuildHome wall-clock so a chain of slow remote
// sections can't exceed the HTTP server's write timeout. Sized below the 30s
// server WriteTimeout with headroom for serialization + the cache write.
const buildBudget = 20 * time.Second

// Options configure an Engine.
type Options struct {
	DB  *pgxpool.Pool
	YT  YTClient // may be nil → remote sections degrade to empty
	Log zerolog.Logger
	// Clock is injectable for cache-TTL tests; defaults to time.Now.
	Clock func() time.Time
}

// NewEngine builds an Engine with the standard fan-out caps.
func NewEngine(opts Options) *Engine {
	clk := opts.Clock
	if clk == nil {
		clk = time.Now
	}
	return &Engine{
		db:          opts.DB,
		yt:          opts.YT,
		log:         opts.Log,
		cache:       &Cache{db: opts.DB, clock: clk},
		clock:       clk,
		maxFanout:   5,
		callTimeout: 8 * time.Second,
	}
}

// BuildHome assembles the full Home feed for a user. Section builders run with
// bounded fan-out; a failed remote section is dropped (logged), never fatal.
// Local-first sections (Quick Picks) never depend on the network.
func (e *Engine) BuildHome(ctx context.Context, userID uuid.UUID, prefs Prefs) (Home, error) {
	// Cap the whole build so a slow remote can't blow past the server's write
	// timeout. Remote sections that don't finish within the budget just return
	// empty (their per-call ctx is cancelled) and are dropped — Quick Picks is
	// local-first and already done by then.
	ctx, cancel := context.WithTimeout(ctx, buildBudget)
	defer cancel()
	var home Home

	// Quick Picks — local-first, no network in the critical path.
	if qp := e.QuickPicks(ctx, userID, prefs); len(qp.Items) > 0 {
		home.Sections = append(home.Sections, qp)
	}

	// Daily Discover — liked-seed → related expansion (remote).
	if dd := e.DailyDiscover(ctx, userID, prefs); len(dd.Items) > 0 {
		home.Sections = append(home.Sections, dd)
	}

	// Similar to <top artists> — one row per top-3 most-played artists.
	for _, sec := range e.SimilarToArtists(ctx, userID, prefs, 3) {
		if len(sec.Items) > 0 {
			home.Sections = append(home.Sections, sec)
		}
	}

	// YouTube Music home feed + chips (remote).
	if yh := e.YouTubeHome(ctx, userID, prefs); len(yh.Items) > 0 {
		home.Chips = yh.chips
		home.Sections = append(home.Sections, yh.Section)
	}

	// Community Playlists (remote search).
	if cp := e.CommunityPlaylists(ctx, userID, prefs); len(cp.Items) > 0 {
		home.Sections = append(home.Sections, cp)
	}

	return home, nil
}

// GetHomeCached returns the cached home for a user (fresh indicates whether it
// is within TTL; found indicates an entry existed at all).
func (e *Engine) GetHomeCached(ctx context.Context, userID uuid.UUID, prefs Prefs) (Home, bool, bool) {
	return e.cache.GetHome(ctx, userID, prefs)
}

// PutHomeCached stores a freshly built home in the cache with the home TTL.
func (e *Engine) PutHomeCached(ctx context.Context, userID uuid.UUID, prefs Prefs, home Home) error {
	return e.cache.PutHome(ctx, userID, prefs, home)
}
