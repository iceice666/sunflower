{
  description = "Sunflower — self-hosted music system (Go server + Flutter client)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        defaultDbUrl = "postgres://postgres@localhost:5432/sunflower?sslmode=disable";
        pgData      = "$PWD/server/.pgdata";
        pgSock      = "$PWD/server/.pgsock";

        # Idempotent: init, start, wait, createdb
        pgUpScript = pkgs.writeShellApplication {
          name = "pg-up";
          runtimeInputs = [ pkgs.postgresql_16 ];
          text = ''
            PGDATA="${pgData}"
            PGSOCK="${pgSock}"
            mkdir -p "$PGSOCK"

            if [ ! -s "$PGDATA/PG_VERSION" ]; then
              initdb -U postgres --auth=trust --no-locale --encoding=UTF8 -D "$PGDATA"
            fi

            if ! pg_ctl -D "$PGDATA" status -q 2>/dev/null; then
              pg_ctl -D "$PGDATA" -l "$PGDATA/server.log" \
                -o "-p 5432 -k $PGSOCK -c listen_addresses=127.0.0.1" start
            fi

            until pg_isready -h 127.0.0.1 -p 5432 -U postgres -q; do sleep 0.3; done

            if ! psql -h "$PGSOCK" -U postgres -tc \
                "SELECT 1 FROM pg_database WHERE datname='sunflower'" | grep -q 1; then
              createdb -h "$PGSOCK" -U postgres sunflower
            fi

            echo "Postgres is up: ${defaultDbUrl}"
          '';
        };

        pgDownScript = pkgs.writeShellApplication {
          name = "pg-down";
          runtimeInputs = [ pkgs.postgresql_16 ];
          text = ''
            PGDATA="${pgData}"
            pg_ctl -D "$PGDATA" -m fast stop
          '';
        };

        migrateScript = pkgs.writeShellApplication {
          name = "migrate";
          runtimeInputs = [ pkgs.goose ];
          text = ''
            DB_URL="''${DATABASE_URL:-${defaultDbUrl}}"
            goose -dir server/db/migrations postgres "$DB_URL" up
          '';
        };

        sunflowerdScript = pkgs.writeShellApplication {
          name = "sunflowerd";
          runtimeInputs = [ pkgs.go ];
          text = ''
            export GOTOOLCHAIN=local
            cd server
            go run ./cmd/sunflowerd "$@"
          '';
        };

        sqlcScript = pkgs.writeShellApplication {
          name = "sqlc-gen";
          runtimeInputs = [ pkgs.sqlc ];
          text = ''
            cd server
            sqlc generate
          '';
        };

        # macOS native-asset hooks (e.g. objective_c, pulled transitively) shell
        # out to clang + xcrun. nixpkgs' darwin stdenv points DEVELOPER_DIR at a
        # bare apple-sdk and puts the xcbuild `xcrun` shim first on PATH; that
        # shim returns "error: unable to find sdk: 'macosx'" inside the hook
        # subprocess, which is then passed verbatim as -isysroot and the build
        # fails. Wrap `flutter` so its child hooks use the host Xcode toolchain
        # (Apple xcrun + clang + real macOS SDK). Scoped to flutter only — the
        # Go/cgo half keeps the nix toolchain untouched.
        flutterWrapped =
          if pkgs.stdenv.isDarwin then
            pkgs.writeShellScriptBin "flutter" ''
              unset DEVELOPER_DIR SDKROOT
              export PATH="/usr/bin:$PATH"
              sdk="$(/usr/bin/xcrun --sdk macosx --show-sdk-path 2>/dev/null || true)"
              [ -n "$sdk" ] && export SDKROOT="$sdk"
              exec ${pkgs.flutter}/bin/flutter "$@"
            ''
          else pkgs.flutter;

      in
      {
        # Development shell — `nix develop`
        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.go
            pkgs.goose
            pkgs.sqlc
            pkgs.postgresql_16
            pkgs.gnumake
            pkgs.gopls
            pkgs.delve
            flutterWrapped # `flutter` wrapped for host Xcode toolchain (see above)
            pkgs.flutter   # bundles Dart 3.11.5 (satisfies sdk '>=3.5.0 <4.0.0')
            pkgs.jdk17     # Android Gradle builds + emulator launch
          ];

          env = {
            GOTOOLCHAIN = "local";
            JAVA_HOME   = "${pkgs.jdk17.home}";
          };

          shellHook = ''
            export PGDATA="$PWD/server/.pgdata"
            export PGHOST="$PWD/server/.pgsock"
            export DATABASE_URL="${defaultDbUrl}"
            echo "Sunflower dev shell ready."
            echo "  make dev-up   — start local Postgres"
            echo "  make run      — run sunflowerd"
            echo "  make migrate  — apply migrations"
            echo "  make test     — run tests"

            # Flutter / Android: use the system Android SDK (not nix-managed).
            export ANDROID_SDK_ROOT="$HOME/Library/Android/sdk"
            export ANDROID_HOME="$ANDROID_SDK_ROOT"
            export PATH="$ANDROID_SDK_ROOT/platform-tools:$ANDROID_SDK_ROOT/emulator:$PATH"
            echo "  flutter pub get        — fetch client deps"
            echo "  flutter analyze        — static check client"
            echo "  emulator -avd Pixel_10 — boot Android emulator"
            if [ ! -d "$ANDROID_SDK_ROOT" ]; then
              echo "  ! ANDROID_SDK_ROOT not found at $ANDROID_SDK_ROOT"
            fi
          '';
        };

        # Runnable apps — `nix run .#<name>`
        apps = {
          pg-up      = { type = "app"; program = "${pgUpScript}/bin/pg-up"; };
          pg-down    = { type = "app"; program = "${pgDownScript}/bin/pg-down"; };
          migrate    = { type = "app"; program = "${migrateScript}/bin/migrate"; };
          sunflowerd = { type = "app"; program = "${sunflowerdScript}/bin/sunflowerd"; };
          sqlc       = { type = "app"; program = "${sqlcScript}/bin/sqlc-gen"; };

          # Default app
          default = { type = "app"; program = "${sunflowerdScript}/bin/sunflowerd"; };
        };
      }
    );
}
