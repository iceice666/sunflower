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

mod innertube;
mod jobs;
mod now_playing;
mod stream_proxy;

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

fn start_idempotency_gc(store: PostgresStore) {
    tokio::spawn(async move {
        let _ = store.gc_expired_idempotency_log().await;
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            let _ = store.gc_expired_idempotency_log().await;
        }
    });
}

fn configured_database_url(value: Option<String>) -> String {
    non_empty_or(value, DEFAULT_DATABASE_URL)
}

fn configured_listen_addr(value: Option<String>) -> String {
    non_empty_or(value, DEFAULT_LISTEN_ADDR)
}

fn configured_data_dir(value: Option<String>) -> String {
    non_empty_or(value, "./data")
}

async fn bind_listen_addr(listen_addr: &str) -> std::io::Result<tokio::net::TcpListener> {
    match go_wildcard_socket_addr(listen_addr) {
        Some(addr) => tokio::net::TcpListener::bind(addr).await,
        None => tokio::net::TcpListener::bind(listen_addr).await,
    }
}

fn go_wildcard_socket_addr(listen_addr: &str) -> Option<SocketAddr> {
    let port = listen_addr.strip_prefix(':')?;
    if port.is_empty() || port.contains(':') {
        return None;
    }
    let port = port.parse::<u16>().ok()?;
    Some(SocketAddr::from(([0, 0, 0, 0], port)))
}

fn runtime_setup_token() -> anyhow::Result<String> {
    configured_setup_token(env::var("SUNFLOWER_SETUP_TOKEN").ok())
}

fn configured_setup_token(value: Option<String>) -> anyhow::Result<String> {
    match value.filter(|token| !token.is_empty()) {
        Some(token) => Ok(token),
        None => generate_setup_token(),
    }
}

fn runtime_dev_open_registration() -> bool {
    configured_dev_open_registration(
        env::var("SUNFLOWER_ENV").ok(),
        env::var("SUNFLOWER_DEV_OPEN_REGISTRATION").ok(),
    )
}

fn configured_dev_open_registration(env_value: Option<String>, flag_value: Option<String>) -> bool {
    env_value.as_deref() == Some("development") && flag_value.as_deref() == Some("1")
}

fn generate_setup_token() -> anyhow::Result<String> {
    let mut token = [0u8; 16];
    rand::thread_rng().try_fill_bytes(&mut token)?;
    Ok(hex_lower_bytes(&token))
}

fn non_empty_or(value: Option<String>, fallback: &str) -> String {
    value
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn parse_stream_proxy_key_env() -> anyhow::Result<Vec<u8>> {
    match env::var("SUNFLOWER_STREAM_PROXY_KEY") {
        Ok(raw) if !raw.is_empty() => {
            if raw.len() < 64 || raw.len() % 2 != 0 {
                anyhow::bail!(
                    "SUNFLOWER_STREAM_PROXY_KEY must be at least 64 hex chars (32 bytes)"
                );
            }
            let mut key = Vec::with_capacity(raw.len() / 2);
            for chunk in raw.as_bytes().chunks_exact(2) {
                let Some(hi) = hex_value(chunk[0]) else {
                    anyhow::bail!(
                        "SUNFLOWER_STREAM_PROXY_KEY must be at least 64 hex chars (32 bytes)"
                    );
                };
                let Some(lo) = hex_value(chunk[1]) else {
                    anyhow::bail!(
                        "SUNFLOWER_STREAM_PROXY_KEY must be at least 64 hex chars (32 bytes)"
                    );
                };
                key.push((hi << 4) | lo);
            }
            Ok(key)
        }
        _ => Ok(random_stream_proxy_key()),
    }
}

fn random_stream_proxy_key() -> Vec<u8> {
    let mut key = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

fn parse_cookie_key_env() -> anyhow::Result<Option<[u8; 32]>> {
    let raw = match env::var("SUNFLOWER_COOKIE_KEY") {
        Ok(raw) if !raw.is_empty() => raw,
        _ => return Ok(None),
    };
    if raw.len() != 64 {
        anyhow::bail!("SUNFLOWER_COOKIE_KEY must be 64 hex chars (32 bytes)");
    }
    let mut key = [0u8; 32];
    for (index, chunk) in raw.as_bytes().chunks_exact(2).enumerate() {
        let Some(hi) = hex_value(chunk[0]) else {
            anyhow::bail!("SUNFLOWER_COOKIE_KEY must be 64 hex chars (32 bytes)");
        };
        let Some(lo) = hex_value(chunk[1]) else {
            anyhow::bail!("SUNFLOWER_COOKIE_KEY must be 64 hex chars (32 bytes)");
        };
        key[index] = (hi << 4) | lo;
    }
    Ok(Some(key))
}

fn configured_cookie_file() -> Option<String> {
    configured_cookie_file_from(env::var("SUNFLOWER_YT_COOKIE_FILE").ok())
}

fn configured_cookie_file_from(value: Option<String>) -> Option<String> {
    Some(non_empty_or(value, ".env.innertube_cookie"))
}

fn stream_proxy_mode() -> String {
    env::var("SUNFLOWER_STREAM_PROXY").unwrap_or_else(|_| "auto".into())
}

fn should_proxy_youtube(mode: &str, cookies_configured: bool) -> bool {
    match mode.trim().to_ascii_lowercase().as_str() {
        "always" => true,
        "never" => false,
        _ => cookies_configured,
    }
}

fn default_innertube_backend(
    store: Option<PostgresStore>,
    cookie_key: Option<[u8; 32]>,
    cookie_file: Option<String>,
) -> Option<Arc<dyn innertube::InnerTubeBackend>> {
    if matches!(
        env::var("SUNFLOWER_INNERTUBE_DISABLED").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "True")
    ) {
        return None;
    }
    let locale = innertube::Locale {
        hl: env::var("SUNFLOWER_YT_HL").unwrap_or_else(|_| "en".into()),
        gl: env::var("SUNFLOWER_YT_GL").unwrap_or_else(|_| "US".into()),
    };
    let mut client = match env::var("SUNFLOWER_INNERTUBE_BASE_URL") {
        Ok(base_url) if !base_url.is_empty() => {
            innertube::HttpInnerTubeClient::new(base_url, locale)
        }
        _ => innertube::HttpInnerTubeClient::production(locale),
    }
    .ok()?;
    if cookie_key.is_some() || cookie_file.is_some() {
        client = client.with_cookie_provider(Arc::new(YoutubeCookieProvider::new(
            store,
            cookie_key,
            cookie_file,
        )));
    }
    Some(Arc::new(client))
}

struct YoutubeCookieProvider {
    store: Option<PostgresStore>,
    key: Option<[u8; 32]>,
    file: Option<String>,
    cache: Mutex<Option<(SystemTime, Option<String>)>>,
}

impl YoutubeCookieProvider {
    fn new(store: Option<PostgresStore>, key: Option<[u8; 32]>, file: Option<String>) -> Self {
        Self {
            store,
            key,
            file,
            cache: Mutex::new(None),
        }
    }

    async fn load_cookie_header(&self) -> Option<String> {
        if let (Some(store), Some(key)) = (&self.store, self.key)
            && let Ok(Some(raw)) = store.load_first_youtube_cookies(key).await
            && let Some(header) = parse_youtube_cookie_header(&raw)
        {
            return Some(header);
        }
        let Some(file) = &self.file else {
            return None;
        };
        std::fs::read(file)
            .ok()
            .and_then(|raw| parse_youtube_cookie_header(&raw))
    }
}

impl innertube::CookieProvider for YoutubeCookieProvider {
    fn cookie_header<'a>(&'a self) -> BoxFuture<'a, Option<String>> {
        Box::pin(async move {
            if let Some((fetched_at, cached)) =
                self.cache.lock().ok().and_then(|cache| cache.clone())
                && fetched_at.elapsed().unwrap_or_default() < Duration::from_secs(60)
            {
                return cached;
            }
            let loaded = self.load_cookie_header().await;
            if let Ok(mut cache) = self.cache.lock() {
                *cache = Some((SystemTime::now(), loaded.clone()));
            }
            loaded
        })
    }
}

fn parse_youtube_cookie_header(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("***INNERTUBE COOKIE***")
            && let Some((_, value)) = rest.split_once('=')
        {
            return normalize_cookie_header(value);
        }
    }
    if text.contains('\t') {
        return parse_netscape_cookie_header(&text);
    }
    let trimmed = text.trim();
    if trimmed.contains('=') && !trimmed.contains('\n') {
        return normalize_cookie_header(trimmed);
    }
    None
}

fn normalize_cookie_header(raw: &str) -> Option<String> {
    let cookies = parse_cookie_header_pairs(raw)?;
    Some(
        cookies
            .into_iter()
            .map(|(name, value, quoted)| request_cookie_pair(&name, &value, quoted))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

fn parse_netscape_cookie_header(raw: &str) -> Option<String> {
    let parts = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let fields = line.splitn(7, '\t').collect::<Vec<_>>();
            (fields.len() >= 7).then(|| request_cookie_pair(fields[5], fields[6], false))
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("; "))
}

fn parse_cookie_header_pairs(raw: &str) -> Option<Vec<(String, String, bool)>> {
    let trimmed = trim_http_space(raw);
    if trimmed.is_empty() {
        return None;
    }
    let mut cookies = Vec::new();
    for part in trimmed.split(';') {
        let part = trim_http_space(part);
        let (name, value) = part.split_once('=')?;
        if !is_cookie_token(name) {
            return None;
        }
        let (value, quoted) = parse_cookie_header_value(value)?;
        cookies.push((name.to_string(), value, quoted));
    }
    (!cookies.is_empty()).then_some(cookies)
}

fn parse_cookie_header_value(raw: &str) -> Option<(String, bool)> {
    let mut value = raw;
    let quoted = value.len() > 1 && value.starts_with('"') && value.ends_with('"');
    if quoted {
        value = &value[1..value.len() - 1];
    }
    value
        .bytes()
        .all(valid_cookie_value_byte)
        .then(|| (value.to_string(), quoted))
}

fn request_cookie_pair(name: &str, value: &str, quoted: bool) -> String {
    format!(
        "{}={}",
        sanitize_cookie_name(name),
        sanitize_cookie_value(value, quoted)
    )
}

fn sanitize_cookie_name(name: &str) -> String {
    name.replace(['\n', '\r'], "-")
}

fn sanitize_cookie_value(value: &str, quoted: bool) -> String {
    let sanitized = value
        .bytes()
        .filter(|byte| valid_cookie_value_byte(*byte))
        .map(char::from)
        .collect::<String>();
    if quoted || sanitized.contains([' ', ',']) {
        format!("\"{sanitized}\"")
    } else {
        sanitized
    }
}

fn valid_cookie_value_byte(byte: u8) -> bool {
    (0x20..0x7f).contains(&byte) && byte != b'"' && byte != b';' && byte != b'\\'
}

fn is_cookie_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii()
                && byte > 0x20
                && byte < 0x7f
                && !matches!(
                    byte,
                    b'(' | b')'
                        | b'<'
                        | b'>'
                        | b'@'
                        | b','
                        | b';'
                        | b':'
                        | b'\\'
                        | b'"'
                        | b'/'
                        | b'['
                        | b']'
                        | b'?'
                        | b'='
                        | b'{'
                        | b'}'
                )
        })
}

fn trim_http_space(value: &str) -> &str {
    value.trim_matches(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
}

#[cfg(test)]
fn router_with_store(store: Option<PostgresStore>) -> Router {
    let auth_mode = if store.is_some() {
        AuthMode::Database
    } else {
        AuthMode::RejectAllTokens
    };
    router_with_state(auth_mode, store)
}

#[cfg(test)]
fn router_with_auth(auth_mode: AuthMode) -> Router {
    router_with_state(auth_mode, None)
}

#[cfg(test)]
fn router_with_state(auth_mode: AuthMode, store: Option<PostgresStore>) -> Router {
    router_with_state_and_data_dir(
        auth_mode,
        store,
        configured_data_dir(env::var("DATA_DIR").ok()),
    )
}

#[cfg(test)]
fn router_with_state_and_data_dir(
    auth_mode: AuthMode,
    store: Option<PostgresStore>,
    data_dir: impl Into<String>,
) -> Router {
    router_with_state_and_config(auth_mode, store, data_dir, DEFAULT_SETUP_TOKEN, "", None)
}

#[cfg(test)]
fn router_with_state_and_config(
    auth_mode: AuthMode,
    store: Option<PostgresStore>,
    data_dir: impl Into<String>,
    setup_token: impl Into<String>,
    public_base_url: impl Into<String>,
    cookie_key: Option<[u8; 32]>,
) -> Router {
    router_with_state_and_config_and_hub(
        auth_mode,
        store,
        data_dir,
        setup_token,
        public_base_url,
        cookie_key,
        Some(Arc::new(NowPlayingHub::default())),
    )
}

#[cfg(test)]
fn router_with_state_and_config_and_hub(
    auth_mode: AuthMode,
    store: Option<PostgresStore>,
    data_dir: impl Into<String>,
    setup_token: impl Into<String>,
    public_base_url: impl Into<String>,
    cookie_key: Option<[u8; 32]>,
    hub: Option<Arc<NowPlayingHub>>,
) -> Router {
    router_with_config(
        RouterBuildConfig::new(
            auth_mode,
            store,
            data_dir,
            setup_token,
            public_base_url,
            cookie_key,
        )
        .with_hub(hub)
        .with_proxy(Some(Arc::new(StreamProxy::new(ProxySigner::new(
            random_stream_proxy_key(),
        ))))),
    )
}

#[cfg(test)]
fn test_router_config(auth_mode: AuthMode, store: Option<PostgresStore>) -> RouterBuildConfig {
    RouterBuildConfig::new(auth_mode, store, "./data", DEFAULT_SETUP_TOKEN, "", None)
}

struct RouterBuildConfig {
    auth_mode: AuthMode,
    store: Option<PostgresStore>,
    data_dir: String,
    setup_token: String,
    public_base_url: String,
    cookie_key: Option<[u8; 32]>,
    hub: Option<Arc<NowPlayingHub>>,
    proxy: Option<Arc<StreamProxy>>,
    proxy_youtube: bool,
    yt: Option<Arc<dyn innertube::InnerTubeBackend>>,
    dev_open_registration: bool,
}

impl RouterBuildConfig {
    fn new(
        auth_mode: AuthMode,
        store: Option<PostgresStore>,
        data_dir: impl Into<String>,
        setup_token: impl Into<String>,
        public_base_url: impl Into<String>,
        cookie_key: Option<[u8; 32]>,
    ) -> Self {
        Self {
            auth_mode,
            store,
            data_dir: data_dir.into(),
            setup_token: setup_token.into(),
            public_base_url: public_base_url.into(),
            cookie_key,
            hub: None,
            proxy: None,
            proxy_youtube: false,
            yt: None,
            dev_open_registration: false,
        }
    }

    fn with_hub(mut self, hub: Option<Arc<NowPlayingHub>>) -> Self {
        self.hub = hub;
        self
    }

    fn with_proxy(mut self, proxy: Option<Arc<StreamProxy>>) -> Self {
        self.proxy = proxy;
        self
    }

    fn with_proxy_youtube(mut self, proxy_youtube: bool) -> Self {
        self.proxy_youtube = proxy_youtube;
        self
    }

    fn with_yt(mut self, yt: Option<Arc<dyn innertube::InnerTubeBackend>>) -> Self {
        self.yt = yt;
        self
    }

    fn with_dev_open_registration(mut self, dev_open_registration: bool) -> Self {
        self.dev_open_registration = dev_open_registration;
        self
    }
}

fn router_with_config(config: RouterBuildConfig) -> Router {
    let legacy_routes = LegacyRouteConfig {
        streams_proxy_enabled: config.proxy.is_some(),
    };
    let state = AppState {
        auth_mode: config.auth_mode,
        server_version: DEFAULT_SERVER_VERSION.to_string(),
        setup_token: config.setup_token,
        public_base_url: config.public_base_url,
        cookie_key: config.cookie_key,
        hub: config.hub,
        proxy: config.proxy,
        proxy_youtube: config.proxy_youtube,
        yt: config.yt,
        jobs: Arc::new(JobRegistry::default()),
        started_at: SystemTime::now(),
        data_dir: config.data_dir,
        dev_open_registration: config.dev_open_registration,
        setup_limiter: RateLimiter::new(10, Duration::from_secs(10 * 60)),
        admin_login_limiter: RateLimiter::new(8, Duration::from_secs(5 * 60)),
        pairing_limiter: RateLimiter::new(20, Duration::from_secs(10 * 60)),
        store: config.store,
    };
    let router = Router::new()
        .route("/healthz", get(healthz))
        .route("/admin/static/", get(admin_static_dir_listing))
        .route("/admin/static/*path", get(admin_static_asset))
        .route("/admin/login", get(admin_login_page).post(admin_login_form))
        .route("/admin/", get(admin_overview_page))
        .route("/admin/logout", post(admin_logout_form))
        .route("/admin/devices", get(admin_devices_page))
        .route("/admin/devices/:id/revoke", post(admin_revoke_device_form))
        .route("/admin/pairing/new", get(admin_pairing_new_page))
        .route("/admin/pairing", post(admin_create_pairing_form))
        .route("/admin/library", get(admin_library_page))
        .route("/admin/library/scan", post(admin_start_scan_form))
        .route("/admin/cookies/youtube", get(admin_cookies_page))
        .route(
            "/admin/cookies/youtube",
            post(admin_upload_youtube_cookies_form),
        )
        .route(
            "/admin/cookies/youtube/probe",
            post(admin_probe_youtube_cookies_form),
        )
        .route(
            "/admin/cookies/youtube/clear",
            post(admin_clear_youtube_cookies_form),
        )
        .route("/admin/now-playing", get(admin_now_playing_page))
        .route(
            "/admin/now-playing/command",
            post(admin_now_playing_command_form),
        )
        .route("/admin/audit", get(admin_audit_page))
        .route("/api/v1/setup/status", get(setup_status))
        .route("/api/v1/setup/owner", post(setup_owner))
        .route("/api/v1/auth/register-device", post(register_device))
        .route("/api/v1/admin/auth/login", post(admin_login))
        .route("/api/v1/admin/auth/logout", post(admin_logout))
        .route("/api/v1/admin/me", get(admin_me))
        .route("/api/v1/admin", get(admin_status))
        .route("/api/v1/admin/status", get(admin_status))
        .route("/api/v1/admin/devices", get(admin_devices))
        .route(
            "/api/v1/admin/devices/:id/revoke",
            post(admin_revoke_device),
        )
        .route("/api/v1/admin/pairing-codes", post(admin_create_pairing))
        .route("/api/v1/admin/library/status", get(admin_library_status))
        .route("/api/v1/admin/library/scan", post(admin_start_scan))
        .route(
            "/api/v1/admin/cookies/youtube/status",
            get(admin_cookies_youtube_status),
        )
        .route(
            "/api/v1/admin/cookies/youtube",
            post(admin_upload_youtube_cookies),
        )
        .route(
            "/api/v1/admin/cookies/youtube/probe",
            post(admin_probe_youtube_cookies),
        )
        .route(
            "/api/v1/admin/cookies/youtube/clear",
            post(admin_clear_youtube_cookies),
        )
        .route("/api/v1/admin/now-playing", get(admin_now_playing))
        .route(
            "/api/v1/admin/now-playing/command",
            post(admin_now_playing_command),
        )
        .route("/api/v1/admin/audit", get(admin_audit))
        .route("/api/v1/queue/start", post(start_queue))
        .route("/api/v1/queue/:id", get(get_queue))
        .route("/api/v1/next", get(get_next))
        .route("/api/v1/home", get(get_home))
        .route("/api/v1/search", get(search))
        .route("/api/v1/likes", post(post_like))
        .route("/api/v1/events", post(post_events))
        .route("/api/v1/impressions", post(post_impressions))
        .route(
            "/api/v1/playlists",
            get(list_playlists).post(create_playlist),
        )
        .route(
            "/api/v1/playlists/:id",
            get(get_playlist)
                .patch(update_playlist)
                .delete(delete_playlist),
        )
        .route("/api/v1/playlists/:id/items", post(add_playlist_item))
        .route(
            "/api/v1/playlists/:id/items/:media_id",
            delete(remove_playlist_item),
        )
        .route("/api/v1/library/songs", get(list_songs))
        .route("/api/v1/library/albums", get(list_albums))
        .route("/api/v1/library/artists", get(list_artists))
        .route("/api/v1/library/scan", post(start_scan))
        .route("/api/v1/jobs/:id", get(get_job))
        .route(
            "/api/v1/library/albums/:album_media_id/art",
            get(serve_album_art),
        )
        .route("/api/v1/library/songs/:media_id/hash", get(song_hash))
        .route("/api/v1/library/songs/:media_id/stream", get(stream_song))
        .route(
            "/api/v1/cookies/youtube/status",
            get(device_youtube_cookie_status),
        )
        .route(
            "/api/v1/cookies/youtube",
            post(device_upload_youtube_cookies),
        )
        .route(
            "/api/v1/devices/:id/downloads",
            get(list_downloads).post(register_download),
        )
        .route(
            "/api/v1/devices/:id/downloads/:media_id",
            delete(delete_download),
        )
        .route("/api/v1/ws/now-playing", get(ws_now_playing))
        .route("/api/v1/streams/resolve", post(resolve_stream));
    let router = if legacy_routes.streams_proxy_enabled {
        router.route("/api/v1/streams/proxy", get(streams_proxy))
    } else {
        router
    };

    router
        .fallback(legacy_not_found_fallback)
        .method_not_allowed_fallback(legacy_method_not_allowed_fallback)
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            legacy_routes,
            cors_middleware,
        ))
}

#[derive(Clone, Copy)]
struct LegacyRouteConfig {
    streams_proxy_enabled: bool,
}

async fn cors_middleware(
    State(legacy_routes): State<LegacyRouteConfig>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request.method() == Method::OPTIONS
        && request
            .headers()
            .get(header::ACCESS_CONTROL_REQUEST_METHOD)
            .is_some()
    {
        return legacy_cors_preflight(request.headers());
    }

    let method = request.method().clone();
    let request_headers = request.headers().clone();

    if request.method() == Method::HEAD
        && let Some(methods) = legacy_allowed_methods_for_path(request.uri().path(), legacy_routes)
    {
        let mut response = legacy_method_not_allowed(methods);
        apply_legacy_cors_actual(&request_headers, &method, response.headers_mut());
        return response;
    }

    let mut response = next.run(request).await;
    if response.status() == StatusCode::METHOD_NOT_ALLOWED {
        normalize_legacy_allow_header(response.headers_mut());
    }
    response = append_legacy_json_newline(response).await;
    apply_legacy_cors_actual(&request_headers, &method, response.headers_mut());
    response
}

fn legacy_cors_preflight(request_headers: &HeaderMap) -> Response {
    let mut response = StatusCode::OK.into_response();
    append_header_once(response.headers_mut(), header::VARY, "Origin");
    append_header_once(
        response.headers_mut(),
        header::VARY,
        "Access-Control-Request-Method",
    );
    append_header_once(
        response.headers_mut(),
        header::VARY,
        "Access-Control-Request-Headers",
    );

    let Some(origin) = request_headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .filter(|origin| !origin.is_empty())
    else {
        return response;
    };
    if origin.is_empty() {
        return response;
    }
    let Some(request_method) = request_headers
        .get(header::ACCESS_CONTROL_REQUEST_METHOD)
        .and_then(|value| value.to_str().ok())
    else {
        return response;
    };
    let request_method = request_method.to_ascii_uppercase();
    if !legacy_cors_method_allowed(&request_method) {
        return response;
    }
    let requested_headers = request_headers
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .and_then(|value| value.to_str().ok())
        .map(parse_legacy_cors_header_list)
        .unwrap_or_default();
    if !legacy_cors_headers_allowed(&requested_headers) {
        return response;
    }

    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    if let Ok(value) = HeaderValue::from_str(&request_method) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_METHODS, value);
    }
    if !requested_headers.is_empty()
        && let Ok(value) = HeaderValue::from_str(&requested_headers.join(", "))
    {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_HEADERS, value);
    }
    response.headers_mut().insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("300"),
    );
    response
}

fn apply_legacy_cors_actual(
    request_headers: &HeaderMap,
    method: &Method,
    response_headers: &mut HeaderMap,
) {
    append_header_once(response_headers, header::VARY, "Origin");
    let Some(origin) = request_headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .filter(|origin| !origin.is_empty())
    else {
        return;
    };
    if origin.is_empty() || !legacy_cors_method_allowed(method.as_str()) {
        return;
    }
    response_headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    response_headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("Link"),
    );
}

fn legacy_cors_method_allowed(method: &str) -> bool {
    matches!(
        method.to_ascii_uppercase().as_str(),
        "GET" | "POST" | "PATCH" | "PUT" | "DELETE" | "OPTIONS"
    )
}

fn legacy_cors_headers_allowed(headers: &[String]) -> bool {
    headers.iter().all(|header| {
        matches!(
            header.as_str(),
            "Accept" | "Authorization" | "Content-Type" | "Idempotency-Key" | "Origin"
        )
    })
}

fn parse_legacy_cors_header_list(header_list: &str) -> Vec<String> {
    let mut headers = Vec::new();
    let mut current = String::new();
    let mut upper = true;
    for (index, byte) in header_list.bytes().enumerate() {
        match byte {
            b'a'..=b'z' => {
                if upper {
                    current.push((byte - (b'a' - b'A')) as char);
                } else {
                    current.push(byte as char);
                }
            }
            b'A'..=b'Z' => {
                if upper {
                    current.push(byte as char);
                } else {
                    current.push((byte + (b'a' - b'A')) as char);
                }
            }
            b'-' | b'_' | b'.' | b'0'..=b'9' => current.push(byte as char),
            _ => {}
        }

        if byte == b' ' || byte == b',' || index == header_list.len().saturating_sub(1) {
            if !current.is_empty() {
                headers.push(std::mem::take(&mut current));
                upper = true;
            }
        } else {
            upper = byte == b'-';
        }
    }
    headers
}

fn append_header_once(headers: &mut HeaderMap, name: header::HeaderName, value: &'static str) {
    if headers
        .get_all(&name)
        .iter()
        .any(|existing| existing.to_str().ok() == Some(value))
    {
        return;
    }
    headers.append(name, HeaderValue::from_static(value));
}

async fn append_legacy_json_newline(response: Response) -> Response {
    if !is_legacy_json_encoded_response(response.headers()) {
        return response;
    }

    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    if bytes.is_empty() {
        return Response::from_parts(parts, Body::from(bytes));
    }

    let mut body = escape_legacy_json_html_bytes(&bytes);
    if !body.ends_with(b"\n") {
        body.push(b'\n');
    }
    if body.as_slice() != bytes.as_ref() {
        parts.headers.remove(header::CONTENT_LENGTH);
    }
    Response::from_parts(parts, Body::from(body))
}

fn is_legacy_json_encoded_response(headers: &HeaderMap) -> bool {
    if headers
        .get("Idempotent-Replay")
        .and_then(|value| value.to_str().ok())
        == Some("true")
    {
        return false;
    }
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|content_type| {
            content_type
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        })
        .unwrap_or(false)
}

fn legacy_method_not_allowed(allowed_methods: &'static [&'static str]) -> Response {
    let mut response = StatusCode::METHOD_NOT_ALLOWED.into_response();
    for method in allowed_methods {
        response
            .headers_mut()
            .append(header::ALLOW, HeaderValue::from_static(method));
    }
    response
}

async fn legacy_method_not_allowed_fallback(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let legacy_routes = LegacyRouteConfig {
        streams_proxy_enabled: state.proxy.is_some(),
    };
    let mut response = legacy_allowed_methods_for_path(uri.path(), legacy_routes)
        .map(legacy_method_not_allowed)
        .unwrap_or_else(|| StatusCode::METHOD_NOT_ALLOWED.into_response());
    apply_legacy_cors_actual(&headers, &method, response.headers_mut());
    response
}

async fn legacy_not_found_fallback(method: Method, headers: HeaderMap) -> Response {
    let mut response = legacy_not_found_response();
    apply_legacy_cors_actual(&headers, &method, response.headers_mut());
    response
}

fn legacy_not_found_response() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header("x-content-type-options", "nosniff")
        .body(Body::from("404 page not found\n"))
        .unwrap_or_else(|_| StatusCode::NOT_FOUND.into_response())
}

fn normalize_legacy_allow_header(headers: &mut HeaderMap) {
    let mut methods = Vec::<String>::new();
    for value in headers.get_all(header::ALLOW) {
        let Ok(raw) = value.to_str() else {
            continue;
        };
        for method in raw.split(',') {
            let method = method.trim();
            if method.is_empty() || method.eq_ignore_ascii_case("HEAD") {
                continue;
            }
            if !methods.iter().any(|existing| existing == method) {
                methods.push(method.to_string());
            }
        }
    }
    if methods.is_empty() {
        return;
    }
    headers.remove(header::ALLOW);
    for method in methods {
        if let Ok(value) = HeaderValue::from_str(&method) {
            headers.append(header::ALLOW, value);
        }
    }
}

fn legacy_allowed_methods_for_path(
    path: &str,
    legacy_routes: LegacyRouteConfig,
) -> Option<&'static [&'static str]> {
    match path {
        "/healthz" => Some(LEGACY_ALLOW_GET),
        "/admin/login" => Some(LEGACY_ALLOW_GET_POST),
        "/admin/" => Some(LEGACY_ALLOW_GET),
        "/admin/logout" => Some(LEGACY_ALLOW_POST),
        "/admin/devices" => Some(LEGACY_ALLOW_GET),
        "/admin/pairing/new" => Some(LEGACY_ALLOW_GET),
        "/admin/pairing" => Some(LEGACY_ALLOW_POST),
        "/admin/library" => Some(LEGACY_ALLOW_GET),
        "/admin/library/scan" => Some(LEGACY_ALLOW_POST),
        "/admin/cookies/youtube" => Some(LEGACY_ALLOW_GET_POST),
        "/admin/cookies/youtube/probe" => Some(LEGACY_ALLOW_POST),
        "/admin/cookies/youtube/clear" => Some(LEGACY_ALLOW_POST),
        "/admin/now-playing" => Some(LEGACY_ALLOW_GET),
        "/admin/now-playing/command" => Some(LEGACY_ALLOW_POST),
        "/admin/audit" => Some(LEGACY_ALLOW_GET),
        "/api/v1/setup/status" => Some(LEGACY_ALLOW_GET),
        "/api/v1/setup/owner" => Some(LEGACY_ALLOW_POST),
        "/api/v1/auth/register-device" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/auth/login" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/auth/logout" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/me" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/status" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/devices" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/pairing-codes" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/library/status" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/library/scan" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/cookies/youtube/status" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/cookies/youtube" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/cookies/youtube/probe" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/cookies/youtube/clear" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/now-playing" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/now-playing/command" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/audit" => Some(LEGACY_ALLOW_GET),
        "/api/v1/queue/start" => Some(LEGACY_ALLOW_POST),
        "/api/v1/next" => Some(LEGACY_ALLOW_GET),
        "/api/v1/home" => Some(LEGACY_ALLOW_GET),
        "/api/v1/search" => Some(LEGACY_ALLOW_GET),
        "/api/v1/likes" => Some(LEGACY_ALLOW_POST),
        "/api/v1/events" => Some(LEGACY_ALLOW_POST),
        "/api/v1/impressions" => Some(LEGACY_ALLOW_POST),
        "/api/v1/playlists" => Some(LEGACY_ALLOW_GET_POST),
        "/api/v1/library/songs" => Some(LEGACY_ALLOW_GET),
        "/api/v1/library/albums" => Some(LEGACY_ALLOW_GET),
        "/api/v1/library/artists" => Some(LEGACY_ALLOW_GET),
        "/api/v1/library/scan" => Some(LEGACY_ALLOW_POST),
        "/api/v1/cookies/youtube/status" => Some(LEGACY_ALLOW_GET),
        "/api/v1/cookies/youtube" => Some(LEGACY_ALLOW_POST),
        "/api/v1/ws/now-playing" => Some(LEGACY_ALLOW_GET),
        "/api/v1/streams/proxy" if legacy_routes.streams_proxy_enabled => Some(LEGACY_ALLOW_GET),
        "/api/v1/streams/resolve" => Some(LEGACY_ALLOW_POST),
        _ if path.starts_with("/admin/static/") => Some(LEGACY_ALLOW_GET),
        _ => LEGACY_DYNAMIC_ROUTES.iter().find_map(|(pattern, methods)| {
            legacy_route_pattern_matches(pattern, path).then_some(*methods)
        }),
    }
}

fn legacy_route_pattern_matches(pattern: &str, path: &str) -> bool {
    let pattern_segments = path_segments(pattern);
    let path_segments = path_segments(path);
    pattern_segments.len() == path_segments.len()
        && pattern_segments
            .iter()
            .zip(path_segments.iter())
            .all(|(pattern, actual)| {
                if pattern.starts_with(':') {
                    !actual.is_empty()
                } else {
                    pattern == actual
                }
            })
}

