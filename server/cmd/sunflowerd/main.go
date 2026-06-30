// Package main is the entrypoint for sunflowerd, the Sunflower music server.
package main

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"github.com/iceice666/sunflower/server/internal/api"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/config"
	"github.com/iceice666/sunflower/server/internal/cookies"
	"github.com/iceice666/sunflower/server/internal/db"
	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/innertube/sig"
	"github.com/iceice666/sunflower/server/internal/jobs"
	"github.com/iceice666/sunflower/server/internal/library"
	"github.com/iceice666/sunflower/server/internal/queue"
	"github.com/iceice666/sunflower/server/internal/recs"
	"github.com/iceice666/sunflower/server/internal/streamproxy"
	"github.com/iceice666/sunflower/server/internal/streams"
	syncpkg "github.com/iceice666/sunflower/server/internal/sync"
	"github.com/iceice666/sunflower/server/internal/ws"
	"github.com/rs/zerolog"
)

func main() {
	log := zerolog.New(zerolog.ConsoleWriter{Out: os.Stderr, TimeFormat: time.RFC3339}).
		With().Timestamp().Logger()

	cfg := config.Load()
	log.Info().Str("listen", cfg.ListenAddr).Msg("sunflowerd starting")

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	pool, err := db.New(ctx, cfg.DatabaseURL)
	if err != nil {
		log.Fatal().Err(err).Msg("failed to connect to postgres")
	}
	defer pool.Close()
	log.Info().Str("url", cfg.DatabaseURL).Msg("postgres connected")

	if err := db.Migrate(ctx, cfg.DatabaseURL); err != nil {
		log.Fatal().Err(err).Msg("migration failed")
	}
	log.Info().Msg("migrations applied")

	setupToken := cfg.SetupToken
	if setupToken == "" {
		setupToken = generateSetupToken(log)
	}
	if configured, err := auth.OwnerConfigured(ctx, pool); err == nil && !configured {
		if cfg.SetupToken == "" {
			log.Warn().Str("setup_token", setupToken).Msg("first-run owner setup token generated for this process")
		} else {
			log.Info().Msg("first-run owner setup token loaded from SUNFLOWER_SETUP_TOKEN")
		}
	}
	devOpenRegistration := cfg.Env == "development" && cfg.DevOpenRegistration
	if devOpenRegistration {
		log.Warn().Msg("SUNFLOWER_DEV_OPEN_REGISTRATION=1 enabled; device registration is open in development")
	}

	scanner := library.NewScanner(pool, cfg.DataDir, log)
	jobRegistry := jobs.NewRegistry()

	var cookieKey [32]byte
	if cfg.CookieKey != "" {
		b, err := hex.DecodeString(cfg.CookieKey)
		if err != nil || len(b) != 32 {
			log.Fatal().Msg("SUNFLOWER_COOKIE_KEY must be 64 hex chars (32 bytes)")
		}
		copy(cookieKey[:], b)
	}

	// Cookie provider: encrypted store (API-uploaded) with a cookie-file
	// fallback for self-host bootstrap. Cookies() returns nil = guest mode.
	cookieProvider := cookies.NewProvider(pool, cookieKey, cfg.CookieFile)

	// Start cookie health probe when a cookie source is configured.
	cookiesConfigured := cookieKey != [32]byte{} || cfg.CookieFile != ""
	if cookiesConfigured {
		cookies.StartRefreshJob(ctx, pool, cookieProvider, log)
	}

	// M7: idempotency-log GC (hourly; removes rows older than 24h).
	syncpkg.StartGC(ctx, pool, log)

	// M4: InnerTube client (guest mode) for radio expansion + stream resolution.
	// Sig bootstrap is best-effort: on failure the client is left nil and
	// YouTube seeds report unavailable, but local library + queue still work.
	httpClient := &http.Client{Timeout: 30 * time.Second}
	var yt *innertube.Client
	sigCache := sig.NewCache(httpClient)
	if err := sigCache.Bootstrap(ctx); err != nil {
		log.Warn().Err(err).Msg("innertube sig bootstrap failed; youtube streams disabled")
	} else {
		yt = innertube.NewClient(innertube.ClientOpts{
			HTTPClient: httpClient,
			SigCache:   sigCache,
			Locale:     models.Locale{HL: "en", GL: "US"},
			Cookies:    cookieProvider.Cookies,
		})
	}

	// M4: stream proxy signer. Use the configured key if present, else a random
	// per-process key (single-instance only; tokens won't survive a restart).
	proxyKey := loadStreamProxyKey(cfg.StreamProxyKey, log)
	signer := streamproxy.NewSigner(proxyKey, 15*time.Minute)

	const proxyPath = "/api/v1/streams/proxy"
	resolver := &streams.Resolver{YT: yt, Signer: signer, ProxyPath: proxyPath}
	resolver.ProxyYouTube = shouldProxyYouTube(cfg.StreamProxyMode, cookiesConfigured)
	if resolver.ProxyYouTube {
		log.Info().Msg("serving youtube audio through the server proxy")
	}
	// The proxy uses a dedicated client with no whole-request timeout (long
	// ranged streams) and per-redirect host re-validation (SSRF hardening).
	proxy := &streamproxy.Handler{Signer: signer, Client: streamproxy.NewClient(), Log: log}

	deps := api.Deps{
		Log:                 log,
		DB:                  pool,
		Jobs:                jobRegistry,
		Scanner:             scanner,
		DataDir:             cfg.DataDir,
		CookieKey:           cookieKey,
		Queue:               queue.NewStore(pool),
		Streams:             resolver,
		Proxy:               proxy,
		SetupToken:          setupToken,
		ServerVersion:       "0.3.0",
		PublicBaseURL:       cfg.PublicBaseURL,
		DevOpenRegistration: devOpenRegistration,
		StartedAt:           time.Now(),
	}
	if yt != nil {
		deps.YT = yt
	}

	// M5: recommendation engine. yt may be nil (guest/bootstrap-failed) — remote
	// sections then degrade to empty; local-first Quick Picks still work.
	recsOpts := recs.Options{DB: pool, Log: log}
	if yt != nil {
		recsOpts.YT = yt
	}
	deps.Recs = recs.NewEngine(recsOpts)

	// M8: now-playing WebSocket hub.
	deps.Hub = ws.NewHub(log)
	handler := api.NewRouter(deps)

	srv := &http.Server{
		Addr:         cfg.ListenAddr,
		Handler:      handler,
		ReadTimeout:  15 * time.Second,
		WriteTimeout: 30 * time.Second,
		IdleTimeout:  60 * time.Second,
	}

	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		log.Info().Str("addr", cfg.ListenAddr).Msg("http server listening")
		if err := srv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			log.Fatal().Err(err).Msg("http server error")
		}
	}()

	<-quit
	log.Info().Msg("shutting down...")

	shutCtx, shutCancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer shutCancel()
	if err := srv.Shutdown(shutCtx); err != nil {
		log.Error().Err(err).Msg("graceful shutdown failed")
	}
	log.Info().Msg("stopped")
}

