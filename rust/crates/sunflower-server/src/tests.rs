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
use futures_util::{SinkExt, StreamExt};
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
    DEFAULT_DATABASE_URL, DEFAULT_LISTEN_ADDR, DEFAULT_SETUP_TOKEN, FakeInnerTube,
    LegacyRouteConfig, ProxySigner, StreamProxy, admin_api_csrf_token, admin_audit_limit,
    admin_cookie, admin_form_csrf_token, album_art_size, append_legacy_json_newline, bool_param,
    clear_admin_cookie, configured_cookie_file_from, configured_data_dir, configured_database_url,
    configured_dev_open_registration, configured_listen_addr, configured_setup_token, cookie_value,
    decoded_query_param, form_value, go_wildcard_socket_addr, healthz, hex_lower_bytes, innertube,
    is_legacy_idempotent_mutation, legacy_allowed_methods_for_path,
    legacy_idempotent_mutating_route_patterns, legacy_json_response, legacy_url_path,
    legacy_wire_body_for_hash, pagination, parse_form, parse_request_form,
    parse_youtube_cookie_header, path_segment, query_param, query_token, rate_limit_key,
    require_admin_csrf, router_with_auth, router_with_config, router_with_state_and_config,
    router_with_state_and_config_and_hub, router_with_state_and_data_dir, router_with_store,
    scrobble_qualifies, search_limit, serve_local_file, should_proxy_youtube, test_router_config,
};

static PG_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

    let oversized_cookie_form_body = format!("csrf_token={csrf}&cookies={}", "a".repeat(1 << 20));
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
    let oversized_cookie_form_body = body::to_bytes(oversized_cookie_form.into_body(), usize::MAX)
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
            r#"{"type":"tick","media_id":"yt:abc","position_ms":5000,"is_playing":true}"#.into(),
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
    let image =
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 2, image::Rgb([255, 0, 0])));
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
    let app =
        router_with_config(test_router_config(AuthMode::Database, Some(store)).with_yt(Some(yt)));

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
    let app =
        router_with_config(test_router_config(AuthMode::Database, Some(store)).with_yt(Some(yt)));

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
    let app =
        router_with_config(test_router_config(AuthMode::Database, Some(store)).with_yt(Some(yt)));

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
    let like_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM likes WHERE user_id = $1 AND song_media_id = $2")
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
    let like_count_after_replay: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM likes WHERE user_id = $1 AND song_media_id = $2")
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

    sqlx::query("UPDATE idempotency_log SET expires_at = now() - interval '1 hour' WHERE key = $1")
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
    let like_count_after_unlike: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM likes WHERE user_id = $1 AND song_media_id = $2")
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
            .any(|download| download["media_id"] == current_media_id && download["bytes"] == 16)
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
