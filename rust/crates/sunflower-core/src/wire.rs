//! Legacy HTTP wire DTOs.
//!
//! These types intentionally preserve the established Sunflower JSON contracts.
//! The Rust core may carry richer internal state, but `sunflower-server` should
//! serialize these DTOs at the API boundary until the client contract is
//! deliberately versioned.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize, Serializer, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

use crate::{NextDecision, QueueItem, QueueSession, ResolvedStream};

fn default_on_null<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

struct DefaultOnNull<T>(T);

impl<'de, T> Deserialize<'de> for DefaultOnNull<T>
where
    T: Deserialize<'de> + Default,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(
            Option::<T>::deserialize(deserializer)?.unwrap_or_default(),
        ))
    }
}

fn vec_default_on_null<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    let items = Option::<Vec<DefaultOnNull<T>>>::deserialize(deserializer)?;
    Ok(items
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.0)
        .collect())
}

fn decode_legacy_json<T: DeserializeOwned + Default>(raw: &str) -> Result<T, LegacyRequestError> {
    let mut stream = serde_json::Deserializer::from_str(raw).into_iter::<Option<T>>();
    match stream.next() {
        Some(Ok(Some(value))) => Ok(value),
        Some(Ok(None)) => Ok(T::default()),
        _ => Err(LegacyRequestError::InvalidRequest),
    }
}

pub fn legacy_rfc3339_nano(time: DateTime<Utc>) -> String {
    let raw = time.to_rfc3339_opts(SecondsFormat::Nanos, true);
    let Some(dot) = raw.find('.') else {
        return raw;
    };
    let Some(zone_start) = raw[dot..].find('Z').map(|index| dot + index) else {
        return raw;
    };

    let mut fractional_end = zone_start;
    let bytes = raw.as_bytes();
    while fractional_end > dot + 1 && bytes[fractional_end - 1] == b'0' {
        fractional_end -= 1;
    }

    if fractional_end == dot + 1 {
        format!("{}{}", &raw[..dot], &raw[zone_start..])
    } else {
        format!("{}{}", &raw[..fractional_end], &raw[zone_start..])
    }
}

fn serialize_legacy_rfc3339_nano<S>(time: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&legacy_rfc3339_nano(*time))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthzResponse {
    pub status: String,
}

impl Default for HealthzResponse {
    fn default() -> Self {
        Self {
            status: "ok".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupStatusResponse {
    pub configured: bool,
    pub pairing_required: bool,
    pub server_version: String,
    pub server_capabilities: Vec<String>,
}

impl SetupStatusResponse {
    pub fn legacy_default(server_version: impl Into<String>) -> Self {
        Self {
            configured: false,
            pairing_required: true,
            server_version: server_version.into(),
            server_capabilities: setup_capabilities()
                .iter()
                .map(|capability| (*capability).to_string())
                .collect(),
        }
    }
}

pub fn setup_capabilities() -> &'static [&'static str] {
    &["auth.pairing.v1", "admin.sessions.v1", "device.revoke.v1"]
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerSetupRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub setup_token: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub display_name: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub password: String,
}

impl OwnerSetupRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        decode_legacy_json(raw)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminLoginRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub password: String,
}

impl AdminLoginRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        decode_legacy_json(raw)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminRevokeDeviceRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub reason: String,
}

impl AdminRevokeDeviceRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        decode_legacy_json(raw)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminLoginResponse {
    pub csrf_token: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminMeResponse {
    pub user_id: String,
    pub display_name: String,
    pub csrf_token: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminPairingCodeRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub label: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub ttl_seconds: i64,
}

impl AdminPairingCodeRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        decode_legacy_json(raw)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminPairingCodeResponse {
    pub pairing_code: String,
    pub pairing_url: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminLibraryCountsResponse {
    pub songs: i64,
    pub albums: i64,
    pub artists: i64,
    pub playlists: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminCookieStatusResponse {
    pub status: String,
    #[serde(default)]
    pub checked_at: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminUploadCookiesRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub cookies: String,
}

impl AdminUploadCookiesRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        decode_legacy_json(raw)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminDeviceResponse {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub token_label: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
    pub revoked_reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminDevicesResponse {
    pub devices: Vec<AdminDeviceResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminLibraryStatusResponse {
    pub counts: AdminLibraryCountsResponse,
    pub jobs: Vec<JobResponse>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartScanRequest {
    #[serde(default, deserialize_with = "vec_default_on_null")]
    pub roots: Vec<String>,
}

impl StartScanRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        let req: Self = decode_legacy_json(raw)?;
        if req.roots.is_empty() {
            return Err(LegacyRequestError::InvalidRequest);
        }
        Ok(req)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartScanResponse {
    pub job_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobResponse {
    pub id: String,
    pub status: String,
    pub processed_files: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminAuditEventResponse {
    pub id: String,
    pub actor_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub actor_id: String,
    pub event: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_id: String,
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminAuditResponse {
    pub events: Vec<AdminAuditEventResponse>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdminNowPlayingResponse {
    pub now_playing: Vec<NowPlayingStateResponse>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminNowPlayingCommandRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub device_id: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub command: String,
}

impl AdminNowPlayingCommandRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        decode_legacy_json(raw)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminNowPlayingCommandResponse {
    pub delivered: i32,
}

pub const NOW_PLAYING_SUBPROTOCOL: &str = "sunflower.now-playing.v1";
pub const NOW_PLAYING_KIND_TICK: &str = "tick";
pub const NOW_PLAYING_KIND_TRANSITION: &str = "transition";
pub const NOW_PLAYING_KIND_STATE: &str = "state";
pub const NOW_PLAYING_KIND_COMMAND: &str = "command";
pub const NOW_PLAYING_CMD_PAUSE: &str = "pause";
pub const NOW_PLAYING_CMD_PLAY: &str = "play";
pub const NOW_PLAYING_CMD_SKIP_NEXT: &str = "skip_next";
pub const NOW_PLAYING_CMD_SKIP_PREV: &str = "skip_prev";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NowPlayingClientMessage {
    #[serde(rename = "type")]
    #[serde(default, deserialize_with = "default_on_null")]
    pub kind: String,
    #[serde(
        default,
        deserialize_with = "default_on_null",
        skip_serializing_if = "String::is_empty"
    )]
    pub queue_id: String,
    #[serde(
        default,
        deserialize_with = "default_on_null",
        skip_serializing_if = "String::is_empty"
    )]
    pub media_id: String,
    #[serde(
        default,
        deserialize_with = "default_on_null",
        skip_serializing_if = "String::is_empty"
    )]
    pub title: String,
    #[serde(
        default,
        deserialize_with = "default_on_null",
        skip_serializing_if = "String::is_empty"
    )]
    pub artist: String,
    #[serde(
        default,
        deserialize_with = "default_on_null",
        skip_serializing_if = "is_zero_i32"
    )]
    pub position_ms: i32,
    #[serde(
        default,
        deserialize_with = "default_on_null",
        skip_serializing_if = "is_zero_i32"
    )]
    pub duration_ms: i32,
    #[serde(default, deserialize_with = "default_on_null")]
    pub is_playing: bool,
    #[serde(
        default,
        deserialize_with = "default_on_null",
        skip_serializing_if = "is_false"
    )]
    pub shuffle: bool,
    #[serde(
        default,
        deserialize_with = "default_on_null",
        skip_serializing_if = "String::is_empty"
    )]
    pub repeat: String,
}

