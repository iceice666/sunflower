use crate::*;

pub(crate) async fn healthz() -> Response {
    legacy_json_response(
        StatusCode::OK,
        serde_json::json!({ "status": HealthzResponse::default().status }),
    )
}

pub(crate) async fn setup_status(State(state): State<AppState>) -> Response {
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

pub(crate) async fn setup_owner(
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
