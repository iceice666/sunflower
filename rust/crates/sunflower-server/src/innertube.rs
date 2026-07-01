use std::{collections::HashSet, fmt, sync::Arc};

use chrono::{DateTime, Utc};
use futures_util::future::BoxFuture;
use reqwest::StatusCode;
use serde_json::{Value, json};
use sunflower_core::{MediaId, QueueItem};

const DEFAULT_BASE_URL: &str = "https://music.youtube.com";
const ANDROID_MUSIC_CLIENT_NAME: &str = "ANDROID_MUSIC";
const ANDROID_MUSIC_CLIENT_VERSION: &str = "7.27.52";
const ANDROID_MUSIC_CLIENT_ID: &str = "21";
const ANDROID_MUSIC_API_KEY: &str = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";
const ANDROID_MUSIC_USER_AGENT: &str =
    "com.google.android.apps.youtube.music/7.27.52 (Linux; U; Android 11) gzip";

const ANDROID_VR_CLIENT_NAME: &str = "ANDROID_VR";
const ANDROID_VR_CLIENT_VERSION: &str = "1.60.19";
const ANDROID_VR_CLIENT_ID: &str = "28";
const ANDROID_VR_USER_AGENT: &str = "com.google.android.apps.youtube.vr.oculus/1.60.19 (Linux; U; Android 12L; eureka-user Build/SQ3A.220605.009.A1) gzip";

const WEB_REMIX_CLIENT_NAME: &str = "WEB_REMIX";
const WEB_REMIX_CLIENT_VERSION: &str = "1.20230501.01.00";
const WEB_REMIX_CLIENT_ID: &str = "67";
const WEB_REMIX_API_KEY: &str = "AIzaSyC9XL3ZjWddXya6X74dJoCTL-NKNELL6Cg";
const WEB_REMIX_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locale {
    pub hl: String,
    pub gl: String,
}

