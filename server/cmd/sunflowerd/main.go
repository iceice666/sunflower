// Package main is the entrypoint for sunflowerd, the Sunflower music server.
package main

import (
	"context"
	"encoding/hex"
	"errors"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/api"
	"github.com/iceice666/sunflower/server/internal/config"
	"github.com/iceice666/sunflower/server/internal/cookies"
	"github.com/iceice666/sunflower/server/internal/db"
	"github.com/iceice666/sunflower/server/internal/jobs"
	"github.com/iceice666/sunflower/server/internal/library"
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

	// Start cookie health probe (noop if CookieKey is zero).
	if cookieKey != [32]byte{} {
		// Use a placeholder userID — in a single-user system the first (only) user.
		var adminUserID uuid.UUID
		_ = pool.QueryRow(ctx, `SELECT id FROM users LIMIT 1`).Scan(&adminUserID)
		cookies.StartRefreshJob(ctx, pool, cookieKey, adminUserID, log)
	}

	handler := api.NewRouter(api.Deps{
		Log:       log,
		DB:        pool,
		Jobs:      jobRegistry,
		Scanner:   scanner,
		DataDir:   cfg.DataDir,
		CookieKey: cookieKey,
	})

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
