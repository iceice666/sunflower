package auth

import (
	"context"
	"encoding/json"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// AuditEvent is a redacted security/admin event persisted for M9/M10.
type AuditEvent struct {
	UserID     uuid.UUID
	ActorType  string
	ActorID    string
	Event      string
	TargetType string
	TargetID   string
	Metadata   map[string]any
}

// WriteAudit inserts a best-effort audit event. Metadata must already be
// redacted by the caller.
func WriteAudit(ctx context.Context, pool *pgxpool.Pool, ev AuditEvent) error {
	if pool == nil {
		return nil
	}
	meta, err := json.Marshal(ev.Metadata)
	if err != nil {
		return err
	}
	_, err = pool.Exec(ctx, `
		INSERT INTO audit_events
			(user_id, actor_type, actor_id, event, target_type, target_id, metadata)
		VALUES
			(nullif($1::uuid, '00000000-0000-0000-0000-000000000000'::uuid),
			 $2, nullif($3,''), $4, nullif($5,''), nullif($6,''), $7)
	`, ev.UserID, ev.ActorType, ev.ActorID, ev.Event, ev.TargetType, ev.TargetID, meta)
	return err
}

func writeAuditTx(ctx context.Context, tx pgx.Tx, ev AuditEvent) error {
	meta, err := json.Marshal(ev.Metadata)
	if err != nil {
		return err
	}
	_, err = tx.Exec(ctx, `
		INSERT INTO audit_events
			(user_id, actor_type, actor_id, event, target_type, target_id, metadata)
		VALUES
			(nullif($1::uuid, '00000000-0000-0000-0000-000000000000'::uuid),
			 $2, nullif($3,''), $4, nullif($5,''), nullif($6,''), $7)
	`, ev.UserID, ev.ActorType, ev.ActorID, ev.Event, ev.TargetType, ev.TargetID, meta)
	return err
}
