use crate::*;

pub(crate) async fn register_download(
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
    let device_id = match authorized_device_id(&id, &auth) {
        Ok(device_id) => device_id,
        Err(response) => return *response,
    };

    run_idempotent(&state, &headers, &uri, "POST", &auth, async {
        let raw = String::from_utf8_lossy(&body);
        let request = match RegisterDownloadRequest::parse_json(&raw) {
            Ok(request) => request,
            Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        };
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };
        match store
            .upsert_download(
                device_id,
                &request.media_id,
                &request.local_path,
                request.bytes,
            )
            .await
        {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    })
    .await
}

pub(crate) async fn list_downloads(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let device_id = match authorized_device_id(&id, &auth) {
        Ok(device_id) => device_id,
        Err(response) => return *response,
    };

    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    match store.list_downloads(device_id).await {
        Ok(downloads) => Json(DownloadListResponse { downloads }).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

pub(crate) async fn delete_download(
    State(state): State<AppState>,
    Path((id, media_id)): Path<(String, String)>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let device_id = match authorized_device_id(&id, &auth) {
        Ok(device_id) => device_id,
        Err(response) => return *response,
    };

    run_idempotent(&state, &headers, &uri, "DELETE", &auth, async {
        let Some(store) = &state.store else {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        };
        match store.delete_download(device_id, &media_id).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    })
    .await
}
