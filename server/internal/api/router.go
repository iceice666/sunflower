package api

import (
	"encoding/json"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/jobs"
	"github.com/iceice666/sunflower/server/internal/library"
	"github.com/iceice666/sunflower/server/internal/queue"
	"github.com/iceice666/sunflower/server/internal/recs"
	"github.com/iceice666/sunflower/server/internal/streamproxy"
	"github.com/iceice666/sunflower/server/internal/streams"
	"github.com/iceice666/sunflower/server/internal/ws"
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
	// DataDir is the root directory for server-managed data files (cover art, etc.).
	// Required by the stream and art handlers; matches config.Config.DataDir.
	DataDir   string
	CookieKey [32]byte // zero = cookies disabled

	// M4 queue/streams dependencies. Nil disables the corresponding routes'
	// network behavior (e.g. a nil YT client makes YouTube seeds unavailable).
	Queue   *queue.Store
	Streams *streams.Resolver
	Proxy   *streamproxy.Handler
	YT      queueAndStreamYT // InnerTube client for radio + resolve; may be nil

	// M5 recommendation engine. Nil disables /home, /likes, /playlists,
	// /impressions (they 503).
	Recs *recs.Engine

	// M8 now-playing WebSocket hub. Nil disables /ws/now-playing and /admin.
	Hub *ws.Hub
}

// queueAndStreamYT is the InnerTube surface the queue and stream handlers need.
// *innertube.Client satisfies it.
type queueAndStreamYT interface {
	queue.NextClient
	streams.YTResolver
}

// NewRouter builds the chi router with all M0–M2 routes and middleware.
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

			// M7 idempotency middleware for mutating routes. Applied per-route
			// via .With so it never wraps GETs. A nil DB (unit tests for
			// /healthz) leaves idem nil → .With(nil-safe passthrough).
			idem := newIdempotency(d)

			r.Get("/library/songs", d.listSongs)
			r.Get("/library/albums", d.listAlbums)
			r.Get("/library/artists", d.listArtists)
			r.Get("/library/songs/{media_id}/stream", d.streamSong)
			r.Get("/library/albums/{album_media_id}/art", d.serveAlbumArt)
			r.Get("/jobs/{id}", d.getJob)
			r.Get("/cookies/youtube/status", d.ytCookieStatus)
			r.With(idem).Post("/library/scan", d.startScan)
			r.With(idem).Post("/cookies/youtube", d.uploadYTCookies)

			// M4 queue + streams (require device auth).
			r.With(idem).Post("/queue/start", d.startQueue)
			r.Get("/queue/{id}", d.getQueue)
			r.Get("/next", d.getNext)
			r.With(idem).Post("/streams/resolve", d.resolveStream)

			// M5 recommendations, likes, playlists, impressions.
			r.Get("/home", d.getHome)
			r.Get("/search", d.search)
			r.With(idem).Post("/likes", d.postLike)
			r.With(idem).Post("/impressions", d.postImpressions)
			r.Get("/playlists", d.listPlaylists)
			r.With(idem).Post("/playlists", d.createPlaylist)
			r.Get("/playlists/{id}", d.getPlaylist)
			r.With(idem).Patch("/playlists/{id}", d.updatePlaylist)
			r.With(idem).Delete("/playlists/{id}", d.deletePlaylist)
			r.With(idem).Post("/playlists/{id}/items", d.addPlaylistItem)
			r.With(idem).Delete("/playlists/{id}/items/{media_id}", d.removePlaylistItem)

			// M6 offline downloads (per-device registry + local-song hash).
			r.Get("/devices/{id}/downloads", d.listDownloads)
			r.With(idem).Post("/devices/{id}/downloads", d.registerDownload)
			r.With(idem).Delete("/devices/{id}/downloads/{media_id}", d.deleteDownload)
			r.Get("/library/songs/{media_id}/hash", d.songHash)

			// M7 batched play events (idempotent).
			r.With(idem).Post("/events", d.postEvents)

			// M8 now-playing WebSocket + admin dashboard.
			r.Get("/ws/now-playing", d.wsNowPlaying)
			r.Post("/ws/command", d.wsCommand)
			r.Get("/admin", d.adminDashboard)
		})

		// Stream proxy is authorized by its short-lived HMAC token, not the
		// device auth middleware: the OS media player / lock-screen loader may
		// not attach Authorization headers to range sub-requests.
		if d.Proxy != nil {
			r.Get("/streams/proxy", d.Proxy.ServeHTTP)
		}
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