impl Default for Locale {
    fn default() -> Self {
        Self {
            hl: "en".into(),
            gl: "US".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SongItem {
    pub video_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub duration_ms: i32,
    pub thumbnail_url: String,
    /// Set when the item carries an `MUSIC_EXPLICIT_BADGE` inline badge.
    /// Defaults to `false` when the badge array is absent (optional-field tolerant).
    pub is_explicit: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumItem {
    pub browse_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub thumbnail_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtistItem {
    pub browse_id: String,
    pub name: String,
    pub thumbnail_url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchPage {
    pub songs: Vec<SongItem>,
    pub albums: Vec<AlbumItem>,
    pub artists: Vec<ArtistItem>,
    pub continuation: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HomeSection {
    pub title: String,
    pub songs: Vec<SongItem>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HomePage {
    pub sections: Vec<HomeSection>,
    pub chips: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NextPage {
    pub related: Vec<SongItem>,
    pub continuation: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamUrl {
    pub url: String,
    pub itag: i32,
    pub mime_type: String,
    pub bitrate: i32,
    pub loudness: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerResponse {
    pub video_id: String,
    pub stream: StreamUrl,
    pub all_streams: Vec<StreamUrl>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InnerTubeError {
    detail: String,
}

impl InnerTubeError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for InnerTubeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for InnerTubeError {}

pub trait InnerTubeBackend: Send + Sync {
    fn browse<'a>(
        &'a self,
        browse_id: &'a str,
        continuation: Option<&'a str>,
    ) -> BoxFuture<'a, Result<HomePage, InnerTubeError>>;

    fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<SearchPage, InnerTubeError>>;

    fn next<'a>(
        &'a self,
        video_id: &'a str,
        continuation: Option<&'a str>,
    ) -> BoxFuture<'a, Result<NextPage, InnerTubeError>>;

    fn player<'a>(
        &'a self,
        video_id: &'a str,
    ) -> BoxFuture<'a, Result<PlayerResponse, InnerTubeError>>;
}

#[derive(Clone)]
pub struct HttpInnerTubeClient {
    http: reqwest::Client,
    base_url: String,
    locale: Locale,
    cookie_provider: Option<Arc<dyn CookieProvider>>,
}

pub trait CookieProvider: Send + Sync {
    fn cookie_header<'a>(&'a self) -> BoxFuture<'a, Option<String>>;
}

impl HttpInnerTubeClient {
    pub fn new(base_url: impl Into<String>, locale: Locale) -> Result<Self, InnerTubeError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .map_err(|err| InnerTubeError::new(format!("innertube client: {err}")))?;
        Ok(Self {
            http,
            base_url: base_url.into(),
            locale,
            cookie_provider: None,
        })
    }

    pub fn production(locale: Locale) -> Result<Self, InnerTubeError> {
        Self::new(DEFAULT_BASE_URL, locale)
    }

    pub fn with_cookie_provider(mut self, provider: Arc<dyn CookieProvider>) -> Self {
        self.cookie_provider = Some(provider);
        self
    }

    async fn post(
        &self,
        path: &str,
        profile: ClientProfile,
        payload: Value,
    ) -> Result<Value, InnerTubeError> {
        let url = format!("{}{}?key={}", self.base_url, path, profile.api_key);
        let body = payload.to_string();
        let mut response = self.post_once(&url, profile, body.clone()).await?;
        if response.status().as_u16() >= 500 {
            response = self.post_once(&url, profile, body).await?;
        }
        if response.status() != StatusCode::OK {
            return Err(InnerTubeError::new(format!(
                "innertube post {path}: status {}",
                response.status()
            )));
        }
        let body = response
            .bytes()
            .await
            .map_err(|err| InnerTubeError::new(format!("innertube post {path}: {err}")))?;
        serde_json::from_slice(&body)
            .map_err(|err| InnerTubeError::new(format!("innertube post {path}: {err}")))
    }

    async fn post_once(
        &self,
        url: &str,
        profile: ClientProfile,
        body: String,
    ) -> Result<reqwest::Response, InnerTubeError> {
        let mut request = self
            .http
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::USER_AGENT, profile.user_agent)
            .header("X-YouTube-Client-Name", profile.client_name_id)
            .header("X-YouTube-Client-Version", profile.client_version)
            .body(body);
        if let Some(provider) = &self.cookie_provider
            && let Some(cookie_header) = provider
                .cookie_header()
                .await
                .filter(|value| !value.is_empty())
        {
            request = request.header(reqwest::header::COOKIE, cookie_header);
        }
        request
            .send()
            .await
            .map_err(|err| InnerTubeError::new(format!("innertube post: {err}")))
    }
}

impl InnerTubeBackend for HttpInnerTubeClient {
    fn browse<'a>(
        &'a self,
        browse_id: &'a str,
        continuation: Option<&'a str>,
    ) -> BoxFuture<'a, Result<HomePage, InnerTubeError>> {
        Box::pin(async move {
            let mut body = build_web_remix_context(&self.locale);
            set_field(&mut body, "browseId", json!(browse_id));
            if let Some(continuation) = continuation.filter(|value| !value.is_empty()) {
                set_field(&mut body, "continuation", json!(continuation));
            }
            self.post("/youtubei/v1/browse", WEB_REMIX_PROFILE, body)
                .await
                .map(|raw| parse_home_page(&raw))
        })
    }

    fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<SearchPage, InnerTubeError>> {
        Box::pin(async move {
            let mut body = build_web_remix_context(&self.locale);
            set_field(&mut body, "query", json!(query));
            self.post("/youtubei/v1/search", WEB_REMIX_PROFILE, body)
                .await
                .map(|raw| parse_search_page(&raw))
        })
    }

    fn next<'a>(
        &'a self,
        video_id: &'a str,
        continuation: Option<&'a str>,
    ) -> BoxFuture<'a, Result<NextPage, InnerTubeError>> {
        Box::pin(async move {
            let mut body = build_android_music_context(&self.locale);
            set_field(&mut body, "videoId", json!(video_id));
            if let Some(continuation) = continuation.filter(|value| !value.is_empty()) {
                set_field(&mut body, "continuation", json!(continuation));
            }
            self.post("/youtubei/v1/next", ANDROID_MUSIC_PROFILE, body)
                .await
                .map(|raw| parse_next_page(&raw))
        })
    }

    fn player<'a>(
        &'a self,
        video_id: &'a str,
    ) -> BoxFuture<'a, Result<PlayerResponse, InnerTubeError>> {
        Box::pin(async move {
            let mut body = build_android_vr_context(&self.locale);
            set_field(&mut body, "videoId", json!(video_id));
            set_field(&mut body, "params", json!("CgIQBg=="));
            self.post("/youtubei/v1/player", ANDROID_VR_PROFILE, body)
                .await
                .map(|raw| parse_player_response(&raw))
        })
    }
}

#[derive(Clone, Copy)]
struct ClientProfile {
    api_key: &'static str,
    user_agent: &'static str,
    client_name_id: &'static str,
    client_version: &'static str,
}

const ANDROID_MUSIC_PROFILE: ClientProfile = ClientProfile {
    api_key: ANDROID_MUSIC_API_KEY,
    user_agent: ANDROID_MUSIC_USER_AGENT,
    client_name_id: ANDROID_MUSIC_CLIENT_ID,
    client_version: ANDROID_MUSIC_CLIENT_VERSION,
};

const ANDROID_VR_PROFILE: ClientProfile = ClientProfile {
    api_key: ANDROID_MUSIC_API_KEY,
    user_agent: ANDROID_VR_USER_AGENT,
    client_name_id: ANDROID_VR_CLIENT_ID,
    client_version: ANDROID_VR_CLIENT_VERSION,
};

const WEB_REMIX_PROFILE: ClientProfile = ClientProfile {
    api_key: WEB_REMIX_API_KEY,
    user_agent: WEB_REMIX_USER_AGENT,
    client_name_id: WEB_REMIX_CLIENT_ID,
    client_version: WEB_REMIX_CLIENT_VERSION,
};

