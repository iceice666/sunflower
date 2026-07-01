use crate::*;

pub(crate) async fn start_scan(
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
        let request = match StartScanRequest::parse_json(&raw) {
            Ok(request) => request,
            Err(err) => return legacy_json_error(StatusCode::BAD_REQUEST, err.legacy_error_code()),
        };
        start_scan_job(&state, request)
    })
    .await
}

pub(crate) fn start_scan_job(state: &AppState, request: StartScanRequest) -> Response {
    match enqueue_scan_job(state, request) {
        Ok(response) => Json(response).into_response(),
        Err(response) => *response,
    }
}

pub(crate) fn enqueue_scan_job(
    state: &AppState,
    request: StartScanRequest,
) -> ResponseResult<StartScanResponse> {
    let Some(store) = &state.store else {
        return Err(Box::new(legacy_json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
        )));
    };
    let job = state.jobs.create();
    tokio::spawn(jobs::run_scan_job(
        state.jobs.clone(),
        store.clone(),
        job.id.clone(),
        request.roots,
        state.data_dir.clone(),
    ));
    Ok(StartScanResponse { job_id: job.id })
}

pub(crate) async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }
    match state.jobs.get(&id) {
        Some(job) => Json(job).into_response(),
        None => legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
    }
}

pub(crate) async fn list_songs(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }

    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let (limit, offset) = pagination(uri.query());
    match store.list_library_songs(limit, offset).await {
        Ok(songs) => Json(SongListResponse { songs }).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

pub(crate) async fn list_albums(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }

    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let (limit, offset) = pagination(uri.query());
    match store.list_library_albums(limit, offset).await {
        Ok(albums) => Json(AlbumListResponse { albums }).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

pub(crate) async fn list_artists(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }

    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let (limit, offset) = pagination(uri.query());
    match store.list_library_artists(limit, offset).await {
        Ok(artists) => Json(ArtistListResponse { artists }).into_response(),
        Err(_) => legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}
pub(crate) async fn serve_album_art(
    State(state): State<AppState>,
    Path(album_media_id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }
    let size = match album_art_size(uri.query()) {
        Ok(size) => size,
        Err(response) => return *response,
    };
    let path = FsPath::new(&state.data_dir)
        .join("art")
        .join(album_media_id)
        .join(format!("{size}.jpg"));
    match serve_local_file(
        &path.to_string_lossy(),
        headers
            .get(header::RANGE)
            .and_then(|value| value.to_str().ok()),
    )
    .await
    {
        Ok(response) => response,
        Err(StreamFileError::NotFound) => legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
        Err(StreamFileError::InvalidRange { len }) => range_not_satisfiable(len),
        Err(StreamFileError::Internal) => {
            legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
        }
    }
}

pub(crate) async fn song_hash(
    State(state): State<AppState>,
    Path(media_id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    let path = match store.song_file_lookup(&media_id).await {
        Ok(SongFileLookup::Path(path)) => path,
        Ok(SongFileLookup::Missing) => {
            return legacy_json_error(StatusCode::NOT_FOUND, "not_found");
        }
        Ok(SongFileLookup::NotLocal) => {
            return legacy_json_error(StatusCode::NOT_FOUND, "not_local");
        }
        Err(_) => return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    let (sha256, bytes) = match hash_file(&path) {
        Ok(hash) => hash,
        Err(HashFileError::NotFound) => {
            return legacy_json_error(StatusCode::NOT_FOUND, "not_found");
        }
        Err(HashFileError::Internal) => {
            return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        }
    };
    Json(SongHashResponse {
        media_id,
        sha256,
        bytes,
    })
    .into_response()
}

pub(crate) async fn stream_song(
    State(state): State<AppState>,
    Path(media_id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }
    let Some(store) = &state.store else {
        return legacy_json_error(StatusCode::NOT_FOUND, "not_found");
    };
    let Some(path) = (match store.song_stream_path(&media_id).await {
        Ok(path) => path,
        Err(_) => return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }) else {
        return legacy_json_error(StatusCode::NOT_FOUND, "not_found");
    };
    match serve_local_file(
        &path,
        headers
            .get(header::RANGE)
            .and_then(|value| value.to_str().ok()),
    )
    .await
    {
        Ok(response) => response,
        Err(StreamFileError::NotFound) => legacy_json_error(StatusCode::NOT_FOUND, "not_found"),
        Err(StreamFileError::InvalidRange { len }) => range_not_satisfiable(len),
        Err(StreamFileError::Internal) => {
            legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
        }
    }
}
