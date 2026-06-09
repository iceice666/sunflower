# Sunflower root Makefile — delegates to Nix flake apps and the Go toolchain.
# Run `nix develop` first to get Go, goose, sqlc, etc. on PATH, or use `nix run .#<app>`.
#
# Primary targets (from acceptance criteria):
#   dev-up        — start Nix-managed local Postgres on localhost:5432
#   down          — stop Nix-managed Postgres
#   migrate       — apply goose migrations
#   run           — start sunflowerd
#   sqlc          — regenerate sqlc query code
#   test          — run all Go tests
#
# Docker alternative:
#   dev-up-docker — start Postgres via docker-compose (prod-parity / CI)
#   down-docker   — stop docker-compose Postgres

DATABASE_URL ?= postgres://postgres@localhost:5432/sunflower?sslmode=disable

.PHONY: dev-up down migrate run sqlc test \
        dev-up-docker down-docker \
        up  # alias

# ── Postgres (Nix-driven, Docker-free) ──────────────────────────────────────

dev-up:
	nix run .#pg-up

up: dev-up  ## alias for dev-up

down:
	nix run .#pg-down

# ── Postgres (Docker / OrbStack fallback) ────────────────────────────────────

dev-up-docker:
	docker compose -f dev/docker-compose.yml up -d

down-docker:
	docker compose -f dev/docker-compose.yml down

# ── Migrations ───────────────────────────────────────────────────────────────

migrate:
	cd server && goose -dir db/migrations postgres "$(DATABASE_URL)" up

migrate-down:
	cd server && goose -dir db/migrations postgres "$(DATABASE_URL)" down

migrate-status:
	cd server && goose -dir db/migrations postgres "$(DATABASE_URL)" status

# ── Server ───────────────────────────────────────────────────────────────────

run:
	cd server && go run ./cmd/sunflowerd

# ── Code generation ──────────────────────────────────────────────────────────

sqlc:
	cd server && sqlc generate

# ── Tests ─────────────────────────────────────────────────────────────────────

test:
	cd server && go test ./...

test-verbose:
	cd server && go test -v ./...
