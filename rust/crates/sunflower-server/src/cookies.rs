use crate::*;

pub(crate) fn default_innertube_backend(
    store: Option<PostgresStore>,
    cookie_key: Option<[u8; 32]>,
    cookie_file: Option<String>,
) -> Option<Arc<dyn innertube::InnerTubeBackend>> {
    if matches!(
        env::var("SUNFLOWER_INNERTUBE_DISABLED").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "True")
    ) {
        return None;
    }
    let locale = innertube::Locale {
        hl: env::var("SUNFLOWER_YT_HL").unwrap_or_else(|_| "en".into()),
        gl: env::var("SUNFLOWER_YT_GL").unwrap_or_else(|_| "US".into()),
    };
    let mut client = match env::var("SUNFLOWER_INNERTUBE_BASE_URL") {
        Ok(base_url) if !base_url.is_empty() => {
            innertube::HttpInnerTubeClient::new(base_url, locale)
        }
        _ => innertube::HttpInnerTubeClient::production(locale),
    }
    .ok()?;
    if cookie_key.is_some() || cookie_file.is_some() {
        client = client.with_cookie_provider(Arc::new(YoutubeCookieProvider::new(
            store,
            cookie_key,
            cookie_file,
        )));
    }
    Some(Arc::new(client))
}

pub(crate) struct YoutubeCookieProvider {
    store: Option<PostgresStore>,
    key: Option<[u8; 32]>,
    file: Option<String>,
    cache: Mutex<Option<(SystemTime, Option<String>)>>,
}

impl YoutubeCookieProvider {
    fn new(store: Option<PostgresStore>, key: Option<[u8; 32]>, file: Option<String>) -> Self {
        Self {
            store,
            key,
            file,
            cache: Mutex::new(None),
        }
    }

    async fn load_cookie_header(&self) -> Option<String> {
        if let (Some(store), Some(key)) = (&self.store, self.key)
            && let Ok(Some(raw)) = store.load_first_youtube_cookies(key).await
            && let Some(header) = parse_youtube_cookie_header(&raw)
        {
            return Some(header);
        }
        let Some(file) = &self.file else {
            return None;
        };
        std::fs::read(file)
            .ok()
            .and_then(|raw| parse_youtube_cookie_header(&raw))
    }
}

impl innertube::CookieProvider for YoutubeCookieProvider {
    fn cookie_header<'a>(&'a self) -> BoxFuture<'a, Option<String>> {
        Box::pin(async move {
            if let Some((fetched_at, cached)) =
                self.cache.lock().ok().and_then(|cache| cache.clone())
                && fetched_at.elapsed().unwrap_or_default() < Duration::from_secs(60)
            {
                return cached;
            }
            let loaded = self.load_cookie_header().await;
            if let Ok(mut cache) = self.cache.lock() {
                *cache = Some((SystemTime::now(), loaded.clone()));
            }
            loaded
        })
    }
}

pub(crate) fn parse_youtube_cookie_header(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("***INNERTUBE COOKIE***")
            && let Some((_, value)) = rest.split_once('=')
        {
            return normalize_cookie_header(value);
        }
    }
    if text.contains('\t') {
        return parse_netscape_cookie_header(&text);
    }
    let trimmed = text.trim();
    if trimmed.contains('=') && !trimmed.contains('\n') {
        return normalize_cookie_header(trimmed);
    }
    None
}

