// Package main is the entrypoint for sunflowerd, the Sunflower music server.
package main

import (
	"context"
	"errors"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/iceice666/sunflower/server/internal/api"
	"github.com/iceice666/sunflower/server/internal/config"
	"github.com/iceice666/sunflower/server/internal/db"
	"github.com/rs/zerolog"
)

func main() {
	// Structured logger — human-readable in development via ConsoleWriter.
	log := zerolog.New(zerolog.ConsoleWriter{Out: os.Stderr, TimeFormat: time.RFC3339}).
		With().Timestamp().Logger()

	cfg := config.Load()
	log.Info().Str("listen", cfg.ListenAddr).Msg("sunflowerd starting")

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Connect to Postgres.
	pool, err := db.New(ctx, cfg.DatabaseURL)
	if err != nil {
		log.Fatal().Err(err).Msg("failed to connect to postgres")
	}
	defer pool.Close()
	log.Info().Str("url", cfg.DatabaseURL).Msg("postgres connected")

	// Apply pending migrations on boot (D1 — also runnable via `make migrate`).
	if err := db.Migrate(ctx, cfg.DatabaseURL); err != nil {
		log.Fatal().Err(err).Msg("migration failed")
	}
	log.Info().Msg("migrations applied")

	// Build the HTTP router.
	handler := api.NewRouter(log)

	// HTTP server with graceful shutdown.
	srv := &http.Server{
		Addr:         cfg.ListenAddr,
		Handler:      handler,
		ReadTimeout:  15 * time.Second,
		WriteTimeout: 30 * time.Second,
		IdleTimeout:  60 * time.Second,
	}

	// Listen for OS signals in a goroutine; shut down gracefully on receipt.
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
