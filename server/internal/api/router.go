package api

import (
	"encoding/json"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/jobs"
	"github.com/iceice666/sunflower/server/internal/library"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/rs/zerolog"
)

// Deps holds server-level dependencies injected into HTTP handlers.
// DB, Jobs, and Scanner may be nil in unit tests that only exercise /healthz.
type Deps struct {
	Log     zerolog.Logger
	DB      *pgxpool.Pool
	Jobs    *jobs.Registry
	Scanner *library.Scanner
}

// NewRouter builds the chi router with all M0–M1 routes and middleware.
func NewRouter(d Deps) http.Handler {
	r := chi.NewRouter()

	r.Use(middleware.RequestID)
	r.Use(middleware.Recoverer)
	r.Use(requestLogger(d.Log))
	r.Use(corsMiddleware())

	r.Get("/healthz", healthz)

	r.Route("/api/v1", func(r chi.Router) {
		r.Post("/auth/register-device", d.registerDevice)

		r.Group(func(r chi.Router) {
			r.Use(auth.Middleware(d.DB))
			r.Post("/library/scan", d.startScan)
			r.Get("/library/songs", d.listSongs)
			r.Get("/library/albums", d.listAlbums)
			r.Get("/library/artists", d.listArtists)
			r.Get("/jobs/{id}", d.getJob)
		})
	})

	return r
}

func healthz(w http.ResponseWriter, _ *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	_ = json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
}

func jsonOK(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(v)
}

func jsonError(w http.ResponseWriter, msg string, code int) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	_ = json.NewEncoder(w).Encode(map[string]string{"error": msg})
}
