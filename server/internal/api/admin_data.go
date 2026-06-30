package api

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/jobs"
	"github.com/iceice666/sunflower/server/internal/ws"
)

type libraryCounts struct {
	Songs     int `json:"songs"`
	Albums    int `json:"albums"`
	Artists   int `json:"artists"`
	Playlists int `json:"playlists"`
}

type adminDevice struct {
	ID            string     `json:"id"`
	Name          string     `json:"name"`
	Platform      string     `json:"platform"`
	TokenLabel    string     `json:"token_label"`
	CreatedAt     time.Time  `json:"created_at"`
	LastSeenAt    *time.Time `json:"last_seen_at"`
	RevokedAt     *time.Time `json:"revoked_at"`
	RevokedReason string     `json:"revoked_reason"`
}

type adminAuditEvent struct {
	ID         string          `json:"id"`
	ActorType  string          `json:"actor_type"`
	ActorID    string          `json:"actor_id,omitempty"`
	Event      string          `json:"event"`
	TargetType string          `json:"target_type,omitempty"`
	TargetID   string          `json:"target_id,omitempty"`
	Metadata   json.RawMessage `json:"metadata"`
	CreatedAt  time.Time       `json:"created_at"`
}

type adminStatusPayload struct {
	ServerVersion string               `json:"server_version"`
	UptimeSeconds int64                `json:"uptime_seconds"`
	DBStatus      string               `json:"db_status"`
	LibraryCounts libraryCounts        `json:"library_counts"`
	CookieStatus  cookieStatusResponse `json:"cookie_status"`
	Devices       []adminDevice        `json:"devices"`
	NowPlaying    []ws.NowPlaying      `json:"now_playing"`
	Jobs          []*jobs.Job          `json:"jobs"`
	Warnings      []string             `json:"warnings"`
}

func (d *Deps) buildAdminStatus(ctx context.Context) adminStatusPayload {
	payload := adminStatusPayload{
		ServerVersion: d.ServerVersion,
		UptimeSeconds: int64(time.Since(d.StartedAt).Seconds()),
		DBStatus:      "ok",
		CookieStatus:  cookieStatusResponse{Status: "unknown"},
	}
	if err := d.DB.Ping(ctx); err != nil {
		payload.DBStatus = "error"
		payload.Warnings = append(payload.Warnings, "Database check failed")
	}
	payload.LibraryCounts = d.libraryCounts(ctx)
	payload.CookieStatus = d.cookieStatus(ctx)
	payload.Devices = d.listAdminDevices(ctx)
	if d.Hub != nil {
		payload.NowPlaying = d.Hub.Snapshot()
	}
	if d.Jobs != nil {
		payload.Jobs = d.Jobs.ListRecent(10)
	}
	if payload.CookieStatus.Status == "unknown" {
		payload.Warnings = append(payload.Warnings, "YouTube cookie status is unknown")
	}
	if payload.LibraryCounts.Songs == 0 {
		payload.Warnings = append(payload.Warnings, "Library has no songs")
	}
	return payload
}

func (d *Deps) libraryCounts(ctx context.Context) libraryCounts {
	var c libraryCounts
	_ = d.DB.QueryRow(ctx, `SELECT count(*) FROM songs`).Scan(&c.Songs)
	_ = d.DB.QueryRow(ctx, `SELECT count(*) FROM albums`).Scan(&c.Albums)
	_ = d.DB.QueryRow(ctx, `SELECT count(*) FROM artists`).Scan(&c.Artists)
	_ = d.DB.QueryRow(ctx, `SELECT count(*) FROM playlists`).Scan(&c.Playlists)
	return c
}

func (d *Deps) cookieStatus(ctx context.Context) cookieStatusResponse {
	var status string
	var checkedAt *time.Time
	var detail *string
	err := d.DB.QueryRow(ctx,
		`SELECT status, checked_at, detail FROM cookie_health WHERE provider='youtube'`,
	).Scan(&status, &checkedAt, &detail)
	if err != nil {
		return cookieStatusResponse{Status: "unknown"}
	}
	resp := cookieStatusResponse{Status: status}
	if checkedAt != nil {
		s := checkedAt.Format(time.RFC3339)
		resp.CheckedAt = &s
	}
	resp.Detail = detail
	return resp
}

