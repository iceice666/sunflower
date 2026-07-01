use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use sunflower_core::{
    LocalStatsSnapshot, MediaId, MediaRepository, RecommendationEvent, RecommendationEventKind,
    RecommendationEventRepository, RecommendationSnapshot, RecommendationSnapshotRepository, Song,
    StorageError, StorageResult, TrackStats,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<std::path::Path>) -> StorageResult<Self> {
        let conn = Connection::open(path).map_err(map_backend)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory().map_err(map_backend)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn migrate(&self) -> StorageResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS songs (
                media_id TEXT PRIMARY KEY,
                source_type TEXT NOT NULL,
                title TEXT NOT NULL,
                available INTEGER NOT NULL,
                payload_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS recommendation_events (
                event_id TEXT PRIMARY KEY,
                media_id TEXT NOT NULL,
                client_clock INTEGER NOT NULL,
                occurred_at TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                synced_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_recommendation_events_unsynced
                ON recommendation_events (synced_at, client_clock);

            CREATE TABLE IF NOT EXISTS recommendation_snapshots (
                snapshot_id TEXT PRIMARY KEY,
                model_version TEXT NOT NULL,
                generated_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            "#,
        )
        .map_err(map_backend)
    }

    pub fn upsert_song_sync(&self, song: &Song) -> StorageResult<()> {
        let payload = serde_json::to_string(song).map_err(map_backend)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        conn.execute(
            r#"
            INSERT INTO songs (media_id, source_type, title, available, payload_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
            ON CONFLICT(media_id) DO UPDATE SET
                source_type = excluded.source_type,
                title = excluded.title,
                available = excluded.available,
                payload_json = excluded.payload_json,
                updated_at = CURRENT_TIMESTAMP
            "#,
            params![
                song.media_id.0,
                song.source_type,
                song.title,
                song.available as i32,
                payload
            ],
        )
        .map_err(map_backend)?;
        Ok(())
    }

    pub fn list_songs_sync(&self, limit: i64, offset: i64) -> StorageResult<Vec<Song>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT payload_json
                FROM songs
                ORDER BY title, media_id
                LIMIT ?1 OFFSET ?2
                "#,
            )
            .map_err(map_backend)?;
        let rows = stmt
            .query_map(params![limit, offset], |row| row.get::<_, String>(0))
            .map_err(map_backend)?;

        let mut songs = Vec::new();
        for row in rows {
            let payload = row.map_err(map_backend)?;
            songs.push(serde_json::from_str(&payload).map_err(map_backend)?);
        }
        Ok(songs)
    }

    pub fn append_event_sync(&self, event: &RecommendationEvent) -> StorageResult<()> {
        let payload = serde_json::to_string(event).map_err(map_backend)?;
        let kind = serde_json::to_string(&event.kind).map_err(map_backend)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        conn.execute(
            r#"
            INSERT OR IGNORE INTO recommendation_events
                (event_id, media_id, client_clock, occurred_at, kind, payload_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                event.event_id.to_string(),
                event.media_id.0,
                event.client_clock,
                event.occurred_at.to_rfc3339(),
                kind.trim_matches('"'),
                payload
            ],
        )
        .map_err(map_backend)?;
        Ok(())
    }

    pub fn unsynced_events_sync(&self, limit: i64) -> StorageResult<Vec<RecommendationEvent>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT payload_json
                FROM recommendation_events
                WHERE synced_at IS NULL
                ORDER BY client_clock, event_id
                LIMIT ?1
                "#,
            )
            .map_err(map_backend)?;
        let rows = stmt
            .query_map(params![limit], |row| row.get::<_, String>(0))
            .map_err(map_backend)?;

        let mut events = Vec::new();
        for row in rows {
            let payload = row.map_err(map_backend)?;
            events.push(serde_json::from_str(&payload).map_err(map_backend)?);
        }
        Ok(events)
    }

    pub fn mark_events_synced_sync(&self, event_ids: &[Uuid]) -> StorageResult<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let tx = conn.transaction().map_err(map_backend)?;
        for id in event_ids {
            tx.execute(
                "UPDATE recommendation_events SET synced_at = CURRENT_TIMESTAMP WHERE event_id = ?1",
                params![id.to_string()],
            )
            .map_err(map_backend)?;
        }
        tx.commit().map_err(map_backend)?;
        Ok(())
    }

    pub fn put_snapshot_sync(&self, snapshot: &RecommendationSnapshot) -> StorageResult<()> {
        let payload = serde_json::to_string(snapshot).map_err(map_backend)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        conn.execute(
            r#"
            INSERT INTO recommendation_snapshots
                (snapshot_id, model_version, generated_at, expires_at, payload_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(snapshot_id) DO UPDATE SET
                model_version = excluded.model_version,
                generated_at = excluded.generated_at,
                expires_at = excluded.expires_at,
                payload_json = excluded.payload_json
            "#,
            params![
                snapshot.snapshot_id.to_string(),
                snapshot.model_version,
                snapshot.generated_at.to_rfc3339(),
                snapshot.expires_at.to_rfc3339(),
                payload
            ],
        )
        .map_err(map_backend)?;
        Ok(())
    }

    pub fn latest_snapshot_sync(&self) -> StorageResult<Option<RecommendationSnapshot>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let payload: Option<String> = conn
            .query_row(
                r#"
                SELECT payload_json
                FROM recommendation_snapshots
                ORDER BY generated_at DESC, snapshot_id DESC
                LIMIT 1
                "#,
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_backend)?;
        payload
            .map(|json| serde_json::from_str(&json).map_err(map_backend))
            .transpose()
    }

    pub fn local_stats_snapshot_sync(
        &self,
        recent_limit: i64,
    ) -> StorageResult<LocalStatsSnapshot> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let song_payloads = {
            let mut stmt = conn
                .prepare(
                    r#"
                    SELECT payload_json
                    FROM songs
                    ORDER BY title, media_id
                    "#,
                )
                .map_err(map_backend)?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(map_backend)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(map_backend)?
        };
        let event_payloads = {
            let mut stmt = conn
                .prepare(
                    r#"
                    SELECT payload_json
                    FROM recommendation_events
                    ORDER BY client_clock, event_id
                    "#,
                )
                .map_err(map_backend)?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(map_backend)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(map_backend)?
        };
        drop(conn);

        let mut stats_by_media: HashMap<String, TrackStats> = HashMap::new();
        let mut artists_by_media: HashMap<String, Vec<String>> = HashMap::new();
        for payload in song_payloads {
            let song: Song = serde_json::from_str(&payload).map_err(map_backend)?;
            let media_id = song.media_id.0.clone();
            let has_local_path = song
                .local_path
                .as_deref()
                .is_some_and(|path| !path.trim().is_empty());
            artists_by_media.insert(media_id.clone(), song.artists);
            let local_available = song.available && (song.source_type == "local" || has_local_path);
            stats_by_media.insert(
                media_id.clone(),
                TrackStats {
                    media_id: MediaId::new(media_id),
                    downloaded: song.source_type != "local" && has_local_path,
                    local_available,
                    ..TrackStats::default()
                },
            );
        }

        let mut events = event_payloads
            .into_iter()
            .map(|payload| {
                serde_json::from_str::<RecommendationEvent>(&payload).map_err(map_backend)
            })
            .collect::<StorageResult<Vec<_>>>()?;
        events.sort_by(|a, b| {
            a.occurred_at
                .cmp(&b.occurred_at)
                .then_with(|| a.client_clock.cmp(&b.client_clock))
                .then_with(|| a.event_id.cmp(&b.event_id))
        });

        let mut recent_events = Vec::new();
        for event in events {
            let media_id = event.media_id.0.clone();
            let stats = stats_by_media
                .entry(media_id.clone())
                .or_insert_with(|| TrackStats {
                    media_id: MediaId::new(media_id.clone()),
                    ..TrackStats::default()
                });
            match event.kind {
                RecommendationEventKind::Impression => {
                    stats.impression_count += 1;
                }
                RecommendationEventKind::PlayStarted => {
                    stats.play_count += 1;
                    stats.last_played_at = Some(max_time(stats.last_played_at, event.occurred_at));
                    recent_events.push((event.occurred_at, event.client_clock, media_id));
                }
                RecommendationEventKind::PlayCompleted => {
                    stats.completion_count += 1;
                    stats.last_played_at = Some(max_time(stats.last_played_at, event.occurred_at));
                    recent_events.push((event.occurred_at, event.client_clock, media_id));
                }
                RecommendationEventKind::Skipped => {
                    stats.skip_count += 1;
                    stats.last_played_at = Some(max_time(stats.last_played_at, event.occurred_at));
                    recent_events.push((event.occurred_at, event.client_clock, media_id));
                }
                RecommendationEventKind::Liked => {
                    stats.liked = true;
                }
                RecommendationEventKind::Disliked => {
                    stats.liked = false;
                }
            }
        }

        recent_events.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        let mut seen_recent = HashSet::new();
        let recent_limit = usize::try_from(recent_limit.max(0)).unwrap_or_default();
        let mut recent_media_ids = Vec::new();
        for (_, _, media_id) in recent_events {
            if seen_recent.insert(media_id.clone()) {
                recent_media_ids.push(MediaId::new(media_id));
            }
            if recent_media_ids.len() >= recent_limit {
                break;
            }
        }

        let mut seen_artists = HashSet::new();
        let mut recent_artist_names = Vec::new();
        for media_id in &recent_media_ids {
            if let Some(artists) = artists_by_media.get(&media_id.0) {
                for artist in artists {
                    if !artist.is_empty() && seen_artists.insert(artist.clone()) {
                        recent_artist_names.push(artist.clone());
                    }
                }
            }
        }

        let mut tracks: Vec<_> = stats_by_media
            .into_values()
            .filter(|stats| {
                stats.local_available
                    || stats.downloaded
                    || stats.liked
                    || stats.play_count > 0
                    || stats.skip_count > 0
                    || stats.completion_count > 0
                    || stats.impression_count > 0
            })
            .collect();
        tracks.sort_by(|a, b| {
            b.liked
                .cmp(&a.liked)
                .then_with(|| b.play_count.cmp(&a.play_count))
                .then_with(|| b.last_played_at.cmp(&a.last_played_at))
                .then_with(|| a.media_id.0.cmp(&b.media_id.0))
        });

        Ok(LocalStatsSnapshot {
            generated_at: Utc::now(),
            tracks,
            recent_media_ids,
            recent_artist_names,
        })
    }
}