pub async fn expand_radio(
    backend: &dyn InnerTubeBackend,
    seed_video_id: &str,
    min_items: usize,
) -> Result<Vec<QueueItem>, InnerTubeError> {
    let page = backend.next(seed_video_id, None).await?;
    let mut items = Vec::with_capacity(min_items);
    let mut seen = HashSet::new();
    add_songs(&mut items, &mut seen, page.related);

    let mut continuation = page.continuation;
    for _ in 0..10 {
        if items.len() >= min_items {
            break;
        }
        let Some(cursor) = continuation.as_deref().filter(|cursor| !cursor.is_empty()) else {
            break;
        };
        let Ok(next_page) = backend.next(seed_video_id, Some(cursor)).await else {
            break;
        };
        let before = items.len();
        add_songs(&mut items, &mut seen, next_page.related);
        continuation = next_page.continuation;
        if items.len() == before {
            break;
        }
    }

    Ok(items)
}

fn add_songs(items: &mut Vec<QueueItem>, seen: &mut HashSet<String>, songs: Vec<SongItem>) {
    for song in songs {
        if song.video_id.is_empty() || !seen.insert(song.video_id.clone()) {
            continue;
        }
        items.push(QueueItem {
            media_id: MediaId::new(format!("yt:{}", song.video_id)),
            title: song.title,
            artists: song.artists,
            duration_ms: song.duration_ms,
        });
    }
}

fn parse_player_response(raw: &Value) -> PlayerResponse {
    let video_id = get_string(raw, &["videoDetails", "videoId"]);
    let mut streams = Vec::new();
    for format in get_array(raw, &["streamingData", "adaptiveFormats"])
        .cloned()
        .unwrap_or_default()
    {
        let mime_type = get_string(&format, &["mimeType"]);
        if !mime_type.starts_with("audio/") {
            continue;
        }
        let url = get_string(&format, &["url"]);
        if url.is_empty() {
            continue;
        }
        streams.push(StreamUrl {
            url,
            itag: get_i32(&format, &["itag"]),
            mime_type,
            bitrate: get_i32(&format, &["bitrate"]),
            loudness: get_f64(&format, &["loudnessDb"]),
        });
    }

    let stream = streams
        .iter()
        .cloned()
        .max_by_key(|stream| stream.bitrate)
        .unwrap_or_default();
    PlayerResponse {
        video_id,
        stream,
        all_streams: streams,
    }
}

pub fn parse_search_page(raw: &Value) -> SearchPage {
    let mut page = SearchPage::default();
    let tab = get_array(raw, &["contents", "tabbedSearchResultsRenderer", "tabs"])
        .and_then(|tabs| tabs.first());
    let mut contents = tab
        .and_then(|tab| {
            get_array(
                tab,
                &["tabRenderer", "content", "sectionListRenderer", "contents"],
            )
        })
        .cloned();
    if contents.is_none() {
        contents = get_array(
            raw,
            &["continuationContents", "musicShelfContinuation", "contents"],
        )
        .cloned();
    }

    for section in contents.unwrap_or_default() {
        let Some(shelf) = get_map(&section, &["musicShelfRenderer"]) else {
            continue;
        };
        for item in get_array(shelf, &["contents"]).cloned().unwrap_or_default() {
            let Some(renderer) = get_map(&item, &["musicResponsiveListItemRenderer"]) else {
                continue;
            };

            let mut video_id = get_string(renderer, &["playlistItemData", "videoId"]);
            if video_id.is_empty() {
                video_id = get_string(
                    renderer,
                    &[
                        "overlay",
                        "musicItemThumbnailOverlayRenderer",
                        "content",
                        "musicPlayButtonRenderer",
                        "playNavigationEndpoint",
                        "watchEndpoint",
                        "videoId",
                    ],
                );
            }
            if !video_id.is_empty() {
                page.songs.push(parse_responsive_list_song(renderer));
                continue;
            }

            let browse_id = get_string(
                renderer,
                &["navigationEndpoint", "browseEndpoint", "browseId"],
            );
            if !browse_id.is_empty() {
                let page_type = get_string(
                    renderer,
                    &[
                        "navigationEndpoint",
                        "browseEndpoint",
                        "browseEndpointContextSupportedConfigs",
                        "browseEndpointContextMusicConfig",
                        "pageType",
                    ],
                );
                match page_type.as_str() {
                    "MUSIC_PAGE_TYPE_ALBUM" => page.albums.push(parse_album_item(renderer)),
                    "MUSIC_PAGE_TYPE_ARTIST" => page.artists.push(parse_artist_item(renderer)),
                    _ => page.songs.push(parse_responsive_list_song(renderer)),
                }
                continue;
            }

            page.songs.push(parse_responsive_list_song(renderer));
        }
    }

    if let Some(continuation) = get_array(
        raw,
        &[
            "continuationContents",
            "musicShelfContinuation",
            "continuations",
        ],
    )
    .and_then(|continuations| continuations.first())
    .map(|continuation| get_string(continuation, &["nextContinuationData", "continuation"]))
    .filter(|continuation| !continuation.is_empty())
    {
        page.continuation = Some(continuation);
    }

    page
}