func (d *Deps) listAdminDevices(ctx context.Context) []adminDevice {
	rows, err := d.DB.Query(ctx, `
		SELECT id, coalesce(name,''), coalesce(platform,''), coalesce(token_label,''),
		       created_at, last_seen_at, revoked_at, coalesce(revoked_reason,'')
		FROM devices
		ORDER BY coalesce(last_seen_at, created_at) DESC
		LIMIT 100
	`)
	if err != nil {
		return nil
	}
	defer rows.Close()
	var out []adminDevice
	for rows.Next() {
		var dvc adminDevice
		var id uuid.UUID
		var lastSeen *time.Time
		var revokedAt *time.Time
		if err := rows.Scan(&id, &dvc.Name, &dvc.Platform, &dvc.TokenLabel, &dvc.CreatedAt, &lastSeen, &revokedAt, &dvc.RevokedReason); err != nil {
			continue
		}
		dvc.ID = id.String()
		dvc.LastSeenAt = lastSeen
		dvc.RevokedAt = revokedAt
		out = append(out, dvc)
	}
	return out
}

func (d *Deps) recentAudit(ctx context.Context, limit int) []adminAuditEvent {
	rows, err := d.DB.Query(ctx, `
		SELECT id, actor_type, coalesce(actor_id,''), event, coalesce(target_type,''), coalesce(target_id,''), metadata, created_at
		FROM audit_events
		ORDER BY created_at DESC
		LIMIT $1
	`, limit)
	if err != nil {
		return nil
	}
	defer rows.Close()
	var out []adminAuditEvent
	for rows.Next() {
		var ev adminAuditEvent
		var id uuid.UUID
		if err := rows.Scan(&id, &ev.ActorType, &ev.ActorID, &ev.Event, &ev.TargetType, &ev.TargetID, &ev.Metadata, &ev.CreatedAt); err != nil {
			continue
		}
		ev.ID = id.String()
		ev.Metadata = redactJSON(ev.Metadata)
		out = append(out, ev)
	}
	return out
}

func (d *Deps) revokeDevice(ctx context.Context, sess *auth.AdminSession, deviceID string, reason string) error {
	id, err := uuid.Parse(deviceID)
	if err != nil {
		return fmt.Errorf("invalid_id")
	}
	reason = strings.TrimSpace(reason)
	_, err = d.DB.Exec(ctx, `
		UPDATE devices
		SET revoked_at = coalesce(revoked_at, now()),
		    revoked_reason = nullif($2,'')
		WHERE id = $1
	`, id, reason)
	if err != nil {
		return err
	}
	if d.Hub != nil {
		d.Hub.DisconnectDevice(id.String())
	}
	return auth.WriteAudit(ctx, d.DB, auth.AuditEvent{
		UserID:     sess.UserID,
		ActorType:  "admin_session",
		ActorID:    sess.ID.String(),
		Event:      "device_revoked",
		TargetType: "device",
		TargetID:   id.String(),
		Metadata:   map[string]any{"reason": reason},
	})
}

func redactedMetadataString(raw json.RawMessage) string {
	if len(raw) == 0 {
		return "{}"
	}
	return string(redactJSON(raw))
}

func redactJSON(raw json.RawMessage) json.RawMessage {
	if len(raw) == 0 {
		return json.RawMessage(`{}`)
	}
	var v any
	if err := json.Unmarshal(raw, &v); err != nil {
		return json.RawMessage(`{}`)
	}
	redactValue(v)
	out, err := json.Marshal(v)
	if err != nil {
		return json.RawMessage(`{}`)
	}
	return out
}

func redactValue(v any) {
	switch x := v.(type) {
	case map[string]any:
		for k, val := range x {
			lk := strings.ToLower(k)
			if strings.Contains(lk, "password") || strings.Contains(lk, "token") ||
				strings.Contains(lk, "cookie") || strings.Contains(lk, "code") ||
				strings.Contains(lk, "secret") {
				x[k] = "[redacted]"
				continue
			}
			redactValue(val)
		}
	case []any:
		for _, val := range x {
			redactValue(val)
		}
	}
}
