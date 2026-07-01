use crate::*;

pub(crate) fn router_with_config(config: RouterBuildConfig) -> Router {
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