pub fn parse_home_page(raw: &Value) -> HomePage {
    let mut page = HomePage::default();

    for chip in get_array(
        raw,
        &[
            "header",
            "musicImmersiveHeaderRenderer",
            "menu",
            "chipCloudRenderer",
            "chips",
        ],
    )
    .cloned()
    .unwrap_or_default()
    {
        let Some(renderer) = get_map(&chip, &["chipCloudChipRenderer"]) else {
            continue;
        };
        let text = first_run_text(renderer, "text");
        if !text.is_empty() {
            page.chips.push(text);
        }
    }

    let mut sections = get_array(
        raw,
        &[
            "contents",
            "singleColumnBrowseResultsRenderer",
            "tabbedRenderer",
            "tabRenderer",
            "content",
            "sectionListRenderer",
            "contents",
        ],
    )
    .cloned();
    if sections.is_none() {
        sections = get_array(
            raw,
            &["contents", "singleColumnBrowseResultsRenderer", "tabs"],
        )
        .and_then(|tabs| tabs.first())
        .and_then(|tab| {
            get_array(
                tab,
                &["tabRenderer", "content", "sectionListRenderer", "contents"],
            )
        })
        .cloned();
    }

    for section in sections.unwrap_or_default() {
        let parsed = parse_home_section(&section);
        if !parsed.songs.is_empty() || !parsed.title.is_empty() {
            page.sections.push(parsed);
        }
    }

    page
}

fn parse_home_section(raw: &Value) -> HomeSection {
    if let Some(renderer) = get_map(raw, &["musicShelfRenderer"]) {
        let title = first_run_text(renderer, "title");
        let songs = get_array(renderer, &["contents"])
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                get_map(&item, &["musicResponsiveListItemRenderer"])
                    .map(parse_responsive_list_song)
                    .filter(|song| !song.video_id.is_empty())
            })
            .collect();
        return HomeSection { title, songs };
    }

    let Some(renderer) = get_map(raw, &["musicCarouselShelfRenderer"]) else {
        return HomeSection::default();
    };
    let title = get_map(
        renderer,
        &["header", "musicCarouselShelfBasicHeaderRenderer"],
    )
    .map(|header| first_run_text(header, "title"))
    .unwrap_or_default();
    let mut songs = Vec::new();
    for item in get_array(renderer, &["contents"])
        .cloned()
        .unwrap_or_default()
    {
        let Some(item_renderer) = get_map(&item, &["musicTwoRowItemRenderer"]) else {
            continue;
        };
        let page_type = get_string(
            item_renderer,
            &[
                "navigationEndpoint",
                "browseEndpoint",
                "browseEndpointContextSupportedConfigs",
                "browseEndpointContextMusicConfig",
                "pageType",
            ],
        );
        if matches!(
            page_type.as_str(),
            "MUSIC_PAGE_TYPE_ALBUM" | "MUSIC_PAGE_TYPE_PLAYLIST" | "MUSIC_PAGE_TYPE_ARTIST"
        ) {
            continue;
        }
        if get_string(
            item_renderer,
            &["navigationEndpoint", "watchEndpoint", "videoId"],
        )
        .is_empty()
        {
            continue;
        }
        songs.push(parse_song_item(item_renderer));
    }
    HomeSection { title, songs }
}

pub fn parse_next_page(raw: &Value) -> NextPage {
    NextPage {
        related: extract_related_items(raw),
        continuation: extract_continuation(raw),
    }
}

fn extract_related_items(raw: &Value) -> Vec<SongItem> {
    let tabs = get_array(
        raw,
        &[
            "contents",
            "singleColumnMusicWatchNextResultsRenderer",
            "tabbedRenderer",
            "watchNextTabbedResultsRenderer",
            "tabs",
        ],
    );
    let mut items = Vec::new();
    for tab in tabs.cloned().unwrap_or_default() {
        let Some(content) = get_map(&tab, &["tabRenderer", "content"]) else {
            continue;
        };
        let Some(queue) = get_map(content, &["musicQueueRenderer"]) else {
            continue;
        };
        for item in get_array(queue, &["content", "playlistPanelRenderer", "contents"])
            .cloned()
            .unwrap_or_default()
        {
            if let Some(renderer) = get_map(&item, &["playlistPanelVideoRenderer"]) {
                items.push(parse_song_item(renderer));
            }
        }
    }
    items
}

fn extract_continuation(raw: &Value) -> Option<String> {
    let tabs = get_array(
        raw,
        &[
            "contents",
            "singleColumnMusicWatchNextResultsRenderer",
            "tabbedRenderer",
            "watchNextTabbedResultsRenderer",
            "tabs",
        ],
    );
    if let Some(tab) = tabs.and_then(|tabs| tabs.first())
        && let Some(queue) = get_map(
            tab,
            &[
                "tabRenderer",
                "content",
                "musicQueueRenderer",
                "content",
                "playlistPanelRenderer",
            ],
        )
        && let Some(token) = get_array(queue, &["continuations"])
            .and_then(|continuations| continuations.first())
            .map(|continuation| {
                get_string(continuation, &["nextRadioContinuationData", "continuation"])
            })
            .filter(|token| !token.is_empty())
    {
        return Some(token);
    }

    get_array(
        raw,
        &[
            "continuationContents",
            "playlistPanelContinuation",
            "continuations",
        ],
    )
    .and_then(|continuations| continuations.first())
    .map(|continuation| get_string(continuation, &["nextRadioContinuationData", "continuation"]))
    .filter(|token| !token.is_empty())
}

