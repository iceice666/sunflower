set shell := ["bash", "-uc"]

database_url := env_var_or_default("DATABASE_URL", "postgres://postgres@localhost:5432/sunflower?sslmode=disable")

default:
    @just --list

# Start Nix-managed local Postgres.
dev-up:
    nix run .#pg-up

up: dev-up

# Stop Nix-managed local Postgres.
down:
    nix run .#pg-down

# Docker / OrbStack fallback for local Postgres.
dev-up-docker:
    docker compose -f dev/docker-compose.yml up -d

down-docker:
    docker compose -f dev/docker-compose.yml down

# Run the Rust server. It applies embedded Postgres migrations on startup.
run:
    nix develop -c bash -lc 'cd rust && DATABASE_URL="{{database_url}}" cargo run --locked -p sunflower-server --bin sunflowerd-rs'

run-rust: run

# Generate Flutter Rust Bridge bindings.
frb-gen:
    nix develop -c flutter_rust_bridge_codegen generate \
      --rust-root rust/crates/sunflower-bridge \
      --rust-input crate::api \
      --dart-output client/lib/core/bridge

# Tests and checks.
check-rust-version:
    nix develop -c bash -lc 'rustc --version | grep -E "^rustc 1\\.95\\."'

test-rust:
    nix develop -c bash -lc 'cd rust && cargo test --workspace --locked'

# Run Rust Postgres parity tests against DATABASE_URL.
test-rust-pg:
    nix develop -c bash -lc 'cd rust && export DATABASE_URL="{{database_url}}" SUNFLOWER_RUN_PG_TESTS=1 && cargo test --locked -p sunflower-storage-postgres -- --nocapture && cargo test --locked -p sunflower-server -- --nocapture'

# Run Rust Postgres parity tests with flake-managed local Postgres.
test-rust-pg-local:
    nix run .#pg-up
    trap 'nix run .#pg-down' EXIT; DATABASE_URL="postgres://postgres@localhost:5432/sunflower?sslmode=disable" just test-rust-pg

fmt-rust:
    nix develop -c bash -lc 'cd rust && cargo fmt --all'

fmt-rust-check:
    nix develop -c bash -lc 'cd rust && cargo fmt --all -- --check'

test-parity: test-rust

test-client:
    nix develop -c bash -lc 'cd client && flutter test'

analyze-client:
    nix develop -c bash -lc 'cd client && flutter analyze'

golden-update:
    nix develop -c bash -lc 'cd client && flutter test --update-goldens test/goldens/'

check-all: check-rust-version fmt-rust-check test-rust analyze-client

# Seed Postgres with demo media and mint a device token.
seed-demo:
    bash scripts/seed-demo.sh

# Run the Flutter integration smoke test against the Pixel_10 emulator.
smoke-android:
    test -f .seed-env
    set -a; . ./.seed-env; set +a; \
    EMULATOR_URL=$(echo "$SUNFLOWER_DEMO_URL" | sed 's|//localhost|//10.0.2.2|g; s|//127\.0\.0\.1|//10.0.2.2|g'); \
    EMULATOR_SERIAL=$(adb devices | awk '/emulator-/{print $1; exit}'); \
    echo "==> [smoke] EMULATOR_URL    = $EMULATOR_URL"; \
    echo "==> [smoke] EMULATOR_SERIAL = ${EMULATOR_SERIAL:-auto}"; \
    DEVICE_ARG="${EMULATOR_SERIAL:+-d $EMULATOR_SERIAL}"; \
    cd client && flutter drive \
      --driver=test_driver/integration_test.dart \
      --target=integration_test/visual_smoke_test.dart \
      $DEVICE_ARG \
      --dart-define=SUNFLOWER_DEMO_URL=$EMULATOR_URL \
      --dart-define=SUNFLOWER_DEMO_TOKEN=$SUNFLOWER_DEMO_TOKEN \
      --dart-define=SUNFLOWER_DEMO_PAIRING_CODE=$SUNFLOWER_DEMO_PAIRING_CODE \
      --dart-define=SUNFLOWER_DEMO_ADMIN_PASSWORD=$SUNFLOWER_DEMO_ADMIN_PASSWORD
