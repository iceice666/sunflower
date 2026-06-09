// Package db provides the Postgres connection pool and runs schema migrations.
package db

import (
	"context"
	"fmt"

	"github.com/iceice666/sunflower/server/db/migrations"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/pressly/goose/v3"
)

// New creates a pgxpool connection pool, verifies connectivity via Ping,
// and returns the pool. The caller is responsible for calling pool.Close().
func New(ctx context.Context, url string) (*pgxpool.Pool, error) {
	pool, err := pgxpool.New(ctx, url)
	if err != nil {
		return nil, fmt.Errorf("pgxpool.New: %w", err)
	}
	if err := pool.Ping(ctx); err != nil {
		pool.Close()
		return nil, fmt.Errorf("ping postgres: %w", err)
	}
	return pool, nil
}

// Migrate applies all pending goose migrations from the embedded SQL files.
// It is safe to call on every boot — goose tracks applied versions in
// goose_db_version and is idempotent.
func Migrate(ctx context.Context, url string) error {
	// goose needs a *sql.DB; derive a pgx.ConnConfig from the DSN and open
	// a stdlib DB from it (no separate connection pool — migration is one-shot).
	poolCfg, err := pgxpool.ParseConfig(url)
	if err != nil {
		return fmt.Errorf("parse DATABASE_URL: %w", err)
	}
	sqlDB := stdlib.OpenDB(*poolCfg.ConnConfig)
	defer sqlDB.Close()

	goose.SetBaseFS(migrations.Files)
	if err := goose.SetDialect("postgres"); err != nil {
		return fmt.Errorf("goose.SetDialect: %w", err)
	}

	// "." because the embed.FS root is the migrations directory itself.
	if err := goose.UpContext(ctx, sqlDB, "."); err != nil {
		return fmt.Errorf("goose.Up: %w", err)
	}
	return nil
}