/// Returns `true` when the renderer carries an `MUSIC_EXPLICIT_BADGE` inline badge.
/// Follows the AGENTS.md rule: optional-field tolerant, returns `false` on any
/// missing key rather than erroring.
fn parse_explicit_badge(renderer: &Value) -> bool {
    let Some(badges) = get_array(renderer, &["badges"]) else {
        return false;
    };
    badges.iter().any(|badge| {
        get_string(
            badge,
            &["musicInlineBadgeRenderer", "icon", "iconType"],
        ) == "MUSIC_EXPLICIT_BADGE"
    })
}

fn parse_song_item(renderer: &Value) -> SongItem {
    let thumbnail_url = get_array(renderer, &["thumbnail", "thumbnails"])
        .and_then(|thumbnails| thumbnails.last())
        .map(|thumbnail| get_string(thumbnail, &["url"]))
        .unwrap_or_default();
    SongItem {
        video_id: get_string(renderer, &["videoId"]),
        title: first_run_text(renderer, "title"),
        artists: subtitle_artists(renderer),
        duration_ms: 0,
        thumbnail_url,
        is_explicit: parse_explicit_badge(renderer),
    }
}

fn parse_responsive_list_song(renderer: &Value) -> SongItem {
    let mut video_id = get_string(renderer, &["playlistItemData", "videoId"]);
    if video_id.is_empty() {
        video_id = get_string(
            renderer,
            &[
                "overlay",
                "musicItemThumbnailOverlayRenderer",
                "content",
                "musicPlayButtonRenderer",
                "playNavigationEndpoint",
                "watchEndpoint",
                "videoId",
            ],
        );
    }
    SongItem {
        video_id,
        title: responsive_title(renderer),
        artists: responsive_artists(renderer),
        duration_ms: 0,
        thumbnail_url: responsive_thumbnail(renderer),
        is_explicit: parse_explicit_badge(renderer),
    }
}

fn parse_album_item(renderer: &Value) -> AlbumItem {
    AlbumItem {
        browse_id: get_string(
            renderer,
            &["navigationEndpoint", "browseEndpoint", "browseId"],
        ),
        title: first_non_empty(&[
            first_run_text(renderer, "title"),
            responsive_title(renderer),
        ]),
        artists: responsive_artists(renderer),
        thumbnail_url: responsive_thumbnail(renderer),
    }
}

fn parse_artist_item(renderer: &Value) -> ArtistItem {
    ArtistItem {
        browse_id: get_string(
            renderer,
            &["navigationEndpoint", "browseEndpoint", "browseId"],
        ),
        name: first_non_empty(&[
            first_run_text(renderer, "title"),
            responsive_title(renderer),
        ]),
        thumbnail_url: responsive_thumbnail(renderer),
    }
}

fn subtitle_artists(renderer: &Value) -> Vec<String> {
    let mut artists = Vec::new();
    for run in get_array(renderer, &["subtitle", "runs"])
        .cloned()
        .unwrap_or_default()
    {
        let Some(endpoint) = get_map(&run, &["navigationEndpoint", "browseEndpoint"]) else {
            continue;
        };
        let page_type = get_string(
            endpoint,
            &[
                "browseEndpointContextSupportedConfigs",
                "browseEndpointContextMusicConfig",
                "pageType",
            ],
        );
        if page_type == "MUSIC_PAGE_TYPE_ARTIST" {
            artists.push(get_string(&run, &["text"]));
        }
    }
    artists
}

fn responsive_title(renderer: &Value) -> String {
    let Some(column) = get_array(renderer, &["flexColumns"]).and_then(|columns| columns.first())
    else {
        return String::new();
    };
    let Some(column_renderer) = get_map(column, &["musicResponsiveListItemFlexColumnRenderer"])
    else {
        return String::new();
    };
    first_run_text(column_renderer, "text")
}

fn responsive_artists(renderer: &Value) -> Vec<String> {
    let Some(column) = get_array(renderer, &["flexColumns"]).and_then(|columns| columns.get(1))
    else {
        return vec![];
    };
    let Some(column_renderer) = get_map(column, &["musicResponsiveListItemFlexColumnRenderer"])
    else {
        return vec![];
    };
    let mut artists = Vec::new();
    for run in get_array(column_renderer, &["text", "runs"])
        .cloned()
        .unwrap_or_default()
    {
        let Some(endpoint) = get_map(&run, &["navigationEndpoint", "browseEndpoint"]) else {
            continue;
        };
        let page_type = get_string(
            endpoint,
            &[
                "browseEndpointContextSupportedConfigs",
                "browseEndpointContextMusicConfig",
                "pageType",
            ],
        );
        if page_type == "MUSIC_PAGE_TYPE_ARTIST" {
            artists.push(get_string(&run, &["text"]));
        }
    }
    artists
}