#[cfg(test)]
fn legacy_idempotent_mutating_route_patterns() -> &'static [(&'static str, &'static str)] {
    &[
        ("POST", "/api/v1/auth/register-device"),
        ("POST", "/api/v1/library/scan"),
        ("POST", "/api/v1/cookies/youtube"),
        ("POST", "/api/v1/queue/start"),
        ("POST", "/api/v1/streams/resolve"),
        ("POST", "/api/v1/likes"),
        ("POST", "/api/v1/impressions"),
        ("POST", "/api/v1/playlists"),
        ("PATCH", "/api/v1/playlists/:id"),
        ("DELETE", "/api/v1/playlists/:id"),
        ("POST", "/api/v1/playlists/:id/items"),
        ("DELETE", "/api/v1/playlists/:id/items/:media_id"),
        ("POST", "/api/v1/devices/:id/downloads"),
        ("DELETE", "/api/v1/devices/:id/downloads/:media_id"),
        ("POST", "/api/v1/events"),
    ]
}

#[cfg(test)]
fn is_legacy_idempotent_mutation(method: &str, path: &str) -> bool {
    legacy_idempotent_mutating_route_patterns()
        .iter()
        .any(|(route_method, pattern)| {
            *route_method == method && legacy_route_pattern_matches(pattern, path)
        })
}

fn path_segments(path: &str) -> Vec<&str> {
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

async fn healthz() -> Response {
    legacy_json_response(
        StatusCode::OK,
        serde_json::json!({ "status": HealthzResponse::default().status }),
    )
}

async fn setup_status(State(state): State<AppState>) -> Response {
    let configured = match &state.store {
        Some(store) => match store.owner_configured().await {
            Ok(configured) => configured,
            Err(_) => return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        },
        None => false,
    };
    let mut response = SetupStatusResponse::legacy_default(state.server_version);
    response.configured = configured;
    Json(response).into_response()
}

async fn setup_owner(
    State(state): State<AppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    body: Bytes,
) -> Response {
    let limiter_key = rate_limit_key(connect_info);
    if !state.setup_limiter.allow(&limiter_key) {
        return legacy_json_error(StatusCode::TOO_MANY_REQUESTS, "rate_limited");
    }
    let raw = String::from_utf8_lossy(&body);
    let request = match OwnerSetupRequest::parse_json(&raw) {
        Ok(request) => request,
        Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
    };

    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };

    match store.setup_owner(&state.setup_token, &request).await {
        Ok(()) => {
            state.setup_limiter.reset(&limiter_key);
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(err) => auth_error_response(err),
    }
}

async fn admin_login(
    State(state): State<AppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let limiter_key = rate_limit_key(connect_info);
    if !state.admin_login_limiter.allow(&limiter_key) {
        return legacy_json_error(StatusCode::TOO_MANY_REQUESTS, "rate_limited");
    }
    let raw = String::from_utf8_lossy(&body);
    let request = match AdminLoginRequest::parse_json(&raw) {
        Ok(request) => request,
        Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
    };

    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let login = match store.login_admin(&request.password).await {
        Ok(login) => login,
        Err(err) => return admin_auth_error_response(err),
    };
    state.admin_login_limiter.reset(&limiter_key);

    let mut response = Json(AdminLoginResponse {
        csrf_token: login.csrf.clone(),
        expires_at: api_rfc3339_seconds(login.expires_at),
    })
    .into_response();
    append_cookie(
        &mut response,
        admin_cookie(
            ADMIN_COOKIE_NAME,
            &login.token,
            login.expires_at,
            true,
            is_https(&headers),
        ),
    );
    append_cookie(
        &mut response,
        admin_cookie(
            ADMIN_CSRF_COOKIE_NAME,
            &login.csrf,
            login.expires_at,
            false,
            is_https(&headers),
        ),
    );
    response
}

async fn admin_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let (Some(store), Some(token)) = (&state.store, cookie_value(&headers, ADMIN_COOKIE_NAME)) {
        let _ = store.revoke_admin_session(&token).await;
    }
    let mut response = Json(serde_json::json!({"ok": true})).into_response();
    append_cookie(
        &mut response,
        clear_admin_cookie(ADMIN_COOKIE_NAME, true, is_https(&headers)),
    );
    append_cookie(
        &mut response,
        clear_admin_cookie(ADMIN_CSRF_COOKIE_NAME, false, is_https(&headers)),
    );
    response
}

async fn admin_static_asset(Path(path): Path<String>, headers: HeaderMap) -> Response {
    let cleaned = clean_admin_static_path(&path);
    if path.ends_with('/') {
        match cleaned.as_str() {
            "admin.css" => return admin_static_slash_redirect("../admin.css"),
            "admin.js" => return admin_static_slash_redirect("../admin.js"),
            _ => {}
        }
    }

    match cleaned.as_str() {
        "" | "/" => serve_static_bytes(
            ADMIN_STATIC_DIR_LISTING.as_bytes(),
            "text/html; charset=utf-8",
            None,
        ),
        "admin.css" => serve_static_bytes(
            ADMIN_CSS.as_bytes(),
            "text/css; charset=utf-8",
            headers
                .get(header::RANGE)
                .and_then(|value| value.to_str().ok()),
        ),
        "admin.js" => serve_static_bytes(
            ADMIN_JS.as_bytes(),
            "text/javascript; charset=utf-8",
            headers
                .get(header::RANGE)
                .and_then(|value| value.to_str().ok()),
        ),
        _ => legacy_not_found_response(),
    }
}

async fn admin_static_dir_listing() -> Response {
    serve_static_bytes(
        ADMIN_STATIC_DIR_LISTING.as_bytes(),
        "text/html; charset=utf-8",
        None,
    )
}

fn clean_admin_static_path(path: &str) -> String {
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            _ => segments.push(segment),
        }
    }
    segments.join("/")
}

fn admin_static_slash_redirect(location: &'static str) -> Response {
    Response::builder()
        .status(StatusCode::MOVED_PERMANENTLY)
        .header(header::LOCATION, location)
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::MOVED_PERMANENTLY.into_response())
}

async fn admin_login_page(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    if admin_html_session_from_headers(&state, &headers, &Method::GET)
        .await
        .is_ok()
    {
        return redirect_found("/admin/");
    }
    admin_html_page(
        "Admin Login",
        None,
        query_param(uri.query().unwrap_or_default(), "error").as_deref(),
        r#"<form method="post" action="/admin/login"><label>Password <input type="password" name="password"></label><button type="submit">Login</button></form>"#,
    )
}

async fn admin_login_form(
    State(state): State<AppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let limiter_key = rate_limit_key(connect_info);
    if !state.admin_login_limiter.allow(&limiter_key) {
        return redirect_found_post("/admin/login?error=rate_limited");
    }
    let form = parse_request_form(&Method::POST, &headers, uri.query(), &body);
    if form.invalid {
        return redirect_found_post("/admin/login?error=invalid_request");
    }
    let Some(store) = &state.store else {
        return redirect_found_post("/admin/login?error=internal");
    };
    let login = match store.login_admin(&form_value(&form, "password")).await {
        Ok(login) => login,
        Err(err) => {
            let code = admin_auth_error_code(err);
            return redirect_found_post(&format!("/admin/login?error={code}"));
        }
    };
    state.admin_login_limiter.reset(&limiter_key);
    let mut response = redirect_found_post("/admin/");
    append_cookie(
        &mut response,
        admin_cookie(
            ADMIN_COOKIE_NAME,
            &login.token,
            login.expires_at,
            true,
            is_https(&headers),
        ),
    );
    append_cookie(
        &mut response,
        admin_cookie(
            ADMIN_CSRF_COOKIE_NAME,
            &login.csrf,
            login.expires_at,
            false,
            is_https(&headers),
        ),
    );
    response
}

async fn admin_logout_form(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (session, form) = match admin_form_session(&state, &headers, uri.query(), &body).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let _ = form;
    if let (Some(store), Some(token)) = (&state.store, cookie_value(&headers, ADMIN_COOKIE_NAME)) {
        let _ = store.revoke_admin_session(&token).await;
    }
    let mut response = redirect_found_post("/admin/login");
    append_cookie(
        &mut response,
        clear_admin_cookie(ADMIN_COOKIE_NAME, true, is_https(&headers)),
    );
    append_cookie(
        &mut response,
        clear_admin_cookie(ADMIN_CSRF_COOKIE_NAME, false, is_https(&headers)),
    );
    let _ = session;
    response
}

async fn admin_overview_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let (_, csrf) = match admin_html_session_from_headers(&state, &headers, &Method::GET).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Some(store) = &state.store else {
        return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal error");
    };
    let counts = store.admin_library_counts().await.ok();
    let body = format!(
        "<p>Server version: {}</p><p>Songs: {}</p>",
        escape_html(&state.server_version),
        counts.map(|counts| counts.songs).unwrap_or_default()
    );
    admin_html_page(
        "Overview",
        csrf.as_deref(),
        query_param(uri.query().unwrap_or_default(), "flash").as_deref(),
        &body,
    )
}

async fn admin_devices_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let (_, csrf) = match admin_html_session_from_headers(&state, &headers, &Method::GET).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Some(store) = &state.store else {
        return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal error");
    };
    let devices = store.list_admin_devices().await.unwrap_or_default();
    let mut rows = String::new();
    for device in devices {
        rows.push_str(&format!(
            "<li>{} <code>{}</code></li>",
            escape_html(&device.name),
            escape_html(&device.id)
        ));
    }
    admin_html_page(
        "Devices",
        csrf.as_deref(),
        query_param(uri.query().unwrap_or_default(), "flash").as_deref(),
        &format!("<ul>{rows}</ul>"),
    )
}

async fn admin_revoke_device_form(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (session, form) = match admin_form_session(&state, &headers, uri.query(), &body).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Ok(device_id) = Uuid::parse_str(&id) else {
        return admin_html_error(StatusCode::BAD_REQUEST, "Could not revoke device");
    };
    let Some(store) = &state.store else {
        return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Could not revoke device");
    };
    if store
        .revoke_device(&session, device_id, &form_value(&form, "reason"))
        .await
        .is_err()
    {
        return admin_html_error(StatusCode::BAD_REQUEST, "Could not revoke device");
    }
    if let Some(hub) = &state.hub {
        let _ = hub.disconnect_device(&device_id.to_string());
    }
    redirect_found_post("/admin/devices?flash=device_revoked")
}

async fn admin_pairing_new_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let (_, csrf) = match admin_html_session_from_headers(&state, &headers, &Method::GET).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    admin_html_page(
        "Pairing",
        csrf.as_deref(),
        query_param(uri.query().unwrap_or_default(), "flash").as_deref(),
        r#"<form method="post" action="/admin/pairing"><input type="hidden" name="csrf_token" value="{{csrf}}"><label>Label <input name="label"></label><label>TTL <input name="ttl_seconds" value="600"></label><button type="submit">Create</button></form>"#,
    )
}

async fn admin_create_pairing_form(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (session, form) = match admin_form_session(&state, &headers, uri.query(), &body).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let ttl_seconds = form_value(&form, "ttl_seconds")
        .parse::<i64>()
        .unwrap_or(600);
    let Some(store) = &state.store else {
        return admin_html_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not create pairing code",
        );
    };
    match store
        .create_pairing_code(
            &session,
            &form_value(&form, "label"),
            ttl_seconds,
            &server_base_url(&state, &headers),
        )
        .await
    {
        Ok(pairing) => admin_html_page(
            "Pairing",
            cookie_value(&headers, ADMIN_CSRF_COOKIE_NAME).as_deref(),
            None,
            &format!(
                "<p>Pairing code: <code>{}</code></p><p>{}</p>",
                escape_html(&pairing.pairing_code),
                escape_html(&pairing.pairing_url)
            ),
        ),
        Err(_) => admin_html_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not create pairing code",
        ),
    }
}

async fn admin_library_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let (_, csrf) = match admin_html_session_from_headers(&state, &headers, &Method::GET).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Some(store) = &state.store else {
        return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal error");
    };
    let counts = store.admin_library_counts().await.ok();
    let jobs = state.jobs.list_recent(25);
    admin_html_page(
        "Library",
        csrf.as_deref(),
        query_param(uri.query().unwrap_or_default(), "flash").as_deref(),
        &format!(
            "<p>Songs: {}</p><p>Jobs: {}</p>",
            counts.map(|counts| counts.songs).unwrap_or_default(),
            jobs.len()
        ),
    )
}

async fn admin_start_scan_form(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (session, form) = match admin_form_session(&state, &headers, uri.query(), &body).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let roots = split_roots(&form_value(&form, "roots"));
    if roots.is_empty() {
        return admin_html_error(StatusCode::BAD_REQUEST, "Enter at least one root");
    }
    let root_count = roots.len();
    let scan = match enqueue_scan_job(&state, StartScanRequest { roots }) {
        Ok(scan) => scan,
        Err(_) => {
            return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Could not start scan");
        }
    };
    if let Some(store) = &state.store {
        let _ = store
            .record_library_scan_started(&session, &scan.job_id, root_count)
            .await;
    }
    redirect_found_post("/admin/library?flash=scan_started")
}

async fn admin_cookies_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let (_, csrf) = match admin_html_session_from_headers(&state, &headers, &Method::GET).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let status = match &state.store {
        Some(store) => store.admin_cookie_status().await.ok(),
        None => None,
    };
    admin_html_page(
        "YouTube Cookies",
        csrf.as_deref(),
        query_param(uri.query().unwrap_or_default(), "flash").as_deref(),
        &format!(
            "<p>Status: {}</p>",
            status
                .map(|status| escape_html(&status.status))
                .unwrap_or_else(|| "unknown".to_string())
        ),
    )
}

async fn admin_upload_youtube_cookies_form(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (session, _) = match admin_html_session_from_headers(&state, &headers, &Method::POST).await
    {
        Ok(session) => session,
        Err(response) => return response,
    };
    if body.len() > (1 << 20) {
        return admin_html_error(StatusCode::BAD_REQUEST, "Invalid form");
    }
    let form = parse_request_form(&Method::POST, &headers, uri.query(), &body);
    if form.invalid {
        return admin_html_error(StatusCode::BAD_REQUEST, "Invalid form");
    }
    if !verify_admin_csrf(&session, &admin_form_csrf_token(&headers, &form)) {
        return admin_html_error(StatusCode::FORBIDDEN, "Invalid CSRF token");
    }
    let Some(cookie_key) = state.cookie_key else {
        return admin_html_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Cookie encryption is not configured",
        );
    };
    let raw = form_value(&form, "cookies").trim().to_string();
    if raw.is_empty() {
        return admin_html_error(StatusCode::BAD_REQUEST, "Cookie export is empty");
    }
    let Some(store) = &state.store else {
        return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Could not store cookies");
    };
    if store
        .store_youtube_cookies(&session, cookie_key, raw.as_bytes())
        .await
        .is_err()
    {
        return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Could not store cookies");
    }
    redirect_found_post("/admin/cookies/youtube?flash=cookies_updated")
}

async fn admin_probe_youtube_cookies_form(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let _session = match admin_action_session(&state, &headers, uri.query(), &body).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let Some(store) = &state.store else {
        return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Could not probe cookies");
    };
    if store.mark_youtube_cookie_probe_requested().await.is_err() {
        return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Could not probe cookies");
    }
    redirect_found_post("/admin/cookies/youtube?flash=probe_requested")
}

async fn admin_clear_youtube_cookies_form(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (session, _) = match admin_form_session(&state, &headers, uri.query(), &body).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Some(store) = &state.store else {
        return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Could not clear cookies");
    };
    if store.clear_youtube_cookies(&session).await.is_err() {
        return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Could not clear cookies");
    }
    redirect_found_post("/admin/cookies/youtube?flash=cookies_cleared")
}

async fn admin_now_playing_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let (_, csrf) = match admin_html_session_from_headers(&state, &headers, &Method::GET).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let count = state
        .hub
        .as_ref()
        .map(|hub| hub.snapshot().len())
        .unwrap_or_default();
    admin_html_page(
        "Now Playing",
        csrf.as_deref(),
        query_param(uri.query().unwrap_or_default(), "flash").as_deref(),
        &format!("<p>Active players: {count}</p>"),
    )
}

async fn admin_now_playing_command_form(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (_session, form) = match admin_form_session(&state, &headers, uri.query(), &body).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let _ = send_now_playing_command(
        &state,
        AdminNowPlayingCommandRequest {
            device_id: form_value(&form, "device_id"),
            command: form_value(&form, "command"),
        },
    );
    redirect_found_post("/admin/now-playing?flash=command_sent")
}

async fn admin_audit_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (_, csrf) = match admin_html_session_from_headers(&state, &headers, &Method::GET).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let count = match &state.store {
        Some(store) => store
            .recent_audit_events(200)
            .await
            .map(|events| events.len())
            .unwrap_or_default(),
        None => 0,
    };
    admin_html_page(
        "Audit",
        csrf.as_deref(),
        None,
        &format!("<p>Events: {count}</p>"),
    )
}

