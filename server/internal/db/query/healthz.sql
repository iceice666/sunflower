-- M0 placeholder — gives sqlc something to parse so `sqlc generate` exits 0.
-- Real queries land in M1+.

-- name: Ping :one
SELECT 1::int AS ok;
