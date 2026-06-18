// Package sync implements M7 write-replay support on the server: idempotency-key
// dedupe on mutating routes, last-write-wins conflict resolution, and periodic
// GC of the idempotency log.
package sync

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/rs/zerolog"
)

// retention is how long an idempotency record is honored for replay. The M7
// acceptance requires replays within 24h to return the same response without
// re-applying; GC removes rows older than this.
const retention = 24 * time.Hour

// Idempotency provides middleware that deduplicates mutating requests by their
// Idempotency-Key header.
type Idempotency struct {
	db  *pgxpool.Pool
	log zerolog.Logger
}

// NewIdempotency builds the middleware helper.
func NewIdempotency(db *pgxpool.Pool, log zerolog.Logger) *Idempotency {
	return &Idempotency{db: db, log: log}
}

// recorder buffers the handler's response so a first-time mutation can be hashed
// and stored, while still streaming the bytes to the real client.
type recorder struct {
	http.ResponseWriter
	status int
	buf    bytes.Buffer
}

func (r *recorder) WriteHeader(code int) {
	r.status = code
	r.ResponseWriter.WriteHeader(code)
}

func (r *recorder) Write(b []byte) (int, error) {
	r.buf.Write(b)
	return r.ResponseWriter.Write(b)
}

// Middleware deduplicates mutating requests. Behavior:
//   - No Idempotency-Key header → pass through unchanged (key is advisory at the
//     transport level; the route's own PKs still dedupe logically).
//   - Key seen before (within retention) → short-circuit with 200 and a
//     {"idempotent_replay":true} body; the handler never runs, so the mutation
//     is not applied twice.
//   - New key → run the handler; if it succeeded (2xx), record the key + a hash
//     of the response for observability.
func (i *Idempotency) Middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw := r.Header.Get("Idempotency-Key")
		if raw == "" {
			next.ServeHTTP(w, r)
			return
		}
		key, err := uuid.Parse(raw)
		if err != nil {
			next.ServeHTTP(w, r) // malformed key — treat as keyless
			return
		}

		q := gen.New(i.db)
		if _, err := q.FindIdempotencyLog(r.Context(), pgtype.UUID{Bytes: key, Valid: true}); err == nil {
			// Already applied — replay a stable response without re-running.
			w.Header().Set("Content-Type", "application/json")
			w.Header().Set("Idempotent-Replay", "true")
			w.WriteHeader(http.StatusOK)
			_, _ = w.Write([]byte(`{"idempotent_replay":true}`))
			return
		}

		rec := &recorder{ResponseWriter: w, status: http.StatusOK}
		next.ServeHTTP(rec, r)

		// Only record successful mutations so a failed attempt can be retried
		// with the same key.
		if rec.status < 200 || rec.status >= 300 {
			return
		}
		sum := sha256.Sum256(rec.buf.Bytes())
		userID := auth.UserIDFromCtx(r.Context())
		deviceID := auth.DeviceIDFromCtx(r.Context())
		if err := q.InsertIdempotencyLog(r.Context(), gen.InsertIdempotencyLogParams{
			Key:          pgtype.UUID{Bytes: key, Valid: true},
			UserID:       pgtype.UUID{Bytes: userID, Valid: userID != uuid.Nil},
			DeviceID:     pgtype.UUID{Bytes: deviceID, Valid: deviceID != uuid.Nil},
			Route:        r.Method + " " + r.URL.Path,
			ResponseHash: pgtype.Text{String: hex.EncodeToString(sum[:]), Valid: true},
			ExpiresAt:    pgtype.Timestamptz{Time: time.Now().Add(retention), Valid: true},
		}); err != nil {
			i.log.Warn().Err(err).Msg("idempotency: record")
		}
	})
}