pub(crate) fn normalize_cookie_header(raw: &str) -> Option<String> {
    let cookies = parse_cookie_header_pairs(raw)?;
    Some(
        cookies
            .into_iter()
            .map(|(name, value, quoted)| request_cookie_pair(&name, &value, quoted))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

pub(crate) fn parse_netscape_cookie_header(raw: &str) -> Option<String> {
    let parts = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let fields = line.splitn(7, '\t').collect::<Vec<_>>();
            (fields.len() >= 7).then(|| request_cookie_pair(fields[5], fields[6], false))
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("; "))
}

pub(crate) fn parse_cookie_header_pairs(raw: &str) -> Option<Vec<(String, String, bool)>> {
    let trimmed = trim_http_space(raw);
    if trimmed.is_empty() {
        return None;
    }
    let mut cookies = Vec::new();
    for part in trimmed.split(';') {
        let part = trim_http_space(part);
        let (name, value) = part.split_once('=')?;
        if !is_cookie_token(name) {
            return None;
        }
        let (value, quoted) = parse_cookie_header_value(value)?;
        cookies.push((name.to_string(), value, quoted));
    }
    (!cookies.is_empty()).then_some(cookies)
}

pub(crate) fn parse_cookie_header_value(raw: &str) -> Option<(String, bool)> {
    let mut value = raw;
    let quoted = value.len() > 1 && value.starts_with('"') && value.ends_with('"');
    if quoted {
        value = &value[1..value.len() - 1];
    }
    value
        .bytes()
        .all(valid_cookie_value_byte)
        .then(|| (value.to_string(), quoted))
}

pub(crate) fn request_cookie_pair(name: &str, value: &str, quoted: bool) -> String {
    format!(
        "{}={}",
        sanitize_cookie_name(name),
        sanitize_cookie_value(value, quoted)
    )
}

pub(crate) fn sanitize_cookie_name(name: &str) -> String {
    name.replace(['\n', '\r'], "-")
}

pub(crate) fn sanitize_cookie_value(value: &str, quoted: bool) -> String {
    let sanitized = value
        .bytes()
        .filter(|byte| valid_cookie_value_byte(*byte))
        .map(char::from)
        .collect::<String>();
    if quoted || sanitized.contains([' ', ',']) {
        format!("\"{sanitized}\"")
    } else {
        sanitized
    }
}

pub(crate) fn valid_cookie_value_byte(byte: u8) -> bool {
    (0x20..0x7f).contains(&byte) && byte != b'"' && byte != b';' && byte != b'\\'
}

pub(crate) fn is_cookie_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii()
                && byte > 0x20
                && byte < 0x7f
                && !matches!(
                    byte,
                    b'(' | b')'
                        | b'<'
                        | b'>'
                        | b'@'
                        | b','
                        | b';'
                        | b':'
                        | b'\\'
                        | b'"'
                        | b'/'
                        | b'['
                        | b']'
                        | b'?'
                        | b'='
                        | b'{'
                        | b'}'
                )
        })
}

pub(crate) fn trim_http_space(value: &str) -> &str {
    value.trim_matches(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
}

pub(crate) fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get_all(header::COOKIE).iter().find_map(|value| {
        let raw = value.to_str().ok()?;
        raw.trim_matches(http_space).split(';').find_map(|part| {
            let part = part.trim_matches(http_space);
            if part.is_empty() {
                return None;
            }
            let (candidate, value) = part.split_once('=').unwrap_or((part, ""));
            let candidate = candidate.trim_matches(http_space);
            if candidate != name {
                return None;
            }
            parse_cookie_value(value).map(str::to_string)
        })
    })
}

pub(crate) fn http_space(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

pub(crate) fn parse_cookie_value(raw: &str) -> Option<&str> {
    let value = if raw.len() > 1 && raw.starts_with('"') && raw.ends_with('"') {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };
    value
        .bytes()
        .all(|byte| (0x20..0x7f).contains(&byte) && byte != b'"' && byte != b';' && byte != b'\\')
        .then_some(value)
}

pub(crate) fn append_cookie(response: &mut Response, cookie: String) {
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
}

pub(crate) fn admin_cookie(
    name: &str,
    value: &str,
    expires_at: DateTime<Utc>,
    http_only: bool,
    secure: bool,
) -> String {
    let mut cookie = format!(
        "{name}={value}; Path=/; Expires={}",
        expires_at.format("%a, %d %b %Y %H:%M:%S GMT")
    );
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if secure {
        cookie.push_str("; Secure");
    }
    cookie.push_str("; SameSite=Lax");
    cookie
}

pub(crate) fn clear_admin_cookie(name: &str, http_only: bool, secure: bool) -> String {
    let mut cookie = format!("{name}=; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=0");
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if secure {
        cookie.push_str("; Secure");
    }
    cookie.push_str("; SameSite=Lax");
    cookie
}
