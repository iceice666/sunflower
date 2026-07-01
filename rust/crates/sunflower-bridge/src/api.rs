use std::path::PathBuf;

use anyhow::{Result, anyhow};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sunflower_core::{
    LocalRecommendationEngine, LocalStatsSnapshot, MediaId, RecommendationCandidate,
    RecommendationEvent, RecommendationEventKind, RecommendationSnapshot, RecommendationSource,
    Song, TrackStats,
};
use sunflower_storage_sqlite::SqliteStore;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoreConfig {
    pub sqlite_path: Option<String>,
    pub recommendation_server_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoreHandle {
    pub sqlite_path: Option<String>,
    pub recommendation_server_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackStatsDto {
    pub media_id: String,
    pub play_count: i64,
    pub skip_count: i64,
    pub completion_count: i64,
    pub impression_count: i64,
    pub liked: bool,
    pub downloaded: bool,
    pub local_available: bool,
    pub last_played_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalStatsSnapshotDto {
    pub generated_at_ms: i64,
    pub tracks: Vec<TrackStatsDto>,
    pub recent_media_ids: Vec<String>,
    pub recent_artist_names: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationSourceDto {
    Local,
    Remote,
    Mixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationEventKindDto {
    Impression,
    PlayStarted,
    PlayCompleted,
    Skipped,
    Liked,
    Disliked,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecommendationCandidateDto {
    pub media_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album_id: Option<String>,
    pub duration_ms: i32,
    pub source: RecommendationSourceDto,
    pub remote_score: f32,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SongDto {
    pub media_id: String,
    pub source_type: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album_id: Option<String>,
    pub duration_ms: Option<i32>,
    pub explicit: bool,
    pub video_only: bool,
    pub available: bool,
    pub local_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecommendationEventDto {
    pub event_id: String,
    pub device_id: Option<String>,
    pub client_clock: i64,
    pub occurred_at_ms: i64,
    pub kind: RecommendationEventKindDto,
    pub media_id: String,
    pub queue_id: Option<String>,
    pub recommender_source: RecommendationSourceDto,
    pub context_json: String,
    pub payload_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecommendationSnapshotDto {
    pub snapshot_id: String,
    pub model_version: String,
    pub generated_at_ms: i64,
    pub expires_at_ms: i64,
    pub candidates: Vec<RecommendationCandidateDto>,
}

pub fn open_core(config: CoreConfig) -> Result<CoreHandle> {
    let sqlite_path = match config.sqlite_path {
        Some(path) => path,
        None => ephemeral_sqlite_path().to_string_lossy().into_owned(),
    };
    let store = SqliteStore::open(PathBuf::from(&sqlite_path))?;
    drop(store);
    Ok(CoreHandle {
        sqlite_path: Some(sqlite_path),
        recommendation_server_url: config.recommendation_server_url,
    })
}

pub fn new_event_id() -> String {
    Uuid::now_v7().to_string()
}

pub fn upsert_local_song(handle: CoreHandle, song: SongDto) -> Result<()> {
    store_for_handle(&handle)?.upsert_song_sync(&song.into())?;
    Ok(())
}

pub fn list_local_songs(handle: CoreHandle, limit: i64, offset: i64) -> Result<Vec<SongDto>> {
    Ok(store_for_handle(&handle)?
        .list_songs_sync(limit.max(0), offset.max(0))?
        .into_iter()
        .map(Into::into)
        .collect())
}

pub fn append_recommendation_event(
    handle: CoreHandle,
    event: RecommendationEventDto,
) -> Result<()> {
    store_for_handle(&handle)?.append_event_sync(&event_from_dto(event)?)?;
    Ok(())
}

pub fn unsynced_recommendation_events(
    handle: CoreHandle,
    limit: i64,
) -> Result<Vec<RecommendationEventDto>> {
    Ok(store_for_handle(&handle)?
        .unsynced_events_sync(limit.max(0))?
        .into_iter()
        .map(event_to_dto)
        .collect())
}

pub fn mark_recommendation_events_synced(handle: CoreHandle, event_ids: Vec<String>) -> Result<()> {
    let ids = event_ids
        .into_iter()
        .map(|id| parse_uuid_v7("event_id", &id))
        .collect::<Result<Vec<_>>>()?;
    store_for_handle(&handle)?.mark_events_synced_sync(&ids)?;
    Ok(())
}

pub fn put_recommendation_snapshot(
    handle: CoreHandle,
    snapshot: RecommendationSnapshotDto,
) -> Result<()> {
    store_for_handle(&handle)?.put_snapshot_sync(&snapshot_from_dto(snapshot)?)?;
    Ok(())
}

pub fn latest_recommendation_snapshot(
    handle: CoreHandle,
) -> Result<Option<RecommendationSnapshotDto>> {
    Ok(store_for_handle(&handle)?
        .latest_snapshot_sync()?
        .map(snapshot_to_dto))
}

pub fn local_stats_snapshot(
    handle: CoreHandle,
    recent_limit: i64,
) -> Result<LocalStatsSnapshotDto> {
    Ok(store_for_handle(&handle)?
        .local_stats_snapshot_sync(recent_limit.max(0))?
        .into())
}

pub fn rank_local_from_snapshot(
    handle: CoreHandle,
    stats: LocalStatsSnapshotDto,
    limit: i32,
) -> Result<Vec<RecommendationCandidateDto>> {
    let store = store_for_handle(&handle)?;
    let candidates = match store.latest_snapshot_sync()? {
        Some(snapshot) => snapshot.candidates,
        None => store
            .list_songs_sync(500, 0)?
            .into_iter()
            .filter(is_local_fallback_song)
            .map(local_candidate_from_song)
            .collect(),
    };
    let limit = usize::try_from(limit.max(0)).unwrap_or_default();
    Ok(LocalRecommendationEngine::default()
        .rank(&candidates, &stats.into(), limit)
        .into_iter()
        .map(Into::into)
        .collect())
}

pub fn rank_local_candidates(
    candidates: Vec<RecommendationCandidateDto>,
    stats: LocalStatsSnapshotDto,
    limit: i32,
) -> Vec<RecommendationCandidateDto> {
    let core_candidates: Vec<_> = candidates.into_iter().map(Into::into).collect();
    let core_stats = stats.into();
    let limit = usize::try_from(limit.max(0)).unwrap_or_default();
    LocalRecommendationEngine::default()
        .rank(&core_candidates, &core_stats, limit)
        .into_iter()
        .map(Into::into)
        .collect()
}

pub fn empty_stats_snapshot() -> LocalStatsSnapshotDto {
    LocalStatsSnapshotDto {
        generated_at_ms: Utc::now().timestamp_millis(),
        tracks: vec![],
        recent_media_ids: vec![],
        recent_artist_names: vec![],
    }
}

pub fn song_from_local_file(media_id: String, title: String, path: String) -> SongDto {
    SongDto::from(Song {
        media_id: MediaId::new(media_id),
        source_type: "local".into(),
        title,
        artists: vec![],
        album_id: None,
        duration_ms: None,
        explicit: false,
        video_only: false,
        available: true,
        local_path: Some(path),
        raw_metadata: json!({}),
    })
}

pub fn local_candidate(media_id: String, title: String) -> RecommendationCandidateDto {
    RecommendationCandidateDto::from(RecommendationCandidate {
        media_id: MediaId::new(media_id),
        title,
        artists: vec![],
        album_id: None,
        duration_ms: 0,
        source: RecommendationSource::Local,
        remote_score: 0.0,
        reason: Some("local".into()),
    })
}

impl From<RecommendationSourceDto> for RecommendationSource {
    fn from(source: RecommendationSourceDto) -> Self {
        match source {
            RecommendationSourceDto::Local => Self::Local,
            RecommendationSourceDto::Remote => Self::Remote,
            RecommendationSourceDto::Mixed => Self::Mixed,
        }
    }
}

impl From<RecommendationSource> for RecommendationSourceDto {
    fn from(source: RecommendationSource) -> Self {
        match source {
            RecommendationSource::Local => Self::Local,
            RecommendationSource::Remote => Self::Remote,
            RecommendationSource::Mixed => Self::Mixed,
        }
    }
}

impl From<RecommendationEventKindDto> for RecommendationEventKind {
    fn from(kind: RecommendationEventKindDto) -> Self {
        match kind {
            RecommendationEventKindDto::Impression => Self::Impression,
            RecommendationEventKindDto::PlayStarted => Self::PlayStarted,
            RecommendationEventKindDto::PlayCompleted => Self::PlayCompleted,
            RecommendationEventKindDto::Skipped => Self::Skipped,
            RecommendationEventKindDto::Liked => Self::Liked,
            RecommendationEventKindDto::Disliked => Self::Disliked,
        }
    }
}

impl From<RecommendationEventKind> for RecommendationEventKindDto {
    fn from(kind: RecommendationEventKind) -> Self {
        match kind {
            RecommendationEventKind::Impression => Self::Impression,
            RecommendationEventKind::PlayStarted => Self::PlayStarted,
            RecommendationEventKind::PlayCompleted => Self::PlayCompleted,
            RecommendationEventKind::Skipped => Self::Skipped,
            RecommendationEventKind::Liked => Self::Liked,
            RecommendationEventKind::Disliked => Self::Disliked,
        }
    }
}

impl From<TrackStatsDto> for TrackStats {
    fn from(stats: TrackStatsDto) -> Self {
        Self {
            media_id: MediaId::new(stats.media_id),
            play_count: stats.play_count,
            skip_count: stats.skip_count,
            completion_count: stats.completion_count,
            impression_count: stats.impression_count,
            liked: stats.liked,
            downloaded: stats.downloaded,
            local_available: stats.local_available,
            last_played_at: stats.last_played_at_ms.map(millis_to_utc),
        }
    }
}

impl From<TrackStats> for TrackStatsDto {
    fn from(stats: TrackStats) -> Self {
        Self {
            media_id: stats.media_id.0,
            play_count: stats.play_count,
            skip_count: stats.skip_count,
            completion_count: stats.completion_count,
            impression_count: stats.impression_count,
            liked: stats.liked,
            downloaded: stats.downloaded,
            local_available: stats.local_available,
            last_played_at_ms: stats.last_played_at.map(|time| time.timestamp_millis()),
        }
    }
}

impl From<LocalStatsSnapshotDto> for LocalStatsSnapshot {
    fn from(snapshot: LocalStatsSnapshotDto) -> Self {
        Self {
            generated_at: millis_to_utc(snapshot.generated_at_ms),
            tracks: snapshot.tracks.into_iter().map(Into::into).collect(),
            recent_media_ids: snapshot
                .recent_media_ids
                .into_iter()
                .map(MediaId::new)
                .collect(),
            recent_artist_names: snapshot.recent_artist_names,
        }
    }
}

impl From<LocalStatsSnapshot> for LocalStatsSnapshotDto {
    fn from(snapshot: LocalStatsSnapshot) -> Self {
        Self {
            generated_at_ms: snapshot.generated_at.timestamp_millis(),
            tracks: snapshot.tracks.into_iter().map(Into::into).collect(),
            recent_media_ids: snapshot
                .recent_media_ids
                .into_iter()
                .map(|media_id| media_id.0)
                .collect(),
            recent_artist_names: snapshot.recent_artist_names,
        }
    }
}

impl From<RecommendationCandidateDto> for RecommendationCandidate {
    fn from(candidate: RecommendationCandidateDto) -> Self {
        Self {
            media_id: MediaId::new(candidate.media_id),
            title: candidate.title,
            artists: candidate.artists,
            album_id: candidate.album_id.map(MediaId::new),
            duration_ms: candidate.duration_ms,
            source: candidate.source.into(),
            remote_score: candidate.remote_score,
            reason: candidate.reason,
        }
    }
}

impl From<RecommendationCandidate> for RecommendationCandidateDto {
    fn from(candidate: RecommendationCandidate) -> Self {
        Self {
            media_id: candidate.media_id.0,
            title: candidate.title,
            artists: candidate.artists,
            album_id: candidate.album_id.map(|id| id.0),
            duration_ms: candidate.duration_ms,
            source: candidate.source.into(),
            remote_score: candidate.remote_score,
            reason: candidate.reason,
        }
    }
}

impl From<Song> for SongDto {
    fn from(song: Song) -> Self {
        Self {
            media_id: song.media_id.0,
            source_type: song.source_type,
            title: song.title,
            artists: song.artists,
            album_id: song.album_id.map(|id| id.0),
            duration_ms: song.duration_ms,
            explicit: song.explicit,
            video_only: song.video_only,
            available: song.available,
            local_path: song.local_path,
        }
    }
}

impl From<SongDto> for Song {
    fn from(song: SongDto) -> Self {
        Self {
            media_id: MediaId::new(song.media_id),
            source_type: song.source_type,
            title: song.title,
            artists: song.artists,
            album_id: song.album_id.map(MediaId::new),
            duration_ms: song.duration_ms,
            explicit: song.explicit,
            video_only: song.video_only,
            available: song.available,
            local_path: song.local_path,
            raw_metadata: json!({}),
        }
    }
}

fn store_for_handle(handle: &CoreHandle) -> Result<SqliteStore> {
    match &handle.sqlite_path {
        Some(path) => Ok(SqliteStore::open(PathBuf::from(path))?),
        None => Err(anyhow!(
            "core handle is missing sqlite_path; call open_core"
        )),
    }
}

fn ephemeral_sqlite_path() -> PathBuf {
    std::env::temp_dir().join(format!("sunflower-core-{}.sqlite", Uuid::now_v7()))
}

fn event_from_dto(event: RecommendationEventDto) -> Result<RecommendationEvent> {
    Ok(RecommendationEvent {
        event_id: parse_uuid_v7("event_id", &event.event_id)?,
        device_id: parse_optional_uuid("device_id", event.device_id)?,
        client_clock: event.client_clock,
        occurred_at: millis_to_utc(event.occurred_at_ms),
        kind: event.kind.into(),
        media_id: MediaId::new(event.media_id),
        queue_id: parse_optional_uuid("queue_id", event.queue_id)?,
        recommender_source: event.recommender_source.into(),
        context: parse_json_object(&event.context_json)?,
        payload: parse_json_object(&event.payload_json)?,
    })
}

fn parse_uuid_v7(name: &str, raw: &str) -> Result<Uuid> {
    let id = Uuid::parse_str(raw).map_err(|err| anyhow!("invalid {name} {raw}: {err}"))?;
    if id.get_version_num() == 7 {
        Ok(id)
    } else {
        Err(anyhow!("invalid {name} {raw}: expected UUIDv7"))
    }
}

fn event_to_dto(event: RecommendationEvent) -> RecommendationEventDto {
    RecommendationEventDto {
        event_id: event.event_id.to_string(),
        device_id: event.device_id.map(|id| id.to_string()),
        client_clock: event.client_clock,
        occurred_at_ms: event.occurred_at.timestamp_millis(),
        kind: event.kind.into(),
        media_id: event.media_id.0,
        queue_id: event.queue_id.map(|id| id.to_string()),
        recommender_source: event.recommender_source.into(),
        context_json: event.context.to_string(),
        payload_json: event.payload.to_string(),
    }
}

fn snapshot_from_dto(snapshot: RecommendationSnapshotDto) -> Result<RecommendationSnapshot> {
    Ok(RecommendationSnapshot {
        snapshot_id: Uuid::parse_str(&snapshot.snapshot_id)
            .map_err(|err| anyhow!("invalid snapshot_id {}: {err}", snapshot.snapshot_id))?,
        model_version: snapshot.model_version,
        generated_at: millis_to_utc(snapshot.generated_at_ms),
        expires_at: millis_to_utc(snapshot.expires_at_ms),
        candidates: snapshot.candidates.into_iter().map(Into::into).collect(),
    })
}

fn snapshot_to_dto(snapshot: RecommendationSnapshot) -> RecommendationSnapshotDto {
    RecommendationSnapshotDto {
        snapshot_id: snapshot.snapshot_id.to_string(),
        model_version: snapshot.model_version,
        generated_at_ms: snapshot.generated_at.timestamp_millis(),
        expires_at_ms: snapshot.expires_at.timestamp_millis(),
        candidates: snapshot.candidates.into_iter().map(Into::into).collect(),
    }
}

fn local_candidate_from_song(song: Song) -> RecommendationCandidate {
    RecommendationCandidate {
        media_id: song.media_id,
        title: song.title,
        artists: song.artists,
        album_id: song.album_id,
        duration_ms: song.duration_ms.unwrap_or_default(),
        source: RecommendationSource::Local,
        remote_score: 0.0,
        reason: Some("local".into()),
    }
}

fn is_local_fallback_song(song: &Song) -> bool {
    let has_local_path = song
        .local_path
        .as_deref()
        .is_some_and(|path| !path.trim().is_empty());
    song.available && (song.source_type == "local" || has_local_path)
}

fn parse_optional_uuid(name: &str, value: Option<String>) -> Result<Option<Uuid>> {
    value
        .filter(|raw| !raw.trim().is_empty())
        .map(|raw| Uuid::parse_str(&raw).map_err(|err| anyhow!("invalid {name} {raw}: {err}")))
        .transpose()
}

fn parse_json_object(raw: &str) -> Result<Value> {
    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    Ok(serde_json::from_str(raw)?)
}

fn millis_to_utc(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_core_without_path_keeps_state_across_calls() {
        let handle = open_core(CoreConfig {
            sqlite_path: None,
            recommendation_server_url: None,
        })
        .unwrap();
        let path = handle
            .sqlite_path
            .clone()
            .expect("ephemeral core should expose a sqlite path");

        upsert_local_song(
            handle.clone(),
            SongDto {
                media_id: "local:ephemeral".into(),
                source_type: "local".into(),
                title: "Ephemeral".into(),
                artists: vec!["A".into()],
                album_id: None,
                duration_ms: Some(123),
                explicit: false,
                video_only: false,
                available: true,
                local_path: Some("/music/ephemeral.flac".into()),
            },
        )
        .unwrap();

        let songs = list_local_songs(handle, 10, 0).unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].media_id, "local:ephemeral");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rank_local_from_song_fallback_only_uses_playable_local_candidates() {
        let path = std::env::temp_dir().join(format!("sunflower-{}.sqlite", Uuid::now_v7()));
        let handle = open_core(CoreConfig {
            sqlite_path: Some(path.to_string_lossy().into_owned()),
            recommendation_server_url: None,
        })
        .unwrap();

        for song in [
            SongDto {
                media_id: "local:available".into(),
                source_type: "local".into(),
                title: "Available Local".into(),
                artists: vec!["A".into()],
                album_id: None,
                duration_ms: Some(100),
                explicit: false,
                video_only: false,
                available: true,
                local_path: None,
            },
            SongDto {
                media_id: "yt:downloaded".into(),
                source_type: "yt".into(),
                title: "Downloaded Remote".into(),
                artists: vec!["B".into()],
                album_id: None,
                duration_ms: Some(100),
                explicit: false,
                video_only: false,
                available: true,
                local_path: Some("/downloads/downloaded.m4a".into()),
            },
            SongDto {
                media_id: "local:hidden".into(),
                source_type: "local".into(),
                title: "Hidden Local".into(),
                artists: vec!["C".into()],
                album_id: None,
                duration_ms: Some(100),
                explicit: false,
                video_only: false,
                available: false,
                local_path: Some("/music/hidden.flac".into()),
            },
            SongDto {
                media_id: "yt:stream-only".into(),
                source_type: "yt".into(),
                title: "Stream Only".into(),
                artists: vec!["D".into()],
                album_id: None,
                duration_ms: Some(100),
                explicit: false,
                video_only: false,
                available: true,
                local_path: None,
            },
        ] {
            upsert_local_song(handle.clone(), song).unwrap();
        }

        let ranked = rank_local_from_snapshot(handle.clone(), empty_stats_snapshot(), 10).unwrap();
        let media_ids: Vec<_> = ranked.into_iter().map(|song| song.media_id).collect();
        assert_eq!(
            media_ids,
            vec!["local:available".to_string(), "yt:downloaded".to_string(),]
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn bridge_round_trips_local_mode_state() {
        let path = std::env::temp_dir().join(format!("sunflower-{}.sqlite", Uuid::now_v7()));
        let handle = open_core(CoreConfig {
            sqlite_path: Some(path.to_string_lossy().into_owned()),
            recommendation_server_url: None,
        })
        .unwrap();

        upsert_local_song(
            handle.clone(),
            SongDto {
                media_id: "local:one".into(),
                source_type: "local".into(),
                title: "One".into(),
                artists: vec!["A".into()],
                album_id: None,
                duration_ms: Some(123),
                explicit: false,
                video_only: false,
                available: true,
                local_path: Some("/music/one.flac".into()),
            },
        )
        .unwrap();
        assert_eq!(list_local_songs(handle.clone(), 10, 0).unwrap().len(), 1);

        let snapshot_id = Uuid::now_v7().to_string();
        put_recommendation_snapshot(
            handle.clone(),
            RecommendationSnapshotDto {
                snapshot_id: snapshot_id.clone(),
                model_version: "local-test".into(),
                generated_at_ms: 1_700_000_000_000,
                expires_at_ms: 1_700_003_600_000,
                candidates: vec![RecommendationCandidateDto {
                    media_id: "local:one".into(),
                    title: "One".into(),
                    artists: vec!["A".into()],
                    album_id: None,
                    duration_ms: 123,
                    source: RecommendationSourceDto::Local,
                    remote_score: 0.0,
                    reason: Some("snapshot".into()),
                }],
            },
        )
        .unwrap();
        assert_eq!(
            latest_recommendation_snapshot(handle.clone())
                .unwrap()
                .unwrap()
                .snapshot_id,
            snapshot_id
        );
        assert_eq!(
            rank_local_from_snapshot(handle.clone(), empty_stats_snapshot(), 10)
                .unwrap()
                .first()
                .unwrap()
                .media_id,
            "local:one"
        );

        let first_event = RecommendationEventDto {
            event_id: new_event_id(),
            device_id: None,
            client_clock: 2,
            occurred_at_ms: 1_700_000_000_000,
            kind: RecommendationEventKindDto::PlayStarted,
            media_id: "local:one".into(),
            queue_id: None,
            recommender_source: RecommendationSourceDto::Local,
            context_json: "{}".into(),
            payload_json: "{}".into(),
        };
        let second_event = RecommendationEventDto {
            client_clock: 1,
            event_id: new_event_id(),
            ..first_event.clone()
        };
        append_recommendation_event(handle.clone(), first_event.clone()).unwrap();
        append_recommendation_event(handle.clone(), second_event.clone()).unwrap();

        let unsynced = unsynced_recommendation_events(handle.clone(), 10).unwrap();
        assert_eq!(unsynced[0].client_clock, 1);
        assert_eq!(unsynced[1].client_clock, 2);
        mark_recommendation_events_synced(handle.clone(), vec![unsynced[0].event_id.clone()])
            .unwrap();
        let remaining = unsynced_recommendation_events(handle.clone(), 10).unwrap();
        assert_eq!(remaining, vec![first_event]);

        let stats = local_stats_snapshot(handle.clone(), 10).unwrap();
        assert_eq!(stats.recent_media_ids, vec!["local:one".to_string()]);
        assert_eq!(stats.recent_artist_names, vec!["A".to_string()]);
        let track = stats
            .tracks
            .iter()
            .find(|track| track.media_id == "local:one")
            .unwrap();
        assert_eq!(track.play_count, 2);
        assert!(track.local_available);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recommendation_event_ids_must_be_uuidv7_at_bridge_boundary() {
        let path = std::env::temp_dir().join(format!("sunflower-{}.sqlite", Uuid::now_v7()));
        let handle = open_core(CoreConfig {
            sqlite_path: Some(path.to_string_lossy().into_owned()),
            recommendation_server_url: None,
        })
        .unwrap();
        let invalid_event = RecommendationEventDto {
            event_id: Uuid::new_v4().to_string(),
            device_id: None,
            client_clock: 1,
            occurred_at_ms: 1_700_000_000_000,
            kind: RecommendationEventKindDto::PlayCompleted,
            media_id: "local:one".into(),
            queue_id: None,
            recommender_source: RecommendationSourceDto::Local,
            context_json: "{}".into(),
            payload_json: "{}".into(),
        };

        let append_error = append_recommendation_event(handle.clone(), invalid_event)
            .unwrap_err()
            .to_string();
        assert!(append_error.contains("expected UUIDv7"));

        let mark_error =
            mark_recommendation_events_synced(handle.clone(), vec![Uuid::new_v4().to_string()])
                .unwrap_err()
                .to_string();
        assert!(mark_error.contains("expected UUIDv7"));

        let _ = std::fs::remove_file(path);
    }
}
