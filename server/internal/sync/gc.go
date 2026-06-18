package sync

import (
	"context"
	"time"

	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/rs/zerolog"
)

// StartGC runs the idempotency-log garbage collector hourly until ctx is
// cancelled. It removes rows past their expires_at (older than the 24h
// retention), satisfying the M7 "GC removes rows older than 24h hourly"
// criterion.
func StartGC(ctx context.Context, db *pgxpool.Pool, log zerolog.Logger) {
	go func() {
		ticker := time.NewTicker(time.Hour)
		defer ticker.Stop()
		// Run once promptly on startup, then on each tick.
		RunGC(ctx, db, log)
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				RunGC(ctx, db, log)
			}
		}
	}()
}

// RunGC performs a single GC pass, deleting expired idempotency rows. Exported
// so tests can invoke it deterministically.
func RunGC(ctx context.Context, db *pgxpool.Pool, log zerolog.Logger) {
	n, err := gen.New(db).GCIdempotencyLog(ctx, pgtype.Timestamptz{Time: time.Now(), Valid: true})
	if err != nil {
		log.Warn().Err(err).Msg("idempotency: gc")
		return
	}
	if n > 0 {
		log.Info().Int64("removed", n).Msg("idempotency: gc")
	}
}
