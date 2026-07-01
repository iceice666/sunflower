use crate::*;

pub(crate) async fn ws_now_playing(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let auth = match authorize(&headers, &uri, &state).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let Some(hub) = state.hub.clone() else {
        return legacy_json_error(StatusCode::SERVICE_UNAVAILABLE, "ws_unavailable");
    };
    let device_id = auth.device_id.to_string();
    ws.protocols([NOW_PLAYING_SUBPROTOCOL])
        .on_upgrade(move |socket| now_playing::serve_socket(socket, hub, device_id))
}
