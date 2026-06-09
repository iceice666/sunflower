package api

import (
	"encoding/json"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/rs/zerolog"
)

// NewRouter wires up all routes and middleware and returns the handler.
// The pool parameter is accepted for forward-compat (M1+ handlers will use it)
// but is not required by the M0 /healthz route, so it may be nil in tests.
func NewRouter(log zerolog.Logger) http.Handler {
	r := chi.NewRouter()

	// Middleware stack (applied in order)
	r.Use(middleware.RequestID)
	r.Use(middleware.Recoverer)
	r.Use(requestLogger(log))
	r.Use(corsMiddleware())

	// M0: boot health check only
	r.Get("/healthz", healthz)

	return r
}

// healthz returns 200 {"status":"ok"}.
func healthz(w http.ResponseWriter, _ *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	_ = json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
}