fn responsive_thumbnail(renderer: &Value) -> String {
    get_array(
        renderer,
        &[
            "thumbnail",
            "musicThumbnailRenderer",
            "thumbnail",
            "thumbnails",
        ],
    )
    .and_then(|thumbnails| thumbnails.last())
    .map(|thumbnail| get_string(thumbnail, &["url"]))
    .unwrap_or_default()
}

fn first_run_text(renderer: &Value, field: &str) -> String {
    if let Some(text) = renderer
        .get(field)
        .and_then(|field| get_array(field, &["runs"]))
        .and_then(|runs| runs.first())
        .map(|run| get_string(run, &["text"]))
        .filter(|text| !text.is_empty())
    {
        return text;
    }
    renderer
        .get(field)
        .map(|field| get_string(field, &["simpleText"]))
        .unwrap_or_default()
}

fn first_non_empty(values: &[String]) -> String {
    values
        .iter()
        .find(|value| !value.is_empty())
        .cloned()
        .unwrap_or_default()
}

fn get_map<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
        if !current.is_object() {
            return None;
        }
    }
    Some(current)
}

fn get_array<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Vec<Value>> {
    let mut current = value;
    for (index, key) in path.iter().enumerate() {
        current = current.get(*key)?;
        if index == path.len() - 1 {
            return current.as_array();
        }
        if !current.is_object() {
            return None;
        }
    }
    None
}

fn get_string(value: &Value, path: &[&str]) -> String {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return String::new();
        };
        current = next;
    }
    current.as_str().unwrap_or_default().to_string()
}

fn get_i32(value: &Value, path: &[&str]) -> i32 {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return 0;
        };
        current = next;
    }
    current
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or_default()
}

fn get_f64(value: &Value, path: &[&str]) -> f64 {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return 0.0;
        };
        current = next;
    }
    current.as_f64().unwrap_or_default()
}

fn set_field(value: &mut Value, key: &str, field: Value) {
    if let Some(object) = value.as_object_mut() {
        object.insert(key.to_string(), field);
    }
}

fn build_android_music_context(locale: &Locale) -> Value {
    json!({
        "context": {
            "client": {
                "clientName": ANDROID_MUSIC_CLIENT_NAME,
                "clientVersion": ANDROID_MUSIC_CLIENT_VERSION,
                "androidSdkVersion": 30,
                "userAgent": ANDROID_MUSIC_USER_AGENT,
                "hl": locale.hl,
                "gl": locale.gl,
                "utcOffsetMinutes": 0
            }
        }
    })
}

fn build_android_vr_context(locale: &Locale) -> Value {
    json!({
        "context": {
            "client": {
                "clientName": ANDROID_VR_CLIENT_NAME,
                "clientVersion": ANDROID_VR_CLIENT_VERSION,
                "deviceMake": "Oculus",
                "deviceModel": "Quest 3",
                "androidSdkVersion": 32,
                "osName": "Android",
                "osVersion": "12L",
                "userAgent": ANDROID_VR_USER_AGENT,
                "hl": locale.hl,
                "gl": locale.gl
            }
        }
    })
}

fn build_web_remix_context(locale: &Locale) -> Value {
    json!({
        "context": {
            "client": {
                "clientName": WEB_REMIX_CLIENT_NAME,
                "clientVersion": WEB_REMIX_CLIENT_VERSION,
                "hl": locale.hl,
                "gl": locale.gl
            }
        }
    })
}

