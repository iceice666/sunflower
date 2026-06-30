package db_test

import (
	"context"
	"database/sql"
	"sort"
	"testing"

	"github.com/iceice666/sunflower/server/db/migrations"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/pressly/goose/v3"
	"github.com/testcontainers/testcontainers-go"
	tcpostgres "github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

// expectedTables is the complete set of tables that must exist after all M0
// migrations have been applied.
var expectedTables = []string{
	"admin_sessions",
	"albums",
	"artists",
	"audit_events",
	"devices",
	"downloaded_tracks",
	"encrypted_cookies",
	"idempotency_log",
	"likes",
	"pairing_codes",
	"play_events",
	"playlist_items",
	"playlists",
	"play_events",
	"queue_items",
	"queue_sessions",
	"rec_cache",
	"recommendation_impressions",
	"song_artists",
	"songs",
	"users",
}

// TestMigrationRoundTrip spins an ephemeral postgres:16 container via
// testcontainers-go, runs all migrations up, asserts all expected tables are
// present, then runs all migrations down and asserts only goose_db_version
// remains. Requires a running Docker / OrbStack daemon.
func TestMigrationRoundTrip(t *testing.T) {
	ctx := context.Background()

	// Start an ephemeral Postgres 16 container.
	pgc, err := tcpostgres.Run(ctx,
		"postgres:16-alpine",
		tcpostgres.WithDatabase("sunflower"),
		tcpostgres.WithUsername("postgres"),
		tcpostgres.WithPassword("postgres"),
		testcontainers.WithWaitStrategy(
			wait.ForLog("database system is ready to accept connections").
				WithOccurrence(2),
		),
	)
	if err != nil {
		t.Fatalf("start postgres container: %v", err)
	}
	t.Cleanup(func() {
		if err := pgc.Terminate(ctx); err != nil {
			t.Logf("terminate container: %v", err)
		}
	})

	dsn, err := pgc.ConnectionString(ctx, "sslmode=disable")
	if err != nil {
		t.Fatalf("get connection string: %v", err)
	}

	sqlDB := openSQLDB(t, dsn)
	defer sqlDB.Close()

	goose.SetBaseFS(migrations.Files)
	if err := goose.SetDialect("postgres"); err != nil {
		t.Fatalf("goose.SetDialect: %v", err)
	}

	// --- UP ---
	if err := goose.UpContext(ctx, sqlDB, "."); err != nil {
		t.Fatalf("goose.Up: %v", err)
	}

	tables := publicTables(t, sqlDB)
	assertTablesPresent(t, tables, expectedTables)

	// --- DOWN ---
	if err := goose.DownToContext(ctx, sqlDB, ".", 0); err != nil {
		t.Fatalf("goose.DownTo(0): %v", err)
	}

	remaining := publicTables(t, sqlDB)
	for _, tbl := range remaining {
		if tbl != "goose_db_version" {
			t.Errorf("unexpected table after full down: %q", tbl)
		}
	}
}

// openSQLDB opens a *sql.DB from a DSN via the pgx stdlib adapter.
func openSQLDB(t *testing.T, dsn string) *sql.DB {
	t.Helper()
	cfg, err := pgxpool.ParseConfig(dsn)
	if err != nil {
		t.Fatalf("parse dsn: %v", err)
	}
	return stdlib.OpenDB(*cfg.ConnConfig)
}

// publicTables returns the sorted list of table names in the public schema.
func publicTables(t *testing.T, db *sql.DB) []string {
	t.Helper()
	rows, err := db.Query(
		`SELECT tablename FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename`,
	)
	if err != nil {
		t.Fatalf("query pg_tables: %v", err)
	}
	defer rows.Close()

	var tables []string
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			t.Fatalf("scan: %v", err)
		}
		tables = append(tables, name)
	}
	return tables
}

// assertTablesPresent checks that every expected table appears in the actual list.
func assertTablesPresent(t *testing.T, actual, expected []string) {
	t.Helper()
	set := make(map[string]bool, len(actual))
	for _, tbl := range actual {
		set[tbl] = true
	}
	want := dedupe(expected)
	for _, tbl := range want {
		if !set[tbl] {
			t.Errorf("expected table %q missing after migrations up", tbl)
		}
	}
}

func dedupe(ss []string) []string {
	seen := make(map[string]bool)
	var out []string
	for _, s := range ss {
		if !seen[s] {
			seen[s] = true
			out = append(out, s)
		}
	}
	sort.Strings(out)
	return out
}
