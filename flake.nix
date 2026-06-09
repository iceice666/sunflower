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
          ];

          env = {
            GOTOOLCHAIN = "local";
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
