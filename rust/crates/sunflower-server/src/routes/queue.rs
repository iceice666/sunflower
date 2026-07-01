use crate::*;

pub(crate) async fn start_queue(
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

pub(crate) async fn get_queue(
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

pub(crate) async fn get_next(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
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
pub(crate) fn shuffle_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or_default()
}
