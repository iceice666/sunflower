use crate::*;

#[derive(Clone, Copy)]
pub(crate) struct LegacyRouteConfig {
    pub(crate) streams_proxy_enabled: bool,
}

pub(crate) async fn cors_middleware(
    State(legacy_routes): State<LegacyRouteConfig>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request.method() == Method::OPTIONS
        && request
            .headers()
            .get(header::ACCESS_CONTROL_REQUEST_METHOD)
            .is_some()
    {
        return legacy_cors_preflight(request.headers());
    }

    let method = request.method().clone();
    let request_headers = request.headers().clone();

    if request.method() == Method::HEAD
        && let Some(methods) = legacy_allowed_methods_for_path(request.uri().path(), legacy_routes)
    {
        let mut response = legacy_method_not_allowed(methods);
        apply_legacy_cors_actual(&request_headers, &method, response.headers_mut());
        return response;
    }

    let mut response = next.run(request).await;
    if response.status() == StatusCode::METHOD_NOT_ALLOWED {
        normalize_legacy_allow_header(response.headers_mut());
    }
    response = append_legacy_json_newline(response).await;
    apply_legacy_cors_actual(&request_headers, &method, response.headers_mut());
    response
}

pub(crate) fn legacy_cors_preflight(request_headers: &HeaderMap) -> Response {
    let mut response = StatusCode::OK.into_response();
    append_header_once(response.headers_mut(), header::VARY, "Origin");
    append_header_once(
        response.headers_mut(),
        header::VARY,
        "Access-Control-Request-Method",
    );
    append_header_once(
        response.headers_mut(),
        header::VARY,
        "Access-Control-Request-Headers",
    );

    let Some(origin) = request_headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .filter(|origin| !origin.is_empty())
    else {
        return response;
    };
    if origin.is_empty() {
        return response;
    }
    let Some(request_method) = request_headers
        .get(header::ACCESS_CONTROL_REQUEST_METHOD)
        .and_then(|value| value.to_str().ok())
    else {
        return response;
    };
    let request_method = request_method.to_ascii_uppercase();
    if !legacy_cors_method_allowed(&request_method) {
        return response;
    }
    let requested_headers = request_headers
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .and_then(|value| value.to_str().ok())
        .map(parse_legacy_cors_header_list)
        .unwrap_or_default();
    if !legacy_cors_headers_allowed(&requested_headers) {
        return response;
    }

    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    if let Ok(value) = HeaderValue::from_str(&request_method) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_METHODS, value);
    }
    if !requested_headers.is_empty()
        && let Ok(value) = HeaderValue::from_str(&requested_headers.join(", "))
    {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_HEADERS, value);
    }
    response.headers_mut().insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("300"),
    );
    response
}

pub(crate) fn apply_legacy_cors_actual(
    request_headers: &HeaderMap,
    method: &Method,
    response_headers: &mut HeaderMap,
) {
    append_header_once(response_headers, header::VARY, "Origin");
    let Some(origin) = request_headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .filter(|origin| !origin.is_empty())
    else {
        return;
    };
    if origin.is_empty() || !legacy_cors_method_allowed(method.as_str()) {
        return;
    }
    response_headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    response_headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("Link"),
    );
}

pub(crate) fn legacy_cors_method_allowed(method: &str) -> bool {
    matches!(
        method.to_ascii_uppercase().as_str(),
        "GET" | "POST" | "PATCH" | "PUT" | "DELETE" | "OPTIONS"
    )
}

pub(crate) fn legacy_cors_headers_allowed(headers: &[String]) -> bool {
    headers.iter().all(|header| {
        matches!(
            header.as_str(),
            "Accept" | "Authorization" | "Content-Type" | "Idempotency-Key" | "Origin"
        )
    })
}

pub(crate) fn parse_legacy_cors_header_list(header_list: &str) -> Vec<String> {
    let mut headers = Vec::new();
    let mut current = String::new();
    let mut upper = true;
    for (index, byte) in header_list.bytes().enumerate() {
        match byte {
            b'a'..=b'z' => {
                if upper {
                    current.push((byte - (b'a' - b'A')) as char);
                } else {
                    current.push(byte as char);
                }
            }
            b'A'..=b'Z' => {
                if upper {
                    current.push(byte as char);
                } else {
                    current.push((byte + (b'a' - b'A')) as char);
                }
            }
            b'-' | b'_' | b'.' | b'0'..=b'9' => current.push(byte as char),
            _ => {}
        }

        if byte == b' ' || byte == b',' || index == header_list.len().saturating_sub(1) {
            if !current.is_empty() {
                headers.push(std::mem::take(&mut current));
                upper = true;
            }
        } else {
            upper = byte == b'-';
        }
    }
    headers
}

pub(crate) fn append_header_once(
    headers: &mut HeaderMap,
    name: header::HeaderName,
    value: &'static str,
) {
    if headers
        .get_all(&name)
        .iter()
        .any(|existing| existing.to_str().ok() == Some(value))
    {
        return;
    }
    headers.append(name, HeaderValue::from_static(value));
}