async fn admin_me(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (session, csrf) = match admin_session_from_headers(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store.admin_me(&session).await {
        Ok(me) => Json(AdminMeResponse {
            user_id: me.user_id.to_string(),
            display_name: me.display_name,
            csrf_token: csrf.unwrap_or_default(),
            expires_at: api_rfc3339_seconds(me.expires_at),
        })
        .into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn admin_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = admin_session_from_headers(&state, &headers).await {
        return response;
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let library_counts = match store.admin_library_counts().await {
        Ok(counts) => counts,
        Err(_) => return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    let cookie_status = match store.admin_cookie_status().await {
        Ok(status) => status,
        Err(_) => return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    let devices = match store.list_admin_devices().await {
        Ok(devices) => devices,
        Err(_) => return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    let mut warnings = Vec::new();
    if cookie_status.status == "unknown" {
        warnings.push("YouTube cookie status is unknown".to_string());
    }
    if library_counts.songs == 0 {
        warnings.push("Library has no songs".to_string());
    }
    let now_playing = state
        .hub
        .as_ref()
        .map(|hub| hub.snapshot())
        .unwrap_or_default();
    Json(AdminStatusResponse {
        server_version: state.server_version,
        uptime_seconds: SystemTime::now()
            .duration_since(state.started_at)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default(),
        db_status: "ok".to_string(),
        library_counts,
        cookie_status,
        devices,
        now_playing,
        jobs: state.jobs.list_recent(25),
        warnings,
    })
    .into_response()
}

async fn admin_devices(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = admin_session_from_headers(&state, &headers).await {
        return response;
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store.list_admin_devices().await {
        Ok(devices) => Json(AdminDevicesResponse { devices }).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn admin_revoke_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (session, _) = match admin_session_from_headers(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let csrf = match require_admin_csrf(&session, &headers, uri.query(), Some(&body)) {
        Ok(csrf) => csrf,
        Err(response) => return *response,
    };
    let body = csrf.body_after_middleware(body);

    let Ok(device_id) = Uuid::parse_str(&id) else {
        return legacy_json_error(StatusCode::BAD_REQUEST, "invalid_id");
    };
    let raw = String::from_utf8_lossy(&body);
    let reason = AdminRevokeDeviceRequest::parse_json(&raw)
        .map(|request| request.reason)
        .unwrap_or_default();

    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store.revoke_device(&session, device_id, &reason).await {
        Ok(()) => {
            if let Some(hub) = &state.hub {
                let _ = hub.disconnect_device(&device_id.to_string());
            }
            Json(serde_json::json!({"ok": true})).into_response()
        }
        Err(_) => legacy_json_error(StatusCode::BAD_REQUEST, "invalid_id"),
    }
}

async fn admin_create_pairing(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (session, _) = match admin_session_from_headers(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let csrf = match require_admin_csrf(&session, &headers, uri.query(), Some(&body)) {
        Ok(csrf) => csrf,
        Err(response) => return *response,
    };
    let body = csrf.body_after_middleware(body);

    let raw = String::from_utf8_lossy(&body);
    let request = match AdminPairingCodeRequest::parse_json(&raw) {
        Ok(request) => request,
        Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
    };
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store
        .create_pairing_code(
            &session,
            &request.label,
            request.ttl_seconds,
            &server_base_url(&state, &headers),
        )
        .await
    {
        Ok(response) => Json(response).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn admin_library_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = admin_session_from_headers(&state, &headers).await {
        return response;
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store.admin_library_counts().await {
        Ok(counts) => Json(AdminLibraryStatusResponse {
            counts,
            jobs: state.jobs.list_recent(25),
        })
        .into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn admin_start_scan(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (session, _) = match admin_session_from_headers(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let csrf = match require_admin_csrf(&session, &headers, uri.query(), Some(&body)) {
        Ok(csrf) => csrf,
        Err(response) => return *response,
    };
    let body = csrf.body_after_middleware(body);
    let raw = String::from_utf8_lossy(&body);
    let request = match StartScanRequest::parse_json(&raw) {
        Ok(request) => request,
        Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
    };
    let root_count = request.roots.len();
    let scan = match enqueue_scan_job(&state, request) {
        Ok(scan) => scan,
        Err(response) => return *response,
    };
    if let Some(store) = &state.store {
        let _ = store
            .record_library_scan_started(&session, &scan.job_id, root_count)
            .await;
    }
    Json(scan).into_response()
}

async fn admin_cookies_youtube_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = admin_session_from_headers(&state, &headers).await {
        return response;
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store.admin_cookie_status().await {
        Ok(status) => Json(status).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn admin_upload_youtube_cookies(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (session, _) = match admin_session_from_headers(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let csrf = match require_admin_csrf(&session, &headers, uri.query(), Some(&body)) {
        Ok(csrf) => csrf,
        Err(response) => return *response,
    };
    let body = csrf.body_after_middleware(body);
    let Some(cookie_key) = state.cookie_key else {
        return legacy_json_error(StatusCode::SERVICE_UNAVAILABLE, "cookies_disabled");
    };
    if body.len() > (1 << 20) {
        return legacy_json_error(StatusCode::BAD_REQUEST, "invalid_format");
    }
    let raw = String::from_utf8_lossy(&body);
    let request = match AdminUploadCookiesRequest::parse_json(&raw) {
        Ok(request) if !request.cookies.trim().is_empty() => request,
        _ => return legacy_json_error(StatusCode::BAD_REQUEST, "invalid_format"),
    };
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store
        .store_youtube_cookies(&session, cookie_key, request.cookies.as_bytes())
        .await
    {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn admin_probe_youtube_cookies(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let (session, _) = match admin_session_from_headers(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if let Err(response) = require_admin_csrf(&session, &headers, uri.query(), None) {
        return *response;
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store.probe_youtube_cookies(&session).await {
        Ok(status) => Json(status).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn admin_clear_youtube_cookies(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let (session, _) = match admin_session_from_headers(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    if let Err(response) = require_admin_csrf(&session, &headers, uri.query(), None) {
        return *response;
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store.clear_youtube_cookies(&session).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn admin_now_playing(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = admin_session_from_headers(&state, &headers).await {
        return response;
    }
    Json(AdminNowPlayingResponse {
        now_playing: state
            .hub
            .as_ref()
            .map(|hub| hub.snapshot())
            .unwrap_or_default(),
    })
    .into_response()
}

async fn admin_now_playing_command(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (session, _) = match admin_session_from_headers(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let csrf = match require_admin_csrf(&session, &headers, uri.query(), Some(&body)) {
        Ok(csrf) => csrf,
        Err(response) => return *response,
    };
    let body = csrf.body_after_middleware(body);

    let raw = String::from_utf8_lossy(&body);
    let request = match AdminNowPlayingCommandRequest::parse_json(&raw) {
        Ok(request) => request,
        Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
    };

    send_now_playing_command(&state, request)
}

fn send_now_playing_command(state: &AppState, request: AdminNowPlayingCommandRequest) -> Response {
    let Some(hub) = &state.hub else {
        return legacy_json_error(StatusCode::SERVICE_UNAVAILABLE, "ws_unavailable");
    };
    if request.device_id.is_empty() || request.command.is_empty() {
        return legacy_json_error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    match request.command.as_str() {
        NOW_PLAYING_CMD_PAUSE
        | NOW_PLAYING_CMD_PLAY
        | NOW_PLAYING_CMD_SKIP_NEXT
        | NOW_PLAYING_CMD_SKIP_PREV => {}
        _ => return legacy_json_error(StatusCode::BAD_REQUEST, "invalid_command"),
    }
    Json(AdminNowPlayingCommandResponse {
        delivered: hub.send_command(&request.device_id, &request.command),
    })
    .into_response()
}

async fn ws_now_playing(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let Some(hub) = state.hub.clone() else {
        return legacy_json_error(StatusCode::SERVICE_UNAVAILABLE, "ws_unavailable");
    };
    let device_id = auth.device_id.to_string();
    ws.protocols([NOW_PLAYING_SUBPROTOCOL])
        .on_upgrade(move |socket| now_playing::serve_socket(socket, hub, device_id))
}

async fn device_youtube_cookie_status(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }
    if state.cookie_key.is_none() {
        return legacy_json_error(StatusCode::SERVICE_UNAVAILABLE, "cookies_disabled");
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store.admin_cookie_status().await {
        Ok(status) => Json(status).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn device_upload_youtube_cookies(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    run_idempotent(&state, &headers, &uri, "POST", &auth, async {
        let Some(cookie_key) = state.cookie_key else {
            return legacy_json_error(StatusCode::SERVICE_UNAVAILABLE, "cookies_disabled");
        };
        let raw = String::from_utf8_lossy(&body);
        let request = match AdminUploadCookiesRequest::parse_json(&raw) {
            Ok(request) if !request.cookies.is_empty() => request,
            _ => return legacy_json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };
        match store
            .store_youtube_cookies_for_user(auth.user_id, cookie_key, request.cookies.as_bytes())
            .await
        {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    })
    .await
}

async fn admin_audit(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    if let Err(response) = admin_session_from_headers(&state, &headers).await {
        return response;
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let limit = admin_audit_limit(uri.query());
    match store.recent_audit_events(limit).await {
        Ok(events) => Json(AdminAuditResponse { events }).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthMode {
    Database,
    RejectAllTokens,
    #[cfg(test)]
    AllowAllForContractTests,
}

#[derive(Clone)]
struct AppState {
    auth_mode: AuthMode,
    server_version: String,
    setup_token: String,
    public_base_url: String,
    cookie_key: Option<[u8; 32]>,
    hub: Option<Arc<NowPlayingHub>>,
    proxy: Option<Arc<StreamProxy>>,
    proxy_youtube: bool,
    yt: Option<Arc<dyn innertube::InnerTubeBackend>>,
    jobs: Arc<JobRegistry>,
    started_at: SystemTime,
    data_dir: String,
    dev_open_registration: bool,
    setup_limiter: RateLimiter,
    admin_login_limiter: RateLimiter,
    pairing_limiter: RateLimiter,
    store: Option<PostgresStore>,
}

#[derive(Clone)]
struct RateLimiter {
    limit: usize,
    window: Duration,
    entries: Arc<Mutex<HashMap<String, RateEntry>>>,
}

#[derive(Clone, Copy)]
struct RateEntry {
    start: SystemTime,
    count: usize,
}

impl RateLimiter {
    fn new(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn allow(&self, key: &str) -> bool {
        let now = SystemTime::now();
        let Ok(mut entries) = self.entries.lock() else {
            return true;
        };
        let entry = entries.entry(key.to_string()).or_insert(RateEntry {
            start: now,
            count: 0,
        });
        if now.duration_since(entry.start).unwrap_or_default() > self.window {
            *entry = RateEntry {
                start: now,
                count: 0,
            };
        }
        if entry.count >= self.limit {
            return false;
        }
        entry.count += 1;
        true
    }

    fn reset(&self, key: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(key);
        }
    }
}

fn rate_limit_key(connect_info: Option<ConnectInfo<SocketAddr>>) -> String {
    connect_info
        .map(|ConnectInfo(addr)| addr.to_string())
        .unwrap_or_default()
}

async fn admin_session_from_headers(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(AdminSession, Option<String>), Response> {
    let token = cookie_value(headers, ADMIN_COOKIE_NAME)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| legacy_json_error(StatusCode::UNAUTHORIZED, "missing_admin_session"))?;
    let Some(store) = &state.store else {
        return Err(legacy_json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
        ));
    };
    let session = store
        .verify_admin_session(&token)
        .await
        .map_err(admin_auth_error_response)?;
    Ok((session, cookie_value(headers, ADMIN_CSRF_COOKIE_NAME)))
}

async fn admin_html_session_from_headers(
    state: &AppState,
    headers: &HeaderMap,
    method: &Method,
) -> Result<(AdminSession, Option<String>), Response> {
    let Some(token) = cookie_value(headers, ADMIN_COOKIE_NAME).filter(|token| !token.is_empty())
    else {
        return Err(redirect_found_for_method("/admin/login", method));
    };
    let Some(store) = &state.store else {
        return Err(redirect_found_for_method("/admin/login", method));
    };
    match store.verify_admin_session(&token).await {
        Ok(session) => Ok((session, cookie_value(headers, ADMIN_CSRF_COOKIE_NAME))),
        Err(_) => Err(redirect_found_for_method("/admin/login", method)),
    }
}

async fn admin_form_session(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
    body: &Bytes,
) -> Result<(AdminSession, ParsedForm), Response> {
    let (session, _) = admin_html_session_from_headers(state, headers, &Method::POST).await?;
    let form = parse_request_form(&Method::POST, headers, query, body);
    if form.invalid {
        return Err(admin_html_error(StatusCode::BAD_REQUEST, "Invalid form"));
    }
    if verify_admin_csrf(&session, &admin_form_csrf_token(headers, &form)) {
        Ok((session, form))
    } else {
        Err(admin_html_error(
            StatusCode::FORBIDDEN,
            "Invalid CSRF token",
        ))
    }
}

async fn admin_action_session(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
    body: &Bytes,
) -> ResponseResult<AdminSession> {
    let (session, _) = admin_html_session_from_headers(state, headers, &Method::POST).await?;
    if let Some(token) = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .filter(|token| !token.is_empty())
    {
        return verify_admin_csrf_response(session, token);
    }
    let form = parse_request_form(&Method::POST, headers, query, body);
    if form.invalid {
        return Err(Box::new(admin_html_error(
            StatusCode::BAD_REQUEST,
            "Invalid form",
        )));
    }
    verify_admin_csrf_response(session, form_value(&form, "csrf_token").trim())
}

fn verify_admin_csrf_response(session: AdminSession, token: &str) -> ResponseResult<AdminSession> {
    if verify_admin_csrf(&session, token) {
        Ok(session)
    } else {
        Err(Box::new(admin_html_error(
            StatusCode::FORBIDDEN,
            "Invalid CSRF token",
        )))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AdminCsrfCheck {
    form_body_consumed: bool,
}

type ResponseResult<T> = Result<T, Box<Response>>;

impl AdminCsrfCheck {
    fn body_after_middleware(self, body: Bytes) -> Bytes {
        if self.form_body_consumed {
            Bytes::new()
        } else {
            body
        }
    }
}

fn require_admin_csrf(
    session: &AdminSession,
    headers: &HeaderMap,
    query: Option<&str>,
    body: Option<&Bytes>,
) -> ResponseResult<AdminCsrfCheck> {
    let csrf = admin_api_csrf_token(headers, query, body);
    if verify_admin_csrf(session, &csrf.token) {
        Ok(AdminCsrfCheck {
            form_body_consumed: csrf.form_body_consumed,
        })
    } else {
        Err(Box::new(legacy_json_error(
            StatusCode::FORBIDDEN,
            "invalid_csrf",
        )))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct AdminApiCsrfToken {
    token: String,
    form_body_consumed: bool,
}

fn admin_api_csrf_token(
    headers: &HeaderMap,
    query: Option<&str>,
    body: Option<&Bytes>,
) -> AdminApiCsrfToken {
    if let Some(token) = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .filter(|token| !token.is_empty())
    {
        return AdminApiCsrfToken {
            token: token.to_string(),
            form_body_consumed: false,
        };
    }

    if is_form_urlencoded(headers)
        && let Some(form) = body.map(|body| parse_form(body))
    {
        return AdminApiCsrfToken {
            token: form_value_opt(&form, "csrf_token")
                .or_else(|| query_param(query.unwrap_or_default(), "csrf_token"))
                .unwrap_or_default(),
            form_body_consumed: true,
        };
    }

    AdminApiCsrfToken {
        token: query_param(query.unwrap_or_default(), "csrf_token").unwrap_or_default(),
        form_body_consumed: false,
    }
}

fn is_form_urlencoded(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|mime| {
            mime.trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        })
}

fn redirect_found(location: &str) -> Response {
    redirect_found_for_method(location, &Method::GET)
}

fn redirect_found_post(location: &str) -> Response {
    redirect_found_for_method(location, &Method::POST)
}

fn redirect_found_for_method(location: &str, method: &Method) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, location);
    let body = if method == Method::GET {
        builder = builder.header(header::CONTENT_TYPE, "text/html; charset=utf-8");
        Body::from(format!(
            "<a href=\"{}\">Found</a>.\n\n",
            escape_html(location)
        ))
    } else {
        Body::empty()
    };
    builder
        .body(body)
        .unwrap_or_else(|_| legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"))
}

fn admin_html_page(
    title: &str,
    csrf_token: Option<&str>,
    flash: Option<&str>,
    body: &str,
) -> Response {
    let csrf = csrf_token.unwrap_or_default();
    let flash_html = flash
        .filter(|flash| !flash.is_empty())
        .map(|flash| format!(r#"<p class="flash">{}</p>"#, escape_html(flash)))
        .unwrap_or_default();
    let body = body.replace("{{csrf}}", &escape_html(csrf));
    let html = format!(
        r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>{title}</title><link rel="stylesheet" href="/admin/static/admin.css"></head>
<body><main>
<nav>
<a href="/admin/">Overview</a>
<a href="/admin/devices">Devices</a>
<a href="/admin/pairing/new">Pairing</a>
<a href="/admin/library">Library</a>
<a href="/admin/cookies/youtube">Cookies</a>
<a href="/admin/now-playing">Now Playing</a>
<a href="/admin/audit">Audit</a>
</nav>
<h1>{title}</h1>
{flash_html}
{body}
</main></body>
</html>"#,
        title = escape_html(title),
        flash_html = flash_html,
        body = body
    );
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

fn admin_html_error(status: StatusCode, message: &str) -> Response {
    let mut response = admin_html_page(
        "Error",
        None,
        None,
        &format!(r#"<p class="error">{}</p>"#, escape_html(message)),
    );
    *response.status_mut() = status;
    response
}

fn admin_auth_error_code(err: AuthStoreError) -> &'static str {
    match err {
        AuthStoreError::InvalidPassword => "invalid_password",
        AuthStoreError::SetupRequired => "setup_required",
        AuthStoreError::MissingAdminSession => "missing_admin_session",
        AuthStoreError::InvalidAdminSession => "invalid_admin_session",
        AuthStoreError::Backend(_) => "internal",
        _ => "invalid_password",
    }
}

#[derive(Clone, Debug, Default)]
struct ParsedForm {
    values: Vec<(String, String)>,
    invalid: bool,
}

fn parse_form(body: &[u8]) -> ParsedForm {
    let mut form = ParsedForm::default();
    parse_urlencoded_into(&mut form, &String::from_utf8_lossy(body));
    form
}

fn parse_request_form(
    method: &Method,
    headers: &HeaderMap,
    query: Option<&str>,
    body: &[u8],
) -> ParsedForm {
    let mut form = ParsedForm::default();
    if matches!(*method, Method::POST | Method::PUT | Method::PATCH) && is_form_urlencoded(headers)
    {
        parse_urlencoded_into(&mut form, &String::from_utf8_lossy(body));
    }
    parse_urlencoded_into(&mut form, query.unwrap_or_default());
    form
}

fn parse_urlencoded_into(form: &mut ParsedForm, raw: &str) {
    for part in raw.split('&').filter(|part| !part.is_empty()) {
        if part.contains(';') {
            form.invalid = true;
            continue;
        }
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        match (form_decode(key), form_decode(value)) {
            (Ok(key), Ok(value)) => form.values.push((key, value)),
            _ => form.invalid = true,
        }
    }
}

fn form_value(form: &ParsedForm, key: &str) -> String {
    form_value_opt(form, key).unwrap_or_default()
}

fn form_value_opt(form: &ParsedForm, key: &str) -> Option<String> {
    form.values
        .iter()
        .find_map(|(candidate, value)| (candidate == key).then(|| value.clone()))
}

fn admin_form_csrf_token(headers: &HeaderMap, form: &ParsedForm) -> String {
    headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| form_value(form, "csrf_token").trim().to_string())
}

fn form_decode(raw: &str) -> Result<String, ()> {
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                if let (Some(hi), Some(lo)) =
                    (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
                {
                    out.push((hi << 4) | lo);
                    index += 3;
                } else {
                    return Err(());
                }
            }
            b'%' => return Err(()),
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn percent_decode_path(raw: &str) -> Option<String> {
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hi = hex_value(bytes[index + 1])?;
                let lo = hex_value(bytes[index + 2])?;
                out.push((hi << 4) | lo);
                index += 3;
            }
            b'%' => return None,
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

fn path_segment(raw: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => {
                out.push('%');
                out.push(char::from(HEX[(byte >> 4) as usize]));
                out.push(char::from(HEX[(byte & 0x0f) as usize]));
            }
        }
    }
    out
}

fn split_roots(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn escape_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

async fn authorize(
    headers: &HeaderMap,
    uri: &Uri,
    state: &AppState,
) -> Result<AuthenticatedDevice, Response> {
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty())
        .map(Cow::Borrowed)
        .or_else(|| query_token(uri.query()).map(Cow::Owned));

    let Some(token) = token else {
        return Err(legacy_http_error(StatusCode::UNAUTHORIZED, "missing_token"));
    };

    match state.auth_mode {
        #[cfg(test)]
        AuthMode::AllowAllForContractTests => Ok(AuthenticatedDevice {
            user_id: uuid::Uuid::nil(),
            device_id: uuid::Uuid::nil(),
        }),
        AuthMode::Database => {
            let Some(store) = &state.store else {
                return Err(legacy_http_error(StatusCode::UNAUTHORIZED, "invalid_token"));
            };
            store
                .validate_device_token(&token)
                .await
                .map_err(auth_error_response)
        }
        AuthMode::RejectAllTokens => {
            Err(legacy_http_error(StatusCode::UNAUTHORIZED, "invalid_token"))
        }
    }
}

fn authorized_device_id(id: &str, auth: &AuthenticatedDevice) -> ResponseResult<Uuid> {
    let path_id = Uuid::parse_str(id)
        .map_err(|_| Box::new(legacy_json_error(StatusCode::BAD_REQUEST, "invalid_id")))?;
    if path_id != auth.device_id {
        return Err(Box::new(legacy_json_error(
            StatusCode::FORBIDDEN,
            "forbidden",
        )));
    }
    Ok(path_id)
}

fn parse_playlist_id(id: &str) -> ResponseResult<Uuid> {
    Uuid::parse_str(id)
        .map_err(|_| Box::new(legacy_json_error(StatusCode::BAD_REQUEST, "invalid_id")))
}

fn parse_uuid_v7(raw: &str) -> Option<Uuid> {
    let key = Uuid::parse_str(raw).ok()?;
    (key.get_version_num() == 7).then_some(key)
}

fn idempotency_key_from_headers(headers: &HeaderMap) -> Uuid {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_uuid_v7)
        .unwrap_or_else(Uuid::now_v7)
}

fn required_idempotency_key(headers: &HeaderMap) -> ResponseResult<Uuid> {
    let Some(raw) = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
    else {
        return Err(Box::new(legacy_json_error(
            StatusCode::BAD_REQUEST,
            "invalid_idempotency_key",
        )));
    };
    parse_uuid_v7(raw).ok_or_else(|| {
        Box::new(legacy_json_error(
            StatusCode::BAD_REQUEST,
            "invalid_idempotency_key",
        ))
    })
}

async fn run_idempotent<Fut>(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    method: &str,
    auth: &AuthenticatedDevice,
    future: Fut,
) -> Response
where
    Fut: Future<Output = Response>,
{
    let key = match required_idempotency_key(headers) {
        Ok(key) => key,
        Err(response) => return *response,
    };
    let Some(store) = &state.store else {
        return future.await;
    };
    let route = format!("{method} {}", legacy_url_path(uri.path()));
    if let Ok(Some(record)) = store.find_idempotency_log(key).await {
        if record.route != route
            || record
                .expires_at
                .is_some_and(|expires_at| expires_at <= Utc::now())
        {
            return legacy_json_error(StatusCode::CONFLICT, "conflict");
        }
        return idempotent_replay_response(record);
    }

    let response = future.await;
    if !response.status().is_success() {
        return response;
    }

    record_idempotent_response(
        store,
        IdempotencyLogIdentity {
            key,
            user_id: Some(auth.user_id),
            device_id: Some(auth.device_id),
            route: &route,
        },
        response,
    )
    .await
}

struct IdempotencyLogIdentity<'a> {
    key: Uuid,
    user_id: Option<Uuid>,
    device_id: Option<Uuid>,
    route: &'a str,
}

async fn record_idempotent_response(
    store: &PostgresStore,
    identity: IdempotencyLogIdentity<'_>,
    response: Response,
) -> Response {
    if !response.status().is_success() {
        return response;
    }

    let (parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    let replay_body = legacy_wire_body_for_hash(&parts.headers, &bytes);
    let hash = hex_lower_bytes(&Sha256::digest(&replay_body));
    let response_status = parts.status.as_u16();
    let response_content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let _ = store
        .insert_idempotency_log(IdempotencyLogInsert {
            key: identity.key,
            user_id: identity.user_id,
            device_id: identity.device_id,
            route: identity.route,
            response_hash: &hash,
            response_status,
            response_body: &replay_body,
            response_content_type,
        })
        .await;
    Response::from_parts(parts, Body::from(bytes))
}

fn idempotent_replay_response(record: IdempotencyLogRecord) -> Response {
    if let (Some(status), Some(body)) = (record.response_status, record.response_body) {
        let status = u16::try_from(status)
            .ok()
            .and_then(|status| StatusCode::from_u16(status).ok())
            .unwrap_or(StatusCode::OK);
        let mut builder = Response::builder()
            .status(status)
            .header("Idempotent-Replay", "true");
        if let Some(content_type) = record.response_content_type {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }
        return builder
            .body(Body::from(body))
            .unwrap_or_else(|_| legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"));
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotent-Replay", "true")
        .body(Body::from(r#"{"idempotent_replay":true}"#))
        .unwrap_or_else(|_| legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"))
}

fn legacy_wire_body_for_hash(headers: &HeaderMap, bytes: &[u8]) -> Vec<u8> {
    if bytes.is_empty() || !is_legacy_json_encoded_response(headers) {
        return bytes.to_vec();
    }
    let mut body = escape_legacy_json_html_bytes(bytes);
    if !body.ends_with(b"\n") {
        body.push(b'\n');
    }
    body
}

fn legacy_url_path(path: &str) -> String {
    percent_decode_path(path).unwrap_or_else(|| path.to_string())
}

async fn register_device(
    State(state): State<AppState>,
    uri: Uri,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let raw = String::from_utf8_lossy(&body);
    let request = match RegisterDeviceRequest::parse_json(&raw) {
        Ok(request) => request,
        Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
    };
    let key = match required_idempotency_key(&headers) {
        Ok(key) => key,
        Err(response) => return *response,
    };
    let route = format!("POST {}", legacy_url_path(uri.path()));
    if let Some(store) = &state.store
        && let Ok(Some(record)) = store.find_idempotency_log(key).await
    {
        if record.route != route
            || record
                .expires_at
                .is_some_and(|expires_at| expires_at <= Utc::now())
        {
            return legacy_json_error(StatusCode::CONFLICT, "conflict");
        }
        return idempotent_replay_response(record);
    }

    let limiter_key = rate_limit_key(connect_info);
    if !state.pairing_limiter.allow(&limiter_key) {
        return legacy_json_error(StatusCode::TOO_MANY_REQUESTS, "rate_limited");
    }

    if let Some(store) = &state.store {
        return match store.register_device(&request).await {
            Ok(response) => {
                state.pairing_limiter.reset(&limiter_key);
                record_register_device_response(store, key, &route, response).await
            }
            Err(AuthStoreError::PairingRequired) if state.dev_open_registration => {
                match store.register_device_open(&request).await {
                    Ok(response) => {
                        state.pairing_limiter.reset(&limiter_key);
                        record_register_device_response(store, key, &route, response).await
                    }
                    Err(err) => auth_error_response(err),
                }
            }
            Err(err) => auth_error_response(err),
        };
    }

    if request.pairing_code.trim().is_empty() {
        return legacy_json_error(StatusCode::FORBIDDEN, "pairing_required");
    }

    legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_pairing_code")
}

async fn record_register_device_response(
    store: &PostgresStore,
    key: Uuid,
    route: &str,
    response: RegisterDeviceResponse,
) -> Response {
    let device_id = Uuid::parse_str(&response.device_id).ok();
    record_idempotent_response(
        store,
        IdempotencyLogIdentity {
            key,
            user_id: None,
            device_id,
            route,
        },
        Json(response).into_response(),
    )
    .await
}

async fn start_scan(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    run_idempotent(&state, &headers, &uri, "POST", &auth, async {
        let raw = String::from_utf8_lossy(&body);
        let request = match StartScanRequest::parse_json(&raw) {
            Ok(request) => request,
            Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        };
        start_scan_job(&state, request)
    })
    .await
}

fn start_scan_job(state: &AppState, request: StartScanRequest) -> Response {
    match enqueue_scan_job(state, request) {
        Ok(response) => Json(response).into_response(),
        Err(response) => *response,
    }
}

fn enqueue_scan_job(
    state: &AppState,
    request: StartScanRequest,
) -> ResponseResult<StartScanResponse> {
    let Some(store) = &state.store else {
        return Err(Box::new(legacy_json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
        )));
    };
    let job = state.jobs.create();
    tokio::spawn(jobs::run_scan_job(
        state.jobs.clone(),
        store.clone(),
        job.id.clone(),
        request.roots,
        state.data_dir.clone(),
    ));
    Ok(StartScanResponse { job_id: job.id })
}

async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }
    match state.jobs.get(&id) {
        Some(job) => Json(job).into_response(),
        None => legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
    }
}

async fn list_songs(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }

    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let (limit, offset) = pagination(uri.query());
    match store.list_library_songs(limit, offset).await {
        Ok(songs) => Json(SongListResponse { songs }).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn list_albums(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }

    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let (limit, offset) = pagination(uri.query());
    match store.list_library_albums(limit, offset).await {
        Ok(albums) => Json(AlbumListResponse { albums }).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn list_artists(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }

    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let (limit, offset) = pagination(uri.query());
    match store.list_library_artists(limit, offset).await {
        Ok(artists) => Json(ArtistListResponse { artists }).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn get_home(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::SERVICE_UNAVAILABLE, "recs_unavailable");
    };
    let hide_explicit = bool_param(uri.query(), "hide_explicit");
    let hide_video = bool_param(uri.query(), "hide_video");
    let hide_shorts = bool_param(uri.query(), "hide_shorts");
    let cached_home = store
        .cached_home(auth.user_id, hide_explicit, hide_video, hide_shorts)
        .await
        .ok()
        .flatten();
    if let Some(cached) = cached_home.as_ref().filter(|cached| cached.fresh) {
        return Json(cached.home.clone()).into_response();
    }

    let (candidates, stats) = match store
        .local_home_inputs(
            auth.user_id,
            auth.device_id,
            HOME_LOCAL_CANDIDATE_LIMIT,
            hide_explicit,
            hide_video,
        )
        .await
    {
        Ok(inputs) => inputs,
        Err(_) => {
            if let Some(cached) = cached_home {
                return Json(cached.home).into_response();
            }
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        }
    };
    let ranked =
        LocalRecommendationEngine::default().rank(&candidates, &stats, HOME_QUICK_PICKS_LIMIT);
    let items: Vec<_> = ranked
        .into_iter()
        .map(|candidate| HomeItemResponse {
            source: candidate.media_id.source().to_string(),
            media_id: candidate.media_id.0,
            title: candidate.title,
            artists: candidate.artists,
            album_id: candidate.album_id.map(|album_id| album_id.0),
            duration_ms: candidate.duration_ms,
            thumbnail_url: None,
            score: 0.0,
        })
        .collect();
    let mut sections = if items.is_empty() {
        vec![]
    } else {
        vec![HomeSectionResponse {
            id: "quick_picks".into(),
            title: "Quick Picks".into(),
            kind: "quick_picks".into(),
            seed: None,
            items,
        }]
    };
    if let Some(daily_discover) = daily_discover_section(
        &state,
        store,
        auth.user_id,
        hide_explicit,
        hide_video,
        hide_shorts,
    )
    .await
    {
        sections.push(daily_discover);
    }
    for similar_artist in similar_artist_sections(
        &state,
        store,
        auth.user_id,
        hide_explicit,
        hide_video,
        hide_shorts,
    )
    .await
    {
        sections.push(similar_artist);
    }
    let mut chips = vec![];
    if let Some((yt_home, yt_chips)) = youtube_home_section(
        &state,
        store,
        auth.user_id,
        hide_explicit,
        hide_video,
        hide_shorts,
    )
    .await
    {
        chips = yt_chips;
        sections.push(yt_home);
    }
    if let Some(community_playlists) = community_playlists_section(
        &state,
        store,
        auth.user_id,
        hide_explicit,
        hide_video,
        hide_shorts,
    )
    .await
    {
        sections.push(community_playlists);
    }

    let home = HomeResponse {
        sections,
        chips,
        stale: false,
    };
    let _ = store
        .put_home_cache(auth.user_id, hide_explicit, hide_video, hide_shorts, &home)
        .await;
    Json(home).into_response()
}

async fn search(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }
    let query_raw = uri.query().unwrap_or_default();
    let query = decoded_query_param(query_raw, "q")
        .unwrap_or_default()
        .trim()
        .to_string();
    if query.len() < 2 {
        return legacy_json_error(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let Some(yt) = &state.yt else {
        return legacy_json_error(StatusCode::SERVICE_UNAVAILABLE, "yt_unavailable");
    };
    match tokio::time::timeout(Duration::from_secs(8), yt.search(&query)).await {
        Ok(Ok(page)) => Json(search_response_from_page(
            &query,
            page,
            search_limit(query_raw),
        ))
        .into_response(),
        Ok(Err(_)) | Err(_) => legacy_json_error(StatusCode::BAD_GATEWAY, "search_unavailable"),
    }
}

async fn post_like(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    run_idempotent(&state, &headers, &uri, "POST", &auth, async {
        let raw = String::from_utf8_lossy(&body);
        let request = match LikeRequest::parse_json(&raw) {
            Ok(request) => request,
            Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        };
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };

        let result = if request.liked {
            store
                .upsert_like(
                    auth.user_id,
                    &request.media_id,
                    request.occurred_at.as_deref(),
                    idempotency_key_from_headers(&headers),
                )
                .await
        } else {
            store
                .delete_like(
                    auth.user_id,
                    &request.media_id,
                    request.occurred_at.as_deref(),
                    idempotency_key_from_headers(&headers),
                )
                .await
        };

        match result {
            Ok(()) => Json(LikeResponse {
                media_id: request.media_id,
                liked: request.liked,
            })
            .into_response(),
            Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    })
    .await
}

async fn post_events(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    run_idempotent(&state, &headers, &uri, "POST", &auth, async {
        let raw = String::from_utf8_lossy(&body);
        let request = match EventsRequest::parse_json(&raw) {
            Ok(request) => request,
            Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        };

        let mut results = Vec::with_capacity(request.events.len());
        for event in request.events {
            let mut result = EventResultResponse {
                event_id: event.event_id.clone(),
                accepted: true,
                reason: None,
            };
            if parse_uuid_v7(&event.event_id).is_none() {
                result.accepted = false;
                result.reason = Some("invalid_event_id".into());
                results.push(result);
                continue;
            }
            if event.media_id.is_empty() {
                result.accepted = false;
                result.reason = Some("missing_media_id".into());
                results.push(result);
                continue;
            }
            if event.kind == "play" && !scrobble_qualifies(event.total_played_ms, event.duration_ms)
            {
                result.accepted = false;
                result.reason = Some("below_scrobble_threshold".into());
                results.push(result);
                continue;
            }
            let Some(store) = &state.store else {
                return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
            };
            match store
                .insert_play_event(auth.user_id, auth.device_id, &event)
                .await
            {
                Ok(_) => {}
                Err(_) => {
                    result.accepted = false;
                    result.reason = Some("internal".into());
                }
            }
            results.push(result);
        }

        Json(EventsResponse { results }).into_response()
    })
    .await
}

async fn post_impressions(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    run_idempotent(&state, &headers, &uri, "POST", &auth, async {
        let raw = String::from_utf8_lossy(&body);
        let request = match ImpressionsRequest::parse_json(&raw) {
            Ok(request) => request,
            Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        };
        if request.impressions.is_empty() {
            return StatusCode::NO_CONTENT.into_response();
        }
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };

        let mut written = 0;
        for impression in request.impressions {
            if impression.media_id.is_empty() {
                continue;
            }
            if store
                .insert_impression(auth.user_id, &impression)
                .await
                .is_ok()
            {
                written += 1;
            }
        }
        Json(ImpressionsResponse { written }).into_response()
    })
    .await
}

async fn list_playlists(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let (limit, offset) = pagination(uri.query());
    match store.list_playlists(auth.user_id, limit, offset).await {
        Ok(playlists) => Json(PlaylistListResponse { playlists }).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn create_playlist(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    run_idempotent(&state, &headers, &uri, "POST", &auth, async {
        let raw = String::from_utf8_lossy(&body);
        let request = match PlaylistTitleRequest::parse_json(&raw) {
            Ok(request) => request,
            Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        };
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };
        match store.create_playlist(auth.user_id, &request.title).await {
            Ok(playlist) => Json(playlist).into_response(),
            Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    })
    .await
}

async fn get_playlist(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let playlist_id = match parse_playlist_id(&id) {
        Ok(id) => id,
        Err(response) => return *response,
    };
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store.get_playlist(auth.user_id, playlist_id).await {
        Ok(Some(playlist)) => Json(playlist).into_response(),
        Ok(None) => legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn update_playlist(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    run_idempotent(&state, &headers, &uri, "PATCH", &auth, async {
        let playlist_id = match parse_playlist_id(&id) {
            Ok(id) => id,
            Err(response) => return *response,
        };
        let raw = String::from_utf8_lossy(&body);
        let request = match PlaylistTitleRequest::parse_json(&raw) {
            Ok(request) => request,
            Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        };
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };
        match store
            .update_playlist_title(auth.user_id, playlist_id, &request.title)
            .await
        {
            Ok(Some(playlist)) => Json(playlist).into_response(),
            Ok(None) => legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
            Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    })
    .await
}

async fn delete_playlist(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    run_idempotent(&state, &headers, &uri, "DELETE", &auth, async {
        let playlist_id = match parse_playlist_id(&id) {
            Ok(id) => id,
            Err(response) => return *response,
        };
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };
        match store.delete_playlist(auth.user_id, playlist_id).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    })
    .await
}

async fn add_playlist_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    run_idempotent(&state, &headers, &uri, "POST", &auth, async {
        let playlist_id = match parse_playlist_id(&id) {
            Ok(id) => id,
            Err(response) => return *response,
        };
        let raw = String::from_utf8_lossy(&body);
        let request = match AddPlaylistItemRequest::parse_json(&raw) {
            Ok(request) => request,
            Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        };
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };
        match store
            .add_playlist_item(auth.user_id, auth.device_id, playlist_id, &request.media_id)
            .await
        {
            Ok(true) => StatusCode::NO_CONTENT.into_response(),
            Ok(false) => legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
            Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    })
    .await
}

async fn remove_playlist_item(
    State(state): State<AppState>,
    Path((id, media_id)): Path<(String, String)>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    run_idempotent(&state, &headers, &uri, "DELETE", &auth, async {
        let playlist_id = match parse_playlist_id(&id) {
            Ok(id) => id,
            Err(response) => return *response,
        };
        if media_id.is_empty() {
            return legacy_json_error(StatusCode::BAD_REQUEST, "invalid_request");
        }
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };
        match store
            .remove_playlist_item(auth.user_id, playlist_id, &media_id)
            .await
        {
            Ok(true) => StatusCode::NO_CONTENT.into_response(),
            Ok(false) => legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
            Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    })
    .await
}

async fn register_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let device_id = match authorized_device_id(&id, &auth) {
        Ok(device_id) => device_id,
        Err(response) => return *response,
    };

    run_idempotent(&state, &headers, &uri, "POST", &auth, async {
        let raw = String::from_utf8_lossy(&body);
        let request = match RegisterDownloadRequest::parse_json(&raw) {
            Ok(request) => request,
            Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        };
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };
        match store
            .upsert_download(
                device_id,
                &request.media_id,
                &request.local_path,
                request.bytes,
            )
            .await
        {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    })
    .await
}

async fn list_downloads(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let device_id = match authorized_device_id(&id, &auth) {
        Ok(device_id) => device_id,
        Err(response) => return *response,
    };

    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store.list_downloads(device_id).await {
        Ok(downloads) => Json(DownloadListResponse { downloads }).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn delete_download(
    State(state): State<AppState>,
    Path((id, media_id)): Path<(String, String)>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let device_id = match authorized_device_id(&id, &auth) {
        Ok(device_id) => device_id,
        Err(response) => return *response,
    };

    run_idempotent(&state, &headers, &uri, "DELETE", &auth, async {
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };
        match store.delete_download(device_id, &media_id).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    })
    .await
}

async fn start_queue(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };

    run_idempotent(&state, &headers, &uri, "POST", &auth, async {
        let raw = String::from_utf8_lossy(&body);
        let request = match StartQueueRequest::parse_json(&raw) {
            Ok(request) => request,
            Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        };

        match request.seed_kind.as_str() {
            "song" => {
                let video_id = match request.song_seed_video_id() {
                    Ok(video_id) => video_id,
                    Err(err) => {
                        return legacy_json_error(StatusCode::BAD_GATEWAY, err.legacy_error_code());
                    }
                };
                let Some(yt) = &state.yt else {
                    return legacy_json_error(
                        StatusCode::BAD_GATEWAY,
                        LegacyRequestError::SeedUnavailable.legacy_error_code(),
                    );
                };
                let items =
                    match innertube::expand_radio(yt.as_ref(), video_id, MIN_QUEUE_ITEMS).await {
                        Ok(items) => items,
                        Err(_) => {
                            return legacy_json_error(
                                StatusCode::BAD_GATEWAY,
                                LegacyRequestError::SeedUnavailable.legacy_error_code(),
                            );
                        }
                    };
                if items.is_empty() {
                    return legacy_json_error(StatusCode::UNPROCESSABLE_ENTITY, "empty_queue");
                }
                let Some(store) = &state.store else {
                    return legacy_json_error(StatusCode::UNPROCESSABLE_ENTITY, "empty_queue");
                };
                match store
                    .create_queue(
                        auth.user_id,
                        auth.device_id,
                        &request.seed_kind,
                        &request.seed_id,
                        &request.title,
                        &items,
                    )
                    .await
                {
                    Ok(session) => Json(QueueResponse::from(&session)).into_response(),
                    Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
                }
            }
            "shuffle_liked" => {
                let Some(store) = &state.store else {
                    return legacy_json_error(StatusCode::UNPROCESSABLE_ENTITY, "empty_queue");
                };
                let liked = match store.list_liked_songs(auth.user_id).await {
                    Ok(liked) => liked,
                    Err(_) => {
                        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
                    }
                };
                let items = build_automix(&liked, shuffle_seed());
                if items.is_empty() {
                    return legacy_json_error(StatusCode::UNPROCESSABLE_ENTITY, "empty_queue");
                }
                match store
                    .create_queue(
                        auth.user_id,
                        auth.device_id,
                        &request.seed_kind,
                        &request.seed_id,
                        &request.title,
                        &items,
                    )
                    .await
                {
                    Ok(session) => Json(QueueResponse::from(&session)).into_response(),
                    Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
                }
            }
            _ => legacy_json_error(
                StatusCode::BAD_REQUEST,
                LegacyRequestError::InvalidSeedKind.legacy_error_code(),
            ),
        }
    })
    .await
}

async fn get_queue(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let queue_id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => return legacy_json_error(StatusCode::BAD_REQUEST, "invalid_id"),
    };
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::NOT_FOUND, "not_found");
    };
    match store.get_queue(queue_id, auth.user_id).await {
        Ok(Some(session)) => Json(QueueResponse::from(&session)).into_response(),
        Ok(None) => legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

async fn get_next(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let query = uri.query().unwrap_or_default();
    let queue_id_raw = query_param(query, "queue_id");
    let position_raw = query_param(query, "position");
    let next_query = match NextQuery::parse(queue_id_raw.as_deref(), position_raw.as_deref()) {
        Ok(query) => query,
        Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
    };
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::NOT_FOUND, "not_found");
    };
    let session = match store.get_queue(next_query.queue_id, auth.user_id).await {
        Ok(Some(session)) => session,
        Ok(None) => return legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
        Err(_) => return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    if next_query.position >= session.items.len() {
        return legacy_json_error(StatusCode::NOT_FOUND, "position_out_of_range");
    }
    let current_item = &session.items[next_query.position];
    let current = match resolve_queue_item(&state, current_item, false).await {
        Ok(current) => current,
        Err(ResolveMediaError::Unavailable) => {
            return legacy_json_error(StatusCode::GONE, "current_unavailable");
        }
        Err(ResolveMediaError::Failed) => {
            return legacy_json_error(StatusCode::BAD_GATEWAY, "resolve_failed");
        }
    };
    let current_core = resolved_response_to_core(&current);
    let decision = match next_window(
        &session,
        next_query.position,
        current_core,
        DEFAULT_LOOKAHEAD_COUNT,
        RecommendationSource::Remote,
    ) {
        Ok(decision) => decision,
        Err(_) => return legacy_json_error(StatusCode::NOT_FOUND, "position_out_of_range"),
    };
    let lookahead = resolve_lookahead_items(&state, &decision.lookahead).await;
    Json(NextResponse::from_decision_with_streams(
        &decision,
        Some(current),
        lookahead,
    ))
    .into_response()
}

async fn streams_proxy(State(state): State<AppState>, uri: Uri, headers: HeaderMap) -> Response {
    let Some(proxy) = &state.proxy else {
        return StatusCode::NOT_FOUND.into_response();
    };
    proxy
        .serve(
            query_param(uri.query().unwrap_or_default(), "token").as_deref(),
            &headers,
        )
        .await
}

async fn resolve_stream(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };

    run_idempotent(&state, &headers, &uri, "POST", &auth, async {
        let raw = String::from_utf8_lossy(&body);
        match ResolveStreamRequest::parse_json(&raw) {
            Ok(request) => match resolve_media_id(&state, &request.media_id, request.proxy).await {
                Ok(resolved) => Json(ResolvedStreamResponse::from(&resolved)).into_response(),
                Err(ResolveMediaError::Unavailable) => {
                    legacy_json_error(StatusCode::GONE, "unavailable")
                }
                Err(ResolveMediaError::Failed) => {
                    legacy_json_error(StatusCode::BAD_GATEWAY, "resolve_failed")
                }
            },
            Err(err) => legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        }
    })
    .await
}

async fn serve_album_art(
    State(state): State<AppState>,
    Path(album_media_id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }
    let size = match album_art_size(uri.query()) {
        Ok(size) => size,
        Err(response) => return *response,
    };
    let path = FsPath::new(&state.data_dir)
        .join("art")
        .join(album_media_id)
        .join(format!("{size}.jpg"));
    match serve_local_file(
        &path.to_string_lossy(),
        headers
            .get(header::RANGE)
            .and_then(|value| value.to_str().ok()),
    )
    .await
    {
        Ok(response) => response,
        Err(StreamFileError::NotFound) => legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
        Err(StreamFileError::InvalidRange { len }) => range_not_satisfiable(len),
        Err(StreamFileError::Internal) => {
            legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
        }
    }
}

async fn song_hash(
    State(state): State<AppState>,
    Path(media_id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let path = match store.song_file_lookup(&media_id).await {
        Ok(SongFileLookup::Path(path)) => path,
        Ok(SongFileLookup::Missing) => {
            return legacy_json_error(StatusCode::NOT_FOUND, "not_found");
        }
        Ok(SongFileLookup::NotLocal) => {
            return legacy_json_error(StatusCode::NOT_FOUND, "not_local");
        }
        Err(_) => return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    let (sha256, bytes) = match hash_file(&path) {
        Ok(hash) => hash,
        Err(HashFileError::NotFound) => {
            return legacy_json_error(StatusCode::NOT_FOUND, "not_found");
        }
        Err(HashFileError::Internal) => {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        }
    };
    Json(SongHashResponse {
        media_id,
        sha256,
        bytes,
    })
    .into_response()
}

async fn stream_song(
    State(state): State<AppState>,
    Path(media_id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::NOT_FOUND, "not_found");
    };
    let Some(path) = (match store.song_stream_path(&media_id).await {
        Ok(path) => path,
        Err(_) => return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }) else {
        return legacy_json_error(StatusCode::NOT_FOUND, "not_found");
    };
    match serve_local_file(
        &path,
        headers
            .get(header::RANGE)
            .and_then(|value| value.to_str().ok()),
    )
    .await
    {
        Ok(response) => response,
        Err(StreamFileError::NotFound) => legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
        Err(StreamFileError::InvalidRange { len }) => range_not_satisfiable(len),
        Err(StreamFileError::Internal) => {
            legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
        }
    }
}

fn query_token(query: Option<&str>) -> Option<String> {
    query_param(query.unwrap_or_default(), "token").filter(|value| !value.is_empty())
}

fn query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if pair.is_empty() || pair.contains(';') {
            continue;
        }
        let (candidate, value) = pair.split_once('=').unwrap_or((pair, ""));
        let Ok(candidate) = form_decode(candidate) else {
            continue;
        };
        if candidate != key {
            continue;
        }
        let Ok(value) = form_decode(value) else {
            continue;
        };
        return Some(value);
    }
    None
}

fn pagination(query: Option<&str>) -> (i64, i64) {
    let query = query.unwrap_or_default();
    let limit = query_param(query, "limit")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|value| *value > 0 && *value <= 100)
        .unwrap_or(20);
    let offset = query_param(query, "offset")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|value| *value >= 0)
        .unwrap_or(0);
    (limit, offset)
}

fn bool_param(query: Option<&str>, key: &str) -> bool {
    let Some(value) = query_param(query.unwrap_or_default(), key) else {
        return false;
    };
    matches!(value.as_str(), "1" | "t" | "T" | "true" | "TRUE" | "True")
}

fn album_art_size(query: Option<&str>) -> ResponseResult<i32> {
    let Some(raw) = query_param(query.unwrap_or_default(), "size") else {
        return Ok(512);
    };
    match raw.parse::<i32>() {
        Ok(size @ (256 | 512 | 1024)) => Ok(size),
        _ => Err(Box::new(legacy_json_error(
            StatusCode::BAD_REQUEST,
            "invalid_size",
        ))),
    }
}

fn search_limit(query: &str) -> i64 {
    query_param(query, "limit")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(25))
        .unwrap_or(20)
}

fn admin_audit_limit(query: Option<&str>) -> i64 {
    query_param(query.unwrap_or_default(), "limit")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|value| *value > 0 && *value <= 500)
        .unwrap_or(100)
}

fn decoded_query_param(query: &str, key: &str) -> Option<String> {
    query_param(query, key)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn scrobble_qualifies(total_played_ms: i32, duration_ms: i32) -> bool {
    if total_played_ms >= 30_000 {
        return true;
    }
    duration_ms > 0 && (total_played_ms as f64) >= (duration_ms as f64 * 0.5)
}

fn search_response_from_page(
    query: &str,
    page: innertube::SearchPage,
    limit: i64,
) -> SearchResponse {
    let limit = usize::try_from(limit.max(0)).unwrap_or_default();
    let mut songs = Vec::with_capacity(page.songs.len().min(limit));
    for song in page.songs {
        if songs.len() >= limit {
            break;
        }
        if song.video_id.is_empty() || song.title.is_empty() {
            continue;
        }
        songs.push(SearchSongResponse {
            media_id: format!("yt:{}", song.video_id.trim_start_matches("yt:")),
            source: "yt".into(),
            title: song.title,
            artists: song.artists,
            thumbnail_url: non_empty_string(song.thumbnail_url),
            duration_ms: song.duration_ms,
        });
    }

    let mut albums = Vec::with_capacity(page.albums.len().min(limit));
    for album in page.albums {
        if albums.len() >= limit {
            break;
        }
        if album.browse_id.is_empty() || album.title.is_empty() {
            continue;
        }
        albums.push(SearchAlbumResponse {
            browse_id: album.browse_id,
            title: album.title,
            artists: album.artists,
            thumbnail_url: non_empty_string(album.thumbnail_url),
        });
    }

    let mut artists = Vec::with_capacity(page.artists.len().min(limit));
    for artist in page.artists {
        if artists.len() >= limit {
            break;
        }
        if artist.browse_id.is_empty() || artist.name.is_empty() {
            continue;
        }
        artists.push(SearchArtistResponse {
            browse_id: artist.browse_id,
            name: artist.name,
            thumbnail_url: non_empty_string(artist.thumbnail_url),
        });
    }

    SearchResponse {
        query: query.to_string(),
        songs,
        albums,
        artists,
        continuation: page.continuation.filter(|value| !value.is_empty()),
    }
}

fn non_empty_string(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

async fn youtube_home_section(
    state: &AppState,
    store: &PostgresStore,
    user_id: Uuid,
    _hide_explicit: bool,
    _hide_video: bool,
    _hide_shorts: bool,
) -> Option<(HomeSectionResponse, Vec<String>)> {
    let yt = state.yt.as_ref()?;
    let page = tokio::time::timeout(Duration::from_secs(8), yt.browse(YT_HOME_BROWSE_ID, None))
        .await
        .ok()?
        .ok()?;

    let mut candidates = Vec::new();
    for section in page.sections {
        for song in section.songs {
            if let Some(candidate) = remote_home_candidate_from_song(song, 0.5) {
                candidates.push(candidate);
            }
        }
    }
    let impressions = store
        .recent_impression_counts(user_id)
        .await
        .unwrap_or_default();
    let items = rank_remote_home(candidates, &impressions, YT_HOME_LIMIT);
    if items.is_empty() {
        return None;
    }
    Some((
        HomeSectionResponse {
            id: "yt_home".into(),
            title: "From YouTube Music".into(),
            kind: "yt_home".into(),
            seed: None,
            items,
        },
        page.chips,
    ))
}

async fn similar_artist_sections(
    state: &AppState,
    store: &PostgresStore,
    user_id: Uuid,
    _hide_explicit: bool,
    _hide_video: bool,
    _hide_shorts: bool,
) -> Vec<HomeSectionResponse> {
    let Some(yt) = state.yt.as_ref() else {
        return vec![];
    };
    let artists = store
        .most_played_artists(user_id, SIMILAR_ARTIST_SECTIONS)
        .await
        .unwrap_or_default();
    if artists.is_empty() {
        return vec![];
    }
    let impressions = store
        .recent_impression_counts(user_id)
        .await
        .unwrap_or_default();
    let mut sections = Vec::new();
    for artist in artists {
        if artist.artist_id.is_empty() {
            continue;
        }
        let browse_id = artist
            .artist_id
            .strip_prefix("yt:")
            .unwrap_or(&artist.artist_id);
        let page =
            match tokio::time::timeout(Duration::from_secs(8), yt.browse(browse_id, None)).await {
                Ok(Ok(page)) => page,
                Ok(Err(_)) | Err(_) => continue,
            };
        let candidates = page
            .sections
            .into_iter()
            .flat_map(|section| section.songs)
            .filter_map(|song| remote_home_candidate_from_song(song, 0.6))
            .collect::<Vec<_>>();
        let items = rank_remote_home(candidates, &impressions, SIMILAR_ARTIST_LIMIT);
        if items.is_empty() {
            continue;
        }
        sections.push(HomeSectionResponse {
            id: format!("similar_artist:{browse_id}"),
            title: format!("Similar to {}", artist.artist_name),
            kind: "similar_artist".into(),
            seed: Some(artist.artist_name),
            items,
        });
    }
    sections
}

async fn daily_discover_section(
    state: &AppState,
    store: &PostgresStore,
    user_id: Uuid,
    _hide_explicit: bool,
    _hide_video: bool,
    _hide_shorts: bool,
) -> Option<HomeSectionResponse> {
    let yt = state.yt.as_ref()?;
    let seeds = store
        .liked_yt_seed_media_ids(user_id, DAILY_DISCOVER_SEEDS as i64)
        .await
        .ok()?;
    if seeds.is_empty() {
        return None;
    }
    let seed_set = seeds
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for seed in seeds {
        let video_id = seed.strip_prefix("yt:").unwrap_or(&seed);
        let page = match tokio::time::timeout(Duration::from_secs(8), yt.next(video_id, None)).await
        {
            Ok(Ok(page)) => page,
            Ok(Err(_)) | Err(_) => continue,
        };
        for song in page.related {
            let Some(candidate) = remote_home_candidate_from_song(song, 0.7) else {
                continue;
            };
            if seed_set.contains(&candidate.media_id) || !seen.insert(candidate.media_id.clone()) {
                continue;
            }
            candidates.push(candidate);
        }
    }

    let impressions = store
        .recent_impression_counts(user_id)
        .await
        .unwrap_or_default();
    let items = rank_remote_home(candidates, &impressions, DAILY_DISCOVER_LIMIT);
    if items.is_empty() {
        return None;
    }
    Some(HomeSectionResponse {
        id: "daily_discover".into(),
        title: "Daily Discover".into(),
        kind: "daily_discover".into(),
        seed: None,
        items,
    })
}

async fn community_playlists_section(
    state: &AppState,
    store: &PostgresStore,
    user_id: Uuid,
    _hide_explicit: bool,
    _hide_video: bool,
    _hide_shorts: bool,
) -> Option<HomeSectionResponse> {
    let yt = state.yt.as_ref()?;
    let page =
        match tokio::time::timeout(Duration::from_secs(8), yt.search("popular music playlists"))
            .await
        {
            Ok(Ok(page)) => page,
            Ok(Err(_)) | Err(_) => return None,
        };
    let candidates = page
        .songs
        .into_iter()
        .filter_map(|song| remote_home_candidate_from_song(song, 0.4))
        .collect::<Vec<_>>();
    let impressions = store
        .recent_impression_counts(user_id)
        .await
        .unwrap_or_default();
    let items = rank_remote_home(candidates, &impressions, COMMUNITY_PLAYLIST_LIMIT);
    if items.is_empty() {
        return None;
    }
    Some(HomeSectionResponse {
        id: "community_playlists".into(),
        title: "Community Playlists".into(),
        kind: "community_playlists".into(),
        seed: None,
        items,
    })
}

#[derive(Clone, Debug)]
struct RemoteHomeCandidate {
    media_id: String,
    title: String,
    artists: Vec<String>,
    duration_ms: i32,
    thumbnail_url: Option<String>,
    remote_confidence: f64,
}

fn remote_home_candidate_from_song(
    song: innertube::SongItem,
    remote_confidence: f64,
) -> Option<RemoteHomeCandidate> {
    if song.video_id.is_empty() {
        return None;
    }
    Some(RemoteHomeCandidate {
        media_id: format!("yt:{}", song.video_id.trim_start_matches("yt:")),
        title: song.title,
        artists: song.artists,
        duration_ms: song.duration_ms,
        thumbnail_url: non_empty_string(song.thumbnail_url),
        remote_confidence,
    })
}

fn rank_remote_home(
    candidates: Vec<RemoteHomeCandidate>,
    impressions: &std::collections::HashMap<String, i32>,
    limit: usize,
) -> Vec<HomeItemResponse> {
    if limit == 0 || candidates.is_empty() {
        return vec![];
    }

    let mut seen = std::collections::HashSet::new();
    let mut pool = candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.media_id.clone()))
        .map(|candidate| {
            let base = remote_home_score(
                &candidate,
                impressions
                    .get(&candidate.media_id)
                    .copied()
                    .unwrap_or_default(),
                0.0,
            );
            (candidate, base)
        })
        .collect::<Vec<_>>();
    pool.sort_by(|(_, left), (_, right)| right.total_cmp(left));

    let mut used = vec![false; pool.len()];
    let mut artist_counts = std::collections::HashMap::<String, i32>::new();
    let mut out = Vec::with_capacity(limit.min(pool.len()));
    while out.len() < limit {
        let mut best = None;
        let mut best_score = f64::NEG_INFINITY;
        for (index, (candidate, _)) in pool.iter().enumerate() {
            if used[index] {
                continue;
            }
            let score = remote_home_score(
                candidate,
                impressions
                    .get(&candidate.media_id)
                    .copied()
                    .unwrap_or_default(),
                diversity_boost(candidate, &artist_counts),
            );
            if score > best_score {
                best = Some(index);
                best_score = score;
            }
        }
        let Some(index) = best else {
            break;
        };
        used[index] = true;
        let candidate = pool[index].0.clone();
        let artist = candidate.artists.first().cloned().unwrap_or_default();
        *artist_counts.entry(artist).or_default() += 1;
        out.push(HomeItemResponse {
            media_id: candidate.media_id,
            title: candidate.title,
            artists: candidate.artists,
            album_id: None,
            duration_ms: candidate.duration_ms,
            source: "yt".into(),
            thumbnail_url: candidate.thumbnail_url,
            score: best_score as f32,
        });
    }
    out
}

fn remote_home_score(candidate: &RemoteHomeCandidate, impressions: i32, diversity: f64) -> f64 {
    0.35 * 0.6
        + 0.20 * 0.0
        + 0.15 * 0.0
        + 0.15 * novelty_score(impressions)
        + 0.10 * candidate.remote_confidence.clamp(0.0, 1.0)
        + 0.05 * diversity.clamp(0.0, 1.0)
}

fn novelty_score(impressions: i32) -> f64 {
    if impressions <= 0 {
        return 1.0;
    }
    if impressions >= 5 {
        return 0.0;
    }
    1.0 - f64::from(impressions) / 5.0
}

fn diversity_boost(
    candidate: &RemoteHomeCandidate,
    artist_counts: &std::collections::HashMap<String, i32>,
) -> f64 {
    let artist = candidate.artists.first().cloned().unwrap_or_default();
    let artist_count = artist_counts.get(&artist).copied().unwrap_or_default();
    let artist_score = 1.0 / f64::from(1 + artist_count);
    (artist_score + 1.0) / 2.0
}

async fn resolve_queue_item(
    state: &AppState,
    item: &sunflower_core::QueueItem,
    prefer_proxy: bool,
) -> Result<ResolvedStreamResponse, ResolveMediaError> {
    let resolved = resolve_media_id(state, &item.media_id.0, prefer_proxy).await?;
    let mut response = ResolvedStreamResponse::from(&resolved);
    response.title = item.title.clone();
    response.artists = item.artists.clone();
    response.duration_ms = item.duration_ms;
    Ok(response)
}

async fn resolve_lookahead_items(
    state: &AppState,
    items: &[sunflower_core::QueueItem],
) -> Vec<ResolvedStreamResponse> {
    let mut resolved = Vec::with_capacity(items.len());
    for item in items {
        match resolve_queue_item(state, item, false).await {
            Ok(stream) => resolved.push(stream),
            Err(_) => resolved.push(ResolvedStreamResponse::from(item)),
        }
    }
    resolved
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResolveMediaError {
    Unavailable,
    Failed,
}

async fn resolve_media_id(
    state: &AppState,
    media_id: &str,
    prefer_proxy: bool,
) -> Result<ResolvedStream, ResolveMediaError> {
    let Some((source, external_id)) = media_id.split_once(':') else {
        return Err(ResolveMediaError::Failed);
    };
    if external_id.is_empty() {
        return Err(ResolveMediaError::Failed);
    }
    match source {
        "local" => Ok(ResolvedStream {
            media_id: MediaId::new(media_id),
            source: "local".to_string(),
            stream_url: format!("/api/v1/library/songs/{}/stream", path_segment(media_id)),
            stream_expires_at: None,
            mime_type: None,
            content_length: None,
            loudness_db: None,
            playback_tracking_url: None,
            metadata: Value::Null,
        }),
        "yt" => resolve_youtube_media_id(state, media_id, external_id, prefer_proxy).await,
        _ => Err(ResolveMediaError::Failed),
    }
}

async fn resolve_youtube_media_id(
    state: &AppState,
    media_id: &str,
    video_id: &str,
    prefer_proxy: bool,
) -> Result<ResolvedStream, ResolveMediaError> {
    let Some(yt) = &state.yt else {
        return Err(ResolveMediaError::Unavailable);
    };
    let player = yt
        .player(video_id)
        .await
        .map_err(|_| ResolveMediaError::Failed)?;
    let stream = player.stream;
    if stream.url.is_empty() {
        return Err(ResolveMediaError::Unavailable);
    }
    let expires_at = innertube::expiry_from_url(&stream.url);
    let (source, stream_url) = if prefer_proxy || state.proxy_youtube {
        match &state.proxy {
            Some(proxy) => {
                let token = match expires_at {
                    Some(expires_at) => {
                        proxy.sign_until(&stream.url, system_time_from_utc(expires_at))
                    }
                    None => proxy.sign(&stream.url),
                };
                (
                    "proxy".to_string(),
                    format!("/api/v1/streams/proxy?token={token}"),
                )
            }
            None => ("youtube".to_string(), stream.url.clone()),
        }
    } else {
        ("youtube".to_string(), stream.url.clone())
    };
    let loudness_db = non_zero_f64_to_f32(stream.loudness);
    let metadata = youtube_stream_metadata(&stream);
    Ok(ResolvedStream {
        media_id: MediaId::new(media_id),
        source,
        stream_url,
        stream_expires_at: expires_at,
        mime_type: non_empty_string(stream.mime_type),
        content_length: None,
        loudness_db,
        playback_tracking_url: None,
        metadata,
    })
}

fn resolved_response_to_core(response: &ResolvedStreamResponse) -> ResolvedStream {
    ResolvedStream {
        media_id: MediaId::new(response.media_id.clone()),
        source: response.source.clone(),
        stream_url: response.stream_url.clone(),
        stream_expires_at: response
            .stream_expires_at
            .as_deref()
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|time| time.with_timezone(&Utc)),
        mime_type: non_empty_string(response.mime_type.clone()),
        content_length: response.content_length,
        loudness_db: response.loudness_db,
        playback_tracking_url: response.playback_tracking_url.clone(),
        metadata: response.metadata.clone(),
    }
}

fn youtube_stream_metadata(stream: &innertube::StreamUrl) -> Value {
    let mut metadata = serde_json::Map::new();
    if stream.itag > 0 {
        metadata.insert("itag".to_string(), json!(stream.itag));
    }
    if stream.bitrate > 0 {
        metadata.insert("bitrate".to_string(), json!(stream.bitrate));
    }
    if metadata.is_empty() {
        Value::Null
    } else {
        Value::Object(metadata)
    }
}

fn non_zero_f64_to_f32(value: f64) -> Option<f32> {
    if value == 0.0 || !value.is_finite() {
        None
    } else {
        Some(value as f32)
    }
}

fn system_time_from_utc(time: DateTime<Utc>) -> SystemTime {
    let Ok(seconds) = u64::try_from(time.timestamp()) else {
        return UNIX_EPOCH;
    };
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn shuffle_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StreamFileError {
    NotFound,
    InvalidRange { len: u64 },
    Internal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HashFileError {
    NotFound,
    Internal,
}

fn hash_file(path: &str) -> Result<(String, u64), HashFileError> {
    let mut file = fs::File::open(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            HashFileError::NotFound
        } else {
            HashFileError::Internal
        }
    })?;
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 32 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| HashFileError::Internal)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes += read as u64;
    }
    let digest = hasher.finalize();
    Ok((hex_lower_bytes(&digest), bytes))
}

fn hex_lower_bytes(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(TABLE[(byte >> 4) as usize] as char);
        out.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    out
}

fn serve_static_bytes(
    bytes: &'static [u8],
    content_type: &'static str,
    range_header: Option<&str>,
) -> Response {
    let len = bytes.len() as u64;
    let range = match range_header {
        Some(raw) => match parse_single_range(raw, len) {
            Some(range) => Some(range),
            None => return range_not_satisfiable(len),
        },
        None => None,
    };
    let (status, start, end) = match range {
        Some((start, end)) => (StatusCode::PARTIAL_CONTENT, start, end),
        None => (StatusCode::OK, 0, len.saturating_sub(1)),
    };
    let body_len = if len == 0 { 0 } else { end - start + 1 };
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, body_len.to_string());
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    let body = if body_len == 0 {
        Body::empty()
    } else {
        Body::from(bytes[start as usize..=end as usize].to_vec())
    };
    builder
        .body(body)
        .unwrap_or_else(|_| legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"))
}

async fn serve_local_file(
    path: &str,
    range_header: Option<&str>,
) -> Result<Response, StreamFileError> {
    let metadata = tokio::fs::metadata(path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            StreamFileError::NotFound
        } else {
            StreamFileError::Internal
        }
    })?;
    if !metadata.is_file() {
        return Err(StreamFileError::NotFound);
    }
    let len = metadata.len();
    let range = match range_header {
        Some(raw) => {
            Some(parse_single_range(raw, len).ok_or(StreamFileError::InvalidRange { len })?)
        }
        None => None,
    };
    let (status, start, end) = match range {
        Some((start, end)) => (StatusCode::PARTIAL_CONTENT, start, end),
        None => (StatusCode::OK, 0, len.saturating_sub(1)),
    };
    let body_len = if len == 0 { 0 } else { end - start + 1 };
    let mut file = tokio::fs::File::open(path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            StreamFileError::NotFound
        } else {
            StreamFileError::Internal
        }
    })?;
    if start > 0 {
        file.seek(SeekFrom::Start(start))
            .await
            .map_err(|_| StreamFileError::Internal)?;
    }

    let mut builder = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, body_len.to_string());
    if let Ok(modified) = metadata.modified() {
        builder = builder.header(header::LAST_MODIFIED, http_date(modified));
    }
    if let Some(content_type) = content_type_for_path(path) {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    builder
        .body(stream_file_body(file, body_len))
        .map_err(|_| StreamFileError::Internal)
}

fn stream_file_body(file: tokio::fs::File, body_len: u64) -> Body {
    if body_len == 0 {
        return Body::empty();
    }

    let chunks = stream::try_unfold((file, body_len), |(mut file, remaining)| async move {
        if remaining == 0 {
            return Ok(None);
        }
        let chunk_len = remaining.min(FILE_STREAM_CHUNK_SIZE as u64) as usize;
        let mut chunk = vec![0u8; chunk_len];
        let read = file.read(&mut chunk).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "file ended before advertised content length",
            ));
        }
        chunk.truncate(read);
        let remaining = remaining.saturating_sub(read as u64);
        Ok(Some((Bytes::from(chunk), (file, remaining))))
    });

    Body::from_stream(chunks)
}

fn parse_single_range(raw: &str, len: u64) -> Option<(u64, u64)> {
    let spec = raw.strip_prefix("bytes=")?;
    if spec.contains(',') || len == 0 {
        return None;
    }
    let (start_raw, end_raw) = spec.split_once('-')?;
    if start_raw.is_empty() {
        let suffix_len = end_raw.parse::<u64>().ok()?;
        if suffix_len == 0 {
            return None;
        }
        let start = len.saturating_sub(suffix_len);
        return Some((start, len - 1));
    }
    let start = start_raw.parse::<u64>().ok()?;
    if start >= len {
        return None;
    }
    let end = if end_raw.is_empty() {
        len - 1
    } else {
        end_raw.parse::<u64>().ok()?.min(len - 1)
    };
    (start <= end).then_some((start, end))
}

fn content_type_for_path(path: &str) -> Option<&'static str> {
    match FsPath::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("mp3") => Some("audio/mpeg"),
        Some("flac") => Some("audio/flac"),
        Some("m4a") => Some("audio/mp4"),
        Some("ogg" | "opus") => Some("audio/ogg"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        _ => None,
    }
}

fn http_date(time: SystemTime) -> String {
    let time = DateTime::<Utc>::from(time);
    time.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get_all(header::COOKIE).iter().find_map(|value| {
        let raw = value.to_str().ok()?;
        raw.trim_matches(http_space).split(';').find_map(|part| {
            let part = part.trim_matches(http_space);
            if part.is_empty() {
                return None;
            }
            let (candidate, value) = part.split_once('=').unwrap_or((part, ""));
            let candidate = candidate.trim_matches(http_space);
            if candidate != name {
                return None;
            }
            parse_cookie_value(value).map(str::to_string)
        })
    })
}

fn http_space(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

fn parse_cookie_value(raw: &str) -> Option<&str> {
    let value = if raw.len() > 1 && raw.starts_with('"') && raw.ends_with('"') {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };
    value
        .bytes()
        .all(|byte| (0x20..0x7f).contains(&byte) && byte != b'"' && byte != b';' && byte != b'\\')
        .then_some(value)
}

fn append_cookie(response: &mut Response, cookie: String) {
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
}

fn admin_cookie(
    name: &str,
    value: &str,
    expires_at: DateTime<Utc>,
    http_only: bool,
    secure: bool,
) -> String {
    let mut cookie = format!(
        "{name}={value}; Path=/; Expires={}",
        expires_at.format("%a, %d %b %Y %H:%M:%S GMT")
    );
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if secure {
        cookie.push_str("; Secure");
    }
    cookie.push_str("; SameSite=Lax");
    cookie
}

fn clear_admin_cookie(name: &str, http_only: bool, secure: bool) -> String {
    let mut cookie = format!("{name}=; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=0");
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if secure {
        cookie.push_str("; Secure");
    }
    cookie.push_str("; SameSite=Lax");
    cookie
}

fn api_rfc3339_seconds(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        == Some("https")
}

fn server_base_url(state: &AppState, headers: &HeaderMap) -> String {
    if !state.public_base_url.is_empty() {
        return state.public_base_url.trim_end_matches('/').to_string();
    }
    let scheme = if is_https(headers) { "https" } else { "http" };
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|host| !host.is_empty())
        .unwrap_or("localhost");
    format!("{scheme}://{host}")
}

fn range_not_satisfiable(len: u64) -> Response {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{len}"))
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header("x-content-type-options", "nosniff")
        .body(Body::from("invalid range: failed to overlap\n"))
        .unwrap_or_else(|_| legacy_json_error(StatusCode::RANGE_NOT_SATISFIABLE, "invalid_range"))
}

fn legacy_json_error(status: StatusCode, code: &str) -> Response {
    legacy_json_response(status, serde_json::json!({ "error": code }))
}

fn legacy_json_response(status: StatusCode, value: serde_json::Value) -> Response {
    let body = serde_json::to_vec(&value).unwrap_or_else(|_| b"{\"error\":\"internal\"}".to_vec());
    let mut body = escape_legacy_json_html_bytes(&body);
    body.push(b'\n');
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn escape_legacy_json_html_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'<' => {
                out.extend_from_slice(br"\u003c");
                index += 1;
            }
            b'>' => {
                out.extend_from_slice(br"\u003e");
                index += 1;
            }
            b'&' => {
                out.extend_from_slice(br"\u0026");
                index += 1;
            }
            0xe2 if index + 2 < bytes.len()
                && bytes[index + 1] == 0x80
                && bytes[index + 2] == 0xa8 =>
            {
                out.extend_from_slice(br"\u2028");
                index += 3;
            }
            0xe2 if index + 2 < bytes.len()
                && bytes[index + 1] == 0x80
                && bytes[index + 2] == 0xa9 =>
            {
                out.extend_from_slice(br"\u2029");
                index += 3;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    out
}

fn legacy_http_error(status: StatusCode, code: &str) -> Response {
    let body = format!("{{\"error\":\"{code}\"}}\n");
    (
        status,
        [
            ("content-type", "text/plain; charset=utf-8"),
            ("x-content-type-options", "nosniff"),
        ],
        body,
    )
        .into_response()
}

fn auth_error_response(err: AuthStoreError) -> Response {
    match err {
        AuthStoreError::SetupDisabled => legacy_json_error(StatusCode::FORBIDDEN, "setup_disabled"),
        AuthStoreError::InvalidSetupToken => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_setup_token")
        }
        AuthStoreError::WeakPassword => legacy_json_error(StatusCode::BAD_REQUEST, "weak_password"),
        AuthStoreError::SetupRequired => legacy_json_error(StatusCode::FORBIDDEN, "setup_required"),
        AuthStoreError::InvalidPassword => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_password")
        }
        AuthStoreError::MissingAdminSession => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "missing_admin_session")
        }
        AuthStoreError::InvalidAdminSession => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_admin_session")
        }
        AuthStoreError::PairingRequired => {
            legacy_json_error(StatusCode::FORBIDDEN, "pairing_required")
        }
        AuthStoreError::InvalidPairingCode => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_pairing_code")
        }
        AuthStoreError::InvalidToken => {
            legacy_http_error(StatusCode::UNAUTHORIZED, "invalid_token")
        }
        AuthStoreError::DeviceRevoked => {
            legacy_http_error(StatusCode::UNAUTHORIZED, "device_revoked")
        }
        AuthStoreError::Backend(_) => {
            legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
        }
    }
}

fn admin_auth_error_response(err: AuthStoreError) -> Response {
    match err {
        AuthStoreError::SetupRequired => legacy_json_error(StatusCode::FORBIDDEN, "setup_required"),
        AuthStoreError::InvalidPassword => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_password")
        }
        AuthStoreError::MissingAdminSession => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "missing_admin_session")
        }
        AuthStoreError::InvalidAdminSession => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_admin_session")
        }
        AuthStoreError::Backend(_) => {
            legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
        }
        other => auth_error_response(other),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        net::SocketAddr,
        sync::{Arc, Mutex},
    };

    use axum::{
        body::{self, Bytes},
        extract::ConnectInfo,
        http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
        response::IntoResponse,
    };
    use chrono::{TimeZone, Utc};
    use futures_util::{SinkExt, StreamExt, future::BoxFuture};
    use image::ImageFormat;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use sqlx::Row;
    use sunflower_storage_postgres::{AdminSession, PostgresStore, hash_token};
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{
            Message as WsMessage, client::IntoClientRequest, http::HeaderValue as WsHeaderValue,
        },
    };
    use tower::util::ServiceExt;
    use uuid::Uuid;

    use super::{
        ADMIN_CSS, ADMIN_JS, ADMIN_STATIC_DIR_LISTING, AdminApiCsrfToken, AdminCsrfCheck, AuthMode,
        DEFAULT_DATABASE_URL, DEFAULT_LISTEN_ADDR, DEFAULT_SETUP_TOKEN, LegacyRouteConfig,
        ProxySigner, StreamProxy, admin_api_csrf_token, admin_audit_limit, admin_cookie,
        admin_form_csrf_token, album_art_size, append_legacy_json_newline, bool_param,
        clear_admin_cookie, configured_cookie_file_from, configured_data_dir,
        configured_database_url, configured_dev_open_registration, configured_listen_addr,
        configured_setup_token, cookie_value, decoded_query_param, form_value,
        go_wildcard_socket_addr, healthz, hex_lower_bytes, innertube,
        is_legacy_idempotent_mutation, legacy_allowed_methods_for_path,
        legacy_idempotent_mutating_route_patterns, legacy_json_response, legacy_url_path,
        legacy_wire_body_for_hash, pagination, parse_form, parse_request_form,
        parse_youtube_cookie_header, path_segment, query_param, query_token, rate_limit_key,
        require_admin_csrf, router_with_auth, router_with_config, router_with_state_and_config,
        router_with_state_and_config_and_hub, router_with_state_and_data_dir, router_with_store,
        scrobble_qualifies, search_limit, serve_local_file, should_proxy_youtube,
        test_router_config,
    };

    static PG_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    struct FakeInnerTube {
        home_page: innertube::HomePage,
        search_page: innertube::SearchPage,
        next_pages: Mutex<Vec<innertube::NextPage>>,
        player: innertube::PlayerResponse,
    }

    impl FakeInnerTube {
        fn with_player(player: innertube::PlayerResponse) -> Self {
            Self {
                home_page: innertube::HomePage::default(),
                search_page: innertube::SearchPage::default(),
                next_pages: Mutex::new(vec![]),
                player,
            }
        }
    }

    impl innertube::InnerTubeBackend for FakeInnerTube {
        fn browse<'a>(
            &'a self,
            _browse_id: &'a str,
            _continuation: Option<&'a str>,
        ) -> BoxFuture<'a, Result<innertube::HomePage, innertube::InnerTubeError>> {
            let page = self.home_page.clone();
            Box::pin(async move { Ok(page) })
        }

        fn search<'a>(
            &'a self,
            _query: &'a str,
        ) -> BoxFuture<'a, Result<innertube::SearchPage, innertube::InnerTubeError>> {
            let page = self.search_page.clone();
            Box::pin(async move { Ok(page) })
        }

        fn next<'a>(
            &'a self,
            _video_id: &'a str,
            _continuation: Option<&'a str>,
        ) -> BoxFuture<'a, Result<innertube::NextPage, innertube::InnerTubeError>> {
            let page = self.next_pages.lock().unwrap().remove(0);
            Box::pin(async move { Ok(page) })
        }

        fn player<'a>(
            &'a self,
            _video_id: &'a str,
        ) -> BoxFuture<'a, Result<innertube::PlayerResponse, innertube::InnerTubeError>> {
            let player = self.player.clone();
            Box::pin(async move { Ok(player) })
        }
    }

    #[tokio::test]
    async fn healthz_matches_legacy_go_contract() {
        let response = healthz().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );

        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"{\"status\":\"ok\"}\n");
        assert_eq!(body.last(), Some(&b'\n'));
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value, json!({ "status": "ok" }));
    }

    #[tokio::test]
    async fn json_responses_escape_html_like_go_encoder() {
        let direct = legacy_json_response(
            StatusCode::OK,
            json!({"title": "A < B & C > D \u{2028}\u{2029}"}),
        );
        let direct_body = body::to_bytes(direct.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            &direct_body[..],
            br#"{"title":"A \u003c B \u0026 C \u003e D \u2028\u2029"}
"#
        );

        let axum_json = axum::Json(json!({"title": "A < B & C > D"})).into_response();
        let normalized = append_legacy_json_newline(axum_json).await;
        let normalized_body = body::to_bytes(normalized.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            &normalized_body[..],
            br#"{"title":"A \u003c B \u0026 C \u003e D"}
"#
        );
    }

    #[tokio::test]
    async fn cors_headers_match_legacy_go_contract() {
        let app = router_with_auth(AuthMode::RejectAllTokens);
        let preflight = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/v1/home")
                    .header(header::ORIGIN, "http://localhost:3000")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .header(
                        header::ACCESS_CONTROL_REQUEST_HEADERS,
                        "authorization, idempotency-key",
                    )
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(preflight.status(), StatusCode::OK);
        assert_eq!(
            header_values(preflight.headers(), header::VARY),
            vec![
                "Origin",
                "Access-Control-Request-Method",
                "Access-Control-Request-Headers"
            ]
        );
        assert_eq!(
            preflight
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "*"
        );
        assert_eq!(
            preflight
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_METHODS)
                .unwrap(),
            "GET"
        );
        assert_eq!(
            preflight
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
                .unwrap(),
            "Authorization, Idempotency-Key"
        );
        assert_eq!(
            preflight
                .headers()
                .get(header::ACCESS_CONTROL_MAX_AGE)
                .unwrap(),
            "300"
        );
        assert!(
            preflight
                .headers()
                .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
                .is_none()
        );

        let origin_requested_preflight = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/v1/home")
                    .header(header::ORIGIN, "http://localhost:3000")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .header(
                        header::ACCESS_CONTROL_REQUEST_HEADERS,
                        "authorization, origin",
                    )
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(origin_requested_preflight.status(), StatusCode::OK);
        assert_eq!(
            origin_requested_preflight
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "*"
        );
        assert_eq!(
            origin_requested_preflight
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
                .unwrap(),
            "Authorization, Origin"
        );

        let health = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/healthz")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
        assert_eq!(
            header_values(health.headers(), header::VARY),
            vec!["Origin"]
        );
        assert!(
            health
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
        assert!(
            health
                .headers()
                .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
                .is_none()
        );

        let health_with_origin = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/healthz")
                    .header(header::ORIGIN, "http://localhost:3000")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health_with_origin.status(), StatusCode::OK);
        assert_eq!(
            header_values(health_with_origin.headers(), header::VARY),
            vec!["Origin"]
        );
        assert_eq!(
            health_with_origin
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            "*"
        );
        assert_eq!(
            health_with_origin
                .headers()
                .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
                .unwrap(),
            "Link"
        );

        let disallowed_head_preflight = app
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/healthz")
                    .header(header::ORIGIN, "http://localhost:3000")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "HEAD")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(disallowed_head_preflight.status(), StatusCode::OK);
        assert_eq!(
            header_values(disallowed_head_preflight.headers(), header::VARY),
            vec![
                "Origin",
                "Access-Control-Request-Method",
                "Access-Control-Request-Headers"
            ]
        );
        assert!(
            disallowed_head_preflight
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
        assert!(
            disallowed_head_preflight
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_METHODS)
                .is_none()
        );
    }

    fn header_values(headers: &HeaderMap, name: header::HeaderName) -> Vec<String> {
        headers
            .get_all(&name)
            .iter()
            .map(|value| value.to_str().unwrap().to_string())
            .collect()
    }

    fn allow_header_values(headers: &HeaderMap) -> Vec<String> {
        headers
            .get_all(header::ALLOW)
            .iter()
            .flat_map(|value| {
                value
                    .to_str()
                    .unwrap()
                    .split(',')
                    .map(str::trim)
                    .filter(|method| !method.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[tokio::test]
    async fn head_requests_match_legacy_chi_method_contract() {
        let app = router_with_auth(AuthMode::RejectAllTokens);

        let health = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::HEAD)
                    .uri("/healthz")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(allow_header_values(health.headers()), vec!["GET"]);
        assert_eq!(
            header_values(health.headers(), header::VARY),
            vec!["Origin"]
        );
        assert!(
            health
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
        let body = body::to_bytes(health.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(body.is_empty());

        let protected_get = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::HEAD)
                    .uri("/api/v1/home")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(protected_get.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(allow_header_values(protected_get.headers()), vec!["GET"]);

        let multi_method = app
            .oneshot(
                Request::builder()
                    .method(Method::HEAD)
                    .uri("/api/v1/playlists")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(multi_method.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(
            allow_header_values(multi_method.headers()),
            vec!["GET", "POST"]
        );
    }

    #[tokio::test]
    async fn method_not_allowed_allow_header_omits_axum_implicit_head() {
        let app = router_with_auth(AuthMode::RejectAllTokens);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/healthz")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(allow_header_values(response.headers()), vec!["GET"]);
    }

    #[test]
    fn legacy_allowed_methods_table_matches_go_router_contract() {
        let without_proxy = LegacyRouteConfig {
            streams_proxy_enabled: false,
        };
        let with_proxy = LegacyRouteConfig {
            streams_proxy_enabled: true,
        };

        let expected: &[(&str, &[&str])] = &[
            ("/healthz", &["GET"]),
            ("/admin/static/admin.css", &["GET"]),
            ("/admin/login", &["GET", "POST"]),
            ("/admin/", &["GET"]),
            ("/admin/logout", &["POST"]),
            ("/admin/devices", &["GET"]),
            (
                "/admin/devices/018f3f27-0000-7000-8000-000000000001/revoke",
                &["POST"],
            ),
            ("/admin/pairing/new", &["GET"]),
            ("/admin/pairing", &["POST"]),
            ("/admin/library", &["GET"]),
            ("/admin/library/scan", &["POST"]),
            ("/admin/cookies/youtube", &["GET", "POST"]),
            ("/admin/cookies/youtube/probe", &["POST"]),
            ("/admin/cookies/youtube/clear", &["POST"]),
            ("/admin/now-playing", &["GET"]),
            ("/admin/now-playing/command", &["POST"]),
            ("/admin/audit", &["GET"]),
            ("/api/v1/setup/status", &["GET"]),
            ("/api/v1/setup/owner", &["POST"]),
            ("/api/v1/auth/register-device", &["POST"]),
            ("/api/v1/admin/auth/login", &["POST"]),
            ("/api/v1/admin/auth/logout", &["POST"]),
            ("/api/v1/admin/me", &["GET"]),
            ("/api/v1/admin", &["GET"]),
            ("/api/v1/admin/status", &["GET"]),
            ("/api/v1/admin/devices", &["GET"]),
            (
                "/api/v1/admin/devices/018f3f27-0000-7000-8000-000000000001/revoke",
                &["POST"],
            ),
            ("/api/v1/admin/pairing-codes", &["POST"]),
            ("/api/v1/admin/library/status", &["GET"]),
            ("/api/v1/admin/library/scan", &["POST"]),
            ("/api/v1/admin/cookies/youtube/status", &["GET"]),
            ("/api/v1/admin/cookies/youtube", &["POST"]),
            ("/api/v1/admin/cookies/youtube/probe", &["POST"]),
            ("/api/v1/admin/cookies/youtube/clear", &["POST"]),
            ("/api/v1/admin/now-playing", &["GET"]),
            ("/api/v1/admin/now-playing/command", &["POST"]),
            ("/api/v1/admin/audit", &["GET"]),
            ("/api/v1/queue/start", &["POST"]),
            (
                "/api/v1/queue/018f3f27-0000-7000-8000-000000000010",
                &["GET"],
            ),
            ("/api/v1/next", &["GET"]),
            ("/api/v1/home", &["GET"]),
            ("/api/v1/search", &["GET"]),
            ("/api/v1/likes", &["POST"]),
            ("/api/v1/events", &["POST"]),
            ("/api/v1/impressions", &["POST"]),
            ("/api/v1/playlists", &["GET", "POST"]),
            (
                "/api/v1/playlists/018f3f27-0000-7000-8000-000000000020",
                &["GET", "PATCH", "DELETE"],
            ),
            (
                "/api/v1/playlists/018f3f27-0000-7000-8000-000000000020/items",
                &["POST"],
            ),
            (
                "/api/v1/playlists/018f3f27-0000-7000-8000-000000000020/items/local:abc",
                &["DELETE"],
            ),
            ("/api/v1/library/songs", &["GET"]),
            ("/api/v1/library/albums", &["GET"]),
            ("/api/v1/library/artists", &["GET"]),
            ("/api/v1/library/scan", &["POST"]),
            ("/api/v1/jobs/scan-1", &["GET"]),
            ("/api/v1/library/albums/local:album/art", &["GET"]),
            ("/api/v1/library/songs/local:track/hash", &["GET"]),
            ("/api/v1/library/songs/local:track/stream", &["GET"]),
            ("/api/v1/cookies/youtube/status", &["GET"]),
            ("/api/v1/cookies/youtube", &["POST"]),
            (
                "/api/v1/devices/018f3f27-0000-7000-8000-000000000001/downloads",
                &["GET", "POST"],
            ),
            (
                "/api/v1/devices/018f3f27-0000-7000-8000-000000000001/downloads/local:track",
                &["DELETE"],
            ),
            ("/api/v1/ws/now-playing", &["GET"]),
            ("/api/v1/streams/resolve", &["POST"]),
        ];

        for (path, methods) in expected {
            assert_eq!(
                legacy_allowed_methods_for_path(path, without_proxy),
                Some(*methods),
                "{path}"
            );
        }
        assert_eq!(
            legacy_allowed_methods_for_path("/api/v1/streams/proxy", without_proxy),
            None
        );
        assert_eq!(
            legacy_allowed_methods_for_path("/api/v1/streams/proxy", with_proxy),
            Some(&["GET"][..])
        );
        assert_eq!(
            legacy_allowed_methods_for_path("/api/v1/ws/command", with_proxy),
            None
        );
    }

    #[test]
    fn legacy_idempotent_mutating_routes_match_go_router_contract() {
        assert_eq!(
            legacy_idempotent_mutating_route_patterns(),
            &[
                ("POST", "/api/v1/auth/register-device"),
                ("POST", "/api/v1/library/scan"),
                ("POST", "/api/v1/cookies/youtube"),
                ("POST", "/api/v1/queue/start"),
                ("POST", "/api/v1/streams/resolve"),
                ("POST", "/api/v1/likes"),
                ("POST", "/api/v1/impressions"),
                ("POST", "/api/v1/playlists"),
                ("PATCH", "/api/v1/playlists/:id"),
                ("DELETE", "/api/v1/playlists/:id"),
                ("POST", "/api/v1/playlists/:id/items"),
                ("DELETE", "/api/v1/playlists/:id/items/:media_id"),
                ("POST", "/api/v1/devices/:id/downloads"),
                ("DELETE", "/api/v1/devices/:id/downloads/:media_id"),
                ("POST", "/api/v1/events"),
            ]
        );

        for (method, path) in [
            ("POST", "/api/v1/auth/register-device"),
            ("POST", "/api/v1/library/scan"),
            ("POST", "/api/v1/cookies/youtube"),
            ("POST", "/api/v1/queue/start"),
            ("POST", "/api/v1/streams/resolve"),
            ("POST", "/api/v1/likes"),
            ("POST", "/api/v1/impressions"),
            ("POST", "/api/v1/playlists"),
            (
                "PATCH",
                "/api/v1/playlists/018f3f27-0000-7000-8000-000000000020",
            ),
            (
                "DELETE",
                "/api/v1/playlists/018f3f27-0000-7000-8000-000000000020",
            ),
            (
                "POST",
                "/api/v1/playlists/018f3f27-0000-7000-8000-000000000020/items",
            ),
            (
                "DELETE",
                "/api/v1/playlists/018f3f27-0000-7000-8000-000000000020/items/local:abc",
            ),
            (
                "POST",
                "/api/v1/devices/018f3f27-0000-7000-8000-000000000001/downloads",
            ),
            (
                "DELETE",
                "/api/v1/devices/018f3f27-0000-7000-8000-000000000001/downloads/local:track",
            ),
            ("POST", "/api/v1/events"),
        ] {
            assert!(
                is_legacy_idempotent_mutation(method, path),
                "{method} {path}"
            );
        }

        for (method, path) in [
            ("POST", "/api/v1/setup/owner"),
            ("POST", "/api/v1/admin/auth/login"),
            ("POST", "/api/v1/admin/library/scan"),
            ("GET", "/api/v1/home"),
            ("GET", "/api/v1/playlists"),
            ("POST", "/api/v1/unknown"),
        ] {
            assert!(
                !is_legacy_idempotent_mutation(method, path),
                "{method} {path}"
            );
        }
    }

    #[tokio::test]
    async fn idempotent_device_mutations_require_valid_idempotency_key() {
        for (header_name, header_value) in [
            (None, ""),
            (Some("idempotency-key"), "not-a-uuid"),
            (
                Some("idempotency-key"),
                "018f3f27-0000-4000-8000-000000000001",
            ),
        ] {
            let app = router_with_auth(AuthMode::AllowAllForContractTests);
            let mut request = Request::builder()
                .method(Method::POST)
                .uri("/api/v1/streams/resolve")
                .header(header::AUTHORIZATION, "Bearer contract-test")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(header_name) = header_name {
                request = request.header(header_name, header_value);
            }
            let response = app
                .oneshot(
                    request
                        .body(body::Body::from(r#"{"media_id":"yt:abc"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert_json_error(response, "invalid_idempotency_key").await;
        }
    }

    #[tokio::test]
    async fn register_device_requires_valid_idempotency_key_after_json_parse() {
        let app = router_with_auth(AuthMode::RejectAllTokens);
        let invalid_json = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/register-device")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid_json.status(), StatusCode::BAD_REQUEST);
        assert_json_error(invalid_json, "invalid_request").await;

        for (header_name, header_value) in [
            (None, ""),
            (Some("idempotency-key"), "not-a-uuid"),
            (
                Some("idempotency-key"),
                "018f3f27-0000-4000-8000-000000000001",
            ),
        ] {
            let mut request = Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/register-device")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(header_name) = header_name {
                request = request.header(header_name, header_value);
            }
            let response = app
                .clone()
                .oneshot(request.body(body::Body::from("{}")).unwrap())
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert_json_error(response, "invalid_idempotency_key").await;
        }
    }

    #[tokio::test]
    async fn events_require_uuidv7_event_ids_before_ingest() {
        let app = router_with_auth(AuthMode::AllowAllForContractTests);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/events")
                    .header(header::AUTHORIZATION, "Bearer contract-test")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        json!({
                            "events": [
                                {
                                    "event_id": "e1",
                                    "kind": "play",
                                    "media_id": "local:one",
                                    "total_played_ms": 60000,
                                    "duration_ms": 120000
                                },
                                {
                                    "event_id": "018f3f27-0000-4000-8000-000000000001",
                                    "kind": "play",
                                    "media_id": "local:two",
                                    "total_played_ms": 60000,
                                    "duration_ms": 120000
                                },
                                {
                                    "event_id": " 018f3f27-0000-7000-8000-000000000002 ",
                                    "kind": "play",
                                    "media_id": "local:space",
                                    "total_played_ms": 60000,
                                    "duration_ms": 120000
                                },
                                {
                                    "event_id": "018f3f27-0000-7000-8000-000000000001",
                                    "kind": "play",
                                    "media_id": "local:three",
                                    "total_played_ms": 1000,
                                    "duration_ms": 120000
                                }
                            ]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let value = response_json(response).await;
        assert_eq!(value["results"][0]["event_id"], "e1");
        assert_eq!(value["results"][0]["accepted"], false);
        assert_eq!(value["results"][0]["reason"], "invalid_event_id");
        assert_eq!(value["results"][1]["accepted"], false);
        assert_eq!(value["results"][1]["reason"], "invalid_event_id");
        assert_eq!(value["results"][2]["accepted"], false);
        assert_eq!(value["results"][2]["reason"], "invalid_event_id");
        assert_eq!(value["results"][3]["accepted"], false);
        assert_eq!(value["results"][3]["reason"], "below_scrobble_threshold");
    }

    #[tokio::test]
    async fn events_results_preserve_request_order_not_occurred_at_order() {
        let app = router_with_auth(AuthMode::AllowAllForContractTests);
        let first_event_id = Uuid::now_v7().to_string();
        let second_event_id = Uuid::now_v7().to_string();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/events")
                    .header(header::AUTHORIZATION, "Bearer contract-test")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        json!({
                            "events": [
                                {
                                    "event_id": first_event_id,
                                    "kind": "play",
                                    "media_id": "local:newer",
                                    "occurred_at": "2026-07-01T02:00:00Z",
                                    "total_played_ms": 1000,
                                    "duration_ms": 120000
                                },
                                {
                                    "event_id": second_event_id,
                                    "kind": "play",
                                    "media_id": "local:older",
                                    "occurred_at": "2026-07-01T01:00:00Z",
                                    "total_played_ms": 1000,
                                    "duration_ms": 120000
                                }
                            ]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let value = response_json(response).await;
        assert_eq!(value["results"][0]["event_id"], first_event_id);
        assert_eq!(value["results"][1]["event_id"], second_event_id);
    }

    #[tokio::test]
    async fn not_found_fallback_matches_legacy_chi_contract() {
        let app = router_with_auth(AuthMode::RejectAllTokens);

        for (method, uri) in [
            (Method::GET, "/nope"),
            (Method::GET, "/api/v1/nope"),
            (Method::GET, "/healthz/"),
            (Method::POST, "/definitely-missing"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method.clone())
                        .uri(uri)
                        .body(body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {uri}");
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                "text/plain; charset=utf-8",
                "{method} {uri}"
            );
            assert_eq!(
                response.headers().get("x-content-type-options").unwrap(),
                "nosniff",
                "{method} {uri}"
            );
            assert_eq!(
                header_values(response.headers(), header::VARY),
                vec!["Origin"],
                "{method} {uri}"
            );
            assert!(
                response
                    .headers()
                    .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                    .is_none(),
                "{method} {uri}"
            );
            assert!(
                response.headers().get(header::ALLOW).is_none(),
                "{method} {uri}"
            );
            let body = body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            assert_eq!(&body[..], b"404 page not found\n", "{method} {uri}");
        }
    }

    #[tokio::test]
    async fn encoded_dynamic_route_segments_match_legacy_chi_contract() {
        let app = router_with_auth(AuthMode::RejectAllTokens);

        for uri in [
            "/api/v1/library/songs/local%3Aabc/hash",
            "/api/v1/library/songs/local%3Aabc%2Fdef/hash",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(uri)
                        .body(body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                "text/plain; charset=utf-8",
                "{uri}"
            );
            assert_eq!(
                body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap(),
                b"{\"error\":\"missing_token\"}\n"[..],
                "{uri}"
            );
        }

        let plain_slash = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/library/songs/local%3Aabc/def/hash")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(plain_slash.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            body::to_bytes(plain_slash.into_body(), usize::MAX)
                .await
                .unwrap(),
            b"404 page not found\n"[..]
        );

        for method in [Method::GET, Method::POST] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method.clone())
                        .uri("/api/v1/playlists/018f3f27-0000-7000-8000-000000000001/items/local%3Aabc%2Fdef")
                        .body(body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{method}"
            );
            assert_eq!(allow_header_values(response.headers()), vec!["DELETE"]);
            assert!(response.headers().get(header::CONTENT_TYPE).is_none());
            assert!(
                body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }

        let delete = app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/api/v1/playlists/018f3f27-0000-7000-8000-000000000001/items/local%3Aabc%2Fdef")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            body::to_bytes(delete.into_body(), usize::MAX)
                .await
                .unwrap(),
            b"{\"error\":\"missing_token\"}\n"[..]
        );
    }

    #[test]
    fn idempotency_route_path_decoding_matches_legacy_go_url_path() {
        assert_eq!(
            legacy_url_path(
                "/api/v1/playlists/018f3f27-0000-7000-8000-000000000001/items/local%3Aabc%2Fdef"
            ),
            "/api/v1/playlists/018f3f27-0000-7000-8000-000000000001/items/local:abc/def"
        );
        assert_eq!(
            legacy_url_path("/api/v1/search/a+b%20c"),
            "/api/v1/search/a+b c"
        );
        assert_eq!(legacy_url_path("/api/v1/bad/%zz"), "/api/v1/bad/%zz");
    }

    #[test]
    fn path_segment_encoding_matches_client_route_contract() {
        assert_eq!(path_segment("local:abc/def"), "local%3Aabc%2Fdef");
        assert_eq!(path_segment("azAZ09-_.~"), "azAZ09-_.~");
        assert_eq!(path_segment("space and <tag>"), "space%20and%20%3Ctag%3E");
    }

    #[test]
    fn stream_proxy_policy_matches_legacy_go_contract() {
        assert!(should_proxy_youtube("always", false));
        assert!(should_proxy_youtube(" always ", false));
        assert!(!should_proxy_youtube("never", true));
        assert!(should_proxy_youtube("auto", true));
        assert!(!should_proxy_youtube("auto", false));
        assert!(should_proxy_youtube("", true));
        assert!(!should_proxy_youtube("", false));
    }

    #[test]
    fn runtime_config_defaults_match_legacy_go_contract() {
        assert_eq!(
            configured_database_url(None),
            DEFAULT_DATABASE_URL.to_string()
        );
        assert_eq!(
            configured_database_url(Some(String::new())),
            DEFAULT_DATABASE_URL.to_string()
        );
        assert_eq!(
            configured_database_url(Some("postgres://example/sunflower".into())),
            "postgres://example/sunflower"
        );
        assert_eq!(configured_data_dir(None), "./data");
        assert_eq!(configured_data_dir(Some(String::new())), "./data");
        assert_eq!(configured_data_dir(Some("   ".into())), "   ");
        assert_eq!(
            configured_listen_addr(None),
            DEFAULT_LISTEN_ADDR.to_string()
        );
        assert_eq!(
            configured_listen_addr(Some(String::new())),
            DEFAULT_LISTEN_ADDR.to_string()
        );
        assert_eq!(configured_listen_addr(Some(":9090".into())), ":9090");
        assert_eq!(
            go_wildcard_socket_addr(":9090"),
            Some(std::net::SocketAddr::from(([0, 0, 0, 0], 9090)))
        );
        assert_eq!(go_wildcard_socket_addr("127.0.0.1:9090"), None);

        assert_eq!(
            configured_setup_token(Some("configured-token".into())).unwrap(),
            "configured-token"
        );
        assert_eq!(configured_setup_token(Some("   ".into())).unwrap(), "   ");
        let generated = configured_setup_token(None).unwrap();
        assert_eq!(generated.len(), 32);
        assert!(generated.chars().all(|ch| ch.is_ascii_hexdigit()));
        let generated_for_empty = configured_setup_token(Some(String::new())).unwrap();
        assert_eq!(generated_for_empty.len(), 32);
        assert!(generated_for_empty.chars().all(|ch| ch.is_ascii_hexdigit()));

        assert_eq!(
            configured_cookie_file_from(None).as_deref(),
            Some(".env.innertube_cookie")
        );
        assert_eq!(
            configured_cookie_file_from(Some(String::new())).as_deref(),
            Some(".env.innertube_cookie")
        );
        assert_eq!(
            configured_cookie_file_from(Some("   ".into())).as_deref(),
            Some("   ")
        );

        assert!(configured_dev_open_registration(
            Some("development".into()),
            Some("1".into())
        ));
        assert!(!configured_dev_open_registration(
            Some("production".into()),
            Some("1".into())
        ));
        assert!(!configured_dev_open_registration(
            Some("development".into()),
            Some("true".into())
        ));
        assert!(!configured_dev_open_registration(None, Some("1".into())));
    }

    #[test]
    fn idempotency_response_hash_uses_legacy_json_wire_body() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        assert_eq!(
            legacy_wire_body_for_hash(&headers, br#"{"media_id":"local:<one>","liked":true}"#),
            br#"{"media_id":"local:\u003cone\u003e","liked":true}"#
                .as_slice()
                .iter()
                .copied()
                .chain(std::iter::once(b'\n'))
                .collect::<Vec<_>>()
        );

        let mut replay_headers = HeaderMap::new();
        replay_headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        replay_headers.insert("Idempotent-Replay", HeaderValue::from_static("true"));
        assert_eq!(
            legacy_wire_body_for_hash(&replay_headers, br#"{"idempotent_replay":true}"#),
            br#"{"idempotent_replay":true}"#
        );
    }

    #[test]
    fn admin_cookie_headers_match_go_cookie_string_order() {
        let expires = Utc.with_ymd_and_hms(2026, 7, 1, 1, 2, 3).unwrap();
        assert_eq!(
            admin_cookie("sf_admin", "tok", expires, true, true),
            "sf_admin=tok; Path=/; Expires=Wed, 01 Jul 2026 01:02:03 GMT; HttpOnly; Secure; SameSite=Lax"
        );
        assert_eq!(
            admin_cookie("sf_admin_csrf", "csrf", expires, false, false),
            "sf_admin_csrf=csrf; Path=/; Expires=Wed, 01 Jul 2026 01:02:03 GMT; SameSite=Lax"
        );
        assert_eq!(
            clear_admin_cookie("sf_admin", true, true),
            "sf_admin=; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=0; HttpOnly; Secure; SameSite=Lax"
        );
    }

    #[test]
    fn rate_limit_key_matches_go_remote_addr_shape() {
        let addr: SocketAddr = "127.0.0.1:49152".parse().unwrap();
        assert_eq!(rate_limit_key(Some(ConnectInfo(addr))), "127.0.0.1:49152");
        let addr: SocketAddr = "[::1]:49152".parse().unwrap();
        assert_eq!(rate_limit_key(Some(ConnectInfo(addr))), "[::1]:49152");
        assert_eq!(rate_limit_key(None), "");
    }

    #[test]
    fn request_cookie_parsing_matches_go_request_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static(" sf_admin = spaced ; sf_admin_csrf=\"csrf value\""),
        );
        assert_eq!(
            cookie_value(&headers, "sf_admin").as_deref(),
            Some(" spaced")
        );
        assert_eq!(
            cookie_value(&headers, "sf_admin_csrf").as_deref(),
            Some("csrf value")
        );

        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("sf_admin=one; sf_admin=two"),
        );
        assert_eq!(cookie_value(&headers, "sf_admin").as_deref(), Some("one"));

        headers.insert(header::COOKIE, HeaderValue::from_static("sf_admin"));
        assert_eq!(cookie_value(&headers, "sf_admin").as_deref(), Some(""));

        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("sf_admin=\"unterminated; sf_admin_csrf=ok"),
        );
        assert_eq!(cookie_value(&headers, "sf_admin"), None);
        assert_eq!(
            cookie_value(&headers, "sf_admin_csrf").as_deref(),
            Some("ok")
        );

        let mut multi = HeaderMap::new();
        multi.append(header::COOKIE, HeaderValue::from_static("broken"));
        multi.append(header::COOKIE, HeaderValue::from_static("sf_admin=ok"));
        assert_eq!(cookie_value(&multi, "sf_admin").as_deref(), Some("ok"));
    }

    #[test]
    fn youtube_cookie_parser_matches_legacy_provider_formats() {
        assert_eq!(
            parse_youtube_cookie_header(b"***INNERTUBE COOKIE*** =SID=abc; __Secure-3PSID=xyz")
                .as_deref(),
            Some("SID=abc; __Secure-3PSID=xyz")
        );
        assert_eq!(
            parse_youtube_cookie_header(b"SID=abc; __Secure-3PSID=xyz").as_deref(),
            Some("SID=abc; __Secure-3PSID=xyz")
        );
        assert_eq!(
            parse_youtube_cookie_header(b"SID=abc; weird=a b; quoted=\"hello world\"").as_deref(),
            Some("SID=abc; weird=\"a b\"; quoted=\"hello world\"")
        );
        assert_eq!(
            parse_youtube_cookie_header(b"SID=abc; junk; HSID=def"),
            None
        );
        assert_eq!(
            parse_youtube_cookie_header(b"SID=abc; bad name=value; HSID=def"),
            None
        );
        assert_eq!(
            parse_youtube_cookie_header(
                b"***INNERTUBE COOKIE*** =SID=abc; junk; HSID=def\n***VISITOR DATA*** =CgtX\n"
            ),
            None
        );
        assert_eq!(
            parse_youtube_cookie_header(
                b"# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t1893456000\tSID\tabc\n.youtube.com\tTRUE\t/\tTRUE\t1893456000\t__Secure-3PSID\txyz"
            )
            .as_deref(),
            Some("SID=abc; __Secure-3PSID=xyz")
        );
        assert_eq!(
            parse_youtube_cookie_header(
                b"# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t1999999999\tSID\tabc\n.youtube.com\tTRUE\t/\tTRUE\t1999999999\tbad name\tvalue\n.youtube.com\tTRUE\t/\tTRUE\t1999999999\tweird\ta b"
            )
            .as_deref(),
            Some("SID=abc; bad name=value; weird=\"a b\"")
        );
        assert_eq!(parse_youtube_cookie_header(b"no-cookies-here"), None);
    }

    #[tokio::test]
    async fn setup_status_matches_legacy_go_default_contract() {
        let app = router_with_auth(AuthMode::RejectAllTokens);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/setup/status")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.last(), Some(&b'\n'));
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
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

    #[tokio::test]
    async fn admin_html_entrypoints_match_legacy_redirect_contract() {
        let app = router_with_auth(AuthMode::RejectAllTokens);
        let admin = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/admin/")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin.status(), StatusCode::FOUND);
        assert_eq!(
            admin.headers().get(header::LOCATION).unwrap(),
            "/admin/login"
        );
        assert_eq!(
            admin.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        let admin_body = body::to_bytes(admin.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&admin_body[..], b"<a href=\"/admin/login\">Found</a>.\n\n");

        let post_redirect = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/admin/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(body::Body::from("password=nope"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(post_redirect.status(), StatusCode::FOUND);
        assert_eq!(
            post_redirect.headers().get(header::LOCATION).unwrap(),
            "/admin/login?error=internal"
        );
        assert!(post_redirect.headers().get(header::CONTENT_TYPE).is_none());
        let post_body = body::to_bytes(post_redirect.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(post_body.is_empty());

        let invalid_form_redirect = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/admin/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(body::Body::from("password=%zz"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid_form_redirect.status(), StatusCode::FOUND);
        assert_eq!(
            invalid_form_redirect
                .headers()
                .get(header::LOCATION)
                .unwrap(),
            "/admin/login?error=invalid_request"
        );

        let non_form_bad_escape_redirect = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/admin/login")
                    .header(header::CONTENT_TYPE, "text/plain")
                    .body(body::Body::from("password=%zz"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(non_form_bad_escape_redirect.status(), StatusCode::FOUND);
        assert_eq!(
            non_form_bad_escape_redirect
                .headers()
                .get(header::LOCATION)
                .unwrap(),
            "/admin/login?error=internal"
        );

        let login = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/admin/login?error=invalid+password%21")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        assert_eq!(
            login.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        let html = body::to_bytes(login.into_body(), usize::MAX).await.unwrap();
        let html = String::from_utf8_lossy(&html);
        assert!(html.contains("Admin Login"));
        assert!(html.contains(r#"<p class="flash">invalid password!</p>"#));
    }

    #[tokio::test]
    async fn setup_owner_parse_error_matches_legacy_go_contract() {
        let app = router_with_auth(AuthMode::RejectAllTokens);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/setup/owner")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"{\"error\":\"invalid_request\"}\n");
    }

    #[tokio::test]
    async fn rate_limited_routes_match_legacy_go_contract() {
        let setup_app = router_with_auth(AuthMode::RejectAllTokens);
        for _ in 0..10 {
            let response = setup_app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/api/v1/setup/owner")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(body::Body::from("{"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
        let setup_limited = setup_app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/setup/owner")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(setup_limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_json_error(setup_limited, "rate_limited").await;

        let admin_app = router_with_auth(AuthMode::RejectAllTokens);
        for _ in 0..8 {
            let response = admin_app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/api/v1/admin/auth/login")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(body::Body::from("{"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
        let admin_limited = admin_app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_json_error(admin_limited, "rate_limited").await;

        let pairing_app = router_with_auth(AuthMode::RejectAllTokens);
        for _ in 0..20 {
            let response = pairing_app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/api/v1/auth/register-device")
                        .header(header::CONTENT_TYPE, "application/json")
                        .header("idempotency-key", Uuid::now_v7().to_string())
                        .body(body::Body::from(r#"{"device_name":"Phone"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        let pairing_limited = pairing_app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/register-device")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(r#"{"device_name":"Phone"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pairing_limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_json_error(pairing_limited, "rate_limited").await;
    }

    #[tokio::test]
    async fn postgres_setup_owner_matches_legacy_enrollment_contract_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _pg_guard = PG_TEST_LOCK.lock().await;

        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        cleanup_pg_test_users(&pool).await;
        let store = PostgresStore::new(pool.clone());
        sqlx::query(
            r#"
            DELETE FROM audit_events
            WHERE actor_type = 'setup'
              AND event IN ('owner_setup_failed', 'owner_setup_completed')
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM cookie_health WHERE provider = 'youtube'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE display_name = 'Rust Owner'")
            .execute(&pool)
            .await
            .unwrap();
        if store.owner_configured().await.unwrap() {
            return;
        }

        let data_dir = std::env::temp_dir().join(format!("sunflower-pg-data-{}", Uuid::new_v4()));
        let data_dir_string = data_dir.to_string_lossy().to_string();
        let app = router_with_state_and_data_dir(
            AuthMode::Database,
            Some(store.clone()),
            data_dir_string.clone(),
        );
        let app_with_cookie_key = router_with_state_and_config(
            AuthMode::Database,
            Some(store.clone()),
            data_dir_string.clone(),
            DEFAULT_SETUP_TOKEN,
            "",
            Some([7u8; 32]),
        );
        let app_without_hub = router_with_state_and_config_and_hub(
            AuthMode::Database,
            Some(store),
            data_dir_string,
            DEFAULT_SETUP_TOKEN,
            "",
            None,
            None,
        );
        let status = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/setup/status")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(status.status(), StatusCode::OK);
        let status_value = response_json(status).await;
        assert_eq!(status_value["configured"], false);
        assert_eq!(status_value["pairing_required"], true);

        let bad_token = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/setup/owner")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(
                        r#"{"setup_token":"bad","display_name":"Rust Owner","password":"sunflower owner password"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad_token.status(), StatusCode::UNAUTHORIZED);
        assert_json_error(bad_token, "invalid_setup_token").await;

        let weak_password = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/setup/owner")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(
                        r#"{"setup_token":"sunflower-test-setup-token","display_name":"Rust Owner","password":"short"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(weak_password.status(), StatusCode::BAD_REQUEST);
        assert_json_error(weak_password, "weak_password").await;

        let setup = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/setup/owner")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(
                        r#"{"setup_token":"sunflower-test-setup-token","display_name":"Rust Owner","password":"sunflower owner password"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(setup.status(), StatusCode::OK);
        assert_eq!(response_json(setup).await, json!({"ok": true}));

        let configured = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/setup/status")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response_json(configured).await["configured"], true);

        let row = sqlx::query(
            r#"
            SELECT id, admin_password_hash
            FROM users
            WHERE display_name = 'Rust Owner'
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let user_id: Uuid = row.try_get("id").unwrap();
        let hash: String = row.try_get("admin_password_hash").unwrap();
        assert!(hash.starts_with("$argon2id$v=19$m=65536,t=1,p=4$"));

        let completed: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM audit_events
            WHERE actor_type = 'setup'
              AND event = 'owner_setup_completed'
              AND target_type = 'user'
              AND target_id = $1
            "#,
        )
        .bind(user_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(completed, 1);

        let admin_no_cookie = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/admin/status")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_no_cookie.status(), StatusCode::UNAUTHORIZED);
        assert_json_error(admin_no_cookie, "missing_admin_session").await;

        let bad_login = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"password":"wrong password"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad_login.status(), StatusCode::UNAUTHORIZED);
        assert_json_error(bad_login, "invalid_password").await;

        let login = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(
                        r#"{"password":"sunflower owner password"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let set_cookies = set_cookie_headers(&login);
        assert!(
            set_cookies
                .iter()
                .any(|cookie| cookie.starts_with("sf_admin=") && cookie.contains("HttpOnly"))
        );
        assert!(
            set_cookies
                .iter()
                .any(|cookie| cookie.starts_with("sf_admin_csrf=") && !cookie.contains("HttpOnly"))
        );
        let login_value = response_json(login).await;
        let csrf = login_value["csrf_token"].as_str().unwrap().to_string();
        assert!(csrf.starts_with("sf_csrf_"));
        assert!(login_value["expires_at"].as_str().unwrap().ends_with('Z'));
        let cookie_header = cookie_header_from_set_cookie(&set_cookies);

        let admin_html_unauth = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/admin/")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_html_unauth.status(), StatusCode::FOUND);
        assert_eq!(
            admin_html_unauth.headers().get(header::LOCATION).unwrap(),
            "/admin/login"
        );

        let admin_login_page = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/admin/login")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_login_page.status(), StatusCode::OK);
        assert_eq!(
            admin_login_page
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap(),
            "text/html; charset=utf-8"
        );
        let admin_login_html = body::to_bytes(admin_login_page.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&admin_login_html).contains("Admin Login"));

        let admin_css = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/admin/static/admin.css")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_css.status(), StatusCode::OK);
        assert_eq!(
            admin_css.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            admin_css.headers().get(header::CONTENT_LENGTH).unwrap(),
            &ADMIN_CSS.len().to_string()
        );
        assert_eq!(
            admin_css.headers().get(header::ACCEPT_RANGES).unwrap(),
            "bytes"
        );
        assert_eq!(
            body::to_bytes(admin_css.into_body(), usize::MAX)
                .await
                .unwrap(),
            ADMIN_CSS.as_bytes()
        );

        let admin_css_slash = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/admin/static/admin.css/")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_css_slash.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(
            admin_css_slash.headers().get(header::LOCATION).unwrap(),
            "../admin.css"
        );
        assert!(
            admin_css_slash
                .headers()
                .get(header::CONTENT_TYPE)
                .is_none()
        );
        assert!(
            body::to_bytes(admin_css_slash.into_body(), usize::MAX)
                .await
                .unwrap()
                .is_empty()
        );

        for path in [
            "/admin/static/../admin.css",
            "/admin/static/%2e%2e/admin.css",
            "/admin/static//admin.css",
        ] {
            let cleaned_admin_css = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(path)
                        .body(body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(cleaned_admin_css.status(), StatusCode::OK, "{path}");
            assert_eq!(
                body::to_bytes(cleaned_admin_css.into_body(), usize::MAX)
                    .await
                    .unwrap(),
                ADMIN_CSS.as_bytes(),
                "{path}"
            );
        }

        let admin_js = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/admin/static/admin.js")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_js.status(), StatusCode::OK);
        assert_eq!(
            admin_js.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            admin_js.headers().get(header::CONTENT_LENGTH).unwrap(),
            &ADMIN_JS.len().to_string()
        );
        assert_eq!(
            admin_js.headers().get(header::ACCEPT_RANGES).unwrap(),
            "bytes"
        );
        assert_eq!(
            body::to_bytes(admin_js.into_body(), usize::MAX)
                .await
                .unwrap(),
            ADMIN_JS.as_bytes()
        );

        let admin_js_range = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/admin/static/admin.js")
                    .header(header::RANGE, "bytes=0-7")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_js_range.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            admin_js_range.headers().get(header::CONTENT_RANGE).unwrap(),
            &format!("bytes 0-7/{}", ADMIN_JS.len())
        );
        assert_eq!(
            admin_js_range
                .headers()
                .get(header::CONTENT_LENGTH)
                .unwrap(),
            "8"
        );
        assert_eq!(
            body::to_bytes(admin_js_range.into_body(), usize::MAX)
                .await
                .unwrap(),
            b"document"[..]
        );

        let admin_css_bad_range = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/admin/static/admin.css")
                    .header(header::RANGE, "bytes=999999-")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            admin_css_bad_range.status(),
            StatusCode::RANGE_NOT_SATISFIABLE
        );
        assert_eq!(
            admin_css_bad_range
                .headers()
                .get(header::CONTENT_RANGE)
                .unwrap(),
            &format!("bytes */{}", ADMIN_CSS.len())
        );
        assert_eq!(
            admin_css_bad_range
                .headers()
                .get("x-content-type-options")
                .unwrap(),
            "nosniff"
        );
        assert_eq!(
            body::to_bytes(admin_css_bad_range.into_body(), usize::MAX)
                .await
                .unwrap(),
            b"invalid range: failed to overlap\n"[..]
        );

        let static_dir = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/admin/static/")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(static_dir.status(), StatusCode::OK);
        assert_eq!(
            static_dir.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            body::to_bytes(static_dir.into_body(), usize::MAX)
                .await
                .unwrap(),
            ADMIN_STATIC_DIR_LISTING.as_bytes()
        );

        let missing_static = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/admin/static/missing.js")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_static.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            missing_static.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            missing_static
                .headers()
                .get("x-content-type-options")
                .unwrap(),
            "nosniff"
        );
        assert_eq!(
            body::to_bytes(missing_static.into_body(), usize::MAX)
                .await
                .unwrap(),
            b"404 page not found\n"[..]
        );

        for path in [
            "/admin/",
            "/admin/devices",
            "/admin/pairing/new",
            "/admin/library",
            "/admin/cookies/youtube",
            "/admin/now-playing",
            "/admin/audit",
        ] {
            let page = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(path)
                        .header(header::COOKIE, &cookie_header)
                        .body(body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(page.status(), StatusCode::OK, "path={path}");
            assert_eq!(
                page.headers().get(header::CONTENT_TYPE).unwrap(),
                "text/html; charset=utf-8"
            );
        }

        let me = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/admin/me")
                    .header(header::COOKIE, &cookie_header)
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(me.status(), StatusCode::OK);
        let me_value = response_json(me).await;
        assert_eq!(me_value["user_id"], user_id.to_string());
        assert_eq!(me_value["display_name"], "Rust Owner");
        assert_eq!(me_value["csrf_token"], csrf);

        let admin_status = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/admin/status")
                    .header(header::COOKIE, &cookie_header)
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_status.status(), StatusCode::OK);
        let admin_status_value = response_json(admin_status).await;
        assert_eq!(admin_status_value["server_version"], "0.3.0");
        assert_eq!(admin_status_value["db_status"], "ok");
        assert!(admin_status_value["library_counts"].is_object());
        assert_eq!(admin_status_value["cookie_status"]["status"], "unknown");
        assert!(admin_status_value["devices"].as_array().unwrap().is_empty());

        let now_playing = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/admin/now-playing")
                    .header(header::COOKIE, &cookie_header)
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(now_playing.status(), StatusCode::OK);
        assert_eq!(response_json(now_playing).await, json!({"now_playing": []}));

        let now_playing_command_no_csrf = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/now-playing/command")
                    .header(header::COOKIE, &cookie_header)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(
                        r#"{"device_id":"device-1","command":"pause"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(now_playing_command_no_csrf.status(), StatusCode::FORBIDDEN);
        assert_json_error(now_playing_command_no_csrf, "invalid_csrf").await;

        let now_playing_command_bad_json = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/now-playing/command")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            now_playing_command_bad_json.status(),
            StatusCode::BAD_REQUEST
        );
        assert_json_error(now_playing_command_bad_json, "invalid_request").await;

        let now_playing_command_invalid_command = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/now-playing/command")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(
                        r#"{"device_id":"device-1","command":"stop"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            now_playing_command_invalid_command.status(),
            StatusCode::BAD_REQUEST
        );
        assert_json_error(now_playing_command_invalid_command, "invalid_command").await;

        let now_playing_command_delivered_zero = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/now-playing/command")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(
                        r#"{"device_id":"device-1","command":"pause"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(now_playing_command_delivered_zero.status(), StatusCode::OK);
        assert_eq!(
            response_json(now_playing_command_delivered_zero).await,
            json!({"delivered": 0})
        );

        let now_playing_command_no_hub = app_without_hub
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/now-playing/command")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(
                        r#"{"device_id":"device-1","command":"pause"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            now_playing_command_no_hub.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_json_error(now_playing_command_no_hub, "ws_unavailable").await;

        let library_status = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/admin/library/status")
                    .header(header::COOKIE, &cookie_header)
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(library_status.status(), StatusCode::OK);
        let library_status_value = response_json(library_status).await;
        assert!(library_status_value["counts"].is_object());
        assert_eq!(library_status_value["jobs"].as_array().unwrap().len(), 0);

        let admin_scan_no_csrf = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/library/scan")
                    .header(header::COOKIE, &cookie_header)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"roots":["/tmp"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_scan_no_csrf.status(), StatusCode::FORBIDDEN);
        assert_json_error(admin_scan_no_csrf, "invalid_csrf").await;

        let admin_scan_invalid = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/library/scan")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"roots":[]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_scan_invalid.status(), StatusCode::BAD_REQUEST);
        assert_json_error(admin_scan_invalid, "invalid_request").await;

        let admin_scan_dir =
            std::env::temp_dir().join(format!("sunflower-admin-scan-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&admin_scan_dir).unwrap();
        let admin_scan = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/library/scan")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(format!(
                        r#"{{"roots":["{}"]}}"#,
                        admin_scan_dir.to_string_lossy()
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_scan.status(), StatusCode::OK);
        let admin_scan_value = response_json(admin_scan).await;
        let admin_scan_job_id = admin_scan_value["job_id"].as_str().unwrap();
        let audit_row = sqlx::query(
            r#"
            SELECT event, target_type, coalesce(target_id, '') AS target_id, metadata
            FROM audit_events
            WHERE user_id = $1 AND event = 'library_scan_started'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let audit_event: String = audit_row.try_get("event").unwrap();
        let audit_target_type: String = audit_row.try_get("target_type").unwrap();
        let audit_target_id: String = audit_row.try_get("target_id").unwrap();
        let audit_metadata: serde_json::Value = audit_row.try_get("metadata").unwrap();
        assert_eq!(audit_event, "library_scan_started");
        assert_eq!(audit_target_type, "job");
        assert_eq!(audit_target_id, admin_scan_job_id);
        assert_eq!(audit_metadata["root_count"], 1);

        sqlx::query(
            r#"
            INSERT INTO cookie_health (provider, status, checked_at, detail)
            VALUES ('youtube', 'ok', now(), 'probe ok')
            ON CONFLICT (provider) DO UPDATE
            SET status = 'ok', checked_at = now(), detail = 'probe ok'
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let cookie_status = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/admin/cookies/youtube/status")
                    .header(header::COOKIE, &cookie_header)
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cookie_status.status(), StatusCode::OK);
        let cookie_status_value = response_json(cookie_status).await;
        assert_eq!(cookie_status_value["status"], "ok");
        assert_eq!(cookie_status_value["detail"], "probe ok");
        assert!(cookie_status_value["checked_at"].as_str().is_some());

        let upload_disabled = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/cookies/youtube")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"cookies":"netscape"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(upload_disabled.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_json_error(upload_disabled, "cookies_disabled").await;

        let upload_bad_csrf = app_with_cookie_key
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/cookies/youtube")
                    .header(header::COOKIE, &cookie_header)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"cookies":"netscape"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(upload_bad_csrf.status(), StatusCode::FORBIDDEN);
        assert_json_error(upload_bad_csrf, "invalid_csrf").await;

        let upload_invalid = app_with_cookie_key
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/cookies/youtube")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"cookies":"   "}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(upload_invalid.status(), StatusCode::BAD_REQUEST);
        assert_json_error(upload_invalid, "invalid_format").await;

        let oversized_cookie_form_body =
            format!("csrf_token={csrf}&cookies={}", "a".repeat(1 << 20));
        let oversized_cookie_form = app_with_cookie_key
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/admin/cookies/youtube")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(body::Body::from(oversized_cookie_form_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(oversized_cookie_form.status(), StatusCode::BAD_REQUEST);
        let oversized_cookie_form_body =
            body::to_bytes(oversized_cookie_form.into_body(), usize::MAX)
                .await
                .unwrap();
        assert!(
            String::from_utf8_lossy(&oversized_cookie_form_body).contains("Invalid form"),
            "{}",
            String::from_utf8_lossy(&oversized_cookie_form_body)
        );

        let upload = app_with_cookie_key
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/cookies/youtube")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(
                        r##"{"cookies":"# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t1893456000\tSID\tsecret"}"##,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(upload.status(), StatusCode::OK);
        assert_eq!(response_json(upload).await, json!({"ok": true}));

        let cookie_row = sqlx::query(
            r#"
            SELECT ciphertext, nonce
            FROM encrypted_cookies
            WHERE user_id = $1 AND provider = 'youtube'
            "#,
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let ciphertext: Vec<u8> = cookie_row.try_get("ciphertext").unwrap();
        let nonce: Vec<u8> = cookie_row.try_get("nonce").unwrap();
        assert!(ciphertext.len() > 16);
        assert_eq!(nonce.len(), 24);
        let loaded_raw = PostgresStore::new(pool.clone())
            .load_first_youtube_cookies([7u8; 32])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            parse_youtube_cookie_header(&loaded_raw).as_deref(),
            Some("SID=secret")
        );

        let probe_audit_before: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE user_id = $1 AND event = 'youtube_cookies_probe_requested'",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let html_probe = app_with_cookie_key
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/admin/cookies/youtube/probe")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(body::Body::from("csrf_token=%zz"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(html_probe.status(), StatusCode::FOUND);
        assert_eq!(
            html_probe.headers().get(header::LOCATION).unwrap(),
            "/admin/cookies/youtube?flash=probe_requested"
        );
        let probe_audit_after_html: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE user_id = $1 AND event = 'youtube_cookies_probe_requested'",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(probe_audit_after_html, probe_audit_before);

        let probe = app_with_cookie_key
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/cookies/youtube/probe")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(probe.status(), StatusCode::OK);
        let probe_value = response_json(probe).await;
        assert_eq!(probe_value["status"], "unknown");
        assert_eq!(probe_value["detail"], "manual probe requested");
        assert!(probe_value["checked_at"].as_str().is_some());
        let probe_audit_after_json: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE user_id = $1 AND event = 'youtube_cookies_probe_requested'",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(probe_audit_after_json, probe_audit_before + 1);

        let clear = app_with_cookie_key
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/cookies/youtube/clear")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(clear.status(), StatusCode::OK);
        assert_eq!(response_json(clear).await, json!({"ok": true}));
        let encrypted_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM encrypted_cookies WHERE user_id = $1 AND provider = 'youtube'",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(encrypted_count, 0);

        sqlx::query(
            r#"
            INSERT INTO audit_events
                (user_id, actor_type, event, target_type, target_id, metadata)
            VALUES
                ($1, 'admin_session', 'sensitive_event', 'test', 'sensitive',
                 '{"password":"pw","nested":{"csrf_secret":"secret","safe":"value"},"items":[{"pairing_code":"ABCD-EFGH"}]}'::jsonb)
            "#,
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
        let audit = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/admin/audit?limit=1")
                    .header(header::COOKIE, &cookie_header)
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(audit.status(), StatusCode::OK);
        let audit_value = response_json(audit).await;
        let events = audit_value["events"].as_array().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["event"], "sensitive_event");
        assert_eq!(events[0]["metadata"]["password"], "[redacted]");
        assert_eq!(events[0]["metadata"]["nested"]["csrf_secret"], "[redacted]");
        assert_eq!(events[0]["metadata"]["nested"]["safe"], "value");
        assert_eq!(
            events[0]["metadata"]["items"][0]["pairing_code"],
            "[redacted]"
        );

        let pair_no_csrf = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/pairing-codes")
                    .header(header::COOKIE, &cookie_header)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"label":"Pixel","ttl_seconds":600}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pair_no_csrf.status(), StatusCode::FORBIDDEN);
        assert_json_error(pair_no_csrf, "invalid_csrf").await;

        let pair = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/pairing-codes")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::HOST, "rust.local:8080")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"label":"Pixel","ttl_seconds":600}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pair.status(), StatusCode::OK);
        let pair_value = response_json(pair).await;
        let pairing_code = pair_value["pairing_code"].as_str().unwrap().to_string();
        assert_eq!(pairing_code.len(), 9);
        assert!(
            pair_value["pairing_url"]
                .as_str()
                .unwrap()
                .starts_with("sunflower://pair?code=")
        );
        assert!(
            pair_value["pairing_url"]
                .as_str()
                .unwrap()
                .contains("server=http%3A%2F%2Frust.local%3A8080")
        );

        let register_key = Uuid::now_v7().to_string();
        let register = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/register-device")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", &register_key)
                    .body(body::Body::from(format!(
                        r#"{{"device_name":"Pixel","platform":"android","client_version":"0.3.0","pairing_code":"{pairing_code}"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(register.status(), StatusCode::OK);
        let register_value = response_json(register).await;
        assert!(
            register_value["token"]
                .as_str()
                .unwrap()
                .starts_with("sf_dev_")
        );
        let device_token = register_value["token"].as_str().unwrap().to_string();
        let device_id = Uuid::parse_str(register_value["device_id"].as_str().unwrap()).unwrap();

        let register_replay = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/register-device")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", &register_key)
                    .body(body::Body::from(format!(
                        r#"{{"device_name":"Pixel Retry","platform":"android","client_version":"0.3.0","pairing_code":"{pairing_code}"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(register_replay.status(), StatusCode::OK);
        assert_eq!(
            register_replay
                .headers()
                .get("Idempotent-Replay")
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
        assert_eq!(response_json(register_replay).await, register_value);

        let authed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/library/songs")
                    .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authed.status(), StatusCode::OK);

        let missing_job = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/v1/jobs/{}", Uuid::new_v4()))
                    .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_job.status(), StatusCode::NOT_FOUND);
        assert_json_error(missing_job, "not_found").await;

        let scan_title = format!("Rust Scan {}", Uuid::new_v4());
        let scan_artist = "Rust Artist One";
        let scan_album = "Rust Album Alpha";
        let scan_dir = std::env::temp_dir().join(format!("sunflower-scan-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&scan_dir).unwrap();
        std::fs::write(
            scan_dir.join(format!("{scan_title}.mp3")),
            make_id3v23_mp3_with_cover(&scan_title, scan_artist, scan_album, 7, 2026, &tiny_jpeg()),
        )
        .unwrap();
        let scan = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/library/scan")
                    .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(format!(
                        r#"{{"roots":["{}"]}}"#,
                        scan_dir.to_string_lossy()
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(scan.status(), StatusCode::OK);
        let scan_value = response_json(scan).await;
        let scan_job_id = scan_value["job_id"].as_str().unwrap().to_string();
        let mut completed_job = None;
        for _ in 0..100 {
            let job = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(format!("/api/v1/jobs/{scan_job_id}"))
                        .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
                        .body(body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(job.status(), StatusCode::OK);
            let job_value = response_json(job).await;
            match job_value["status"].as_str() {
                Some("completed") => {
                    completed_job = Some(job_value);
                    break;
                }
                Some("failed") => panic!("scan job failed: {job_value}"),
                _ => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            }
        }
        let completed_job = completed_job.expect("scan job should complete");
        assert_eq!(completed_job["processed_files"], 1);
        let scanned_row = sqlx::query(
            r#"
            SELECT
                s.media_id,
                s.album_id,
                s.source_type,
                s.title,
                s.local_path,
                ar.name AS artist_name,
                al.title AS album_title,
                al.year AS album_year,
                (s.album_id IS NOT NULL) AS has_art
            FROM songs s
            LEFT JOIN artists ar ON ar.media_id = s.primary_artist_id
            LEFT JOIN albums al ON al.media_id = s.album_id
            WHERE s.title = $1
            "#,
        )
        .bind(&scan_title)
        .fetch_one(&pool)
        .await
        .unwrap();
        let scanned_media_id: String = scanned_row.try_get("media_id").unwrap();
        let scanned_album_id: String = scanned_row.try_get("album_id").unwrap();
        let scanned_source_type: String = scanned_row.try_get("source_type").unwrap();
        let scanned_local_path: String = scanned_row.try_get("local_path").unwrap();
        let scanned_artist_name: String = scanned_row.try_get("artist_name").unwrap();
        let scanned_album_title: String = scanned_row.try_get("album_title").unwrap();
        let scanned_album_year: Option<i32> = scanned_row.try_get("album_year").unwrap();
        let scanned_has_art: bool = scanned_row.try_get("has_art").unwrap();
        assert!(scanned_media_id.starts_with("local:"));
        assert_eq!(scanned_media_id.len(), 22);
        assert_eq!(scanned_source_type, "local");
        assert!(scanned_local_path.ends_with(".mp3"));
        assert_eq!(scanned_artist_name, scan_artist);
        assert_eq!(scanned_album_title, scan_album);
        assert_eq!(scanned_album_year, Some(2026));
        assert!(scanned_has_art);

        let art = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!(
                        "/api/v1/library/albums/{}/art?size=256",
                        scanned_album_id.replace(':', "%3A")
                    ))
                    .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(art.status(), StatusCode::OK);
        assert_eq!(
            art.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/jpeg"
        );
        assert!(
            !body::to_bytes(art.into_body(), usize::MAX)
                .await
                .unwrap()
                .is_empty()
        );

        let device_cookie_status_disabled = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/cookies/youtube/status")
                    .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            device_cookie_status_disabled.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_json_error(device_cookie_status_disabled, "cookies_disabled").await;

        let device_cookie_status = app_with_cookie_key
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/cookies/youtube/status")
                    .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(device_cookie_status.status(), StatusCode::OK);
        let device_cookie_status_value = response_json(device_cookie_status).await;
        assert_eq!(device_cookie_status_value["status"], "unknown");
        assert_eq!(
            device_cookie_status_value["checked_at"],
            serde_json::Value::Null
        );
        assert_eq!(
            device_cookie_status_value["detail"],
            serde_json::Value::Null
        );

        let device_upload_invalid = app_with_cookie_key
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/cookies/youtube")
                    .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(r#"{"cookies":""}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(device_upload_invalid.status(), StatusCode::BAD_REQUEST);
        assert_json_error(device_upload_invalid, "invalid_format").await;

        let device_upload = app_with_cookie_key
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/cookies/youtube")
                    .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(r#"{"cookies":"device cookies"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(device_upload.status(), StatusCode::NO_CONTENT);
        let device_cookie_row = sqlx::query(
            r#"
            SELECT ciphertext, nonce
            FROM encrypted_cookies
            WHERE user_id = $1 AND provider = 'youtube'
            "#,
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let device_ciphertext: Vec<u8> = device_cookie_row.try_get("ciphertext").unwrap();
        let device_nonce: Vec<u8> = device_cookie_row.try_get("nonce").unwrap();
        assert!(device_ciphertext.len() > 16);
        assert_eq!(device_nonce.len(), 24);

        let reuse = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/register-device")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(format!(
                        r#"{{"device_name":"Reuse","platform":"android","pairing_code":"{pairing_code}"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reuse.status(), StatusCode::UNAUTHORIZED);
        assert_json_error(reuse, "invalid_pairing_code").await;

        let devices = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/admin/devices")
                    .header(header::COOKIE, &cookie_header)
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(devices.status(), StatusCode::OK);
        let devices_value = response_json(devices).await;
        assert!(
            devices_value["devices"]
                .as_array()
                .unwrap()
                .iter()
                .any(|device| device["id"] == device_id.to_string()
                    && device["name"] == "Pixel"
                    && device["platform"] == "android")
        );

        let revoke_no_csrf = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/admin/devices/{device_id}/revoke"))
                    .header(header::COOKIE, &cookie_header)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"reason":"Lost phone"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(revoke_no_csrf.status(), StatusCode::FORBIDDEN);
        assert_json_error(revoke_no_csrf, "invalid_csrf").await;

        let revoke_bad_id = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/admin/devices/not-a-uuid/revoke")
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"reason":"Lost phone"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(revoke_bad_id.status(), StatusCode::BAD_REQUEST);
        assert_json_error(revoke_bad_id, "invalid_id").await;

        let revoke = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/admin/devices/{device_id}/revoke"))
                    .header(header::COOKIE, &cookie_header)
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"reason":"Lost phone"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(revoke.status(), StatusCode::OK);
        assert_eq!(response_json(revoke).await, json!({"ok": true}));

        let devices_after_revoke = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/admin/devices")
                    .header(header::COOKIE, &cookie_header)
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let devices_after_revoke_value = response_json(devices_after_revoke).await;
        let revoked_device = devices_after_revoke_value["devices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|device| device["id"] == device_id.to_string())
            .expect("revoked device should still be listed");
        assert_eq!(revoked_device["revoked_reason"], "Lost phone");
        assert!(revoked_device["revoked_at"].as_str().is_some());

        let revoked = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/library/songs")
                    .header(header::AUTHORIZATION, format!("Bearer {device_token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
        assert_json_error(revoked, "device_revoked").await;

        let setup_again = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/setup/owner")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(
                        r#"{"setup_token":"sunflower-test-setup-token","display_name":"Rust Owner","password":"sunflower owner password"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(setup_again.status(), StatusCode::FORBIDDEN);
        assert_json_error(setup_again, "setup_disabled").await;

        sqlx::query(
            r#"
            DELETE FROM audit_events
            WHERE user_id = $1
               OR target_id = $2
               OR (actor_type = 'setup'
                   AND event IN ('owner_setup_failed', 'owner_setup_completed'))
            "#,
        )
        .bind(user_id)
        .bind(user_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM encrypted_cookies WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM cookie_health WHERE provider = 'youtube'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM songs WHERE title = $1")
            .bind(&scan_title)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM albums WHERE title = $1")
            .bind(scan_album)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM artists WHERE name = $1")
            .bind(scan_artist)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM idempotency_log WHERE user_id = $1 OR device_id = $2")
            .bind(user_id)
            .bind(device_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        let _ = std::fs::remove_dir_all(scan_dir);
        let _ = std::fs::remove_dir_all(admin_scan_dir);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn postgres_dev_open_registration_matches_legacy_config_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _pg_guard = PG_TEST_LOCK.lock().await;

        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        cleanup_pg_test_users(&pool).await;
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
            .bind(user_id)
            .bind("Rust Dev Registration Test")
            .execute(&pool)
            .await
            .unwrap();
        let store = PostgresStore::new(pool.clone());

        let default_app = router_with_store(Some(store.clone()));
        let blocked = default_app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/register-device")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        r#"{"device_name":"Dev Phone","platform":"android","client_version":"dev"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
        assert_json_error(blocked, "pairing_required").await;

        let dev_app = router_with_config(
            test_router_config(AuthMode::Database, Some(store.clone()))
                .with_dev_open_registration(true),
        );
        let opened = dev_app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/register-device")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        r#"{"device_name":"Dev Phone","platform":"android","client_version":"dev"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(opened.status(), StatusCode::OK);
        let opened_value = response_json(opened).await;
        let device_id = Uuid::parse_str(opened_value["device_id"].as_str().unwrap()).unwrap();
        let token = opened_value["token"].as_str().unwrap();
        assert!(token.starts_with("sf_dev_"));
        assert_eq!(opened_value["server_capabilities"][0], "auth.pairing.v1");

        let authenticated = store.validate_device_token(token).await.unwrap();
        assert_eq!(authenticated.device_id, device_id);
        let device_name: String =
            sqlx::query_scalar("SELECT coalesce(name, '') FROM devices WHERE id = $1")
                .bind(device_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(device_name, "Dev Phone");

        sqlx::query("DELETE FROM idempotency_log WHERE device_id = $1")
            .bind(device_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM devices WHERE id = $1")
            .bind(device_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup_pg_test_users(&pool).await;
    }

    #[test]
    fn pagination_matches_legacy_defaults_and_bounds() {
        assert_eq!(pagination(None), (20, 0));
        assert_eq!(pagination(Some("limit=100&offset=3")), (100, 3));
        assert_eq!(pagination(Some("limit=0&offset=-1")), (20, 0));
        assert_eq!(pagination(Some("limit=101&offset=abc")), (20, 0));
        assert_eq!(pagination(Some("limit=25&offset=0")), (25, 0));
        assert_eq!(pagination(Some("limit=%32%35&offset=%33")), (25, 3));
        for value in ["1", "t", "T", "TRUE", "true", "True"] {
            let query = format!("hide_explicit={value}");
            assert!(bool_param(Some(&query), "hide_explicit"), "{value}");
        }
        for value in ["", "0", "f", "F", "FALSE", "false", "False", "yes"] {
            let query = format!("hide_explicit={value}");
            assert!(!bool_param(Some(&query), "hide_explicit"), "{value}");
        }
        assert!(!bool_param(None, "hide_explicit"));
        assert!(!bool_param(Some("hide_explicit"), "hide_explicit"));
        assert!(bool_param(Some("hide_video=t%72ue"), "hide_video"));
        assert_eq!(album_art_size(None).unwrap(), 512);
        assert_eq!(album_art_size(Some("size=256")).unwrap(), 256);
        assert_eq!(album_art_size(Some("size=1024")).unwrap(), 1024);
        assert!(album_art_size(Some("size=128")).is_err());
        assert_eq!(search_limit(""), 20);
        assert_eq!(search_limit("limit=-1"), 20);
        assert_eq!(search_limit("limit=0"), 20);
        assert_eq!(search_limit("limit=abc"), 20);
        assert_eq!(search_limit("limit=25"), 25);
        assert_eq!(search_limit("limit=%32%35"), 25);
        assert_eq!(search_limit("limit=30"), 25);
        assert_eq!(admin_audit_limit(None), 100);
        assert_eq!(admin_audit_limit(Some("")), 100);
        assert_eq!(admin_audit_limit(Some("limit=-1")), 100);
        assert_eq!(admin_audit_limit(Some("limit=0")), 100);
        assert_eq!(admin_audit_limit(Some("limit=abc")), 100);
        assert_eq!(admin_audit_limit(Some("limit=500")), 500);
        assert_eq!(admin_audit_limit(Some("limit=%35%30%30")), 500);
        assert_eq!(admin_audit_limit(Some("limit=501")), 100);
        assert_eq!(
            query_param("fl%61sh=scan+started%21", "flash").as_deref(),
            Some("scan started!")
        );
        assert_eq!(
            query_param("limit=10&limit=20", "limit").as_deref(),
            Some("10")
        );
        assert_eq!(
            query_token(Some("token=sf%5Fdev%5Fabc")).as_deref(),
            Some("sf_dev_abc")
        );
        assert_eq!(
            decoded_query_param("q=Rust+Library%20Alpha", "q").as_deref(),
            Some("Rust Library Alpha")
        );
        assert_eq!(query_param("q=%zz", "q"), None);
        assert_eq!(
            query_param("q=%zz&q=Rust+OK", "q").as_deref(),
            Some("Rust OK")
        );
        assert_eq!(
            query_param("q=bad;stillbad&q=good", "q").as_deref(),
            Some("good")
        );
        assert_eq!(query_param("q", "q").as_deref(), Some(""));
        assert_eq!(query_token(Some("token=sf%zzdev")), None);
        let valid_form = parse_form(b"password=a+b%21&csrf_token=ok");
        assert!(!valid_form.invalid);
        assert_eq!(form_value(&valid_form, "password"), "a b!");
        assert_eq!(form_value(&valid_form, "csrf_token"), "ok");
        let padded_csrf_form = parse_form(b"csrf_token=+ok%09");
        assert_eq!(
            admin_form_csrf_token(&HeaderMap::new(), &padded_csrf_form),
            "ok"
        );
        let partially_invalid_form = parse_form(b"csrf_token=ok&reason=%zz");
        assert!(partially_invalid_form.invalid);
        assert_eq!(form_value(&partially_invalid_form, "csrf_token"), "ok");
        assert_eq!(form_value(&partially_invalid_form, "reason"), "");
        let semicolon_form = parse_form(b"a=1;b=2&b=ok");
        assert!(semicolon_form.invalid);
        assert_eq!(form_value(&semicolon_form, "a"), "");
        assert_eq!(form_value(&semicolon_form, "b"), "ok");
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        let request_form = parse_request_form(
            &Method::POST,
            &headers,
            Some("password=query"),
            b"password=body",
        );
        assert!(!request_form.invalid);
        assert_eq!(form_value(&request_form, "password"), "body");
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
        let text_request_form = parse_request_form(
            &Method::POST,
            &headers,
            Some("password=query"),
            b"password=body",
        );
        assert!(!text_request_form.invalid);
        assert_eq!(form_value(&text_request_form, "password"), "query");
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded; charset=utf-8"),
        );
        let invalid_body_form =
            parse_request_form(&Method::POST, &headers, Some("password=query"), b"bad=%zz");
        assert!(invalid_body_form.invalid);
        assert_eq!(form_value(&invalid_body_form, "password"), "query");
        let get_form = parse_request_form(
            &Method::GET,
            &headers,
            Some("password=query"),
            b"password=body",
        );
        assert!(!get_form.invalid);
        assert_eq!(form_value(&get_form, "password"), "query");
        assert_eq!(
            admin_api_csrf_token(
                &headers,
                Some("csrf_token=query"),
                Some(&Bytes::from_static(b"csrf_token=body")),
            ),
            AdminApiCsrfToken {
                token: "body".into(),
                form_body_consumed: true,
            }
        );
        assert_eq!(
            admin_api_csrf_token(
                &headers,
                Some("csrf_token=query"),
                Some(&Bytes::from_static(b"csrf_token=")),
            ),
            AdminApiCsrfToken {
                token: "".into(),
                form_body_consumed: true,
            }
        );
        assert_eq!(
            admin_api_csrf_token(
                &headers,
                Some("csrf_token=query"),
                Some(&Bytes::from_static(br#"{"roots":["/music"]}"#)),
            ),
            AdminApiCsrfToken {
                token: "query".into(),
                form_body_consumed: true,
            }
        );
        let session = AdminSession {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            csrf_secret_hash: hex_lower_bytes(&Sha256::digest(b"query")),
            expires_at: Utc::now(),
        };
        let csrf_check = require_admin_csrf(
            &session,
            &headers,
            Some("csrf_token=query"),
            Some(&Bytes::from_static(br#"{"roots":["/music"]}"#)),
        )
        .unwrap();
        assert_eq!(
            csrf_check,
            AdminCsrfCheck {
                form_body_consumed: true,
            }
        );
        assert_eq!(
            csrf_check.body_after_middleware(Bytes::from_static(br#"{"roots":["/music"]}"#)),
            Bytes::new()
        );
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        assert_eq!(
            admin_api_csrf_token(
                &headers,
                Some("csrf_token=query"),
                Some(&Bytes::from_static(b"csrf_token=body")),
            ),
            AdminApiCsrfToken {
                token: "query".into(),
                form_body_consumed: false,
            }
        );
        headers.insert("x-csrf-token", HeaderValue::from_static("header"));
        assert_eq!(
            admin_api_csrf_token(
                &headers,
                Some("csrf_token=query"),
                Some(&Bytes::from_static(b"csrf_token=body")),
            ),
            AdminApiCsrfToken {
                token: "header".into(),
                form_body_consumed: false,
            }
        );
        assert!(scrobble_qualifies(30_000, 180_000));
        assert!(scrobble_qualifies(20_000, 40_000));
        assert!(!scrobble_qualifies(20_000, 180_000));
    }

    #[tokio::test]
    async fn local_file_responses_include_legacy_servefile_headers() {
        let path = std::env::temp_dir().join(format!("sunflower-{}.mp3", Uuid::new_v4()));
        fs::write(&path, b"0123456789").unwrap();

        let full = serve_local_file(path.to_str().unwrap(), None)
            .await
            .unwrap();
        assert_eq!(full.status(), StatusCode::OK);
        assert_eq!(
            full.headers().get(header::CONTENT_TYPE).unwrap(),
            "audio/mpeg"
        );
        assert_eq!(full.headers().get(header::CONTENT_LENGTH).unwrap(), "10");
        assert_eq!(full.headers().get(header::ACCEPT_RANGES).unwrap(), "bytes");
        assert!(
            full.headers()
                .get(header::LAST_MODIFIED)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.ends_with(" GMT"))
        );

        let ranged = serve_local_file(path.to_str().unwrap(), Some("bytes=2-5"))
            .await
            .unwrap();
        assert_eq!(ranged.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(ranged.headers().get(header::CONTENT_LENGTH).unwrap(), "4");
        assert_eq!(
            ranged.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 2-5/10"
        );
        assert!(ranged.headers().get(header::LAST_MODIFIED).is_some());
        let ranged_body = body::to_bytes(ranged.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&ranged_body[..], b"2345");

        let open_ended = serve_local_file(path.to_str().unwrap(), Some("bytes=4-"))
            .await
            .unwrap();
        assert_eq!(open_ended.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            open_ended.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 4-9/10"
        );
        let open_ended_body = body::to_bytes(open_ended.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&open_ended_body[..], b"456789");

        let suffix = serve_local_file(path.to_str().unwrap(), Some("bytes=-4"))
            .await
            .unwrap();
        assert_eq!(suffix.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            suffix.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 6-9/10"
        );
        let suffix_body = body::to_bytes(suffix.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&suffix_body[..], b"6789");

        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn protected_routes_match_legacy_missing_token_response() {
        let app = router_with_auth(AuthMode::RejectAllTokens);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/queue/start")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"seed_kind":"album"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"{\"error\":\"missing_token\"}\n");
    }

    #[tokio::test]
    async fn library_songs_matches_legacy_missing_token_response() {
        let app = router_with_auth(AuthMode::RejectAllTokens);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/library/songs")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"{\"error\":\"missing_token\"}\n");
    }

    #[tokio::test]
    async fn album_art_route_matches_legacy_contract() {
        let data_dir = std::env::temp_dir().join(format!("sunflower-art-{}", Uuid::new_v4()));
        let album_id = "local:album-art";
        let art_dir = data_dir.join("art").join(album_id);
        std::fs::create_dir_all(&art_dir).unwrap();
        std::fs::write(art_dir.join("512.jpg"), b"fake-jpeg").unwrap();

        let app = router_with_state_and_data_dir(
            AuthMode::AllowAllForContractTests,
            None,
            data_dir.to_string_lossy().to_string(),
        );
        let ok = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/library/albums/local%3Aalbum-art/art")
                    .header(header::AUTHORIZATION, "Bearer contract-test")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(
            ok.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/jpeg"
        );
        assert_eq!(
            body::to_bytes(ok.into_body(), usize::MAX).await.unwrap(),
            b"fake-jpeg"[..]
        );

        let invalid = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/library/albums/local%3Aalbum-art/art?size=128")
                    .header(header::AUTHORIZATION, "Bearer contract-test")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert_json_error(invalid, "invalid_size").await;

        let missing = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/library/albums/local%3Aalbum-art/art?size=256")
                    .header(header::AUTHORIZATION, "Bearer contract-test")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert_json_error(missing, "not_found").await;
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn protected_routes_match_legacy_invalid_token_response() {
        let app = router_with_auth(AuthMode::RejectAllTokens);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/queue/start")
                    .header(header::AUTHORIZATION, "Bearer nope")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body::Body::from(r#"{"seed_kind":"album"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"{\"error\":\"invalid_token\"}\n");
    }

    #[tokio::test]
    async fn protected_routes_accept_legacy_query_token_fallback() {
        let app = router_with_auth(AuthMode::AllowAllForContractTests);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/queue/start?token=contract-test")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(r#"{"seed_kind":"shuffle_liked"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_json_error(response, "empty_queue").await;

        let app = router_with_auth(AuthMode::AllowAllForContractTests);
        let blank_bearer_response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/queue/start?token=contract-test")
                    .header(header::AUTHORIZATION, "Bearer ")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(r#"{"seed_kind":"shuffle_liked"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            blank_bearer_response.status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_json_error(blank_bearer_response, "empty_queue").await;
    }

    #[tokio::test]
    async fn search_uses_innertube_backend_like_legacy_handler() {
        let yt: Arc<dyn innertube::InnerTubeBackend> = Arc::new(FakeInnerTube {
            home_page: innertube::HomePage::default(),
            search_page: innertube::SearchPage {
                songs: vec![
                    innertube::SongItem {
                        video_id: "song-a".into(),
                        title: "Song A".into(),
                        artists: vec!["Artist A".into()],
                        duration_ms: 0,
                        thumbnail_url: "https://img.example/song-a.jpg".into(),
                    },
                    innertube::SongItem {
                        video_id: "song-b".into(),
                        title: "Song B".into(),
                        artists: vec![],
                        duration_ms: 0,
                        thumbnail_url: String::new(),
                    },
                ],
                albums: vec![innertube::AlbumItem {
                    browse_id: "album-a".into(),
                    title: "Album A".into(),
                    artists: vec!["Artist A".into()],
                    thumbnail_url: String::new(),
                }],
                artists: vec![innertube::ArtistItem {
                    browse_id: "artist-a".into(),
                    name: "Artist A".into(),
                    thumbnail_url: "https://img.example/artist-a.jpg".into(),
                }],
                continuation: Some("next-page".into()),
            },
            next_pages: Mutex::new(vec![]),
            player: innertube::PlayerResponse::default(),
        });
        let app = router_with_config(
            test_router_config(AuthMode::AllowAllForContractTests, None).with_yt(Some(yt)),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/search?q=Rick+Astley&limit=1")
                    .header(header::AUTHORIZATION, "Bearer contract-test")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value = response_json(response).await;
        assert_eq!(value["query"], "Rick Astley");
        assert_eq!(value["songs"].as_array().unwrap().len(), 1);
        assert_eq!(value["songs"][0]["media_id"], "yt:song-a");
        assert_eq!(value["songs"][0]["source"], "yt");
        assert_eq!(value["albums"].as_array().unwrap().len(), 1);
        assert_eq!(value["artists"].as_array().unwrap().len(), 1);
        assert_eq!(value["continuation"], "next-page");
    }

    #[tokio::test]
    async fn search_query_rejects_malformed_escape_like_legacy_go_handler() {
        let yt: Arc<dyn innertube::InnerTubeBackend> = Arc::new(FakeInnerTube::with_player(
            innertube::PlayerResponse::default(),
        ));
        let app = router_with_config(
            test_router_config(AuthMode::AllowAllForContractTests, None).with_yt(Some(yt)),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/search?q=%zz")
                    .header(header::AUTHORIZATION, "Bearer contract-test")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_json_error(response, "invalid_query").await;
    }

    #[tokio::test]
    async fn streams_resolve_uses_innertube_backend_and_proxy_flag() {
        let yt: Arc<dyn innertube::InnerTubeBackend> =
            Arc::new(FakeInnerTube::with_player(innertube::PlayerResponse {
                stream: innertube::StreamUrl {
                    url: "https://r1.googlevideo.com/videoplayback?expire=2000000000".into(),
                    itag: 251,
                    mime_type: "audio/webm".into(),
                    bitrate: 160_000,
                    loudness: 0.0,
                },
                ..innertube::PlayerResponse::default()
            }));
        let proxy = Arc::new(StreamProxy::new(ProxySigner::new(b"test-key".to_vec())));
        let app = router_with_config(
            test_router_config(AuthMode::AllowAllForContractTests, None)
                .with_proxy(Some(proxy))
                .with_yt(Some(yt)),
        );

        let direct = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/streams/resolve")
                    .header(header::AUTHORIZATION, "Bearer contract-test")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        r#"{"media_id":"yt:abc","audio_quality":"high","reason":"near_expiry"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(direct.status(), StatusCode::OK);
        let direct_value = response_json(direct).await;
        assert_eq!(direct_value["media_id"], "yt:abc");
        assert_eq!(direct_value["source"], "youtube");
        assert_eq!(
            direct_value["stream_url"],
            "https://r1.googlevideo.com/videoplayback?expire=2000000000"
        );
        assert_eq!(direct_value["stream_expires_at"], "2033-05-18T03:33:20Z");
        assert_eq!(direct_value["itag"], 251);
        assert_eq!(direct_value["mime_type"], "audio/webm");
        assert_eq!(direct_value["metadata"]["bitrate"], 160_000);

        let proxied = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/streams/resolve")
                    .header(header::AUTHORIZATION, "Bearer contract-test")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(r#"{"media_id":"yt:abc","proxy":true}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(proxied.status(), StatusCode::OK);
        let proxied_value = response_json(proxied).await;
        assert_eq!(proxied_value["source"], "proxy");
        assert_eq!(proxied_value["itag"], 251);
        assert!(
            proxied_value["stream_url"]
                .as_str()
                .unwrap()
                .starts_with("/api/v1/streams/proxy?token=")
        );

        let policy_yt: Arc<dyn innertube::InnerTubeBackend> =
            Arc::new(FakeInnerTube::with_player(innertube::PlayerResponse {
                stream: innertube::StreamUrl {
                    url: "https://r2.googlevideo.com/videoplayback?expire=2000000001".into(),
                    itag: 251,
                    mime_type: "audio/webm".into(),
                    bitrate: 160_000,
                    loudness: 0.0,
                },
                ..innertube::PlayerResponse::default()
            }));
        let policy_app = router_with_config(
            test_router_config(AuthMode::AllowAllForContractTests, None)
                .with_proxy(Some(Arc::new(StreamProxy::new(ProxySigner::new(
                    b"policy-test-key".to_vec(),
                )))))
                .with_proxy_youtube(true)
                .with_yt(Some(policy_yt)),
        );
        let policy_proxied = policy_app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/streams/resolve")
                    .header(header::AUTHORIZATION, "Bearer contract-test")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(r#"{"media_id":"yt:def"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(policy_proxied.status(), StatusCode::OK);
        let policy_value = response_json(policy_proxied).await;
        assert_eq!(policy_value["source"], "proxy");
        assert!(
            policy_value["stream_url"]
                .as_str()
                .unwrap()
                .starts_with("/api/v1/streams/proxy?token=")
        );

        let unknown = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/streams/resolve")
                    .header(header::AUTHORIZATION, "Bearer contract-test")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(r#"{"media_id":"spotify:abc"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unknown.status(), StatusCode::BAD_GATEWAY);
        assert_json_error(unknown, "resolve_failed").await;
    }

    #[tokio::test]
    async fn stream_proxy_route_uses_signed_token_not_device_auth() {
        let app = router_with_auth(AuthMode::RejectAllTokens);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/streams/proxy?token=garbage")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"{\"error\":\"invalid_token\"}\n");
    }

    #[tokio::test]
    async fn nil_stream_proxy_route_matches_legacy_unregistered_route_contract() {
        let app = router_with_config(test_router_config(AuthMode::RejectAllTokens, None));

        for method in [Method::GET, Method::POST, Method::HEAD] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method.clone())
                        .uri("/api/v1/streams/proxy?token=garbage")
                        .body(body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method}");
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE).unwrap(),
                "text/plain; charset=utf-8",
                "{method}"
            );
            assert_eq!(
                response.headers().get("x-content-type-options").unwrap(),
                "nosniff",
                "{method}"
            );
            assert!(response.headers().get(header::ALLOW).is_none(), "{method}");
            assert_eq!(
                body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap(),
                b"404 page not found\n"[..],
                "{method}"
            );
        }
    }

    #[tokio::test]
    async fn now_playing_websocket_route_broadcasts_like_legacy_hub() {
        let app = router_with_auth(AuthMode::AllowAllForContractTests);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let player_url = format!("ws://{addr}/api/v1/ws/now-playing?token=player");
        let observer_url = format!("ws://{addr}/api/v1/ws/now-playing?token=observer");
        let mut player_request = player_url.into_client_request().unwrap();
        player_request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            WsHeaderValue::from_static(sunflower_core::NOW_PLAYING_SUBPROTOCOL),
        );
        let mut observer_request = observer_url.into_client_request().unwrap();
        observer_request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            WsHeaderValue::from_static(sunflower_core::NOW_PLAYING_SUBPROTOCOL),
        );

        let (mut player, player_response) = connect_async(player_request).await.unwrap();
        assert_eq!(
            player_response
                .headers()
                .get("Sec-WebSocket-Protocol")
                .and_then(|value| value.to_str().ok()),
            Some(sunflower_core::NOW_PLAYING_SUBPROTOCOL)
        );
        let (mut observer, _) = connect_async(observer_request).await.unwrap();

        player
            .send(WsMessage::Text(
                r#"{"type":"tick","media_id":"yt:abc","position_ms":5000,"is_playing":true}"#
                    .into(),
            ))
            .await
            .unwrap();
        let got = tokio::time::timeout(std::time::Duration::from_secs(2), observer.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let WsMessage::Text(got) = got else {
            panic!("observer should receive a text frame");
        };
        let value: serde_json::Value = serde_json::from_str(&got).unwrap();
        assert_eq!(value["type"], "tick");
        assert_eq!(value["media_id"], "yt:abc");
        assert_eq!(value["position_ms"], 5000);
        assert_eq!(value["is_playing"], true);

        let _ = player.close(None).await;
        let _ = observer.close(None).await;
        server.abort();
    }

    fn make_id3v23_mp3_with_cover(
        title: &str,
        artist: &str,
        album: &str,
        track: i32,
        year: i32,
        cover: &[u8],
    ) -> Vec<u8> {
        let mut frames = Vec::new();
        fn write_text_frame(frames: &mut Vec<u8>, id: &str, text: &str) {
            let mut data = Vec::with_capacity(text.len() + 1);
            data.push(0);
            data.extend_from_slice(text.as_bytes());
            frames.extend_from_slice(id.as_bytes());
            frames.extend_from_slice(&(data.len() as u32).to_be_bytes());
            frames.extend_from_slice(&[0, 0]);
            frames.extend_from_slice(&data);
        }
        fn write_apic_frame(frames: &mut Vec<u8>, cover: &[u8]) {
            if cover.is_empty() {
                return;
            }
            let mut data = Vec::new();
            data.push(0);
            data.extend_from_slice(b"image/jpeg");
            data.push(0);
            data.push(3);
            data.push(0);
            data.extend_from_slice(cover);
            frames.extend_from_slice(b"APIC");
            frames.extend_from_slice(&(data.len() as u32).to_be_bytes());
            frames.extend_from_slice(&[0, 0]);
            frames.extend_from_slice(&data);
        }
        if !title.is_empty() {
            write_text_frame(&mut frames, "TIT2", title);
        }
        if !artist.is_empty() {
            write_text_frame(&mut frames, "TPE1", artist);
        }
        if !album.is_empty() {
            write_text_frame(&mut frames, "TALB", album);
        }
        if track > 0 {
            write_text_frame(&mut frames, "TRCK", &track.to_string());
        }
        if year > 0 {
            write_text_frame(&mut frames, "TYER", &year.to_string());
        }
        write_apic_frame(&mut frames, cover);

        let tag_size = frames.len();
        let mut out = Vec::with_capacity(10 + tag_size);
        out.extend_from_slice(b"ID3");
        out.extend_from_slice(&[3, 0, 0]);
        out.extend_from_slice(&[
            ((tag_size >> 21) & 0x7f) as u8,
            ((tag_size >> 14) & 0x7f) as u8,
            ((tag_size >> 7) & 0x7f) as u8,
            (tag_size & 0x7f) as u8,
        ]);
        out.extend_from_slice(&frames);
        out
    }

    fn tiny_jpeg() -> Vec<u8> {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            2,
            2,
            image::Rgb([255, 0, 0]),
        ));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, ImageFormat::Jpeg).unwrap();
        out.into_inner()
    }

    #[tokio::test]
    async fn register_device_error_paths_match_legacy_handler_contract() {
        let cases = [
            ("{", StatusCode::BAD_REQUEST, "invalid_request"),
            ("null", StatusCode::FORBIDDEN, "pairing_required"),
            (
                r#"{"pairing_code":null}"#,
                StatusCode::FORBIDDEN,
                "pairing_required",
            ),
            (
                r#"{"device_name":"phone"}"#,
                StatusCode::FORBIDDEN,
                "pairing_required",
            ),
            (
                r#"{"pairing_code":"111111"} trailing"#,
                StatusCode::UNAUTHORIZED,
                "invalid_pairing_code",
            ),
            (
                r#"{"pairing_code":"111111"}"#,
                StatusCode::UNAUTHORIZED,
                "invalid_pairing_code",
            ),
        ];

        for (body_raw, expected_status, expected_error) in cases {
            let app = router_with_auth(AuthMode::RejectAllTokens);
            let response = app
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/api/v1/auth/register-device")
                        .header(header::CONTENT_TYPE, "application/json")
                        .header("idempotency-key", Uuid::now_v7().to_string())
                        .body(body::Body::from(body_raw))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), expected_status, "body={body_raw}");
            assert_json_error(response, expected_error).await;
        }
    }

    #[tokio::test]
    async fn queue_start_error_paths_match_legacy_handler_contract() {
        let cases = [
            ("{", StatusCode::BAD_REQUEST, "invalid_request"),
            (
                r#"{"seed_kind":"album"}"#,
                StatusCode::BAD_REQUEST,
                "invalid_seed_kind",
            ),
            (
                r#"{"seed_kind":"song","seed_id":"yt:"}"#,
                StatusCode::BAD_GATEWAY,
                "seed_unavailable",
            ),
        ];

        for (body_raw, expected_status, expected_error) in cases {
            let response = authed_post("/api/v1/queue/start", body_raw).await;
            assert_eq!(response.status(), expected_status, "body={body_raw}");
            assert_json_error(response, expected_error).await;
        }
    }

    #[tokio::test]
    async fn streams_resolve_error_paths_match_legacy_handler_contract() {
        for body_raw in ["{", r#"{"proxy":true}"#, r#"{"media_id":""}"#] {
            let response = authed_post("/api/v1/streams/resolve", body_raw).await;
            assert_eq!(
                response.status(),
                StatusCode::BAD_REQUEST,
                "body={body_raw}"
            );
            assert_json_error(response, "invalid_request").await;
        }
    }

    #[tokio::test]
    async fn postgres_home_daily_discover_matches_legacy_related_section_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _pg_guard = PG_TEST_LOCK.lock().await;

        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        cleanup_pg_test_users(&pool).await;
        let store = PostgresStore::new(pool.clone());
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let token = format!("sf_dev_test_{}", user_id.simple());

        sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
            .bind(user_id)
            .bind("Rust Daily Discover Test")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            INSERT INTO devices (id, user_id, name, platform, token_hash)
            VALUES ($1, $2, 'daily', 'rust', $3)
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind(hash_token(&token).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        for (media_id, title) in [
            ("yt:seed-a", "Seed A"),
            ("yt:seed-b", "Seed B"),
            ("local:ignored", "Ignored Local"),
        ] {
            sqlx::query(
                r#"
                INSERT INTO songs (media_id, source_type, title, available)
                VALUES ($1, $2, $3, true)
                "#,
            )
            .bind(media_id)
            .bind(if media_id.starts_with("yt:") {
                "yt"
            } else {
                "local"
            })
            .bind(title)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO likes (user_id, song_media_id) VALUES ($1, $2)")
                .bind(user_id)
                .bind(media_id)
                .execute(&pool)
                .await
                .unwrap();
        }

        let yt: Arc<dyn innertube::InnerTubeBackend> = Arc::new(FakeInnerTube {
            home_page: innertube::HomePage::default(),
            search_page: innertube::SearchPage::default(),
            next_pages: Mutex::new(vec![
                innertube::NextPage {
                    related: vec![
                        innertube::SongItem {
                            video_id: "seed-a".into(),
                            title: "Seed A".into(),
                            artists: vec!["Seed Artist".into()],
                            duration_ms: 0,
                            thumbnail_url: String::new(),
                        },
                        innertube::SongItem {
                            video_id: "daily-a".into(),
                            title: "Daily A".into(),
                            artists: vec!["Daily Artist".into()],
                            duration_ms: 0,
                            thumbnail_url: String::new(),
                        },
                        innertube::SongItem {
                            video_id: "daily-dupe".into(),
                            title: "Daily Dupe".into(),
                            artists: vec!["Daily Artist".into()],
                            duration_ms: 0,
                            thumbnail_url: String::new(),
                        },
                    ],
                    continuation: None,
                },
                innertube::NextPage {
                    related: vec![
                        innertube::SongItem {
                            video_id: "daily-dupe".into(),
                            title: "Daily Dupe".into(),
                            artists: vec!["Daily Artist".into()],
                            duration_ms: 0,
                            thumbnail_url: String::new(),
                        },
                        innertube::SongItem {
                            video_id: "daily-b".into(),
                            title: "Daily B".into(),
                            artists: vec!["Other Artist".into()],
                            duration_ms: 0,
                            thumbnail_url: String::new(),
                        },
                    ],
                    continuation: None,
                },
            ]),
            player: innertube::PlayerResponse::default(),
        });
        let app = router_with_config(
            test_router_config(AuthMode::Database, Some(store)).with_yt(Some(yt)),
        );

        let home = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/home")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(home.status(), StatusCode::OK);
        let value = response_json(home).await;
        let daily = value["sections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|section| section["kind"] == "daily_discover")
            .expect("daily_discover section should be present");
        assert_eq!(daily["id"], "daily_discover");
        assert_eq!(daily["title"], "Daily Discover");
        let ids = daily["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["media_id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"yt:daily-a".to_string()));
        assert!(ids.contains(&"yt:daily-b".to_string()));
        assert_eq!(
            ids.iter()
                .filter(|id| id.as_str() == "yt:daily-dupe")
                .count(),
            1
        );
        assert!(!ids.contains(&"yt:seed-a".to_string()));

        sqlx::query("DELETE FROM rec_cache WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM likes WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM devices WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM songs WHERE media_id = ANY($1)")
            .bind(vec!["yt:seed-a", "yt:seed-b", "local:ignored"])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn postgres_song_radio_queue_and_next_match_legacy_m4_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _pg_guard = PG_TEST_LOCK.lock().await;

        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        cleanup_pg_test_users(&pool).await;
        let store = PostgresStore::new(pool.clone());
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let token = format!("sf_dev_test_{}", user_id.simple());

        sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
            .bind(user_id)
            .bind("Rust Radio Test")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            INSERT INTO devices (id, user_id, name, platform, token_hash)
            VALUES ($1, $2, 'radio', 'rust', $3)
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind(hash_token(&token).unwrap())
        .execute(&pool)
        .await
        .unwrap();

        let related = (0..11)
            .map(|index| innertube::SongItem {
                video_id: if index == 0 {
                    "seed123".into()
                } else {
                    format!("rel{index}")
                },
                title: format!("Radio {index}"),
                artists: vec!["Radio Artist".into()],
                duration_ms: 0,
                thumbnail_url: String::new(),
            })
            .collect();
        let yt: Arc<dyn innertube::InnerTubeBackend> = Arc::new(FakeInnerTube {
            home_page: innertube::HomePage::default(),
            search_page: innertube::SearchPage::default(),
            next_pages: Mutex::new(vec![innertube::NextPage {
                related,
                continuation: None,
            }]),
            player: innertube::PlayerResponse {
                stream: innertube::StreamUrl {
                    url: "https://r1.googlevideo.com/videoplayback?expire=2000000000".into(),
                    itag: 251,
                    mime_type: "audio/webm".into(),
                    bitrate: 160_000,
                    loudness: 0.0,
                },
                ..innertube::PlayerResponse::default()
            },
        });
        let app = router_with_config(
            test_router_config(AuthMode::Database, Some(store))
                .with_proxy(Some(Arc::new(StreamProxy::new(ProxySigner::new(
                    b"radio-test-key".to_vec(),
                )))))
                .with_yt(Some(yt)),
        );

        let start = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/queue/start")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        r#"{"seed_kind":"song","seed_id":"yt:seed123","title":"Test Radio"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(start.status(), StatusCode::OK);
        let start_value = response_json(start).await;
        assert_eq!(start_value["seed_kind"], "song");
        assert_eq!(start_value["items"].as_array().unwrap().len(), 11);
        assert_eq!(start_value["items"][0]["media_id"], "yt:seed123");
        let queue_id = start_value["queue_id"].as_str().unwrap();

        let next = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/v1/next?queue_id={queue_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(next.status(), StatusCode::OK);
        let next_value = response_json(next).await;
        assert_eq!(next_value["current"]["media_id"], "yt:seed123");
        assert_eq!(next_value["current"]["source"], "youtube");
        assert_eq!(next_value["current"]["itag"], 251);
        assert_eq!(next_value["current"]["mime_type"], "audio/webm");
        assert_eq!(next_value["lookahead"].as_array().unwrap().len(), 8);
        assert_eq!(next_value["lookahead"][0]["source"], "youtube");
        assert!(
            next_value["lookahead"][0]["stream_url"]
                .as_str()
                .unwrap()
                .contains("googlevideo.com")
        );
        assert_eq!(next_value["lookahead"][0]["itag"], 251);
        assert_eq!(next_value["queue_version"], start_value["version"]);
        assert_eq!(next_value["has_more"], true);

        sqlx::query("DELETE FROM idempotency_log WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM queue_sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM devices WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn postgres_library_songs_list_matches_legacy_shape_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _pg_guard = PG_TEST_LOCK.lock().await;

        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        cleanup_pg_test_users(&pool).await;
        let store = PostgresStore::new(pool.clone());
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let token = format!("sf_dev_test_{}", user_id.simple());
        let artist_id = format!("local:artist-{}", Uuid::new_v4().simple());
        let album_id = format!("local:album-{}", Uuid::new_v4().simple());
        let song_with_album = format!("local:song-{}", Uuid::new_v4().simple());
        let song_without_album = format!("local:song-{}", Uuid::new_v4().simple());
        let hidden_song = format!("local:song-{}", Uuid::new_v4().simple());

        sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
            .bind(user_id)
            .bind("Rust Library Test")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            INSERT INTO devices (id, user_id, name, platform, token_hash)
            VALUES ($1, $2, 'test', 'rust', $3)
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind(hash_token(&token).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO artists (media_id, source_type, name)
            VALUES ($1, 'local', 'Artist One')
            "#,
        )
        .bind(&artist_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO albums (media_id, source_type, title, primary_artist_id)
            VALUES ($1, 'local', 'Album Alpha', $2)
            "#,
        )
        .bind(&album_id)
        .bind(&artist_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO songs
                (media_id, source_type, title, duration_ms, album_id, primary_artist_id, available)
            VALUES ($1, 'local', $2, $3, $4, $5, true)
            "#,
        )
        .bind(&song_with_album)
        .bind("Rust Library Alpha")
        .bind(123_000_i32)
        .bind(&album_id)
        .bind(&artist_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO songs
                (media_id, source_type, title, duration_ms, album_id, primary_artist_id, available)
            VALUES ($1, 'local', $2, $3, $4, $5, true)
            "#,
        )
        .bind(&song_without_album)
        .bind("Rust Library Beta")
        .bind(Option::<i32>::None)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO songs (media_id, source_type, title, available)
            VALUES ($1, 'local', 'Rust Library Hidden', false)
            "#,
        )
        .bind(&hidden_song)
        .execute(&pool)
        .await
        .unwrap();

        let app = router_with_store(Some(store));
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/library/songs?limit=100&offset=0")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let value = response_json(response).await;
        let songs = value["songs"].as_array().unwrap();
        let with_album = songs
            .iter()
            .find(|song| song["media_id"] == song_with_album)
            .expect("song with album should be listed");
        assert_eq!(with_album["source_type"], "local");
        assert_eq!(with_album["title"], "Rust Library Alpha");
        assert_eq!(with_album["duration_ms"], 123_000);
        assert_eq!(with_album["album_id"], album_id);
        assert_eq!(with_album["artist_name"], "Artist One");
        assert_eq!(with_album["album_title"], "Album Alpha");
        assert_eq!(with_album["has_art"], true);

        let without_album = songs
            .iter()
            .find(|song| song["media_id"] == song_without_album)
            .expect("song without album should be listed");
        assert_eq!(without_album["duration_ms"], serde_json::Value::Null);
        assert_eq!(without_album["album_id"], serde_json::Value::Null);
        assert_eq!(without_album["artist_name"], "");
        assert_eq!(without_album["album_title"], "");
        assert_eq!(without_album["has_art"], false);
        assert!(
            songs
                .iter()
                .all(|song| song["media_id"].as_str() != Some(hidden_song.as_str()))
        );

        let albums_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/library/albums?limit=100&offset=0")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(albums_response.status(), StatusCode::OK);
        let albums_value = response_json(albums_response).await;
        let album = albums_value["albums"]
            .as_array()
            .unwrap()
            .iter()
            .find(|album| album["media_id"] == album_id)
            .expect("album should be listed");
        assert_eq!(album["source_type"], "local");
        assert_eq!(album["title"], "Album Alpha");
        assert_eq!(album["primary_artist_id"], artist_id);
        assert_eq!(album["year"], serde_json::Value::Null);
        assert_eq!(album["available"], true);
        assert!(album["raw_metadata"].is_object());
        assert!(album["created_at"].as_str().is_some());

        let artists_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/library/artists?limit=100&offset=0")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(artists_response.status(), StatusCode::OK);
        let artists_value = response_json(artists_response).await;
        let artist = artists_value["artists"]
            .as_array()
            .unwrap()
            .iter()
            .find(|artist| artist["media_id"] == artist_id)
            .expect("artist should be listed");
        assert_eq!(artist["source_type"], "local");
        assert_eq!(artist["name"], "Artist One");
        assert_eq!(artist["available"], true);
        assert!(artist["raw_metadata"].is_object());
        assert!(artist["created_at"].as_str().is_some());

        let search_response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/search?q=Rust+Library&limit=5")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(search_response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_json_error(search_response, "yt_unavailable").await;

        sqlx::query("DELETE FROM songs WHERE media_id = ANY($1)")
            .bind(vec![song_with_album, song_without_album, hidden_song])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM albums WHERE media_id = $1")
            .bind(album_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM artists WHERE media_id = $1")
            .bind(artist_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn postgres_home_similar_artist_matches_legacy_top_artist_section_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _pg_guard = PG_TEST_LOCK.lock().await;

        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        cleanup_pg_test_users(&pool).await;
        let store = PostgresStore::new(pool.clone());
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let token = format!("sf_dev_test_{}", user_id.simple());
        let artist_id = "yt:artist-alpha";
        let song_id = "yt:artist-alpha-song";

        sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
            .bind(user_id)
            .bind("Rust Similar Artist Test")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            INSERT INTO devices (id, user_id, name, platform, token_hash)
            VALUES ($1, $2, 'similar', 'rust', $3)
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind(hash_token(&token).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO artists (media_id, source_type, name)
            VALUES ($1, 'yt', 'Artist Alpha')
            "#,
        )
        .bind(artist_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO songs (media_id, source_type, title, primary_artist_id, available)
            VALUES ($1, 'yt', 'Artist Alpha Song', $2, true)
            "#,
        )
        .bind(song_id)
        .bind(artist_id)
        .execute(&pool)
        .await
        .unwrap();
        for idx in 0..4 {
            sqlx::query(
                r#"
                INSERT INTO play_events
                    (user_id, device_id, song_media_id, kind, occurred_at)
                VALUES ($1, $2, $3, 'play', now() - (($4::int) * interval '1 minute'))
                "#,
            )
            .bind(user_id)
            .bind(device_id)
            .bind(song_id)
            .bind(idx)
            .execute(&pool)
            .await
            .unwrap();
        }

        let yt: Arc<dyn innertube::InnerTubeBackend> = Arc::new(FakeInnerTube {
            home_page: innertube::HomePage {
                sections: vec![innertube::HomeSection {
                    title: "Related".into(),
                    songs: vec![
                        innertube::SongItem {
                            video_id: "similar-a".into(),
                            title: "Similar A".into(),
                            artists: vec!["Related Artist".into()],
                            duration_ms: 180_000,
                            thumbnail_url: "https://img.example/similar-a.jpg".into(),
                        },
                        innertube::SongItem {
                            video_id: "similar-b".into(),
                            title: "Similar B".into(),
                            artists: vec!["Other Related Artist".into()],
                            duration_ms: 181_000,
                            thumbnail_url: String::new(),
                        },
                    ],
                }],
                chips: vec![],
            },
            search_page: innertube::SearchPage::default(),
            next_pages: Mutex::new(vec![]),
            player: innertube::PlayerResponse::default(),
        });
        let app = router_with_config(
            test_router_config(AuthMode::Database, Some(store)).with_yt(Some(yt)),
        );

        let home = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/home")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(home.status(), StatusCode::OK);
        let value = response_json(home).await;
        let similar = value["sections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|section| section["kind"] == "similar_artist")
            .expect("similar_artist section should be present");
        assert_eq!(similar["id"], "similar_artist:artist-alpha");
        assert_eq!(similar["title"], "Similar to Artist Alpha");
        assert_eq!(similar["seed"], "Artist Alpha");
        let ids = similar["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["media_id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"yt:similar-a".to_string()));
        assert!(ids.contains(&"yt:similar-b".to_string()));

        sqlx::query("DELETE FROM rec_cache WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM play_events WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM devices WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM songs WHERE media_id = $1")
            .bind(song_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM artists WHERE media_id = $1")
            .bind(artist_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn postgres_home_community_playlists_matches_legacy_search_section_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _pg_guard = PG_TEST_LOCK.lock().await;

        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        cleanup_pg_test_users(&pool).await;
        let store = PostgresStore::new(pool.clone());
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let token = format!("sf_dev_test_{}", user_id.simple());

        sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
            .bind(user_id)
            .bind("Rust Community Playlists Test")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            INSERT INTO devices (id, user_id, name, platform, token_hash)
            VALUES ($1, $2, 'community', 'rust', $3)
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind(hash_token(&token).unwrap())
        .execute(&pool)
        .await
        .unwrap();

        let mut songs = (0..18)
            .map(|idx| innertube::SongItem {
                video_id: format!("community-{idx}"),
                title: format!("Community {idx}"),
                artists: vec![format!("Artist {idx}")],
                duration_ms: 180_000 + idx,
                thumbnail_url: format!("https://img.example/community-{idx}.jpg"),
            })
            .collect::<Vec<_>>();
        songs.push(innertube::SongItem {
            video_id: "community-3".into(),
            title: "Community Dupe".into(),
            artists: vec!["Duplicate Artist".into()],
            duration_ms: 180_000,
            thumbnail_url: String::new(),
        });
        songs.push(innertube::SongItem {
            video_id: String::new(),
            title: "Missing Video ID".into(),
            artists: vec!["Ignored".into()],
            duration_ms: 180_000,
            thumbnail_url: String::new(),
        });

        let yt: Arc<dyn innertube::InnerTubeBackend> = Arc::new(FakeInnerTube {
            home_page: innertube::HomePage::default(),
            search_page: innertube::SearchPage {
                songs,
                albums: vec![],
                artists: vec![],
                continuation: None,
            },
            next_pages: Mutex::new(vec![]),
            player: innertube::PlayerResponse::default(),
        });
        let app = router_with_config(
            test_router_config(AuthMode::Database, Some(store)).with_yt(Some(yt)),
        );

        let home = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/home")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(home.status(), StatusCode::OK);
        let value = response_json(home).await;
        let community = value["sections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|section| section["kind"] == "community_playlists")
            .expect("community_playlists section should be present");
        assert_eq!(community["id"], "community_playlists");
        assert_eq!(community["title"], "Community Playlists");
        let ids = community["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["media_id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 15);
        assert!(ids.iter().all(|id| id.starts_with("yt:community-")));
        let unique_ids = ids
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique_ids.len(), ids.len());

        sqlx::query("DELETE FROM rec_cache WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM devices WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn postgres_shuffle_liked_queue_http_round_trip_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let _pg_guard = PG_TEST_LOCK.lock().await;

        let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
        cleanup_pg_test_users(&pool).await;
        let store = PostgresStore::new(pool.clone());
        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let other_device_id = Uuid::new_v4();
        let token = format!("sf_dev_test_{}", user_id.simple());
        let other_token = format!("sf_dev_test_other_{}", user_id.simple());
        let song_a = format!("local:{}", Uuid::new_v4().simple());
        let song_b = format!("local:{}", Uuid::new_v4().simple());
        let file_a = std::env::temp_dir().join(format!("sunflower-{}.mp3", Uuid::new_v4()));
        let file_b = std::env::temp_dir().join(format!("sunflower-{}.flac", Uuid::new_v4()));
        std::fs::write(&file_a, b"0123456789abcdef").unwrap();
        std::fs::write(&file_b, b"abcdefghijklmnop").unwrap();

        sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
            .bind(user_id)
            .bind("Rust Queue Test")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            INSERT INTO devices (id, user_id, name, platform, token_hash)
            VALUES ($1, $2, 'test', 'rust', $3)
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind(hash_token(&token).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO devices (id, user_id, name, platform, token_hash)
            VALUES ($1, $2, 'other', 'rust', $3)
            "#,
        )
        .bind(other_device_id)
        .bind(user_id)
        .bind(hash_token(&other_token).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        for (media_id, title, duration_ms, path) in [
            (&song_a, "Alpha", 120_000_i32, &file_a),
            (&song_b, "Beta", 180_000_i32, &file_b),
        ] {
            sqlx::query(
                r#"
                INSERT INTO songs (media_id, source_type, title, duration_ms, available, local_path)
                VALUES ($1, 'local', $2, $3, true, $4)
                "#,
            )
            .bind(media_id)
            .bind(title)
            .bind(duration_ms)
            .bind(path.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO likes (user_id, song_media_id) VALUES ($1, $2)")
                .bind(user_id)
                .bind(media_id)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query(
            r#"
            INSERT INTO play_events
                (user_id, device_id, song_media_id, kind, occurred_at, total_played_ms)
            VALUES ($1, $2, $3, 'play', now(), 120000)
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(&song_a)
        .execute(&pool)
        .await
        .unwrap();

        let yt: Arc<dyn innertube::InnerTubeBackend> = Arc::new(FakeInnerTube {
            home_page: innertube::HomePage {
                sections: vec![innertube::HomeSection {
                    title: "Remote picks".into(),
                    songs: vec![
                        innertube::SongItem {
                            video_id: "yt-home-a".into(),
                            title: "YT Home A".into(),
                            artists: vec!["Remote Artist".into()],
                            duration_ms: 0,
                            thumbnail_url: "https://img.example/yt-home-a.jpg".into(),
                        },
                        innertube::SongItem {
                            video_id: "yt-home-b".into(),
                            title: "YT Home B".into(),
                            artists: vec!["Remote Artist".into()],
                            duration_ms: 0,
                            thumbnail_url: String::new(),
                        },
                    ],
                }],
                chips: vec!["Relax".into(), "Workout".into()],
            },
            search_page: innertube::SearchPage::default(),
            next_pages: Mutex::new(vec![]),
            player: innertube::PlayerResponse::default(),
        });
        let app = router_with_config(
            test_router_config(AuthMode::Database, Some(store.clone())).with_yt(Some(yt)),
        );
        let home = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/home?hide_explicit=true")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(home.status(), StatusCode::OK);
        let home_value = response_json(home).await;
        let quick_picks = home_value["sections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|section| section["kind"] == "quick_picks")
            .expect("quick_picks should be present for local home");
        assert!(
            quick_picks["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["media_id"] == song_a && item["source"] == "local")
        );
        let yt_home = home_value["sections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|section| section["kind"] == "yt_home")
            .expect("yt_home should be present when browse returns songs");
        assert_eq!(yt_home["id"], "yt_home");
        assert_eq!(yt_home["title"], "From YouTube Music");
        assert!(
            yt_home["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["media_id"] == "yt:yt-home-a"
                    && item["source"] == "yt"
                    && item["thumbnail_url"] == "https://img.example/yt-home-a.jpg")
        );
        assert_eq!(home_value["chips"], json!(["Relax", "Workout"]));
        assert_eq!(home_value["stale"], false);

        let home_again = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/home?hide_explicit=true")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(home_again.status(), StatusCode::OK);
        let home_again_value = response_json(home_again).await;
        assert_eq!(home_again_value["stale"], false);
        let rec_cache_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM rec_cache WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(rec_cache_count, 1);

        let start = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/queue/start")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        r#"{"seed_kind":"shuffle_liked","title":"Liked"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(start.status(), StatusCode::OK);
        let start_value = response_json(start).await;
        assert_eq!(start_value["seed_kind"], "shuffle_liked");
        assert_eq!(start_value["title"], "Liked");
        let start_items = start_value["items"].as_array().unwrap();
        assert_eq!(start_items.len(), 2);
        for expected_id in [&song_a, &song_b] {
            assert!(
                start_items
                    .iter()
                    .any(|item| item["media_id"].as_str() == Some(expected_id.as_str())),
                "shuffle_liked queue dropped liked song {expected_id}"
            );
        }
        let queue_id = start_value["queue_id"].as_str().unwrap();

        let get_queue = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/v1/queue/{queue_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_queue.status(), StatusCode::OK);
        assert_eq!(
            response_json(get_queue).await["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        let next = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/v1/next?queue_id={queue_id}&position=0"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(next.status(), StatusCode::OK);
        let next_value = response_json(next).await;
        assert_eq!(next_value["queue_id"], queue_id);
        assert_eq!(next_value["position"], 0);
        assert_eq!(next_value["queue_version"], 1);
        assert_eq!(next_value["current"]["source"], "local");
        assert_eq!(next_value["lookahead"][0]["source"], "local");
        assert!(
            next_value["lookahead"][0]["stream_url"]
                .as_str()
                .unwrap()
                .starts_with("/api/v1/library/songs/")
        );
        assert_eq!(
            next_value["current"]["stream_expires_at"],
            serde_json::Value::Null
        );
        let current_media_id = next_value["current"]["media_id"].as_str().unwrap();
        assert!(
            [song_a.as_str(), song_b.as_str()].contains(&current_media_id),
            "shuffle_liked current media_id {current_media_id} was not one of the liked songs"
        );
        let stream_url = next_value["current"]["stream_url"].as_str().unwrap();
        assert_eq!(
            stream_url,
            format!(
                "/api/v1/library/songs/{}/stream",
                path_segment(current_media_id)
            )
        );

        let full_stream = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(stream_url)
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(full_stream.status(), StatusCode::OK);
        assert_eq!(
            full_stream.headers().get(header::ACCEPT_RANGES).unwrap(),
            "bytes"
        );
        assert!(matches!(
            full_stream
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("audio/mpeg") | Some("audio/flac")
        ));
        let full_body = body::to_bytes(full_stream.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(full_body.len(), 16);

        let invalid_range = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(stream_url)
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::RANGE, "bytes=999-1000")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid_range.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(
            invalid_range.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes */16"
        );
        assert_eq!(
            invalid_range.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            invalid_range
                .headers()
                .get("x-content-type-options")
                .unwrap(),
            "nosniff"
        );
        let invalid_range_body = body::to_bytes(invalid_range.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&invalid_range_body),
            "invalid range: failed to overlap\n"
        );

        let hash = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/v1/library/songs/{current_media_id}/hash"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(hash.status(), StatusCode::OK);
        let hash_value = response_json(hash).await;
        let mut hasher = Sha256::new();
        hasher.update(&full_body);
        assert_eq!(hash_value["media_id"], current_media_id);
        assert_eq!(hash_value["sha256"], hex_lower_bytes(&hasher.finalize()));
        assert_eq!(hash_value["bytes"], 16);

        let play_count_before_event: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM play_events WHERE user_id = $1 AND song_media_id = $2",
        )
        .bind(user_id)
        .bind(current_media_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let event_id = Uuid::now_v7().to_string();
        let events = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/events")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        json!({
                            "events": [{
                                "event_id": event_id.clone(),
                                "kind": "play",
                                "media_id": current_media_id,
                                "queue_id": queue_id,
                                "occurred_at": "2026-07-01T00:00:00Z",
                                "total_played_ms": 60000,
                                "duration_ms": 120000
                            }]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(events.status(), StatusCode::OK);
        let events_value = response_json(events).await;
        assert_eq!(events_value["results"][0]["accepted"], true);
        let play_count_after_event: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM play_events WHERE user_id = $1 AND song_media_id = $2",
        )
        .bind(user_id)
        .bind(current_media_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(play_count_after_event, play_count_before_event + 1);

        let duplicate_event = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/events")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        json!({
                            "events": [{
                                "event_id": event_id,
                                "kind": "play",
                                "media_id": current_media_id,
                                "queue_id": queue_id,
                                "occurred_at": "2026-07-01T00:00:00Z",
                                "total_played_ms": 60000,
                                "duration_ms": 120000
                            }]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(duplicate_event.status(), StatusCode::OK);
        let duplicate_value = response_json(duplicate_event).await;
        assert_eq!(duplicate_value["results"][0]["accepted"], true);
        let play_count_after_duplicate: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM play_events WHERE user_id = $1 AND song_media_id = $2",
        )
        .bind(user_id)
        .bind(current_media_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(play_count_after_duplicate, play_count_after_event);

        let below_threshold = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/events")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        json!({
                            "events": [{
                                "event_id": Uuid::now_v7().to_string(),
                                "kind": "play",
                                "media_id": current_media_id,
                                "total_played_ms": 1000,
                                "duration_ms": 120000
                            }]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(below_threshold.status(), StatusCode::OK);
        let below_threshold_value = response_json(below_threshold).await;
        assert_eq!(below_threshold_value["results"][0]["accepted"], false);
        assert_eq!(
            below_threshold_value["results"][0]["reason"],
            "below_scrobble_threshold"
        );

        let impressions = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/impressions")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        json!({
                            "impressions": [
                                {
                                    "section_id": "quick_picks",
                                    "source": "local",
                                    "seed_id": "liked",
                                    "media_id": current_media_id,
                                    "position": 0
                                },
                                {
                                    "section_id": "quick_picks",
                                    "source": "local",
                                    "media_id": "",
                                    "position": 1
                                }
                            ]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(impressions.status(), StatusCode::OK);
        assert_eq!(response_json(impressions).await["written"], 1);
        let impression_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM recommendation_impressions WHERE user_id = $1 AND media_id = $2",
        )
        .bind(user_id)
        .bind(current_media_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(impression_count, 1);

        let empty_impressions = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/impressions")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(json!({"impressions": []}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(empty_impressions.status(), StatusCode::NO_CONTENT);

        let like_idempotency_key = Uuid::now_v7().to_string();
        let like = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/likes")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(
                        header::HeaderName::from_static("idempotency-key"),
                        &like_idempotency_key,
                    )
                    .body(body::Body::from(
                        json!({
                            "media_id": current_media_id,
                            "liked": true,
                            "occurred_at": "2026-07-01T00:00:00Z"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(like.status(), StatusCode::OK);
        let like_value = response_json(like).await;
        assert_eq!(like_value["media_id"], current_media_id);
        assert_eq!(like_value["liked"], true);
        let like_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM likes WHERE user_id = $1 AND song_media_id = $2",
        )
        .bind(user_id)
        .bind(current_media_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(like_count, 1);
        let expected_wire_body = format!(r#"{{"media_id":"{current_media_id}","liked":true}}"#);
        let expected_wire_body = format!("{expected_wire_body}\n");

        let replay_like = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/likes")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(
                        header::HeaderName::from_static("idempotency-key"),
                        &like_idempotency_key,
                    )
                    .body(body::Body::from(
                        json!({
                            "media_id": current_media_id,
                            "liked": false
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(replay_like.status(), StatusCode::OK);
        assert_eq!(
            replay_like
                .headers()
                .get("Idempotent-Replay")
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
        assert_eq!(
            body::to_bytes(replay_like.into_body(), usize::MAX)
                .await
                .unwrap(),
            expected_wire_body.as_bytes()
        );
        let like_count_after_replay: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM likes WHERE user_id = $1 AND song_media_id = $2",
        )
        .bind(user_id)
        .bind(current_media_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(like_count_after_replay, 1);
        let idempotency_row = sqlx::query(
            r#"
            SELECT route, response_hash, response_status, response_body, response_content_type
            FROM idempotency_log
            WHERE key = $1
            "#,
        )
        .bind(Uuid::parse_str(&like_idempotency_key).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
        let idempotency_route: String = idempotency_row.try_get("route").unwrap();
        let idempotency_response_hash: String = idempotency_row.try_get("response_hash").unwrap();
        let idempotency_response_status: i32 = idempotency_row.try_get("response_status").unwrap();
        let idempotency_response_body: Vec<u8> = idempotency_row.try_get("response_body").unwrap();
        let idempotency_response_content_type: String =
            idempotency_row.try_get("response_content_type").unwrap();
        assert_eq!(idempotency_route, "POST /api/v1/likes");
        assert_eq!(idempotency_response_status, 200);
        assert_eq!(idempotency_response_body, expected_wire_body.as_bytes());
        assert!(
            idempotency_response_content_type
                .split(';')
                .next()
                .is_some_and(|value| value == "application/json")
        );
        assert_eq!(
            idempotency_response_hash,
            hex_lower_bytes(&Sha256::digest(expected_wire_body.as_bytes()))
        );

        let reused_key_on_other_route = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/impressions")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(
                        header::HeaderName::from_static("idempotency-key"),
                        &like_idempotency_key,
                    )
                    .body(body::Body::from(json!({"impressions": []}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reused_key_on_other_route.status(), StatusCode::CONFLICT);
        assert_json_error(reused_key_on_other_route, "conflict").await;

        sqlx::query(
            "UPDATE idempotency_log SET expires_at = now() - interval '1 hour' WHERE key = $1",
        )
        .bind(Uuid::parse_str(&like_idempotency_key).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        let stale_replay = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/likes")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(
                        header::HeaderName::from_static("idempotency-key"),
                        &like_idempotency_key,
                    )
                    .body(body::Body::from(
                        json!({
                            "media_id": current_media_id,
                            "liked": true
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale_replay.status(), StatusCode::CONFLICT);
        assert_json_error(stale_replay, "conflict").await;

        let unlike = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/likes")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        json!({
                            "media_id": current_media_id,
                            "liked": false
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unlike.status(), StatusCode::OK);
        assert_eq!(response_json(unlike).await["liked"], false);
        let like_count_after_unlike: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM likes WHERE user_id = $1 AND song_media_id = $2",
        )
        .bind(user_id)
        .bind(current_media_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(like_count_after_unlike, 0);

        let create_playlist_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/playlists")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(json!({"title": "Rust Mix"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_playlist_response.status(), StatusCode::OK);
        let create_playlist_value = response_json(create_playlist_response).await;
        assert_eq!(create_playlist_value["title"], "Rust Mix");
        assert_eq!(create_playlist_value["source_type"], "local");
        assert_eq!(create_playlist_value["version"], 1);
        assert!(create_playlist_value.get("items").is_none());
        let playlist_id = create_playlist_value["id"].as_str().unwrap();

        let list_playlists_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/playlists")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_playlists_response.status(), StatusCode::OK);
        let list_playlists_value = response_json(list_playlists_response).await;
        assert!(
            list_playlists_value["playlists"]
                .as_array()
                .unwrap()
                .iter()
                .any(|playlist| playlist["id"] == playlist_id && playlist.get("items").is_none())
        );

        let add_playlist_item_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/playlists/{playlist_id}/items"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        json!({"media_id": current_media_id}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(add_playlist_item_response.status(), StatusCode::NO_CONTENT);

        let get_playlist_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/v1/playlists/{playlist_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_playlist_response.status(), StatusCode::OK);
        let get_playlist_value = response_json(get_playlist_response).await;
        assert_eq!(get_playlist_value["id"], playlist_id);
        assert_eq!(get_playlist_value["version"], 2);
        assert_eq!(get_playlist_value["items"][0]["position"], 0);
        assert_eq!(get_playlist_value["items"][0]["media_id"], current_media_id);
        assert!(get_playlist_value["items"][0]["title"].as_str().is_some());

        let update_playlist_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PATCH)
                    .uri(format!("/api/v1/playlists/{playlist_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        json!({"title": "Renamed Mix"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(update_playlist_response.status(), StatusCode::OK);
        let update_playlist_value = response_json(update_playlist_response).await;
        assert_eq!(update_playlist_value["title"], "Renamed Mix");
        assert_eq!(update_playlist_value["version"], 3);

        let remove_playlist_item_key = Uuid::now_v7().to_string();
        let encoded_current_media_id = current_media_id.replace(':', "%3A");
        let remove_playlist_item_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!(
                        "/api/v1/playlists/{playlist_id}/items/{encoded_current_media_id}"
                    ))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header("idempotency-key", &remove_playlist_item_key)
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            remove_playlist_item_response.status(),
            StatusCode::NO_CONTENT
        );
        let remove_playlist_item_route: String =
            sqlx::query_scalar("SELECT route FROM idempotency_log WHERE key = $1")
                .bind(Uuid::parse_str(&remove_playlist_item_key).unwrap())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            remove_playlist_item_route,
            format!("DELETE /api/v1/playlists/{playlist_id}/items/{current_media_id}")
        );

        let get_playlist_after_remove = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/v1/playlists/{playlist_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_playlist_after_remove.status(), StatusCode::OK);
        let get_playlist_after_remove_value = response_json(get_playlist_after_remove).await;
        assert!(get_playlist_after_remove_value.get("items").is_none());
        assert_eq!(get_playlist_after_remove_value["version"], 4);

        let delete_playlist_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/api/v1/playlists/{playlist_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_playlist_response.status(), StatusCode::NO_CONTENT);

        let missing_playlist_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/v1/playlists/{playlist_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_playlist_response.status(), StatusCode::NOT_FOUND);
        assert_json_error(missing_playlist_response, "not_found").await;

        let register_download = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/devices/{device_id}/downloads"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        json!({
                            "media_id": current_media_id,
                            "local_path": "/data/current.mp3",
                            "bytes": 16
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(register_download.status(), StatusCode::NO_CONTENT);

        let downloads = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/v1/devices/{device_id}/downloads"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(downloads.status(), StatusCode::OK);
        let downloads_value = response_json(downloads).await;
        assert!(
            downloads_value["downloads"]
                .as_array()
                .unwrap()
                .iter()
                .any(|download| download["media_id"] == current_media_id
                    && download["bytes"] == 16)
        );

        let forbidden_download = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/devices/{device_id}/downloads"))
                    .header(header::AUTHORIZATION, format!("Bearer {other_token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::from(
                        json!({
                            "media_id": current_media_id,
                            "local_path": "/data/other.mp3",
                            "bytes": 1
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(forbidden_download.status(), StatusCode::FORBIDDEN);
        assert_json_error(forbidden_download, "forbidden").await;

        let delete_download = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!(
                        "/api/v1/devices/{device_id}/downloads/{current_media_id}"
                    ))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header("idempotency-key", Uuid::now_v7().to_string())
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_download.status(), StatusCode::NO_CONTENT);

        let downloads_after_delete = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/v1/devices/{device_id}/downloads"))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(downloads_after_delete.status(), StatusCode::OK);
        let downloads_after_delete_value = response_json(downloads_after_delete).await;
        assert!(
            downloads_after_delete_value["downloads"]
                .as_array()
                .unwrap()
                .iter()
                .all(|download| download["media_id"].as_str() != Some(current_media_id))
        );

        let ranged_stream = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(stream_url)
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::RANGE, "bytes=2-5")
                    .body(body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ranged_stream.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            ranged_stream.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 2-5/16"
        );
        let ranged_body = body::to_bytes(ranged_stream.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(ranged_body.len(), 4);

        sqlx::query("DELETE FROM downloaded_tracks WHERE device_id = ANY($1)")
            .bind(vec![device_id, other_device_id])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM queue_sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM play_events WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM recommendation_impressions WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM likes WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM idempotency_log WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM rec_cache WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM songs WHERE media_id = ANY($1)")
            .bind(vec![song_a, song_b])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        let _ = std::fs::remove_file(file_a);
        let _ = std::fs::remove_file(file_b);
    }

    async fn authed_post(uri: &str, body_raw: &str) -> axum::response::Response {
        let app = router_with_auth(AuthMode::AllowAllForContractTests);
        app.oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(header::AUTHORIZATION, "Bearer contract-test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", Uuid::now_v7().to_string())
                .body(body::Body::from(body_raw.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
    }

    async fn assert_json_error(response: axum::response::Response, expected: &str) {
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value, json!({ "error": expected }));
    }

    fn set_cookie_headers(response: &axum::response::Response) -> Vec<String> {
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|value| value.to_str().unwrap().to_string())
            .collect()
    }

    fn cookie_header_from_set_cookie(set_cookies: &[String]) -> String {
        set_cookies
            .iter()
            .filter_map(|cookie| cookie.split(';').next())
            .collect::<Vec<_>>()
            .join("; ")
    }

    async fn cleanup_pg_test_users(pool: &sqlx::PgPool) {
        let test_names = [
            "Rust Owner",
            "Rust Library Test",
            "Rust Queue Test",
            "Rust Radio Test",
            "Rust Daily Discover Test",
            "Rust Similar Artist Test",
            "Rust Community Playlists Test",
            "Rust Dev Registration Test",
        ];
        sqlx::query(
            r#"
            DELETE FROM downloaded_tracks
            WHERE device_id IN (
                SELECT devices.id FROM devices
                JOIN users ON users.id = devices.user_id
                WHERE users.display_name = ANY($1)
            )
            "#,
        )
        .bind(test_names.as_slice())
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            DELETE FROM playlist_items
            WHERE playlist_id IN (
                SELECT playlists.id FROM playlists
                JOIN users ON users.id = playlists.user_id
                WHERE users.display_name = ANY($1)
            )
               OR added_by_device_id IN (
                SELECT devices.id FROM devices
                JOIN users ON users.id = devices.user_id
                WHERE users.display_name = ANY($1)
            )
            "#,
        )
        .bind(test_names.as_slice())
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            DELETE FROM playlists
            WHERE user_id IN (
                SELECT id FROM users WHERE display_name = ANY($1)
            )
            "#,
        )
        .bind(test_names.as_slice())
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            DELETE FROM idempotency_log
            WHERE user_id IN (
                SELECT id FROM users WHERE display_name = ANY($1)
            )
               OR device_id IN (
                SELECT devices.id FROM devices
                JOIN users ON users.id = devices.user_id
                WHERE users.display_name = ANY($1)
            )
            "#,
        )
        .bind(test_names.as_slice())
        .execute(pool)
        .await
        .unwrap();
        for table in [
            "rec_cache",
            "encrypted_cookies",
            "play_events",
            "recommendation_impressions",
            "likes",
            "queue_sessions",
        ] {
            sqlx::query(&format!(
                r#"
                DELETE FROM {table}
                WHERE user_id IN (
                    SELECT id FROM users WHERE display_name = ANY($1)
                )
                "#
            ))
            .bind(test_names.as_slice())
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query(
            r#"
            DELETE FROM pairing_codes
            WHERE user_id IN (
                SELECT id FROM users WHERE display_name = ANY($1)
            )
               OR used_by_device_id IN (
                SELECT devices.id FROM devices
                JOIN users ON users.id = devices.user_id
                WHERE users.display_name = ANY($1)
            )
            "#,
        )
        .bind(test_names.as_slice())
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            DELETE FROM admin_sessions
            WHERE user_id IN (
                SELECT id FROM users WHERE display_name = ANY($1)
            )
            "#,
        )
        .bind(test_names.as_slice())
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            DELETE FROM audit_events
            WHERE user_id IN (
                SELECT id FROM users WHERE display_name = ANY($1)
            )
               OR (
                actor_type = 'setup'
                AND event IN ('owner_setup_failed', 'owner_setup_completed')
            )
            "#,
        )
        .bind(test_names.as_slice())
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            DELETE FROM devices
            WHERE user_id IN (
                SELECT id FROM users WHERE display_name = ANY($1)
            )
            "#,
        )
        .bind(test_names.as_slice())
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            DELETE FROM songs
            WHERE media_id = ANY($1)
            "#,
        )
        .bind(vec![
            "yt:seed-a",
            "yt:seed-b",
            "local:ignored",
            "yt:artist-alpha-song",
        ])
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            DELETE FROM artists
            WHERE media_id = ANY($1)
            "#,
        )
        .bind(vec!["yt:artist-alpha"])
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM users WHERE display_name = ANY($1)")
            .bind(test_names.as_slice())
            .execute(pool)
            .await
            .unwrap();
    }

    async fn response_json(response: axum::response::Response) -> serde_json::Value {
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }
}
