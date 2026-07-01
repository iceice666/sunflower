use crate::*;

pub(crate) async fn post_like(
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

pub(crate) async fn post_events(
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

pub(crate) async fn post_impressions(
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
pub(crate) fn scrobble_qualifies(total_played_ms: i32, duration_ms: i32) -> bool {
    if total_played_ms >= 30_000 {
        return true;
    }
    duration_ms > 0 && (total_played_ms as f64) >= (duration_ms as f64 * 0.5)
}