pub(crate) async fn append_legacy_json_newline(response: Response) -> Response {
    if !is_legacy_json_encoded_response(response.headers()) {
        return response;
    }

    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    };
    if bytes.is_empty() {
        return Response::from_parts(parts, Body::from(bytes));
    }

    let mut body = escape_legacy_json_html_bytes(&bytes);
    if !body.ends_with(b"\n") {
        body.push(b'\n');
    }
    if body.as_slice() != bytes.as_ref() {
        parts.headers.remove(header::CONTENT_LENGTH);
    }
    Response::from_parts(parts, Body::from(body))
}

pub(crate) fn is_legacy_json_encoded_response(headers: &HeaderMap) -> bool {
    if headers
        .get("Idempotent-Replay")
        .and_then(|value| value.to_str().ok())
        == Some("true")
    {
        return false;
    }
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|content_type| {
            content_type
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        })
        .unwrap_or(false)
}

pub(crate) fn legacy_method_not_allowed(allowed_methods: &'static [&'static str]) -> Response {
    let mut response = StatusCode::METHOD_NOT_ALLOWED.into_response();
    for method in allowed_methods {
        response
            .headers_mut()
            .append(header::ALLOW, HeaderValue::from_static(method));
    }
    response
}

pub(crate) async fn legacy_method_not_allowed_fallback(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let legacy_routes = LegacyRouteConfig {
        streams_proxy_enabled: state.proxy.is_some(),
    };
    let mut response = legacy_allowed_methods_for_path(uri.path(), legacy_routes)
        .map(legacy_method_not_allowed)
        .unwrap_or_else(|| StatusCode::METHOD_NOT_ALLOWED.into_response());
    apply_legacy_cors_actual(&headers, &method, response.headers_mut());
    response
}

pub(crate) async fn legacy_not_found_fallback(method: Method, headers: HeaderMap) -> Response {
    let mut response = legacy_not_found_response();
    apply_legacy_cors_actual(&headers, &method, response.headers_mut());
    response
}

pub(crate) fn legacy_not_found_response() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header("x-content-type-options", "nosniff")
        .body(Body::from("404 page not found\n"))
        .unwrap_or_else(|_| StatusCode::NOT_FOUND.into_response())
}

pub(crate) fn normalize_legacy_allow_header(headers: &mut HeaderMap) {
    let mut methods = Vec::<String>::new();
    for value in headers.get_all(header::ALLOW) {
        let Ok(raw) = value.to_str() else {
            continue;
        };
        for method in raw.split(',') {
            let method = method.trim();
            if method.is_empty() || method.eq_ignore_ascii_case("HEAD") {
                continue;
            }
            if !methods.iter().any(|existing| existing == method) {
                methods.push(method.to_string());
            }
        }
    }
    if methods.is_empty() {
        return;
    }
    headers.remove(header::ALLOW);
    for method in methods {
        if let Ok(value) = HeaderValue::from_str(&method) {
            headers.append(header::ALLOW, value);
        }
    }
}

pub(crate) fn legacy_allowed_methods_for_path(
    path: &str,
    legacy_routes: LegacyRouteConfig,
) -> Option<&'static [&'static str]> {
    match path {
        "/healthz" => Some(LEGACY_ALLOW_GET),
        "/admin/login" => Some(LEGACY_ALLOW_GET_POST),
        "/admin/" => Some(LEGACY_ALLOW_GET),
        "/admin/logout" => Some(LEGACY_ALLOW_POST),
        "/admin/devices" => Some(LEGACY_ALLOW_GET),
        "/admin/pairing/new" => Some(LEGACY_ALLOW_GET),
        "/admin/pairing" => Some(LEGACY_ALLOW_POST),
        "/admin/library" => Some(LEGACY_ALLOW_GET),
        "/admin/library/scan" => Some(LEGACY_ALLOW_POST),
        "/admin/cookies/youtube" => Some(LEGACY_ALLOW_GET_POST),
        "/admin/cookies/youtube/probe" => Some(LEGACY_ALLOW_POST),
        "/admin/cookies/youtube/clear" => Some(LEGACY_ALLOW_POST),
        "/admin/now-playing" => Some(LEGACY_ALLOW_GET),
        "/admin/now-playing/command" => Some(LEGACY_ALLOW_POST),
        "/admin/audit" => Some(LEGACY_ALLOW_GET),
        "/api/v1/setup/status" => Some(LEGACY_ALLOW_GET),
        "/api/v1/setup/owner" => Some(LEGACY_ALLOW_POST),
        "/api/v1/auth/register-device" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/auth/login" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/auth/logout" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/me" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/status" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/devices" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/pairing-codes" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/library/status" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/library/scan" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/cookies/youtube/status" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/cookies/youtube" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/cookies/youtube/probe" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/cookies/youtube/clear" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/now-playing" => Some(LEGACY_ALLOW_GET),
        "/api/v1/admin/now-playing/command" => Some(LEGACY_ALLOW_POST),
        "/api/v1/admin/audit" => Some(LEGACY_ALLOW_GET),
        "/api/v1/queue/start" => Some(LEGACY_ALLOW_POST),
        "/api/v1/next" => Some(LEGACY_ALLOW_GET),
        "/api/v1/home" => Some(LEGACY_ALLOW_GET),
        "/api/v1/search" => Some(LEGACY_ALLOW_GET),
        "/api/v1/likes" => Some(LEGACY_ALLOW_POST),
        "/api/v1/events" => Some(LEGACY_ALLOW_POST),
        "/api/v1/impressions" => Some(LEGACY_ALLOW_POST),
        "/api/v1/playlists" => Some(LEGACY_ALLOW_GET_POST),
        "/api/v1/library/songs" => Some(LEGACY_ALLOW_GET),
        "/api/v1/library/albums" => Some(LEGACY_ALLOW_GET),
        "/api/v1/library/artists" => Some(LEGACY_ALLOW_GET),
        "/api/v1/library/scan" => Some(LEGACY_ALLOW_POST),
        "/api/v1/cookies/youtube/status" => Some(LEGACY_ALLOW_GET),
        "/api/v1/cookies/youtube" => Some(LEGACY_ALLOW_POST),
        "/api/v1/ws/now-playing" => Some(LEGACY_ALLOW_GET),
        "/api/v1/streams/proxy" if legacy_routes.streams_proxy_enabled => Some(LEGACY_ALLOW_GET),
        "/api/v1/streams/resolve" => Some(LEGACY_ALLOW_POST),
        _ if path.starts_with("/admin/static/") => Some(LEGACY_ALLOW_GET),
        _ => LEGACY_DYNAMIC_ROUTES.iter().find_map(|(pattern, methods)| {
            legacy_route_pattern_matches(pattern, path).then_some(*methods)
        }),
    }
}