// loadStreamProxyKey returns the HMAC key for stream proxy tokens. A configured
// hex key is preferred; otherwise a random 32-byte key is generated for this
// process (valid only until restart and not shared across instances).
func loadStreamProxyKey(hexKey string, log zerolog.Logger) []byte {
	if hexKey != "" {
		b, err := hex.DecodeString(hexKey)
		if err != nil || len(b) < 32 {
			log.Fatal().Msg("SUNFLOWER_STREAM_PROXY_KEY must be at least 64 hex chars (32 bytes)")
		}
		return b
	}
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		log.Fatal().Err(err).Msg("failed to generate stream proxy key")
	}
	log.Warn().Msg("SUNFLOWER_STREAM_PROXY_KEY unset; using a random per-process key")
	return b
}

func generateSetupToken(log zerolog.Logger) string {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		log.Fatal().Err(err).Msg("failed to generate setup token")
	}
	return hex.EncodeToString(b)
}

// shouldProxyYouTube resolves the SUNFLOWER_STREAM_PROXY policy. "always" and
// "never" are explicit; "auto" (the default) proxies only when cookies are
// configured, because cookie-resolved googlevideo URLs are bound to the
// resolving session/IP and 403 when a client on another network fetches them
// directly.
func shouldProxyYouTube(mode string, cookiesConfigured bool) bool {
	switch strings.ToLower(strings.TrimSpace(mode)) {
	case "always":
		return true
	case "never":
		return false
	default:
		return cookiesConfigured
	}
}
