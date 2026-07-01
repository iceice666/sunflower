use crate::*;

pub(crate) async fn streams_proxy(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
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

pub(crate) async fn resolve_stream(
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
pub(crate) async fn resolve_queue_item(
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

pub(crate) async fn resolve_lookahead_items(
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
pub(crate) enum ResolveMediaError {
    Unavailable,
    Failed,
}

pub(crate) async fn resolve_media_id(
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

pub(crate) async fn resolve_youtube_media_id(
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

pub(crate) fn resolved_response_to_core(response: &ResolvedStreamResponse) -> ResolvedStream {
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

pub(crate) fn youtube_stream_metadata(stream: &innertube::StreamUrl) -> Value {
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

pub(crate) fn non_zero_f64_to_f32(value: f64) -> Option<f32> {
    if value == 0.0 || !value.is_finite() {
        None
    } else {
        Some(value as f32)
    }
}

pub(crate) fn system_time_from_utc(time: DateTime<Utc>) -> SystemTime {
    let Ok(seconds) = u64::try_from(time.timestamp()) else {
        return UNIX_EPOCH;
    };
    UNIX_EPOCH + Duration::from_secs(seconds)
}
