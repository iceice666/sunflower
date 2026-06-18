package api

import (
	"net/http"

	"github.com/iceice666/sunflower/server/internal/sync"
)

// newIdempotency returns the M7 idempotency middleware for mutating routes.
// When DB is nil (smoke tests that only exercise /healthz), it returns a
// passthrough so route registration still works.
func newIdempotency(d Deps) func(http.Handler) http.Handler {
	if d.DB == nil {
		return func(next http.Handler) http.Handler { return next }
	}
	return sync.NewIdempotency(d.DB, d.Log).Middleware
}
