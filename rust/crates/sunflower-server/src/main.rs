use std::{
    borrow::Cow,
    collections::HashMap,
    env, fs,
    future::Future,
    io::{self, Read, SeekFrom},
    net::SocketAddr,
    path::Path as FsPath,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{ConnectInfo, Path, State, ws::WebSocketUpgrade},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use chrono::{DateTime, SecondsFormat, Utc};
use futures_util::{future::BoxFuture, stream};
use jobs::JobRegistry;
use now_playing::NowPlayingHub;
use rand::RngCore;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use stream_proxy::{ProxySigner, StreamProxy};
use sunflower_core::{
    AddPlaylistItemRequest, AdminAuditResponse, AdminDevicesResponse, AdminLibraryStatusResponse,
    AdminLoginRequest, AdminLoginResponse, AdminMeResponse, AdminNowPlayingCommandRequest,
    AdminNowPlayingCommandResponse, AdminNowPlayingResponse, AdminPairingCodeRequest,
    AdminRevokeDeviceRequest, AdminStatusResponse, AdminUploadCookiesRequest, AlbumListResponse,
    ArtistListResponse, DEFAULT_LOOKAHEAD_COUNT, DownloadListResponse, EventResultResponse,
    EventsRequest, EventsResponse, HealthzResponse, HomeItemResponse, HomeResponse,
    HomeSectionResponse, ImpressionsRequest, ImpressionsResponse, LegacyRequestError, LikeRequest,
    LikeResponse, LocalRecommendationEngine, MediaId, NOW_PLAYING_CMD_PAUSE, NOW_PLAYING_CMD_PLAY,
    NOW_PLAYING_CMD_SKIP_NEXT, NOW_PLAYING_CMD_SKIP_PREV, NOW_PLAYING_SUBPROTOCOL, NextQuery,
    NextResponse, OwnerSetupRequest, PlaylistListResponse, PlaylistTitleRequest, QueueResponse,
    RecommendationSource, RegisterDeviceRequest, RegisterDeviceResponse, RegisterDownloadRequest,
    ResolveStreamRequest, ResolvedStream, ResolvedStreamResponse, SearchAlbumResponse,
    SearchArtistResponse, SearchResponse, SearchSongResponse, SetupStatusResponse,
    SongHashResponse, SongListResponse, StartQueueRequest, StartScanRequest, StartScanResponse,
    build_automix, next_window,
};
use sunflower_storage_postgres::{
    AdminSession, AuthStoreError, AuthenticatedDevice, IdempotencyLogInsert, IdempotencyLogRecord,
    PostgresStore, SongFileLookup, verify_admin_csrf,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use uuid::Uuid;

const DEFAULT_SERVER_VERSION: &str = "0.3.0";
#[cfg(test)]
const DEFAULT_SETUP_TOKEN: &str = "sunflower-test-setup-token";
const DEFAULT_DATABASE_URL: &str = "postgres://postgres@localhost:5432/sunflower?sslmode=disable";
const DEFAULT_LISTEN_ADDR: &str = ":8080";
const ADMIN_COOKIE_NAME: &str = "sf_admin";
const ADMIN_CSRF_COOKIE_NAME: &str = "sf_admin_csrf";
const HOME_QUICK_PICKS_LIMIT: usize = 20;
const HOME_LOCAL_CANDIDATE_LIMIT: i64 = 100;
const DAILY_DISCOVER_SEEDS: usize = 5;
const DAILY_DISCOVER_LIMIT: usize = 25;
const SIMILAR_ARTIST_SECTIONS: i64 = 3;
const SIMILAR_ARTIST_LIMIT: usize = 20;
const YT_HOME_BROWSE_ID: &str = "FEmusic_home";
const YT_HOME_LIMIT: usize = 30;
const COMMUNITY_PLAYLIST_LIMIT: usize = 15;
const MIN_QUEUE_ITEMS: usize = 10;
const FILE_STREAM_CHUNK_SIZE: usize = 64 * 1024;
const LEGACY_ALLOW_GET: &[&str] = &["GET"];
const LEGACY_ALLOW_POST: &[&str] = &["POST"];
const LEGACY_ALLOW_GET_POST: &[&str] = &["GET", "POST"];
const LEGACY_ALLOW_GET_PATCH_DELETE: &[&str] = &["GET", "PATCH", "DELETE"];
const LEGACY_ALLOW_DELETE: &[&str] = &["DELETE"];
const LEGACY_DYNAMIC_ROUTES: &[(&str, &[&str])] = &[
    ("/admin/devices/:id/revoke", LEGACY_ALLOW_POST),
    ("/api/v1/admin/devices/:id/revoke", LEGACY_ALLOW_POST),
    ("/api/v1/queue/:id", LEGACY_ALLOW_GET),
    ("/api/v1/playlists/:id", LEGACY_ALLOW_GET_PATCH_DELETE),
    ("/api/v1/playlists/:id/items", LEGACY_ALLOW_POST),
    ("/api/v1/playlists/:id/items/:media_id", LEGACY_ALLOW_DELETE),
    ("/api/v1/jobs/:id", LEGACY_ALLOW_GET),
    (
        "/api/v1/library/albums/:album_media_id/art",
        LEGACY_ALLOW_GET,
    ),
    ("/api/v1/library/songs/:media_id/hash", LEGACY_ALLOW_GET),
    ("/api/v1/library/songs/:media_id/stream", LEGACY_ALLOW_GET),
    ("/api/v1/devices/:id/downloads", LEGACY_ALLOW_GET_POST),
    (
        "/api/v1/devices/:id/downloads/:media_id",
        LEGACY_ALLOW_DELETE,
    ),
];
const ADMIN_CSS: &str = include_str!("../assets/admin/admin.css");
const ADMIN_JS: &str = include_str!("../assets/admin/admin.js");
const ADMIN_STATIC_DIR_LISTING: &str = "<!doctype html>\n<meta name=\"viewport\" content=\"width=device-width\">\n<pre>\n<a href=\"admin.css\">admin.css</a>\n<a href=\"admin.js\">admin.js</a>\n</pre>\n";

mod auth;
mod cookies;
mod file_response;
mod forms;
mod innertube;
mod jobs;
mod legacy_http;
mod now_playing;
mod router;
mod routes;
mod runtime;
mod state;
mod stream_proxy;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub(crate) use auth::*;
pub(crate) use cookies::*;
pub(crate) use file_response::*;
pub(crate) use forms::*;
pub(crate) use legacy_http::*;
pub(crate) use router::*;
pub(crate) use routes::*;
pub(crate) use runtime::*;
pub(crate) use state::*;
#[cfg(test)]
pub(crate) use test_support::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database_url = configured_database_url(env::var("DATABASE_URL").ok());
    let store = PostgresStore::connect(&database_url)
        .await
        .context("connect Postgres")?;
    store
        .migrate()
        .await
        .context("run embedded legacy migrations")?;
    let store = Some(store);

    if let Some(store) = store.clone() {
        start_idempotency_gc(store);
    }

    let auth_mode = if store.is_some() {
        AuthMode::Database
    } else {
        AuthMode::RejectAllTokens
    };
    let cookie_key = parse_cookie_key_env().context("parse SUNFLOWER_COOKIE_KEY")?;
    let cookie_file = configured_cookie_file();
    let cookies_configured = cookie_key.is_some() || cookie_file.is_some();
    let proxy_youtube = should_proxy_youtube(&stream_proxy_mode(), cookies_configured);
    let stream_proxy = Arc::new(StreamProxy::new(ProxySigner::new(
        parse_stream_proxy_key_env().context("parse SUNFLOWER_STREAM_PROXY_KEY")?,
    )));
    let yt = default_innertube_backend(store.clone(), cookie_key, cookie_file);
    let app = router_with_config(
        RouterBuildConfig::new(
            auth_mode,
            store,
            configured_data_dir(env::var("DATA_DIR").ok()),
            runtime_setup_token().context("configure setup token")?,
            env::var("SUNFLOWER_PUBLIC_BASE_URL").unwrap_or_default(),
            cookie_key,
        )
        .with_hub(Some(Arc::new(NowPlayingHub::default())))
        .with_proxy(Some(stream_proxy))
        .with_proxy_youtube(proxy_youtube)
        .with_yt(yt)
        .with_dev_open_registration(runtime_dev_open_registration()),
    );

    let listen_addr = configured_listen_addr(env::var("LISTEN_ADDR").ok());
    let listener = bind_listen_addr(&listen_addr)
        .await
        .with_context(|| format!("bind server on {listen_addr}"))?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("serve")?;
    Ok(())
}
