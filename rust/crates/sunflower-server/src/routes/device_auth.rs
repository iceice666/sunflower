use crate::*;

pub(crate) async fn register_device(
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

pub(crate) async fn record_register_device_response(
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
pub(crate) async fn device_youtube_cookie_status(
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

pub(crate) async fn device_upload_youtube_cookies(
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
