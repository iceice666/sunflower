use crate::*;

pub(crate) async fn search(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = authorize(&headers, &uri, &state).await {
        return response;
    }
    let query_raw = uri.query().unwrap_or_default();
    let query = decoded_query_param(query_raw, "q")
        .unwrap_or_default()
        .trim()
        .to_string();
    if query.len() < 2 {
        return legacy_json_error(StatusCode::BAD_REQUEST, "invalid_query");
    }
    let Some(yt) = &state.yt else {
        return legacy_json_error(StatusCode::SERVICE_UNAVAILABLE, "yt_unavailable");
    };
    match tokio::time::timeout(Duration::from_secs(8), yt.search(&query)).await {
        Ok(Ok(page)) => Json(search_response_from_page(
            &query,
            page,
            search_limit(query_raw),
        ))
        .into_response(),
        Ok(Err(_)) | Err(_) => legacy_json_error(StatusCode::BAD_GATEWAY, "search_unavailable"),
    }
}
pub(crate) fn search_response_from_page(
    query: &str,
    page: innertube::SearchPage,
    limit: i64,
) -> SearchResponse {
    let limit = usize::try_from(limit.max(0)).unwrap_or_default();
    let mut songs = Vec::with_capacity(page.songs.len().min(limit));
    for song in page.songs {
        if songs.len() >= limit {
            break;
        }
        if song.video_id.is_empty() || song.title.is_empty() {
            continue;
        }
        songs.push(SearchSongResponse {
            media_id: format!("yt:{}", song.video_id.trim_start_matches("yt:")),
            source: "yt".into(),
            title: song.title,
            artists: song.artists,
            thumbnail_url: non_empty_string(song.thumbnail_url),
            duration_ms: song.duration_ms,
        });
    }

    let mut albums = Vec::with_capacity(page.albums.len().min(limit));
    for album in page.albums {
        if albums.len() >= limit {
            break;
        }
        if album.browse_id.is_empty() || album.title.is_empty() {
            continue;
        }
        albums.push(SearchAlbumResponse {
            browse_id: album.browse_id,
            title: album.title,
            artists: album.artists,
            thumbnail_url: non_empty_string(album.thumbnail_url),
        });
    }

    let mut artists = Vec::with_capacity(page.artists.len().min(limit));
    for artist in page.artists {
        if artists.len() >= limit {
            break;
        }
        if artist.browse_id.is_empty() || artist.name.is_empty() {
            continue;
        }
        artists.push(SearchArtistResponse {
            browse_id: artist.browse_id,
            name: artist.name,
            thumbnail_url: non_empty_string(artist.thumbnail_url),
        });
    }

    SearchResponse {
        query: query.to_string(),
        songs,
        albums,
        artists,
        continuation: page.continuation.filter(|value| !value.is_empty()),
    }
}
