package recs

import (
	"context"
	"encoding/json"
	"errors"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
)

// TTLs per source (plans/architecture.md §internal/recs). radio/automix are not
// cached and have no entry here.
const (
	ttlHome              = 30 * time.Minute
	ttlSimilarTo         = 6 * time.Hour
	ttlCommunityPlaylist = 24 * time.Hour
)

// Cache reads/writes rendered Home payloads in the rec_cache table with a TTL.
type Cache struct {
	db    *pgxpool.Pool
	clock func() time.Time
}

// cachedHome is the stored payload plus its generation time so a cold-start read
// can mark the result stale.
type cachedHome struct {
	Home        Home      `json:"home"`
	GeneratedAt time.Time `json:"generated_at"`
}

// homeCacheKey derives the rec_cache key for a user's home feed. The key folds
// in the inputs that change the result (user, locale/region elided to defaults
// in v1, and the prefs filter hash) so a prefs change misses the old entry.
func homeCacheKey(userID uuid.UUID, prefs Prefs) string {
	return "home:" + userID.String() + ":" + filtersHash(prefs)
}

func filtersHash(p Prefs) string {
	b := byte(0)
	if p.HideExplicit {
		b |= 1
	}
	if p.HideVideo {
		b |= 2
	}
	if p.HideShorts {
		b |= 4
	}
	return string('a' + b)
}

// GetHome returns the cached home for a user. fresh is true when the entry
// exists and has not passed its expires_at; when the entry exists but is
// expired, the Home is still returned with Stale=true for cold-start rendering.
func (c *Cache) GetHome(ctx context.Context, userID uuid.UUID, prefs Prefs) (home Home, fresh, found bool) {
	if c.db == nil {
		return Home{}, false, false
	}
	row, err := gen.New(c.db).GetRecCache(ctx, homeCacheKey(userID, prefs))
	if errors.Is(err, pgx.ErrNoRows) {
		return Home{}, false, false
	}
	if err != nil {
		return Home{}, false, false
	}
	var payload cachedHome
	if err := json.Unmarshal(row.Payload, &payload); err != nil {
		return Home{}, false, false
	}
	now := c.clock()
	fresh = row.ExpiresAt.Valid && now.Before(row.ExpiresAt.Time)
	h := payload.Home
	h.Stale = !fresh
	return h, fresh, true
}

// PutHome stores the rendered home with the home TTL.
func (c *Cache) PutHome(ctx context.Context, userID uuid.UUID, prefs Prefs, home Home) error {
	if c.db == nil {
		return nil
	}
	now := c.clock()
	payload, err := json.Marshal(cachedHome{Home: home, GeneratedAt: now})
	if err != nil {
		return err
	}
	return gen.New(c.db).UpsertRecCache(ctx, gen.UpsertRecCacheParams{
		CacheKey:  homeCacheKey(userID, prefs),
		UserID:    pgtype.UUID{Bytes: userID, Valid: true},
		Payload:   payload,
		ExpiresAt: pgtype.Timestamptz{Time: now.Add(ttlHome), Valid: true},
	})
}