#[async_trait]
impl MediaRepository for SqliteStore {
    async fn upsert_song(&self, song: &Song) -> StorageResult<()> {
        self.upsert_song_sync(song)
    }

    async fn list_songs(&self, limit: i64, offset: i64) -> StorageResult<Vec<Song>> {
        self.list_songs_sync(limit, offset)
    }
}

#[async_trait]
impl RecommendationEventRepository for SqliteStore {
    async fn append_event(&self, event: &RecommendationEvent) -> StorageResult<()> {
        self.append_event_sync(event)
    }

    async fn unsynced_events(&self, limit: i64) -> StorageResult<Vec<RecommendationEvent>> {
        self.unsynced_events_sync(limit)
    }

    async fn mark_events_synced(&self, event_ids: &[Uuid]) -> StorageResult<()> {
        self.mark_events_synced_sync(event_ids)
    }
}

#[async_trait]
impl RecommendationSnapshotRepository for SqliteStore {
    async fn put_snapshot(&self, snapshot: &RecommendationSnapshot) -> StorageResult<()> {
        self.put_snapshot_sync(snapshot)
    }

    async fn latest_snapshot(&self) -> StorageResult<Option<RecommendationSnapshot>> {
        self.latest_snapshot_sync()
    }
}

fn map_backend<E: std::error::Error>(err: E) -> StorageError {
    StorageError::Backend(err.to_string())
}

