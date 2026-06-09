package library

import (
	"context"
	"encoding/json"

	"github.com/iceice666/sunflower/server/internal/db/gen"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/jackc/pgx/v5/pgxpool"
)

var emptyMeta = json.RawMessage("{}")

// UpsertTrack saves artist → album → song in a single transaction.
func UpsertTrack(ctx context.Context, pool *pgxpool.Pool, m *TrackMeta) error {
	tx, err := pool.Begin(ctx)
	if err != nil {
		return err
	}
	defer tx.Rollback(ctx)
	q := gen.New(tx)

	var artistRef pgtype.Text
	if m.Artist != "" {
		if _, err = q.UpsertArtist(ctx, gen.UpsertArtistParams{
			MediaID:     m.ArtistMediaID,
			SourceType:  "local",
			Name:        m.Artist,
			RawMetadata: emptyMeta,
		}); err != nil {
			return err
		}
		artistRef = pgtype.Text{String: m.ArtistMediaID, Valid: true}
	}

	var albumRef pgtype.Text
	if m.Album != "" {
		year := pgtype.Int4{}
		if m.Year > 0 {
			year = pgtype.Int4{Int32: int32(m.Year), Valid: true}
		}
		if _, err = q.UpsertAlbum(ctx, gen.UpsertAlbumParams{
			MediaID:         m.AlbumMediaID,
			SourceType:      "local",
			Title:           m.Album,
			PrimaryArtistID: artistRef,
			Year:            year,
			RawMetadata:     emptyMeta,
		}); err != nil {
			return err
		}
		albumRef = pgtype.Text{String: m.AlbumMediaID, Valid: true}
	}

	durationMs := pgtype.Int4{}
	if m.DurationMs > 0 {
		durationMs = pgtype.Int4{Int32: int32(m.DurationMs), Valid: true}
	}
	if _, err = q.UpsertSong(ctx, gen.UpsertSongParams{
		MediaID:         m.MediaID,
		SourceType:      "local",
		Title:           m.Title,
		DurationMs:      durationMs,
		AlbumID:         albumRef,
		PrimaryArtistID: artistRef,
		RawMetadata:     emptyMeta,
	}); err != nil {
		return err
	}

	if m.Artist != "" {
		if err = q.UpsertSongArtist(ctx, gen.UpsertSongArtistParams{
			SongMediaID:   m.MediaID,
			ArtistMediaID: m.ArtistMediaID,
			Position:      0,
		}); err != nil {
			return err
		}
	}

	return tx.Commit(ctx)
}
