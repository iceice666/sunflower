use crate::*;

pub(crate) async fn list_playlists(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
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

pub(crate) async fn create_playlist(
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

pub(crate) async fn get_playlist(
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

pub(crate) async fn update_playlist(
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

pub(crate) async fn delete_playlist(
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

pub(crate) async fn add_playlist_item(
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

pub(crate) async fn remove_playlist_item(
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