pub fn expiry_from_url(stream_url: &str) -> Option<DateTime<Utc>> {
    let url = reqwest::Url::parse(stream_url).ok()?;
    let expire = url.query_pairs().find_map(|(key, value)| {
        (key == "expire")
            .then(|| value.parse::<i64>().ok())
            .flatten()
    })?;
    DateTime::<Utc>::from_timestamp(expire, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, http::HeaderMap, routing::post};
    use futures_util::future::BoxFuture;
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    fn next_page_json(video_ids: &[&str], continuation: &str) -> Value {
        let items = video_ids
            .iter()
            .map(|id| {
                json!({
                    "playlistPanelVideoRenderer": {
                        "videoId": id,
                        "title": { "runs": [{ "text": format!("Song {id}") }] }
                    }
                })
            })
            .collect::<Vec<_>>();
        let continuations = if continuation.is_empty() {
            json!([])
        } else {
            json!([{ "nextRadioContinuationData": { "continuation": continuation } }])
        };
        json!({
            "contents": {
                "singleColumnMusicWatchNextResultsRenderer": {
                    "tabbedRenderer": {
                        "watchNextTabbedResultsRenderer": {
                            "tabs": [{
                                "tabRenderer": {
                                    "content": {
                                        "musicQueueRenderer": {
                                            "content": {
                                                "playlistPanelRenderer": {
                                                    "contents": items,
                                                    "continuations": continuations
                                                }
                                            }
                                        }
                                    }
                                }
                            }]
                        }
                    }
                }
            }
        })
    }

    struct StaticCookieProvider;

    impl CookieProvider for StaticCookieProvider {
        fn cookie_header<'a>(&'a self) -> BoxFuture<'a, Option<String>> {
            Box::pin(async { Some("SID=abc; __Secure-3PSID=xyz".into()) })
        }
    }

    struct FakeBackend {
        pages: std::sync::Mutex<Vec<Value>>,
    }

    impl InnerTubeBackend for FakeBackend {
        fn browse<'a>(
            &'a self,
            _browse_id: &'a str,
            _continuation: Option<&'a str>,
        ) -> BoxFuture<'a, Result<HomePage, InnerTubeError>> {
            Box::pin(async { Ok(HomePage::default()) })
        }

        fn search<'a>(
            &'a self,
            _query: &'a str,
        ) -> BoxFuture<'a, Result<SearchPage, InnerTubeError>> {
            Box::pin(async { Ok(SearchPage::default()) })
        }

        fn next<'a>(
            &'a self,
            _video_id: &'a str,
            _continuation: Option<&'a str>,
        ) -> BoxFuture<'a, Result<NextPage, InnerTubeError>> {
            Box::pin(async move {
                let raw = self.pages.lock().unwrap().remove(0);
                Ok(parse_next_page(&raw))
            })
        }

        fn player<'a>(
            &'a self,
            _video_id: &'a str,
        ) -> BoxFuture<'a, Result<PlayerResponse, InnerTubeError>> {
            Box::pin(async { Ok(PlayerResponse::default()) })
        }
    }

    #[test]
    fn parse_search_page_from_legacy_fixture() {
        let raw: Value =
            serde_json::from_str(include_str!("../testdata/innertube/search_response.json"))
                .unwrap();
        let page = parse_search_page(&raw);
        assert!(!page.songs.is_empty());
        assert!(!page.songs[0].video_id.is_empty());
        assert!(!page.songs[0].title.is_empty());
    }

    #[tokio::test]
    async fn http_client_attaches_cookie_provider_header_like_go_client() {
        let captured = Arc::new(Mutex::new(None::<String>));
        let captured_for_route = captured.clone();
        let app = Router::new().route(
            "/youtubei/v1/search",
            post(move |headers: HeaderMap| {
                let captured = captured_for_route.clone();
                async move {
                    let cookie = headers
                        .get(reqwest::header::COOKIE)
                        .and_then(|value| value.to_str().ok())
                        .map(ToOwned::to_owned);
                    *captured.lock().unwrap() = cookie;
                    Json(json!({}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = HttpInnerTubeClient::new(base_url, Locale::default())
            .unwrap()
            .with_cookie_provider(Arc::new(StaticCookieProvider));
        client.search("cookie test").await.unwrap();

        assert_eq!(
            captured.lock().unwrap().as_deref(),
            Some("SID=abc; __Secure-3PSID=xyz")
        );
    }

    #[test]
    fn parse_home_page_from_legacy_fixture() {
        let raw: Value =
            serde_json::from_str(include_str!("../testdata/innertube/home_response.json")).unwrap();
        let page = parse_home_page(&raw);
        assert!(!page.sections.is_empty());
    }

    #[test]
    fn parse_home_page_extracts_song_tiles_and_chips() {
        let raw = json!({
            "header": {
                "musicImmersiveHeaderRenderer": {
                    "menu": {
                        "chipCloudRenderer": {
                            "chips": [{
                                "chipCloudChipRenderer": {
                                    "text": { "runs": [{ "text": "Relax" }] }
                                }
                            }]
                        }
                    }
                }
            },
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicCarouselShelfRenderer": {
                                            "header": {
                                                "musicCarouselShelfBasicHeaderRenderer": {
                                                    "title": { "runs": [{ "text": "For you" }] }
                                                }
                                            },
                                            "contents": [{
                                                "musicTwoRowItemRenderer": {
                                                    "videoId": "abc",
                                                    "title": { "runs": [{ "text": "Song abc" }] },
                                                    "navigationEndpoint": {
                                                        "watchEndpoint": { "videoId": "abc" }
                                                    }
                                                }
                                            }]
                                        }
                                    }]
                                }
                            }
                        }
                    }]
                }
            }
        });
        let page = parse_home_page(&raw);
        assert_eq!(page.chips, vec!["Relax"]);
        assert_eq!(page.sections[0].title, "For you");
        assert_eq!(page.sections[0].songs[0].video_id, "abc");
        assert_eq!(page.sections[0].songs[0].title, "Song abc");
    }

    #[test]
    fn parse_home_page_extracts_related_music_shelf_songs() {
        let raw = json!({
            "contents": {
                "singleColumnBrowseResultsRenderer": {
                    "tabs": [{
                        "tabRenderer": {
                            "content": {
                                "sectionListRenderer": {
                                    "contents": [{
                                        "musicShelfRenderer": {
                                            "title": { "runs": [{ "text": "Related" }] },
                                            "contents": [{
                                                "musicResponsiveListItemRenderer": {
                                                    "playlistItemData": { "videoId": "related-a" },
                                                    "flexColumns": [
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": { "runs": [{ "text": "Related A" }] }
                                                            }
                                                        },
                                                        {
                                                            "musicResponsiveListItemFlexColumnRenderer": {
                                                                "text": { "runs": [{
                                                                    "text": "Related Artist",
                                                                    "navigationEndpoint": {
                                                                        "browseEndpoint": {
                                                                            "browseEndpointContextSupportedConfigs": {
                                                                                "browseEndpointContextMusicConfig": {
                                                                                    "pageType": "MUSIC_PAGE_TYPE_ARTIST"
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }] }
                                                            }
                                                        }
                                                    ],
                                                    "thumbnail": {
                                                        "musicThumbnailRenderer": {
                                                            "thumbnail": {
                                                                "thumbnails": [
                                                                    { "url": "https://img.example/small.jpg" },
                                                                    { "url": "https://img.example/large.jpg" }
                                                                ]
                                                            }
                                                        }
                                                    }
                                                }
                                            }]
                                        }
                                    }]
                                }
                            }
                        }
                    }]
                }
            }
        });
        let page = parse_home_page(&raw);
        assert_eq!(page.sections[0].title, "Related");
        assert_eq!(page.sections[0].songs[0].video_id, "related-a");
        assert_eq!(page.sections[0].songs[0].title, "Related A");
        assert_eq!(page.sections[0].songs[0].artists, vec!["Related Artist"]);
        assert_eq!(
            page.sections[0].songs[0].thumbnail_url,
            "https://img.example/large.jpg"
        );
    }

    #[test]
    fn parse_next_page_from_legacy_fixture() {
        let raw: Value =
            serde_json::from_str(include_str!("../testdata/innertube/next_response.json")).unwrap();
        let page = parse_next_page(&raw);
        assert!(!page.related.is_empty());
        assert!(!page.related[0].video_id.is_empty());
        assert!(page.continuation.is_some());
    }

    #[tokio::test]
    async fn expand_radio_collects_across_continuations_like_go() {
        let backend = FakeBackend {
            pages: std::sync::Mutex::new(vec![
                next_page_json(&["a", "b", "c"], "cont1"),
                next_page_json(&["d", "e", "f"], "cont2"),
                next_page_json(&["g", "h", "i", "j", "k"], ""),
            ]),
        };
        let items = expand_radio(&backend, "a", 10).await.unwrap();
        assert!(items.len() >= 10);
        assert_eq!(items[0].media_id.0, "yt:a");
    }

    #[test]
    fn parse_player_response_picks_highest_bitrate_audio() {
        let raw = json!({
            "videoDetails": { "videoId": "abc" },
            "streamingData": {
                "adaptiveFormats": [
                    { "itag": 18, "mimeType": "video/mp4", "bitrate": 1000, "url": "https://video.example" },
                    { "itag": 140, "mimeType": "audio/mp4", "bitrate": 128000, "url": "https://a.googlevideo.com/videoplayback?expire=2000000000" },
                    { "itag": 251, "mimeType": "audio/webm", "bitrate": 160000, "url": "https://b.googlevideo.com/videoplayback?expire=2000000001" }
                ]
            }
        });
        let player = parse_player_response(&raw);
        assert_eq!(player.video_id, "abc");
        assert_eq!(player.stream.itag, 251);
        assert_eq!(player.all_streams.len(), 2);
        assert!(expiry_from_url(&player.stream.url).is_some());
    }

    #[test]
    fn parse_explicit_badge_detected_and_absent() {
        // Item with MUSIC_EXPLICIT_BADGE — must return true.
        let explicit = json!({
            "videoId": "explicit-vid",
            "title": { "runs": [{ "text": "Explicit Song" }] },
            "badges": [{
                "musicInlineBadgeRenderer": {
                    "icon": { "iconType": "MUSIC_EXPLICIT_BADGE" }
                }
            }]
        });
        let item = parse_song_item(&explicit);
        assert!(item.is_explicit, "expected is_explicit=true for MUSIC_EXPLICIT_BADGE");

        // Item without any badges array — must default to false.
        let clean = json!({
            "videoId": "clean-vid",
            "title": { "runs": [{ "text": "Clean Song" }] }
        });
        let item = parse_song_item(&clean);
        assert!(!item.is_explicit, "expected is_explicit=false when badges absent");

        // Item with unrelated badge — must not set explicit.
        let other_badge = json!({
            "videoId": "other-vid",
            "badges": [{
                "musicInlineBadgeRenderer": {
                    "icon": { "iconType": "MUSIC_AUDIO_QUALITY_HD" }
                }
            }]
        });
        let item = parse_song_item(&other_badge);
        assert!(!item.is_explicit, "expected is_explicit=false for unrelated badge");
    }
}
