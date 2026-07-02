use crate::*;

pub(crate) async fn admin_login(
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
    append_admin_session_cookies(&mut response, &login, is_https(&headers));
    response
}

pub(crate) async fn admin_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
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

pub(crate) async fn admin_static_asset(Path(path): Path<String>, headers: HeaderMap) -> Response {
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

pub(crate) async fn admin_static_dir_listing() -> Response {
    serve_static_bytes(
        ADMIN_STATIC_DIR_LISTING.as_bytes(),
        "text/html; charset=utf-8",
        None,
    )
}

pub(crate) fn clean_admin_static_path(path: &str) -> String {
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

pub(crate) fn admin_static_slash_redirect(location: &'static str) -> Response {
    Response::builder()
        .status(StatusCode::MOVED_PERMANENTLY)
        .header(header::LOCATION, location)
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::MOVED_PERMANENTLY.into_response())
}

pub(crate) async fn admin_login_page(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
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

pub(crate) async fn admin_login_form(
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
    append_admin_session_cookies(&mut response, &login, is_https(&headers));
    response
}

pub(crate) async fn admin_logout_form(
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

pub(crate) async fn admin_overview_page(
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

pub(crate) async fn admin_devices_page(
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

pub(crate) async fn admin_revoke_device_form(
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

pub(crate) async fn admin_pairing_new_page(
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

pub(crate) async fn admin_create_pairing_form(
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

pub(crate) async fn admin_library_page(
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

pub(crate) async fn admin_start_scan_form(
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

pub(crate) async fn admin_cookies_page(
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
    let token_stored = match &state.store {
        Some(store) => store.has_youtube_innertube_token().await.unwrap_or(false),
        None => false,
    };
    admin_html_page(
        "YouTube Cookies",
        csrf.as_deref(),
        query_param(uri.query().unwrap_or_default(), "flash").as_deref(),
        &format!(
            r#"
<p>Status: {}</p>
<p>InnerTube token: {}</p>
<section>
  <h2>Upload YouTube Cookies</h2>
  <form method="post" action="/admin/cookies/youtube">
    <input type="hidden" name="csrf_token" value="{{{{csrf}}}}">
    <label>Cookie export
      <textarea name="cookies" rows="8" spellcheck="false" autocomplete="off"></textarea>
    </label>
    <button type="submit">Save cookies</button>
  </form>
</section>
<section>
  <h2>InnerTube Token</h2>
  <form method="post" action="/admin/cookies/youtube">
    <input type="hidden" name="csrf_token" value="{{{{csrf}}}}">
    <label>PO token / visitor token
      <textarea name="innertube_token" rows="4" spellcheck="false" autocomplete="off"></textarea>
    </label>
    <button type="submit">Save InnerTube token</button>
  </form>
</section>
"#,
            status
                .map(|status| escape_html(&status.status))
                .unwrap_or_else(|| "unknown".to_string()),
            if token_stored { "stored" } else { "not stored" }
        ),
    )
}

pub(crate) async fn admin_upload_youtube_cookies_form(
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
    let innertube_token = form_value(&form, "innertube_token").trim().to_string();
    let token_upload = innertube_token_upload_bytes(&innertube_token, &raw);
    if raw.is_empty() && token_upload.is_none() {
        return admin_html_error(
            StatusCode::BAD_REQUEST,
            "Paste cookies or an InnerTube token",
        );
    }
    let Some(store) = &state.store else {
        return admin_html_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not store YouTube credentials",
        );
    };
    if !raw.is_empty()
        && store
            .store_youtube_cookies(&session, cookie_key, raw.as_bytes())
            .await
            .is_err()
    {
        return admin_html_error(StatusCode::INTERNAL_SERVER_ERROR, "Could not store cookies");
    }
    if let Some(token_upload) = &token_upload
        && store
            .store_youtube_innertube_token(&session, cookie_key, token_upload)
            .await
            .is_err()
    {
        return admin_html_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not store InnerTube token",
        );
    }
    let token_stored = token_upload.is_some();
    let flash = if !raw.is_empty() && token_stored {
        "youtube_credentials_updated"
    } else if token_stored {
        "innertube_token_updated"
    } else {
        "cookies_updated"
    };
    redirect_found_post(&format!("/admin/cookies/youtube?flash={flash}"))
}

pub(crate) async fn admin_probe_youtube_cookies_form(
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

pub(crate) async fn admin_clear_youtube_cookies_form(
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

pub(crate) async fn admin_now_playing_page(
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

pub(crate) async fn admin_now_playing_command_form(
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

pub(crate) async fn admin_audit_page(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
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

pub(crate) async fn admin_me(State(state): State<AppState>, headers: HeaderMap) -> Response {
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

pub(crate) async fn admin_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
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

pub(crate) async fn admin_devices(State(state): State<AppState>, headers: HeaderMap) -> Response {
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

pub(crate) async fn admin_revoke_device(
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

pub(crate) async fn admin_create_pairing(
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

pub(crate) async fn admin_library_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
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

pub(crate) async fn admin_start_scan(
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

pub(crate) async fn admin_cookies_youtube_status(
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

pub(crate) async fn admin_upload_youtube_cookies(
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
        Ok(request)
            if !request.cookies.trim().is_empty() || !request.innertube_token.trim().is_empty() =>
        {
            request
        }
        _ => return legacy_json_error(StatusCode::BAD_REQUEST, "invalid_format"),
    };
    let token_upload = innertube_token_upload_bytes(&request.innertube_token, &request.cookies);
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    if !request.cookies.trim().is_empty()
        && store
            .store_youtube_cookies(&session, cookie_key, request.cookies.as_bytes())
            .await
            .is_err()
    {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    }
    if let Some(token_upload) = &token_upload
        && store
            .store_youtube_innertube_token(&session, cookie_key, token_upload)
            .await
            .is_err()
    {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    }
    Json(serde_json::json!({"ok": true})).into_response()
}

pub(crate) async fn admin_probe_youtube_cookies(
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

pub(crate) async fn admin_clear_youtube_cookies(
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

pub(crate) async fn admin_now_playing(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
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

pub(crate) async fn admin_now_playing_command(
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

pub(crate) fn send_now_playing_command(
    state: &AppState,
    request: AdminNowPlayingCommandRequest,
) -> Response {
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
pub(crate) async fn admin_audit(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
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

pub(crate) fn innertube_token_upload_bytes(
    explicit_token: &str,
    cookie_export: &str,
) -> Option<Vec<u8>> {
    let explicit_token = explicit_token.trim();
    if !explicit_token.is_empty() {
        return Some(normalize_innertube_token_upload(explicit_token));
    }
    let cookie_export = cookie_export.trim();
    if cookie_export.is_empty() {
        return None;
    }
    innertube::parse_innertube_token(cookie_export.as_bytes())
        .map(|token| serialize_innertube_token(&token).into_bytes())
}

fn normalize_innertube_token_upload(raw: &str) -> Vec<u8> {
    innertube::parse_innertube_token(raw.as_bytes())
        .map(|token| serialize_innertube_token(&token).into_bytes())
        .unwrap_or_else(|| raw.as_bytes().to_vec())
}

fn serialize_innertube_token(token: &innertube::InnerTubeToken) -> String {
    let mut lines = Vec::with_capacity(2);
    if !token.po_token.is_empty() {
        lines.push(format!("po_token={}", token.po_token));
    }
    if !token.visitor_data.is_empty() {
        lines.push(format!("visitor_data={}", token.visitor_data));
    }
    lines.join("\n")
}

/// Appends both the session-token cookie and the CSRF cookie to `response`.
/// Extracted from the two identical cookie-append blocks in `admin_login` and
/// `admin_login_form` to keep them in sync.
fn append_admin_session_cookies(
    response: &mut Response,
    login: &sunflower_storage_postgres::AdminLoginResult,
    https: bool,
) {
    append_cookie(
        response,
        admin_cookie(
            ADMIN_COOKIE_NAME,
            &login.token,
            login.expires_at,
            true,
            https,
        ),
    );
    append_cookie(
        response,
        admin_cookie(
            ADMIN_CSRF_COOKIE_NAME,
            &login.csrf,
            login.expires_at,
            false,
            https,
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn innertube_token_upload_derives_metrolist_visitor_data_from_cookie_export() {
        let upload = innertube_token_upload_bytes(
            "",
            "***INNERTUBE COOKIE*** =SID=abc; HSID=def\n***VISITOR DATA*** =visitor-1\n***DATASYNC ID*** =123",
        )
        .unwrap();

        assert_eq!(String::from_utf8(upload).unwrap(), "visitor_data=visitor-1");
    }

    #[test]
    fn innertube_token_upload_normalizes_explicit_metrolist_token() {
        let upload =
            innertube_token_upload_bytes("***PO TOKEN*** =po-1\n***VISITOR DATA*** =visitor-1", "")
                .unwrap();

        assert_eq!(
            String::from_utf8(upload).unwrap(),
            "po_token=po-1\nvisitor_data=visitor-1"
        );
    }
}
