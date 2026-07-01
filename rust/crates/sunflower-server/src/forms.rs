use crate::*;

pub(crate) fn is_form_urlencoded(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|mime| {
            mime.trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        })
}

pub(crate) fn redirect_found(location: &str) -> Response {
    redirect_found_for_method(location, &Method::GET)
}

pub(crate) fn redirect_found_post(location: &str) -> Response {
    redirect_found_for_method(location, &Method::POST)
}

pub(crate) fn redirect_found_for_method(location: &str, method: &Method) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, location);
    let body = if method == Method::GET {
        builder = builder.header(header::CONTENT_TYPE, "text/html; charset=utf-8");
        Body::from(format!(
            "<a href=\"{}\">Found</a>.\n\n",
            escape_html(location)
        ))
    } else {
        Body::empty()
    };
    builder
        .body(body)
        .unwrap_or_else(|_| legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"))
}

pub(crate) fn admin_html_page(
    title: &str,
    csrf_token: Option<&str>,
    flash: Option<&str>,
    body: &str,
) -> Response {
    let csrf = csrf_token.unwrap_or_default();
    let flash_html = flash
        .filter(|flash| !flash.is_empty())
        .map(|flash| format!(r#"<p class="flash">{}</p>"#, escape_html(flash)))
        .unwrap_or_default();
    let body = body.replace("{{csrf}}", &escape_html(csrf));
    let html = format!(
        r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>{title}</title><link rel="stylesheet" href="/admin/static/admin.css"></head>
<body><main>
<nav>
<a href="/admin/">Overview</a>
<a href="/admin/devices">Devices</a>
<a href="/admin/pairing/new">Pairing</a>
<a href="/admin/library">Library</a>
<a href="/admin/cookies/youtube">Cookies</a>
<a href="/admin/now-playing">Now Playing</a>
<a href="/admin/audit">Audit</a>
</nav>
<h1>{title}</h1>
{flash_html}
{body}
</main></body>
</html>"#,
        title = escape_html(title),
        flash_html = flash_html,
        body = body
    );
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

pub(crate) fn admin_html_error(status: StatusCode, message: &str) -> Response {
    let mut response = admin_html_page(
        "Error",
        None,
        None,
        &format!(r#"<p class="error">{}</p>"#, escape_html(message)),
    );
    *response.status_mut() = status;
    response
}

pub(crate) fn admin_auth_error_code(err: AuthStoreError) -> &'static str {
    match err {
        AuthStoreError::InvalidPassword => "invalid_password",
        AuthStoreError::SetupRequired => "setup_required",
        AuthStoreError::MissingAdminSession => "missing_admin_session",
        AuthStoreError::InvalidAdminSession => "invalid_admin_session",
        AuthStoreError::Backend(_) => "internal",
        _ => "invalid_password",
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedForm {
    pub(crate) values: Vec<(String, String)>,
    pub(crate) invalid: bool,
}

pub(crate) fn parse_form(body: &[u8]) -> ParsedForm {
    let mut form = ParsedForm::default();
    parse_urlencoded_into(&mut form, &String::from_utf8_lossy(body));
    form
}

pub(crate) fn parse_request_form(
    method: &Method,
    headers: &HeaderMap,
    query: Option<&str>,
    body: &[u8],
) -> ParsedForm {
    let mut form = ParsedForm::default();
    if matches!(*method, Method::POST | Method::PUT | Method::PATCH) && is_form_urlencoded(headers)
    {
        parse_urlencoded_into(&mut form, &String::from_utf8_lossy(body));
    }
    parse_urlencoded_into(&mut form, query.unwrap_or_default());
    form
}

pub(crate) fn parse_urlencoded_into(form: &mut ParsedForm, raw: &str) {
    for part in raw.split('&').filter(|part| !part.is_empty()) {
        if part.contains(';') {
            form.invalid = true;
            continue;
        }
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        match (form_decode(key), form_decode(value)) {
            (Ok(key), Ok(value)) => form.values.push((key, value)),
            _ => form.invalid = true,
        }
    }
}

pub(crate) fn form_value(form: &ParsedForm, key: &str) -> String {
    form_value_opt(form, key).unwrap_or_default()
}

pub(crate) fn form_value_opt(form: &ParsedForm, key: &str) -> Option<String> {
    form.values
        .iter()
        .find_map(|(candidate, value)| (candidate == key).then(|| value.clone()))
}

pub(crate) fn admin_form_csrf_token(headers: &HeaderMap, form: &ParsedForm) -> String {
    headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| form_value(form, "csrf_token").trim().to_string())
}

pub(crate) fn form_decode(raw: &str) -> Result<String, ()> {
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                if let (Some(hi), Some(lo)) =
                    (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
                {
                    out.push((hi << 4) | lo);
                    index += 3;
                } else {
                    return Err(());
                }
            }
            b'%' => return Err(()),
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

pub(crate) fn percent_decode_path(raw: &str) -> Option<String> {
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hi = hex_value(bytes[index + 1])?;
                let lo = hex_value(bytes[index + 2])?;
                out.push((hi << 4) | lo);
                index += 3;
            }
            b'%' => return None,
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

pub(crate) fn path_segment(raw: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => {
                out.push('%');
                out.push(char::from(HEX[(byte >> 4) as usize]));
                out.push(char::from(HEX[(byte & 0x0f) as usize]));
            }
        }
    }
    out
}

pub(crate) fn split_roots(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn escape_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub(crate) fn query_token(query: Option<&str>) -> Option<String> {
    query_param(query.unwrap_or_default(), "token").filter(|value| !value.is_empty())
}

pub(crate) fn query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if pair.is_empty() || pair.contains(';') {
            continue;
        }
        let (candidate, value) = pair.split_once('=').unwrap_or((pair, ""));
        let Ok(candidate) = form_decode(candidate) else {
            continue;
        };
        if candidate != key {
            continue;
        }
        let Ok(value) = form_decode(value) else {
            continue;
        };
        return Some(value);
    }
    None
}

pub(crate) fn pagination(query: Option<&str>) -> (i64, i64) {
    let query = query.unwrap_or_default();
    let limit = query_param(query, "limit")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|value| *value > 0 && *value <= 100)
        .unwrap_or(20);
    let offset = query_param(query, "offset")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|value| *value >= 0)
        .unwrap_or(0);
    (limit, offset)
}

pub(crate) fn bool_param(query: Option<&str>, key: &str) -> bool {
    let Some(value) = query_param(query.unwrap_or_default(), key) else {
        return false;
    };
    matches!(value.as_str(), "1" | "t" | "T" | "true" | "TRUE" | "True")
}

pub(crate) fn album_art_size(query: Option<&str>) -> ResponseResult<i32> {
    let Some(raw) = query_param(query.unwrap_or_default(), "size") else {
        return Ok(512);
    };
    match raw.parse::<i32>() {
        Ok(size @ (256 | 512 | 1024)) => Ok(size),
        _ => Err(Box::new(legacy_json_error(
            StatusCode::BAD_REQUEST,
            "invalid_size",
        ))),
    }
}

pub(crate) fn search_limit(query: &str) -> i64 {
    query_param(query, "limit")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(25))
        .unwrap_or(20)
}

pub(crate) fn admin_audit_limit(query: Option<&str>) -> i64 {
    query_param(query.unwrap_or_default(), "limit")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|value| *value > 0 && *value <= 500)
        .unwrap_or(100)
}

pub(crate) fn decoded_query_param(query: &str, key: &str) -> Option<String> {
    query_param(query, key)
}

pub(crate) fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn non_empty_string(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
