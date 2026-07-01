use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::{RecommendationEvent, RecommendationSnapshot, Song};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("not found")]
    NotFound,
    #[error("storage backend error: {0}")]
    Backend(String),
}

pub type StorageResult<T> = Result<T, StorageError>;

#[async_trait]
pub trait MediaRepository: Send + Sync {
    async fn upsert_song(&self, song: &Song) -> StorageResult<()>;
    async fn list_songs(&self, limit: i64, offset: i64) -> StorageResult<Vec<Song>>;
}

#[async_trait]
pub trait RecommendationEventRepository: Send + Sync {
    async fn append_event(&self, event: &RecommendationEvent) -> StorageResult<()>;
    async fn unsynced_events(&self, limit: i64) -> StorageResult<Vec<RecommendationEvent>>;
    async fn mark_events_synced(&self, event_ids: &[Uuid]) -> StorageResult<()>;
}

#[async_trait]
pub trait RecommendationSnapshotRepository: Send + Sync {
    async fn put_snapshot(&self, snapshot: &RecommendationSnapshot) -> StorageResult<()>;
    async fn latest_snapshot(&self) -> StorageResult<Option<RecommendationSnapshot>>;
}
