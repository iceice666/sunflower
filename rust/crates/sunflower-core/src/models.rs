use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MediaId(pub String);

impl MediaId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn source(&self) -> &str {
        self.0
            .split_once(':')
            .map(|(source, _)| source)
            .unwrap_or("")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Song {
    pub media_id: MediaId,
    pub source_type: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album_id: Option<MediaId>,
    pub duration_ms: Option<i32>,
    pub explicit: bool,
    pub video_only: bool,
    pub available: bool,
    pub local_path: Option<String>,
    pub raw_metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueItem {
    pub media_id: MediaId,
    pub title: String,
    pub artists: Vec<String>,
    pub duration_ms: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueueSession {
    pub id: Uuid,
    pub seed_kind: String,
    pub seed_id: String,
    pub title: String,
    pub version: i64,
    pub items: Vec<QueueItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedStream {
    pub media_id: MediaId,
    pub source: String,
    pub stream_url: String,
    pub stream_expires_at: Option<DateTime<Utc>>,
    pub mime_type: Option<String>,
    pub content_length: Option<i64>,
    pub loudness_db: Option<f32>,
    pub playback_tracking_url: Option<String>,
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NextDecision {
    pub queue_id: Uuid,
    pub position: usize,
    pub current: Option<ResolvedStream>,
    pub lookahead: Vec<QueueItem>,
    pub continuation: Option<String>,
    pub automix: Vec<QueueItem>,
    pub has_more: bool,
    pub queue_version: i64,
    pub recommender_source: RecommendationSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationSource {
    Local,
    Remote,
    Mixed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecommendationCandidate {
    pub media_id: MediaId,
    pub title: String,
    pub artists: Vec<String>,
    pub album_id: Option<MediaId>,
    pub duration_ms: i32,
    pub source: RecommendationSource,
    pub remote_score: f32,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecommendationSection {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub seed: Option<String>,
    pub items: Vec<RecommendationCandidate>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HomeFeed {
    pub sections: Vec<RecommendationSection>,
    pub chips: Vec<String>,
    pub stale: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecommendationSnapshot {
    pub snapshot_id: Uuid,
    pub model_version: String,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub candidates: Vec<RecommendationCandidate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationEventKind {
    Impression,
    PlayStarted,
    PlayCompleted,
    Skipped,
    Liked,
    Disliked,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecommendationEvent {
    pub event_id: Uuid,
    pub device_id: Option<Uuid>,
    pub client_clock: i64,
    pub occurred_at: DateTime<Utc>,
    pub kind: RecommendationEventKind,
    pub media_id: MediaId,
    pub queue_id: Option<Uuid>,
    pub recommender_source: RecommendationSource,
    pub context: Value,
    pub payload: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackStats {
    pub media_id: MediaId,
    pub play_count: i64,
    pub skip_count: i64,
    pub completion_count: i64,
    pub impression_count: i64,
    pub liked: bool,
    pub downloaded: bool,
    pub local_available: bool,
    pub last_played_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LocalStatsSnapshot {
    pub generated_at: DateTime<Utc>,
    pub tracks: Vec<TrackStats>,
    pub recent_media_ids: Vec<MediaId>,
    pub recent_artist_names: Vec<String>,
}
