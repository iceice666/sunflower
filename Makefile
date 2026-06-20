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
        seed-demo smoke-android \
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
	cd server && SUNFLOWER_YT_COOKIE_FILE="$${SUNFLOWER_YT_COOKIE_FILE:-$(CURDIR)/.env.innertube_cookie}" go run ./cmd/sunflowerd

# ── Code generation ──────────────────────────────────────────────────────────

sqlc:
	cd server && sqlc generate

# ── Tests ─────────────────────────────────────────────────────────────────────

test:
	cd server && go test ./...

test-verbose:
	cd server && go test -v ./...

# ── Demo seed & Android smoke ────────────────────────────────────────────────

# Seed Postgres with demo media and mint a device token.
# Requires sunflowerd to be running (make run) and Postgres to be up (make dev-up).
# Writes .seed-env at the repo root on success.
seed-demo:
	bash scripts/seed-demo.sh

# Run the Flutter integration smoke test against the Pixel_10 emulator.
# Requires: AVD Pixel_10 booted, sunflowerd running, .seed-env present.
#
# Uses `flutter drive` so screenshots and the /admin snapshot are streamed
# host-side by test_driver/integration_test.dart (no device storage, no adb
# pull) into client/build/smoke-artifacts/.
#
# Steps:
#   1. Load .seed-env (written by make seed-demo)
#   2. Translate localhost → 10.0.2.2 so the emulator reaches the host
#   3. flutter drive on the connected device with --dart-define values
smoke-android: .seed-env
	@set -a; . ./.seed-env; set +a; \
	EMULATOR_URL=$$(echo "$$SUNFLOWER_DEMO_URL" | sed 's|//localhost|//10.0.2.2|g; s|//127\.0\.0\.1|//10.0.2.2|g'); \
	EMULATOR_SERIAL=$$(adb devices | awk '/emulator-/{print $$1; exit}'); \
	echo "==> [smoke] EMULATOR_URL    = $$EMULATOR_URL"; \
	echo "==> [smoke] EMULATOR_SERIAL = $${EMULATOR_SERIAL:-auto}"; \
	echo "==> [smoke] Running flutter drive …"; \
	DEVICE_ARG="$${EMULATOR_SERIAL:+-d $$EMULATOR_SERIAL}"; \
	(cd client && flutter drive \
	    --driver=test_driver/integration_test.dart \
	    --target=integration_test/visual_smoke_test.dart \
	    $$DEVICE_ARG \
	    --dart-define=SUNFLOWER_DEMO_URL=$$EMULATOR_URL \
	    --dart-define=SUNFLOWER_DEMO_TOKEN=$$SUNFLOWER_DEMO_TOKEN); \
	echo "==> [smoke] Artifacts in client/build/smoke-artifacts/:"; \
	ls -1 client/build/smoke-artifacts/ 2>/dev/null || echo "  (none written)"
