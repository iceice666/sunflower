package queue

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"

	"github.com/google/uuid"
	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
)

// ErrNotFound is returned when a queue session does not exist or is not owned
// by the requesting user.
var ErrNotFound = errors.New("queue: session not found")

// Session is a materialized queue with its items, returned to handlers.
type Session struct {
	ID       uuid.UUID
	UserID   uuid.UUID
	SeedKind string
	SeedID   string
	Title    string
	Version  int64
	Items    []Item
}

// Store persists and retrieves queue sessions.
type Store struct {
	pool *pgxpool.Pool
}

// NewStore returns a Store backed by pool.
func NewStore(pool *pgxpool.Pool) *Store { return &Store{pool: pool} }

// CreateParams describes a new queue session.
type CreateParams struct {
	UserID   uuid.UUID
	DeviceID uuid.UUID // zero value = no device association
	SeedKind string
	SeedID   string
	Title    string
	Items    []Item
}

// Create inserts a queue session and its materialized items in one transaction.
func (s *Store) Create(ctx context.Context, p CreateParams) (Session, error) {
	tx, err := s.pool.Begin(ctx)
	if err != nil {
		return Session{}, err
	}
	defer tx.Rollback(ctx)
	q := gen.New(tx)

	itemsJSON, err := json.Marshal(p.Items)
	if err != nil {
		return Session{}, fmt.Errorf("queue: marshal items: %w", err)
	}

	device := pgtype.UUID{}
	if p.DeviceID != uuid.Nil {
		device = pgtype.UUID{Bytes: p.DeviceID, Valid: true}
	}

	row, err := q.InsertQueueSession(ctx, gen.InsertQueueSessionParams{
		UserID:   pgtype.UUID{Bytes: p.UserID, Valid: true},
		DeviceID: device,
		SeedKind: pgtype.Text{String: p.SeedKind, Valid: p.SeedKind != ""},
		SeedID:   pgtype.Text{String: p.SeedID, Valid: p.SeedID != ""},
		Title:    pgtype.Text{String: p.Title, Valid: p.Title != ""},
		Items:    itemsJSON,
	})
	if err != nil {
		return Session{}, fmt.Errorf("queue: insert session: %w", err)
	}

	for i, it := range p.Items {
		srcData, err := json.Marshal(it)
		if err != nil {
			return Session{}, fmt.Errorf("queue: marshal item %d: %w", i, err)
		}
		if err := q.InsertQueueItem(ctx, gen.InsertQueueItemParams{
			QueueID:    row.ID,
			Position:   int32(i),
			MediaID:    it.MediaID,
			SourceData: srcData,
		}); err != nil {
			return Session{}, fmt.Errorf("queue: insert item %d: %w", i, err)
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return Session{}, err
	}

	return sessionFromRow(row, p.Items), nil
}

// Get returns a queue session owned by userID, with its items in order.
func (s *Store) Get(ctx context.Context, id, userID uuid.UUID) (Session, error) {
	q := gen.New(s.pool)
	row, err := q.GetQueueSession(ctx, gen.GetQueueSessionParams{
		ID:     pgtype.UUID{Bytes: id, Valid: true},
		UserID: pgtype.UUID{Bytes: userID, Valid: true},
	})
	if errors.Is(err, pgx.ErrNoRows) {
		return Session{}, ErrNotFound
	}
	if err != nil {
		return Session{}, err
	}

	dbItems, err := q.ListQueueItems(ctx, row.ID)
	if err != nil {
		return Session{}, err
	}
	items := make([]Item, 0, len(dbItems))
	for _, di := range dbItems {
		var it Item
		if err := json.Unmarshal(di.SourceData, &it); err != nil || it.MediaID == "" {
			// source_data is the source of truth; fall back to the column.
			it = Item{MediaID: di.MediaID}
		}
		items = append(items, it)
	}

	return sessionFromRow(row, items), nil
}

func sessionFromRow(row gen.QueueSession, items []Item) Session {
	return Session{
		ID:       uuid.UUID(row.ID.Bytes),
		UserID:   uuid.UUID(row.UserID.Bytes),
		SeedKind: row.SeedKind.String,
		SeedID:   row.SeedID.String,
		Title:    row.Title.String,
		Version:  row.Version,
		Items:    items,
	}
}