impl NowPlayingClientMessage {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        decode_legacy_json(raw)
    }

    pub fn from_state(state: &NowPlayingStateResponse) -> Self {
        Self {
            kind: NOW_PLAYING_KIND_TICK.to_string(),
            queue_id: state.queue_id.clone(),
            media_id: state.media_id.clone(),
            title: state.title.clone(),
            artist: state.artist.clone(),
            position_ms: state.position_ms,
            duration_ms: state.duration_ms,
            is_playing: state.is_playing,
            shuffle: false,
            repeat: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NowPlayingServerMessage {
    #[serde(rename = "type")]
    pub kind: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NowPlayingStateResponse {
    pub device_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub queue_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub media_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist: String,
    pub position_ms: i32,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub duration_ms: i32,
    pub is_playing: bool,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminStatusResponse {
    pub server_version: String,
    pub uptime_seconds: i64,
    pub db_status: String,
    pub library_counts: AdminLibraryCountsResponse,
    pub cookie_status: AdminCookieStatusResponse,
    pub devices: Vec<AdminDeviceResponse>,
    pub now_playing: Vec<NowPlayingStateResponse>,
    pub jobs: Vec<JobResponse>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterDeviceRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub device_name: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub platform: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub client_version: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub pairing_code: String,
}

impl RegisterDeviceRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        decode_legacy_json(raw)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterDeviceResponse {
    pub device_id: String,
    pub token: String,
    pub server_capabilities: Vec<String>,
}

pub fn device_capabilities() -> &'static [&'static str] {
    &[
        "auth.pairing.v1",
        "library.v1",
        "recs.v1",
        "stream.proxy",
        "ws.now_playing",
    ]
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongListItemResponse {
    pub media_id: String,
    pub source_type: String,
    pub title: String,
    pub duration_ms: Option<i32>,
    pub album_id: Option<String>,
    pub artist_name: String,
    pub album_title: String,
    pub has_art: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongListResponse {
    pub songs: Vec<SongListItemResponse>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlbumListItemResponse {
    pub media_id: String,
    pub source_type: String,
    pub title: String,
    pub primary_artist_id: Option<String>,
    pub year: Option<i32>,
    pub available: bool,
    pub raw_metadata: Value,
    #[serde(serialize_with = "serialize_legacy_rfc3339_nano")]
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlbumListResponse {
    pub albums: Vec<AlbumListItemResponse>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtistListItemResponse {
    pub media_id: String,
    pub source_type: String,
    pub name: String,
    pub available: bool,
    pub raw_metadata: Value,
    #[serde(serialize_with = "serialize_legacy_rfc3339_nano")]
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtistListResponse {
    pub artists: Vec<ArtistListItemResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongHashResponse {
    pub media_id: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterDownloadRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub media_id: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub local_path: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub bytes: i64,
}

impl RegisterDownloadRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        let req: Self = decode_legacy_json(raw)?;
        if req.media_id.is_empty() {
            return Err(LegacyRequestError::InvalidRequest);
        }
        Ok(req)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadListItemResponse {
    pub media_id: String,
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub bytes: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadListResponse {
    pub downloads: Vec<DownloadListItemResponse>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HomeItemResponse {
    pub media_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub duration_ms: i32,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HomeSectionResponse {
    pub id: String,
    pub title: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    pub items: Vec<HomeItemResponse>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HomeResponse {
    pub sections: Vec<HomeSectionResponse>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chips: Vec<String>,
    pub stale: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LikeRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub media_id: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub liked: bool,
    #[serde(default)]
    pub occurred_at: Option<String>,
}

impl LikeRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        let req: Self = decode_legacy_json(raw)?;
        if req.media_id.is_empty() {
            return Err(LegacyRequestError::InvalidRequest);
        }
        Ok(req)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LikeResponse {
    pub media_id: String,
    pub liked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchSongResponse {
    pub media_id: String,
    pub source: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    pub duration_ms: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchAlbumResponse {
    pub browse_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchArtistResponse {
    pub browse_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResponse {
    pub query: String,
    pub songs: Vec<SearchSongResponse>,
    pub albums: Vec<SearchAlbumResponse>,
    pub artists: Vec<SearchArtistResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsRequest {
    #[serde(default, deserialize_with = "vec_default_on_null")]
    pub events: Vec<EventEntryRequest>,
}

impl EventsRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        decode_legacy_json(raw)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEntryRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub event_id: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub kind: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub media_id: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub queue_id: String,
    #[serde(default)]
    pub occurred_at: Option<String>,
    #[serde(default, deserialize_with = "default_on_null")]
    pub total_played_ms: i32,
    #[serde(default, deserialize_with = "default_on_null")]
    pub duration_ms: i32,
    #[serde(default, deserialize_with = "default_on_null")]
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventResultResponse {
    pub event_id: String,
    pub accepted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsResponse {
    pub results: Vec<EventResultResponse>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpressionsRequest {
    #[serde(default, deserialize_with = "vec_default_on_null")]
    pub impressions: Vec<ImpressionEntryRequest>,
}

impl ImpressionsRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        decode_legacy_json(raw)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpressionEntryRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub section_id: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub source: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub seed_id: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub media_id: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub position: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpressionsResponse {
    pub written: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistResponse {
    pub id: String,
    pub title: String,
    pub source_type: String,
    pub version: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<PlaylistItemResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistItemResponse {
    pub position: i32,
    pub media_id: String,
    pub title: String,
    pub artist_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub album_id: String,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub duration_ms: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistListResponse {
    pub playlists: Vec<PlaylistResponse>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistTitleRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub title: String,
}

impl PlaylistTitleRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        let req: Self = decode_legacy_json(raw)?;
        if req.title.is_empty() {
            return Err(LegacyRequestError::InvalidRequest);
        }
        Ok(req)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddPlaylistItemRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub media_id: String,
}

impl AddPlaylistItemRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        let req: Self = decode_legacy_json(raw)?;
        if req.media_id.is_empty() {
            return Err(LegacyRequestError::InvalidRequest);
        }
        Ok(req)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyRequestError {
    InvalidRequest,
    InvalidSeedKind,
    SeedUnavailable,
}

impl LegacyRequestError {
    pub fn legacy_error_code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::InvalidSeedKind => "invalid_seed_kind",
            Self::SeedUnavailable => "seed_unavailable",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartQueueRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub seed_kind: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub seed_id: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub title: String,
}

impl StartQueueRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        let req: Self = decode_legacy_json(raw)?;
        req.validate_seed_kind()?;
        Ok(req)
    }

    pub fn validate_seed_kind(&self) -> Result<(), LegacyRequestError> {
        match self.seed_kind.as_str() {
            "song" | "shuffle_liked" => Ok(()),
            _ => Err(LegacyRequestError::InvalidSeedKind),
        }
    }

    pub fn song_seed_video_id(&self) -> Result<&str, LegacyRequestError> {
        let video_id = self.seed_id.strip_prefix("yt:").unwrap_or(&self.seed_id);
        if video_id.is_empty() {
            return Err(LegacyRequestError::SeedUnavailable);
        }
        Ok(video_id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveStreamRequest {
    #[serde(default, deserialize_with = "default_on_null")]
    pub media_id: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub proxy: bool,
    #[serde(default, deserialize_with = "default_on_null")]
    pub audio_quality: String,
    #[serde(default, deserialize_with = "default_on_null")]
    pub reason: String,
}

impl ResolveStreamRequest {
    pub fn parse_json(raw: &str) -> Result<Self, LegacyRequestError> {
        let req: Self = decode_legacy_json(raw)?;
        if req.media_id.is_empty() {
            return Err(LegacyRequestError::InvalidRequest);
        }
        Ok(req)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NextQuery {
    pub queue_id: Uuid,
    pub position: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextQueryError {
    InvalidQueueId,
    InvalidPosition,
}

impl NextQueryError {
    pub fn legacy_error_code(self) -> &'static str {
        match self {
            Self::InvalidQueueId => "invalid_queue_id",
            Self::InvalidPosition => "invalid_position",
        }
    }
}

impl NextQuery {
    pub fn parse(queue_id: Option<&str>, position: Option<&str>) -> Result<Self, NextQueryError> {
        let queue_id = queue_id
            .and_then(|raw| Uuid::parse_str(raw).ok())
            .ok_or(NextQueryError::InvalidQueueId)?;
        let position = match position {
            Some(raw) if !raw.is_empty() => {
                let parsed = raw
                    .parse::<i64>()
                    .map_err(|_| NextQueryError::InvalidPosition)?;
                usize::try_from(parsed).map_err(|_| NextQueryError::InvalidPosition)?
            }
            _ => 0,
        };
        Ok(Self { queue_id, position })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueItemResponse {
    pub media_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<String>,
    pub duration_ms: i32,
}

impl From<&QueueItem> for QueueItemResponse {
    fn from(item: &QueueItem) -> Self {
        Self {
            media_id: item.media_id.0.clone(),
            title: item.title.clone(),
            artists: item.artists.clone(),
            duration_ms: item.duration_ms,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueResponse {
    pub queue_id: String,
    pub seed_kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    pub version: i64,
    pub items: Vec<QueueItemResponse>,
}

impl From<&QueueSession> for QueueResponse {
    fn from(session: &QueueSession) -> Self {
        Self {
            queue_id: session.id.to_string(),
            seed_kind: session.seed_kind.clone(),
            title: session.title.clone(),
            version: session.version,
            items: session.items.iter().map(QueueItemResponse::from).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedStreamResponse {
    pub media_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<String>,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub duration_ms: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stream_url: String,
    pub stream_expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub itag: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_length: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loudness_db: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playback_tracking_url: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub metadata: Value,
}

impl From<&QueueItem> for ResolvedStreamResponse {
    fn from(item: &QueueItem) -> Self {
        Self {
            media_id: item.media_id.0.clone(),
            title: item.title.clone(),
            artists: item.artists.clone(),
            duration_ms: item.duration_ms,
            source: String::new(),
            stream_url: String::new(),
            stream_expires_at: None,
            itag: None,
            mime_type: String::new(),
            content_length: None,
            loudness_db: None,
            playback_tracking_url: None,
            metadata: Value::Null,
        }
    }
}

impl From<&ResolvedStream> for ResolvedStreamResponse {
    fn from(stream: &ResolvedStream) -> Self {
        Self {
            media_id: stream.media_id.0.clone(),
            title: String::new(),
            artists: vec![],
            duration_ms: 0,
            source: stream.source.clone(),
            stream_url: stream.stream_url.clone(),
            stream_expires_at: stream
                .stream_expires_at
                .map(|time| time.to_rfc3339_opts(SecondsFormat::Secs, true)),
            itag: stream
                .metadata
                .get("itag")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok()),
            mime_type: stream.mime_type.clone().unwrap_or_default(),
            content_length: stream.content_length,
            loudness_db: stream.loudness_db,
            playback_tracking_url: stream.playback_tracking_url.clone(),
            metadata: stream.metadata.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NextResponse {
    pub queue_id: String,
    pub position: usize,
    pub current: Option<ResolvedStreamResponse>,
    pub lookahead: Vec<ResolvedStreamResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub automix: Vec<QueueItemResponse>,
    pub queue_version: i64,
    pub has_more: bool,
}

impl NextResponse {
    pub fn from_decision_with_current_metadata(decision: &NextDecision) -> Self {
        let current = decision.current.as_ref().map(ResolvedStreamResponse::from);
        let lookahead = decision
            .lookahead
            .iter()
            .map(ResolvedStreamResponse::from)
            .collect();
        Self::from_decision_with_streams(decision, current, lookahead)
    }

    pub fn from_decision_with_streams(
        decision: &NextDecision,
        current: Option<ResolvedStreamResponse>,
        lookahead: Vec<ResolvedStreamResponse>,
    ) -> Self {
        Self {
            queue_id: decision.queue_id.to_string(),
            position: decision.position,
            current,
            lookahead,
            continuation: decision.continuation.clone(),
            automix: decision
                .automix
                .iter()
                .map(QueueItemResponse::from)
                .collect(),
            queue_version: decision.queue_version,
            has_more: decision.has_more,
        }
    }
}

fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}

fn is_zero_i64(value: &i64) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MediaId, QueueItem, QueueSession};
    use serde_json::{Value, json};
    use uuid::Uuid;

    #[test]
    fn healthz_matches_go_contract() {
        let value = serde_json::to_value(HealthzResponse::default()).unwrap();
        assert_eq!(value, json!({ "status": "ok" }));
    }

    #[test]
    fn error_response_matches_go_contract() {
        let value = serde_json::to_value(ErrorResponse {
            error: "invalid_queue_id".into(),
        })
        .unwrap();
        assert_eq!(value, json!({ "error": "invalid_queue_id" }));
    }

    #[test]
    fn setup_status_matches_go_default_contract() {
        let value = serde_json::to_value(SetupStatusResponse::legacy_default("0.3.0")).unwrap();
        assert_eq!(
            value,
            json!({
                "configured": false,
                "pairing_required": true,
                "server_version": "0.3.0",
                "server_capabilities": [
                    "auth.pairing.v1",
                    "admin.sessions.v1",
                    "device.revoke.v1"
                ]
            })
        );
    }

    #[test]
    fn owner_setup_request_matches_go_parse_contract() {
        let req = OwnerSetupRequest::parse_json(
            r#"{"setup_token":" token ","display_name":" Owner ","password":"sunflower owner password"}"#,
        )
        .unwrap();
        assert_eq!(req.setup_token, " token ");
        assert_eq!(req.display_name, " Owner ");
        assert_eq!(req.password, "sunflower owner password");

        let defaults = OwnerSetupRequest::parse_json("{}").unwrap();
        assert_eq!(defaults, OwnerSetupRequest::default());
        assert_eq!(
            OwnerSetupRequest::parse_json("null").unwrap(),
            OwnerSetupRequest::default()
        );
        let null_fields = OwnerSetupRequest::parse_json(
            r#"{"setup_token":null,"display_name":null,"password":null}"#,
        )
        .unwrap();
        assert_eq!(null_fields, OwnerSetupRequest::default());
        let trailing = OwnerSetupRequest::parse_json(
            r#"{"setup_token":"token","password":"sunflower owner password"} trailing"#,
        )
        .unwrap();
        assert_eq!(trailing.setup_token, "token");
        assert_eq!(trailing.password, "sunflower owner password");

        assert_eq!(
            OwnerSetupRequest::parse_json("{")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
    }

    #[test]
    fn request_json_decode_matches_go_decoder_contract() {
        let trailing_object =
            LikeRequest::parse_json(r#"{"media_id":"local:one","liked":true}{"ignored":true}"#)
                .unwrap();
        assert_eq!(trailing_object.media_id, "local:one");
        assert!(trailing_object.liked);

        let trailing_text =
            LikeRequest::parse_json(r#"{"media_id":"local:two","unknown":1} trailing"#).unwrap();
        assert_eq!(trailing_text.media_id, "local:two");
        assert!(!trailing_text.liked);

        let null_like =
            LikeRequest::parse_json(r#"{"media_id":"local:null-like","liked":null}"#).unwrap();
        assert_eq!(null_like.media_id, "local:null-like");
        assert!(!null_like.liked);

        let null_download = RegisterDownloadRequest::parse_json(
            r#"{"media_id":"local:dl","local_path":null,"bytes":null}"#,
        )
        .unwrap();
        assert_eq!(null_download.media_id, "local:dl");
        assert_eq!(null_download.local_path, "");
        assert_eq!(null_download.bytes, 0);

        let scan_with_null_element =
            StartScanRequest::parse_json(r#"{"roots":[null,"/music"]}"#).unwrap();
        assert_eq!(scan_with_null_element.roots, vec!["", "/music"]);

        let events_with_nulls = EventsRequest::parse_json(
            r#"{"events":[null,{"event_id":null,"kind":null,"media_id":null,"queue_id":null,"occurred_at":null,"total_played_ms":null,"duration_ms":null,"reason":null}]}"#,
        )
        .unwrap();
        assert_eq!(events_with_nulls.events.len(), 2);
        assert_eq!(events_with_nulls.events[0], EventEntryRequest::default());
        assert_eq!(events_with_nulls.events[1], EventEntryRequest::default());

        let impressions_with_nulls = ImpressionsRequest::parse_json(
            r#"{"impressions":[null,{"section_id":null,"source":null,"seed_id":null,"media_id":null,"position":null}]}"#,
        )
        .unwrap();
        assert_eq!(impressions_with_nulls.impressions.len(), 2);
        assert_eq!(
            impressions_with_nulls.impressions[0],
            ImpressionEntryRequest::default()
        );
        assert_eq!(
            impressions_with_nulls.impressions[1],
            ImpressionEntryRequest::default()
        );

        let null_now_playing = NowPlayingClientMessage::parse_json(
            r#"{"type":null,"queue_id":null,"media_id":null,"title":null,"artist":null,"position_ms":null,"duration_ms":null,"is_playing":null,"shuffle":null,"repeat":null}"#,
        )
        .unwrap();
        assert_eq!(null_now_playing, NowPlayingClientMessage::default());

        let revoke =
            AdminRevokeDeviceRequest::parse_json(r#"{"reason":"lost phone"} trailing"#).unwrap();
        assert_eq!(revoke.reason, "lost phone");
        let revoke_null = AdminRevokeDeviceRequest::parse_json(r#"{"reason":null}"#).unwrap();
        assert_eq!(revoke_null.reason, "");

        let null_login = AdminLoginRequest::parse_json("null").unwrap();
        assert_eq!(null_login.password, "");
        assert_eq!(
            LikeRequest::parse_json("null")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );

        assert_eq!(
            LikeRequest::parse_json(r#" trailing {"media_id":"local:one"}"#)
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
    }

    #[test]
    fn admin_auth_and_pairing_wire_shapes_match_go_contract() {
        let login =
            AdminLoginRequest::parse_json(r#"{"password":"sunflower owner password"}"#).unwrap();
        assert_eq!(login.password, "sunflower owner password");
        assert_eq!(AdminLoginRequest::parse_json("{}").unwrap().password, "");
        assert_eq!(AdminLoginRequest::parse_json("null").unwrap().password, "");
        assert_eq!(
            AdminLoginRequest::parse_json(r#"{"password":null}"#)
                .unwrap()
                .password,
            ""
        );
        assert_eq!(
            AdminLoginRequest::parse_json(r#"{"password":"pw"} trailing"#)
                .unwrap()
                .password,
            "pw"
        );
        assert_eq!(
            AdminLoginRequest::parse_json("{")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
        let revoke =
            AdminRevokeDeviceRequest::parse_json(r#"{"reason":"retired"}{"ignored":true}"#)
                .unwrap();
        assert_eq!(revoke.reason, "retired");
        assert_eq!(
            AdminRevokeDeviceRequest::parse_json("{")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
        assert_eq!(
            serde_json::to_value(AdminLoginResponse {
                csrf_token: "sf_csrf_token".into(),
                expires_at: "2026-07-15T00:00:00Z".into(),
            })
            .unwrap(),
            json!({
                "csrf_token": "sf_csrf_token",
                "expires_at": "2026-07-15T00:00:00Z"
            })
        );

        let pairing =
            AdminPairingCodeRequest::parse_json(r#"{"label":"Pixel","ttl_seconds":600}"#).unwrap();
        assert_eq!(pairing.label, "Pixel");
        assert_eq!(pairing.ttl_seconds, 600);
        let pairing_defaults = AdminPairingCodeRequest::parse_json("{}").unwrap();
        assert_eq!(pairing_defaults.label, "");
        assert_eq!(pairing_defaults.ttl_seconds, 0);
        let pairing_null =
            AdminPairingCodeRequest::parse_json(r#"{"label":null,"ttl_seconds":null}"#).unwrap();
        assert_eq!(pairing_null.label, "");
        assert_eq!(pairing_null.ttl_seconds, 0);
        let pairing_trailing =
            AdminPairingCodeRequest::parse_json(r#"{"label":" Pixel ","ttl_seconds":-1} trailing"#)
                .unwrap();
        assert_eq!(pairing_trailing.label, " Pixel ");
        assert_eq!(pairing_trailing.ttl_seconds, -1);
        assert_eq!(
            AdminPairingCodeRequest::parse_json(r#"{"ttl_seconds":2147483648}"#)
                .unwrap()
                .ttl_seconds,
            2_147_483_648
        );
        assert_eq!(
            AdminPairingCodeRequest::parse_json("{")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
        assert_eq!(
            serde_json::to_value(AdminPairingCodeResponse {
                pairing_code: "ABCD-EFGH".into(),
                pairing_url: "sunflower://pair?code=ABCD-EFGH&server=http%3A%2F%2Flocalhost".into(),
                expires_at: "2026-07-01T00:10:00Z".into(),
            })
            .unwrap(),
            json!({
                "pairing_code": "ABCD-EFGH",
                "pairing_url": "sunflower://pair?code=ABCD-EFGH&server=http%3A%2F%2Flocalhost",
                "expires_at": "2026-07-01T00:10:00Z"
            })
        );
    }

    #[test]
    fn scan_job_wire_shapes_match_go_contract() {
        let scan = StartScanRequest::parse_json(r#"{"roots":["/music"]}"#).unwrap();
        assert_eq!(scan.roots, vec!["/music"]);
        assert_eq!(
            StartScanRequest::parse_json("{}")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
        assert_eq!(
            StartScanRequest::parse_json("{")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
        assert_eq!(
            serde_json::to_value(StartScanResponse {
                job_id: "job-1".into(),
            })
            .unwrap(),
            json!({"job_id": "job-1"})
        );
        assert_eq!(
            serde_json::to_value(JobResponse {
                id: "job-1".into(),
                status: "completed".into(),
                processed_files: 3,
                error: String::new(),
                created_at: "2026-07-01T00:00:00Z".into(),
                updated_at: "2026-07-01T00:00:01Z".into(),
            })
            .unwrap(),
            json!({
                "id": "job-1",
                "status": "completed",
                "processed_files": 3,
                "created_at": "2026-07-01T00:00:00Z",
                "updated_at": "2026-07-01T00:00:01Z"
            })
        );
    }

    #[test]
    fn admin_cookie_upload_request_matches_go_parse_contract() {
        let req = AdminUploadCookiesRequest::parse_json(r#"{"cookies":"netscape"}"#).unwrap();
        assert_eq!(req.cookies, "netscape");
        assert_eq!(
            AdminUploadCookiesRequest::parse_json("{}").unwrap().cookies,
            ""
        );
        assert_eq!(
            AdminUploadCookiesRequest::parse_json("{")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
    }

    #[test]
    fn admin_status_wire_shape_matches_go_contract() {
        let value = serde_json::to_value(AdminStatusResponse {
            server_version: "0.3.0".into(),
            uptime_seconds: 7,
            db_status: "ok".into(),
            library_counts: AdminLibraryCountsResponse {
                songs: 1,
                albums: 2,
                artists: 3,
                playlists: 4,
            },
            cookie_status: AdminCookieStatusResponse {
                status: "unknown".into(),
                checked_at: None,
                detail: None,
            },
            devices: vec![AdminDeviceResponse {
                id: "018f3f27-0000-7000-8000-000000000001".into(),
                name: "Pixel".into(),
                platform: "android".into(),
                token_label: "Pixel".into(),
                created_at: "2026-07-01T00:00:00Z".into(),
                last_seen_at: None,
                revoked_at: None,
                revoked_reason: "".into(),
            }],
            now_playing: vec![],
            jobs: vec![],
            warnings: vec!["Library has no songs".into()],
        })
        .unwrap();
        assert_eq!(
            value,
            json!({
                "server_version": "0.3.0",
                "uptime_seconds": 7,
                "db_status": "ok",
                "library_counts": {
                    "songs": 1,
                    "albums": 2,
                    "artists": 3,
                    "playlists": 4
                },
                "cookie_status": {
                    "status": "unknown",
                    "checked_at": null,
                    "detail": null
                },
                "devices": [{
                    "id": "018f3f27-0000-7000-8000-000000000001",
                    "name": "Pixel",
                    "platform": "android",
                    "token_label": "Pixel",
                    "created_at": "2026-07-01T00:00:00Z",
                    "revoked_reason": ""
                }],
                "now_playing": [],
                "jobs": [],
                "warnings": ["Library has no songs"]
            })
        );
    }

    #[test]
    fn admin_now_playing_wire_shapes_match_go_contract() {
        assert_eq!(
            serde_json::to_value(AdminNowPlayingResponse {
                now_playing: vec![],
            })
            .unwrap(),
            json!({ "now_playing": [] })
        );

        let command = AdminNowPlayingCommandRequest::parse_json(
            r#"{"device_id":"device-1","command":"pause"}"#,
        )
        .unwrap();
        assert_eq!(command.device_id, "device-1");
        assert_eq!(command.command, "pause");

        let command_defaults = AdminNowPlayingCommandRequest::parse_json("{}").unwrap();
        assert_eq!(command_defaults.device_id, "");
        assert_eq!(command_defaults.command, "");
        let tick_defaults = NowPlayingClientMessage::parse_json("{}").unwrap();
        assert_eq!(tick_defaults.kind, "");
        assert_eq!(tick_defaults.media_id, "");
        assert_eq!(
            AdminNowPlayingCommandRequest::parse_json("{")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
        assert_eq!(
            serde_json::to_value(AdminNowPlayingCommandResponse { delivered: 2 }).unwrap(),
            json!({ "delivered": 2 })
        );

        let tick = NowPlayingClientMessage::parse_json(
            r#"{"type":"tick","queue_id":"q1","media_id":"yt:abc","title":"Song","artist":"Artist","position_ms":12345,"duration_ms":200000,"is_playing":true,"extra":42}"#,
        )
        .unwrap();
        assert_eq!(tick.kind, NOW_PLAYING_KIND_TICK);
        assert_eq!(tick.media_id, "yt:abc");
        assert!(tick.is_playing);
        assert_eq!(
            serde_json::to_value(tick).unwrap(),
            json!({
                "type": "tick",
                "queue_id": "q1",
                "media_id": "yt:abc",
                "title": "Song",
                "artist": "Artist",
                "position_ms": 12345,
                "duration_ms": 200000,
                "is_playing": true
            })
        );

        assert_eq!(
            serde_json::to_value(NowPlayingServerMessage {
                kind: NOW_PLAYING_KIND_COMMAND.into(),
                command: NOW_PLAYING_CMD_PAUSE.into(),
            })
            .unwrap(),
            json!({
                "type": "command",
                "command": "pause"
            })
        );
        assert_eq!(
            serde_json::to_value(NowPlayingStateResponse {
                device_id: "device-1".into(),
                queue_id: String::new(),
                media_id: "yt:abc".into(),
                title: String::new(),
                artist: String::new(),
                position_ms: 5000,
                duration_ms: 0,
                is_playing: true,
                updated_at: "2026-07-01T00:00:00Z".into(),
            })
            .unwrap(),
            json!({
                "device_id": "device-1",
                "media_id": "yt:abc",
                "position_ms": 5000,
                "is_playing": true,
                "updated_at": "2026-07-01T00:00:00Z"
            })
        );
    }

    #[test]
    fn admin_library_and_audit_wire_shapes_match_go_contract() {
        assert_eq!(
            serde_json::to_value(AdminLibraryStatusResponse {
                counts: AdminLibraryCountsResponse {
                    songs: 1,
                    albums: 2,
                    artists: 3,
                    playlists: 4,
                },
                jobs: vec![],
            })
            .unwrap(),
            json!({
                "counts": {
                    "songs": 1,
                    "albums": 2,
                    "artists": 3,
                    "playlists": 4
                },
                "jobs": []
            })
        );

        assert_eq!(
            serde_json::to_value(AdminAuditResponse {
                events: vec![AdminAuditEventResponse {
                    id: "018f3f27-0000-7000-8000-000000000001".into(),
                    actor_type: "admin_session".into(),
                    actor_id: String::new(),
                    event: "youtube_cookies_updated".into(),
                    target_type: "cookie_store".into(),
                    target_id: "youtube".into(),
                    metadata: json!({"cookie": "[redacted]"}),
                    created_at: "2026-07-01T00:00:00Z".into(),
                }],
            })
            .unwrap(),
            json!({
                "events": [{
                    "id": "018f3f27-0000-7000-8000-000000000001",
                    "actor_type": "admin_session",
                    "event": "youtube_cookies_updated",
                    "target_type": "cookie_store",
                    "target_id": "youtube",
                    "metadata": {"cookie": "[redacted]"},
                    "created_at": "2026-07-01T00:00:00Z"
                }]
            })
        );
    }

    #[test]
    fn register_device_request_matches_go_parse_contract() {
        let req = RegisterDeviceRequest::parse_json(
            r#"{"device_name":"Phone","platform":"android","client_version":"1","pairing_code":"123456"}"#,
        )
        .unwrap();
        assert_eq!(req.device_name, "Phone");
        assert_eq!(req.platform, "android");
        assert_eq!(req.client_version, "1");
        assert_eq!(req.pairing_code, "123456");

        let defaults = RegisterDeviceRequest::parse_json("{}").unwrap();
        assert_eq!(defaults.device_name, "");
        assert_eq!(defaults.platform, "");
        assert_eq!(defaults.client_version, "");
        assert_eq!(defaults.pairing_code, "");

        let null_fields = RegisterDeviceRequest::parse_json(
            r#"{"device_name":null,"platform":null,"client_version":null,"pairing_code":null}"#,
        )
        .unwrap();
        assert_eq!(null_fields, RegisterDeviceRequest::default());
        assert_eq!(
            RegisterDeviceRequest::parse_json("null").unwrap(),
            RegisterDeviceRequest::default()
        );
        let trailing = RegisterDeviceRequest::parse_json(
            r#"{"device_name":"Phone","pairing_code":"ABCD-EFGH"} trailing"#,
        )
        .unwrap();
        assert_eq!(trailing.device_name, "Phone");
        assert_eq!(trailing.pairing_code, "ABCD-EFGH");

        assert_eq!(
            RegisterDeviceRequest::parse_json("{")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
    }

    #[test]
    fn start_queue_request_accepts_legacy_seed_kinds_and_defaults_missing_fields() {
        let song =
            StartQueueRequest::parse_json(r#"{"seed_kind":"song","seed_id":"yt:abc"}"#).unwrap();
        assert_eq!(song.seed_kind, "song");
        assert_eq!(song.seed_id, "yt:abc");
        assert_eq!(song.title, "");
        assert_eq!(song.song_seed_video_id().unwrap(), "abc");

        let liked = StartQueueRequest::parse_json(r#"{"seed_kind":"shuffle_liked"}"#).unwrap();
        assert_eq!(liked.seed_kind, "shuffle_liked");
        assert_eq!(liked.seed_id, "");
    }

    #[test]
    fn start_queue_request_matches_go_error_codes() {
        assert_eq!(
            StartQueueRequest::parse_json("{")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
        assert_eq!(
            StartQueueRequest::parse_json(r#"{"seed_kind":"album"}"#)
                .unwrap_err()
                .legacy_error_code(),
            "invalid_seed_kind"
        );
        let empty_song =
            StartQueueRequest::parse_json(r#"{"seed_kind":"song","seed_id":"yt:"}"#).unwrap();
        assert_eq!(
            empty_song
                .song_seed_video_id()
                .unwrap_err()
                .legacy_error_code(),
            "seed_unavailable"
        );
    }

    #[test]
    fn resolve_stream_request_matches_go_parse_contract() {
        let req = ResolveStreamRequest::parse_json(
            r#"{"media_id":"yt:abc","proxy":true,"audio_quality":"high","reason":"http_403"}"#,
        )
        .unwrap();
        assert_eq!(req.media_id, "yt:abc");
        assert!(req.proxy);
        assert_eq!(req.audio_quality, "high");
        assert_eq!(req.reason, "http_403");

        let default_proxy = ResolveStreamRequest::parse_json(r#"{"media_id":"yt:abc"}"#).unwrap();
        assert!(!default_proxy.proxy);
        assert_eq!(default_proxy.audio_quality, "");
        assert_eq!(default_proxy.reason, "");

        for raw in ["{", r#"{"proxy":true}"#, r#"{"media_id":""}"#] {
            assert_eq!(
                ResolveStreamRequest::parse_json(raw)
                    .unwrap_err()
                    .legacy_error_code(),
                "invalid_request"
            );
        }
    }

    #[test]
    fn next_query_defaults_position_to_zero_like_go() {
        let id = "018f3f27-0000-7000-8000-000000000001";
        let parsed = NextQuery::parse(Some(id), None).unwrap();

        assert_eq!(parsed.queue_id.to_string(), id);
        assert_eq!(parsed.position, 0);
    }

    #[test]
    fn next_query_rejects_missing_or_invalid_queue_id_like_go() {
        assert_eq!(
            NextQuery::parse(None, None)
                .unwrap_err()
                .legacy_error_code(),
            "invalid_queue_id"
        );
        assert_eq!(
            NextQuery::parse(Some("not-a-uuid"), None)
                .unwrap_err()
                .legacy_error_code(),
            "invalid_queue_id"
        );
    }

    #[test]
    fn next_query_rejects_invalid_position_like_go() {
        let id = "018f3f27-0000-7000-8000-000000000001";
        for raw in ["-1", "abc", "9223372036854775808"] {
            assert_eq!(
                NextQuery::parse(Some(id), Some(raw))
                    .unwrap_err()
                    .legacy_error_code(),
                "invalid_position"
            );
        }
    }

    #[test]
    fn queue_response_matches_go_contract_and_omitempty() {
        let session = QueueSession {
            id: Uuid::parse_str("018f3f27-0000-7000-8000-000000000001").unwrap(),
            seed_kind: "shuffle_liked".into(),
            seed_id: String::new(),
            title: String::new(),
            version: 2,
            items: vec![QueueItem {
                media_id: MediaId::new("local:one"),
                title: "One".into(),
                artists: vec![],
                duration_ms: 1234,
            }],
        };

        let value = serde_json::to_value(QueueResponse::from(&session)).unwrap();
        assert_eq!(
            value,
            json!({
                "queue_id": "018f3f27-0000-7000-8000-000000000001",
                "seed_kind": "shuffle_liked",
                "version": 2,
                "items": [{
                    "media_id": "local:one",
                    "title": "One",
                    "duration_ms": 1234
                }]
            })
        );
    }

    #[test]
    fn song_list_response_matches_go_contract_with_plain_nullable_fields() {
        let value = serde_json::to_value(SongListResponse {
            songs: vec![
                SongListItemResponse {
                    media_id: "local:one".into(),
                    source_type: "local".into(),
                    title: "One".into(),
                    duration_ms: Some(1234),
                    album_id: Some("local:album".into()),
                    artist_name: "Artist".into(),
                    album_title: "Album".into(),
                    has_art: true,
                },
                SongListItemResponse {
                    media_id: "local:two".into(),
                    source_type: "local".into(),
                    title: "Two".into(),
                    duration_ms: None,
                    album_id: None,
                    artist_name: String::new(),
                    album_title: String::new(),
                    has_art: false,
                },
            ],
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "songs": [
                    {
                        "media_id": "local:one",
                        "source_type": "local",
                        "title": "One",
                        "duration_ms": 1234,
                        "album_id": "local:album",
                        "artist_name": "Artist",
                        "album_title": "Album",
                        "has_art": true
                    },
                    {
                        "media_id": "local:two",
                        "source_type": "local",
                        "title": "Two",
                        "duration_ms": null,
                        "album_id": null,
                        "artist_name": "",
                        "album_title": "",
                        "has_art": false
                    }
                ]
            })
        );
    }

    #[test]
    fn album_artist_and_hash_responses_match_legacy_wrappers() {
        let created_at = DateTime::parse_from_rfc3339("2026-07-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let fractional_created_at = DateTime::parse_from_rfc3339("2026-07-01T00:00:00.123400000Z")
            .unwrap()
            .with_timezone(&Utc);
        let album_value = serde_json::to_value(AlbumListResponse {
            albums: vec![AlbumListItemResponse {
                media_id: "local:album".into(),
                source_type: "local".into(),
                title: "Album".into(),
                primary_artist_id: Some("local:artist".into()),
                year: None,
                available: true,
                raw_metadata: json!({"scanner": "test"}),
                created_at: fractional_created_at,
            }],
        })
        .unwrap();
        assert_eq!(
            album_value,
            json!({
                "albums": [{
                    "media_id": "local:album",
                    "source_type": "local",
                    "title": "Album",
                    "primary_artist_id": "local:artist",
                    "year": null,
                    "available": true,
                    "raw_metadata": {"scanner": "test"},
                    "created_at": "2026-07-01T00:00:00.1234Z"
                }]
            })
        );

        let artist_value = serde_json::to_value(ArtistListResponse {
            artists: vec![ArtistListItemResponse {
                media_id: "local:artist".into(),
                source_type: "local".into(),
                name: "Artist".into(),
                available: true,
                raw_metadata: json!({}),
                created_at,
            }],
        })
        .unwrap();
        assert_eq!(
            artist_value,
            json!({
                "artists": [{
                    "media_id": "local:artist",
                    "source_type": "local",
                    "name": "Artist",
                    "available": true,
                    "raw_metadata": {},
                    "created_at": "2026-07-01T00:00:00Z"
                }]
            })
        );

        assert_eq!(
            serde_json::to_value(SongHashResponse {
                media_id: "local:one".into(),
                sha256: "abc123".into(),
                bytes: 42,
            })
            .unwrap(),
            json!({
                "media_id": "local:one",
                "sha256": "abc123",
                "bytes": 42
            })
        );
    }

    #[test]
    fn download_requests_and_responses_match_legacy_contract() {
        let req = RegisterDownloadRequest::parse_json(
            r#"{"media_id":"local:one","local_path":"/data/one.mp3","bytes":42}"#,
        )
        .unwrap();
        assert_eq!(req.media_id, "local:one");
        assert_eq!(req.local_path, "/data/one.mp3");
        assert_eq!(req.bytes, 42);

        for raw in ["{", "{}", r#"{"media_id":""}"#] {
            assert_eq!(
                RegisterDownloadRequest::parse_json(raw)
                    .unwrap_err()
                    .legacy_error_code(),
                "invalid_request"
            );
        }

        let value = serde_json::to_value(DownloadListResponse {
            downloads: vec![
                DownloadListItemResponse {
                    media_id: "local:one".into(),
                    bytes: 42,
                },
                DownloadListItemResponse {
                    media_id: "local:two".into(),
                    bytes: 0,
                },
            ],
        })
        .unwrap();
        assert_eq!(
            value,
            json!({
                "downloads": [
                    {"media_id": "local:one", "bytes": 42},
                    {"media_id": "local:two"}
                ]
            })
        );
    }

    #[test]
    fn home_response_matches_legacy_shape_and_omitempty() {
        let value = serde_json::to_value(HomeResponse {
            sections: vec![HomeSectionResponse {
                id: "quick_picks".into(),
                title: "Quick Picks".into(),
                kind: "quick_picks".into(),
                seed: None,
                items: vec![HomeItemResponse {
                    media_id: "local:one".into(),
                    title: "One".into(),
                    artists: vec![],
                    album_id: None,
                    duration_ms: 0,
                    source: "local".into(),
                    thumbnail_url: None,
                    score: 0.0,
                }],
            }],
            chips: vec![],
            stale: false,
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "sections": [{
                    "id": "quick_picks",
                    "title": "Quick Picks",
                    "kind": "quick_picks",
                    "items": [{
                        "media_id": "local:one",
                        "title": "One",
                        "source": "local",
                        "score": 0.0
                    }]
                }],
                "stale": false
            })
        );
    }

    #[test]
    fn like_request_and_response_match_legacy_contract() {
        let req = LikeRequest::parse_json(
            r#"{"media_id":"local:one","liked":true,"occurred_at":"2026-07-01T00:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(req.media_id, "local:one");
        assert!(req.liked);
        assert_eq!(req.occurred_at.as_deref(), Some("2026-07-01T00:00:00Z"));

        for raw in ["{", "{}", r#"{"media_id":""}"#] {
            assert_eq!(
                LikeRequest::parse_json(raw)
                    .unwrap_err()
                    .legacy_error_code(),
                "invalid_request"
            );
        }

        assert_eq!(
            serde_json::to_value(LikeResponse {
                media_id: "local:one".into(),
                liked: false,
            })
            .unwrap(),
            json!({"media_id": "local:one", "liked": false})
        );
    }

    #[test]
    fn search_response_matches_legacy_shape() {
        let value = serde_json::to_value(SearchResponse {
            query: "one".into(),
            songs: vec![SearchSongResponse {
                media_id: "local:one".into(),
                source: "local".into(),
                title: "One".into(),
                artists: vec![],
                thumbnail_url: None,
                duration_ms: 0,
            }],
            albums: vec![SearchAlbumResponse {
                browse_id: "local:album".into(),
                title: "Album".into(),
                artists: vec!["Artist".into()],
                thumbnail_url: None,
            }],
            artists: vec![SearchArtistResponse {
                browse_id: "local:artist".into(),
                name: "Artist".into(),
                thumbnail_url: None,
            }],
            continuation: None,
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "query": "one",
                "songs": [{
                    "media_id": "local:one",
                    "source": "local",
                    "title": "One",
                    "duration_ms": 0
                }],
                "albums": [{
                    "browse_id": "local:album",
                    "title": "Album",
                    "artists": ["Artist"]
                }],
                "artists": [{
                    "browse_id": "local:artist",
                    "name": "Artist"
                }]
            })
        );
    }

    #[test]
    fn events_response_matches_legacy_shape() {
        let req = EventsRequest::parse_json(
            r#"{"events":[{"event_id":"e1","kind":"play","media_id":"local:one","total_played_ms":30000,"duration_ms":120000}]}"#,
        )
        .unwrap();
        assert_eq!(req.events.len(), 1);
        assert_eq!(req.events[0].media_id, "local:one");

        assert_eq!(
            EventsRequest::parse_json("{")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
        assert_eq!(
            EventsRequest::parse_json(
                r#"{"events":[{"event_id":"e1","kind":"play","media_id":"local:one","total_played_ms":2147483648,"duration_ms":120000}]}"#,
            )
            .unwrap_err()
            .legacy_error_code(),
            "invalid_request"
        );

        assert_eq!(
            serde_json::to_value(EventsResponse {
                results: vec![
                    EventResultResponse {
                        event_id: "e1".into(),
                        accepted: true,
                        reason: None,
                    },
                    EventResultResponse {
                        event_id: "e2".into(),
                        accepted: false,
                        reason: Some("missing_media_id".into()),
                    },
                ],
            })
            .unwrap(),
            json!({
                "results": [
                    {"event_id": "e1", "accepted": true},
                    {"event_id": "e2", "accepted": false, "reason": "missing_media_id"}
                ]
            })
        );
    }

    #[test]
    fn impressions_request_and_response_match_legacy_shape() {
        let req = ImpressionsRequest::parse_json(
            r#"{"impressions":[{"section_id":"quick_picks","source":"local","seed_id":"seed","media_id":"local:one","position":2}]}"#,
        )
        .unwrap();
        assert_eq!(req.impressions.len(), 1);
        assert_eq!(req.impressions[0].media_id, "local:one");

        assert_eq!(
            ImpressionsRequest::parse_json("{")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
        assert_eq!(
            ImpressionsRequest::parse_json(
                r#"{"impressions":[{"section_id":"quick_picks","source":"local","seed_id":"seed","media_id":"local:one","position":2147483648}]}"#,
            )
            .unwrap_err()
            .legacy_error_code(),
            "invalid_request"
        );
        assert_eq!(
            serde_json::to_value(ImpressionsResponse { written: 1 }).unwrap(),
            json!({"written": 1})
        );
    }

    #[test]
    fn playlist_responses_match_legacy_shape_and_omitempty() {
        let list_value = serde_json::to_value(PlaylistListResponse {
            playlists: vec![PlaylistResponse {
                id: "018f3f27-0000-7000-8000-000000000001".into(),
                title: "My Mix".into(),
                source_type: "local".into(),
                version: 1,
                items: vec![],
            }],
        })
        .unwrap();
        assert_eq!(
            list_value,
            json!({
                "playlists": [{
                    "id": "018f3f27-0000-7000-8000-000000000001",
                    "title": "My Mix",
                    "source_type": "local",
                    "version": 1
                }]
            })
        );

        let detail_value = serde_json::to_value(PlaylistResponse {
            id: "018f3f27-0000-7000-8000-000000000001".into(),
            title: "My Mix".into(),
            source_type: "local".into(),
            version: 2,
            items: vec![PlaylistItemResponse {
                position: 0,
                media_id: "local:one".into(),
                title: "One".into(),
                artist_name: "Artist".into(),
                album_id: String::new(),
                duration_ms: 0,
            }],
        })
        .unwrap();
        assert_eq!(
            detail_value,
            json!({
                "id": "018f3f27-0000-7000-8000-000000000001",
                "title": "My Mix",
                "source_type": "local",
                "version": 2,
                "items": [{
                    "position": 0,
                    "media_id": "local:one",
                    "title": "One",
                    "artist_name": "Artist"
                }]
            })
        );

        assert_eq!(
            PlaylistTitleRequest::parse_json("{}")
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
        assert_eq!(
            AddPlaylistItemRequest::parse_json(r#"{"media_id":""}"#)
                .unwrap_err()
                .legacy_error_code(),
            "invalid_request"
        );
    }

    #[test]
    fn next_response_preserves_legacy_fields_and_adds_documented_stream_lookahead() {
        let value = serde_json::to_value(NextResponse {
            queue_id: "018f3f27-0000-7000-8000-000000000001".into(),
            position: 0,
            current: Some(ResolvedStreamResponse {
                media_id: "yt:abc".into(),
                title: "Song".into(),
                artists: vec!["Artist".into()],
                duration_ms: 200000,
                source: "youtube".into(),
                stream_url: "https://example.invalid/audio".into(),
                stream_expires_at: Some("2026-07-01T00:00:00Z".into()),
                itag: Some(251),
                mime_type: "audio/webm".into(),
                content_length: None,
                loudness_db: Some(-3.5),
                playback_tracking_url: None,
                metadata: json!({ "itag": 251, "bitrate": 160000 }),
            }),
            lookahead: vec![ResolvedStreamResponse {
                media_id: "yt:def".into(),
                title: "Next".into(),
                artists: vec!["Artist".into()],
                duration_ms: 180000,
                source: "youtube".into(),
                stream_url: "https://example.invalid/next".into(),
                stream_expires_at: Some("2026-07-01T01:00:00Z".into()),
                itag: Some(251),
                mime_type: "audio/webm".into(),
                content_length: None,
                loudness_db: None,
                playback_tracking_url: None,
                metadata: json!({ "itag": 251 }),
            }],
            continuation: None,
            automix: vec![],
            queue_version: 7,
            has_more: true,
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "queue_id": "018f3f27-0000-7000-8000-000000000001",
                "position": 0,
                "current": {
                    "media_id": "yt:abc",
                    "title": "Song",
                    "artists": ["Artist"],
                    "duration_ms": 200000,
                    "source": "youtube",
                    "stream_url": "https://example.invalid/audio",
                    "stream_expires_at": "2026-07-01T00:00:00Z",
                    "itag": 251,
                    "mime_type": "audio/webm",
                    "loudness_db": -3.5,
                    "metadata": {
                        "itag": 251,
                        "bitrate": 160000
                    }
                },
                "lookahead": [{
                    "media_id": "yt:def",
                    "title": "Next",
                    "artists": ["Artist"],
                    "duration_ms": 180000,
                    "source": "youtube",
                    "stream_url": "https://example.invalid/next",
                    "stream_expires_at": "2026-07-01T01:00:00Z",
                    "itag": 251,
                    "mime_type": "audio/webm",
                    "metadata": {
                        "itag": 251
                    }
                }],
                "queue_version": 7,
                "has_more": true
            })
        );

        let object = value.as_object().unwrap();
        assert!(!object.contains_key("recommender_source"));
    }

    #[test]
    fn next_response_serializes_empty_legacy_fields_like_go() {
        let value = serde_json::to_value(NextResponse {
            queue_id: "018f3f27-0000-7000-8000-000000000001".into(),
            position: 9,
            current: None,
            lookahead: vec![],
            continuation: None,
            automix: vec![],
            queue_version: 3,
            has_more: false,
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "queue_id": "018f3f27-0000-7000-8000-000000000001",
                "position": 9,
                "current": null,
                "lookahead": [],
                "queue_version": 3,
                "has_more": false
            })
        );
    }

    #[test]
    fn local_resolved_stream_serializes_null_expiry_like_go() {
        let value: Value = serde_json::to_value(ResolvedStreamResponse {
            media_id: "local:one".into(),
            title: String::new(),
            artists: vec![],
            duration_ms: 0,
            source: "local".into(),
            stream_url: "http://127.0.0.1:8080/api/v1/library/songs/local:one/stream".into(),
            stream_expires_at: None,
            itag: None,
            mime_type: String::new(),
            content_length: None,
            loudness_db: None,
            playback_tracking_url: None,
            metadata: Value::Null,
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "media_id": "local:one",
                "source": "local",
                "stream_url": "http://127.0.0.1:8080/api/v1/library/songs/local:one/stream",
                "stream_expires_at": null
            })
        );
    }
}