pub(crate) fn legacy_route_pattern_matches(pattern: &str, path: &str) -> bool {
    let pattern_segments = path_segments(pattern);
    let path_segments = path_segments(path);
    pattern_segments.len() == path_segments.len()
        && pattern_segments
            .iter()
            .zip(path_segments.iter())
            .all(|(pattern, actual)| {
                if pattern.starts_with(':') {
                    !actual.is_empty()
                } else {
                    pattern == actual
                }
            })
}

#[cfg(test)]
pub(crate) fn legacy_idempotent_mutating_route_patterns() -> &'static [(&'static str, &'static str)]
{
    &[
        ("POST", "/api/v1/auth/register-device"),
        ("POST", "/api/v1/library/scan"),
        ("POST", "/api/v1/cookies/youtube"),
        ("POST", "/api/v1/queue/start"),
        ("POST", "/api/v1/streams/resolve"),
        ("POST", "/api/v1/likes"),
        ("POST", "/api/v1/impressions"),
        ("POST", "/api/v1/playlists"),
        ("PATCH", "/api/v1/playlists/:id"),
        ("DELETE", "/api/v1/playlists/:id"),
        ("POST", "/api/v1/playlists/:id/items"),
        ("DELETE", "/api/v1/playlists/:id/items/:media_id"),
        ("POST", "/api/v1/devices/:id/downloads"),
        ("DELETE", "/api/v1/devices/:id/downloads/:media_id"),
        ("POST", "/api/v1/events"),
    ]
}

#[cfg(test)]
pub(crate) fn is_legacy_idempotent_mutation(method: &str, path: &str) -> bool {
    legacy_idempotent_mutating_route_patterns()
        .iter()
        .any(|(route_method, pattern)| {
            *route_method == method && legacy_route_pattern_matches(pattern, path)
        })
}

pub(crate) fn path_segments(path: &str) -> Vec<&str> {
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

pub(crate) fn legacy_json_error(status: StatusCode, code: &str) -> Response {
    legacy_json_response(status, serde_json::json!({ "error": code }))
}

pub(crate) fn legacy_json_response(status: StatusCode, value: serde_json::Value) -> Response {
    let body = serde_json::to_vec(&value).unwrap_or_else(|_| b"{\"error\":\"internal\"}".to_vec());
    let mut body = escape_legacy_json_html_bytes(&body);
    body.push(b'\n');
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub(crate) fn escape_legacy_json_html_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'<' => {
                out.extend_from_slice(br"\u003c");
                index += 1;
            }
            b'>' => {
                out.extend_from_slice(br"\u003e");
                index += 1;
            }
            b'&' => {
                out.extend_from_slice(br"\u0026");
                index += 1;
            }
            0xe2 if index + 2 < bytes.len()
                && bytes[index + 1] == 0x80
                && bytes[index + 2] == 0xa8 =>
            {
                out.extend_from_slice(br"\u2028");
                index += 3;
            }
            0xe2 if index + 2 < bytes.len()
                && bytes[index + 1] == 0x80
                && bytes[index + 2] == 0xa9 =>
            {
                out.extend_from_slice(br"\u2029");
                index += 3;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    out
}

pub(crate) fn legacy_http_error(status: StatusCode, code: &str) -> Response {
    let body = format!("{{\"error\":\"{code}\"}}\n");
    (
        status,
        [
            ("content-type", "text/plain; charset=utf-8"),
            ("x-content-type-options", "nosniff"),
        ],
        body,
    )
        .into_response()
}
