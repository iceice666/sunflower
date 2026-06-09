// Package config loads server configuration from environment variables.
package config

import "os"

// Config holds all runtime configuration for sunflowerd.
// Values are read from environment variables with sensible local-dev defaults.
type Config struct {
	// ListenAddr is the TCP address the HTTP server binds to (e.g. ":8080").
	ListenAddr string

	// DatabaseURL is the Postgres connection string.
	DatabaseURL string

	// CookieKey is the 32-byte hex key for libsodium cookie encryption
	// (SUNFLOWER_COOKIE_KEY). Not validated in M0 — cookie logic lands later.
	CookieKey string
}

// Load returns a Config populated from the environment.
// Defaults are suitable for local development (Nix-driven or docker-compose Postgres).
func Load() Config {
	return Config{
		ListenAddr:  envOr("LISTEN_ADDR", ":8080"),
		DatabaseURL: envOr("DATABASE_URL", "postgres://postgres@localhost:5432/sunflower?sslmode=disable"),
		CookieKey:   os.Getenv("SUNFLOWER_COOKIE_KEY"),
	}
}

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