fn max_time(
    existing: Option<chrono::DateTime<Utc>>,
    incoming: chrono::DateTime<Utc>,
) -> chrono::DateTime<Utc> {
    existing.map_or(incoming, |existing| existing.max(incoming))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use sunflower_core::{MediaId, RecommendationEventKind, RecommendationSource};

    #[tokio::test]
    async fn sqlite_store_round_trips_songs_and_events() {
        let store = SqliteStore::in_memory().unwrap();
        let song = Song {
            media_id: MediaId::new("local:one"),
            source_type: "local".into(),
            title: "One".into(),
            artists: vec!["A".into()],
            album_id: None,
            duration_ms: Some(123),
            explicit: false,
            video_only: false,
            available: true,
            local_path: Some("/music/one.flac".into()),
            raw_metadata: json!({}),
        };
        store.upsert_song(&song).await.unwrap();

        let songs = store.list_songs(10, 0).await.unwrap();
        assert_eq!(songs, vec![song.clone()]);

        let event = RecommendationEvent {
            event_id: Uuid::now_v7(),
            device_id: None,
            client_clock: 1,
            occurred_at: Utc::now(),
            kind: RecommendationEventKind::PlayStarted,
            media_id: song.media_id,
            queue_id: None,
            recommender_source: RecommendationSource::Local,
            context: json!({}),
            payload: json!({}),
        };
        store.append_event(&event).await.unwrap();
        assert_eq!(
            store.unsynced_events(10).await.unwrap(),
            vec![event.clone()]
        );
        store.mark_events_synced(&[event.event_id]).await.unwrap();
        assert!(store.unsynced_events(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn sqlite_marks_synced_events_as_a_batch() {
        let store = SqliteStore::in_memory().unwrap();
        let base = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
        let first = event(1, base, RecommendationEventKind::PlayStarted, "local:one");
        let second = event(
            2,
            base + chrono::Duration::seconds(1),
            RecommendationEventKind::PlayCompleted,
            "local:two",
        );
        let third = event(
            3,
            base + chrono::Duration::seconds(2),
            RecommendationEventKind::Skipped,
            "local:three",
        );
        for event in [&first, &second, &third] {
            store.append_event(event).await.unwrap();
        }

        store
            .mark_events_synced(&[first.event_id, second.event_id, Uuid::new_v4()])
            .await
            .unwrap();

        assert_eq!(store.unsynced_events(10).await.unwrap(), vec![third]);
    }

    #[tokio::test]
    async fn sqlite_local_stats_snapshot_aggregates_events_after_sync() {
        let store = SqliteStore::in_memory().unwrap();
        let local_song = Song {
            media_id: MediaId::new("local:one"),
            source_type: "local".into(),
            title: "One".into(),
            artists: vec!["Artist One".into()],
            album_id: None,
            duration_ms: Some(123),
            explicit: false,
            video_only: false,
            available: true,
            local_path: Some("/music/one.flac".into()),
            raw_metadata: json!({}),
        };
        let downloaded_song = Song {
            media_id: MediaId::new("yt:two"),
            source_type: "yt".into(),
            title: "Two".into(),
            artists: vec!["Artist Two".into()],
            album_id: None,
            duration_ms: Some(456),
            explicit: false,
            video_only: false,
            available: true,
            local_path: Some("/downloads/two.m4a".into()),
            raw_metadata: json!({}),
        };
        let pathless_local_song = Song {
            media_id: MediaId::new("local:pathless"),
            source_type: "local".into(),
            title: "Pathless".into(),
            artists: vec!["Artist One".into()],
            album_id: None,
            duration_ms: Some(789),
            explicit: false,
            video_only: false,
            available: true,
            local_path: None,
            raw_metadata: json!({}),
        };
        store.upsert_song(&local_song).await.unwrap();
        store.upsert_song(&downloaded_song).await.unwrap();
        store.upsert_song(&pathless_local_song).await.unwrap();

        let base = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
        let events = [
            event(1, base, RecommendationEventKind::PlayStarted, "local:one"),
            event(
                2,
                base + chrono::Duration::minutes(1),
                RecommendationEventKind::PlayCompleted,
                "local:one",
            ),
            event(
                3,
                base + chrono::Duration::minutes(2),
                RecommendationEventKind::Impression,
                "local:one",
            ),
            event(
                4,
                base + chrono::Duration::minutes(3),
                RecommendationEventKind::Liked,
                "local:one",
            ),
            event(
                5,
                base + chrono::Duration::minutes(4),
                RecommendationEventKind::Skipped,
                "yt:two",
            ),
        ];
        for event in &events {
            store.append_event(event).await.unwrap();
        }
        store
            .mark_events_synced(&[events[0].event_id, events[1].event_id])
            .await
            .unwrap();

        let snapshot = store.local_stats_snapshot_sync(10).unwrap();
        assert_eq!(
            snapshot.recent_media_ids,
            vec![MediaId::new("yt:two"), MediaId::new("local:one")]
        );
        assert_eq!(
            snapshot.recent_artist_names,
            vec!["Artist Two".to_string(), "Artist One".to_string()]
        );

        let local_stats = snapshot
            .tracks
            .iter()
            .find(|track| track.media_id == MediaId::new("local:one"))
            .unwrap();
        assert_eq!(local_stats.play_count, 1);
        assert_eq!(local_stats.completion_count, 1);
        assert_eq!(local_stats.skip_count, 0);
        assert_eq!(local_stats.impression_count, 1);
        assert!(local_stats.liked);
        assert!(!local_stats.downloaded);
        assert!(local_stats.local_available);
        assert_eq!(
            local_stats.last_played_at,
            Some(base + chrono::Duration::minutes(1))
        );

        let downloaded_stats = snapshot
            .tracks
            .iter()
            .find(|track| track.media_id == MediaId::new("yt:two"))
            .unwrap();
        assert_eq!(downloaded_stats.play_count, 0);
        assert_eq!(downloaded_stats.skip_count, 1);
        assert!(downloaded_stats.downloaded);
        assert!(downloaded_stats.local_available);

        let pathless_local_stats = snapshot
            .tracks
            .iter()
            .find(|track| track.media_id == MediaId::new("local:pathless"))
            .unwrap();
        assert!(!pathless_local_stats.downloaded);
        assert!(pathless_local_stats.local_available);
    }

    fn event(
        client_clock: i64,
        occurred_at: chrono::DateTime<Utc>,
        kind: RecommendationEventKind,
        media_id: &str,
    ) -> RecommendationEvent {
        RecommendationEvent {
            event_id: Uuid::now_v7(),
            device_id: None,
            client_clock,
            occurred_at,
            kind,
            media_id: MediaId::new(media_id),
            queue_id: None,
            recommender_source: RecommendationSource::Local,
            context: json!({}),
            payload: json!({}),
        }
    }
}
