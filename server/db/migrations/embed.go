// Package migrations exposes the embedded SQL migration files for use by
// the goose library at runtime (boot-time self-migration).
package migrations

import "embed"

//go:embed *.sql
var Files embed.FS
