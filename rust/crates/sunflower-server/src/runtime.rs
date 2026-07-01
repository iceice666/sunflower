use crate::*;

pub(crate) fn start_idempotency_gc(store: PostgresStore) {
    tokio::spawn(async move {
        let _ = store.gc_expired_idempotency_log().await;
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
        interval.tick().await;
        loop {
            interval.tick().await;
            let _ = store.gc_expired_idempotency_log().await;
        }
    });
}

pub(crate) fn configured_database_url(value: Option<String>) -> String {
    non_empty_or(value, DEFAULT_DATABASE_URL)
}

pub(crate) fn configured_listen_addr(value: Option<String>) -> String {
    non_empty_or(value, DEFAULT_LISTEN_ADDR)
}

pub(crate) fn configured_data_dir(value: Option<String>) -> String {
    non_empty_or(value, "./data")
}

pub(crate) async fn bind_listen_addr(
    listen_addr: &str,
) -> std::io::Result<tokio::net::TcpListener> {
    match go_wildcard_socket_addr(listen_addr) {
        Some(addr) => tokio::net::TcpListener::bind(addr).await,
        None => tokio::net::TcpListener::bind(listen_addr).await,
    }
}

pub(crate) fn go_wildcard_socket_addr(listen_addr: &str) -> Option<SocketAddr> {
    let port = listen_addr.strip_prefix(':')?;
    if port.is_empty() || port.contains(':') {
        return None;
    }
    let port = port.parse::<u16>().ok()?;
    Some(SocketAddr::from(([0, 0, 0, 0], port)))
}

pub(crate) fn runtime_setup_token() -> anyhow::Result<String> {
    configured_setup_token(env::var("SUNFLOWER_SETUP_TOKEN").ok())
}

pub(crate) fn configured_setup_token(value: Option<String>) -> anyhow::Result<String> {
    match value.filter(|token| !token.is_empty()) {
        Some(token) => Ok(token),
        None => generate_setup_token(),
    }
}

pub(crate) fn runtime_dev_open_registration() -> bool {
    configured_dev_open_registration(
        env::var("SUNFLOWER_ENV").ok(),
        env::var("SUNFLOWER_DEV_OPEN_REGISTRATION").ok(),
    )
}

pub(crate) fn configured_dev_open_registration(
    env_value: Option<String>,
    flag_value: Option<String>,
) -> bool {
    env_value.as_deref() == Some("development") && flag_value.as_deref() == Some("1")
}

pub(crate) fn generate_setup_token() -> anyhow::Result<String> {
    let mut token = [0u8; 16];
    rand::thread_rng().try_fill_bytes(&mut token)?;
    Ok(hex_lower_bytes(&token))
}

pub(crate) fn non_empty_or(value: Option<String>, fallback: &str) -> String {
    value
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

pub(crate) fn parse_stream_proxy_key_env() -> anyhow::Result<Vec<u8>> {
    match env::var("SUNFLOWER_STREAM_PROXY_KEY") {
        Ok(raw) if !raw.is_empty() => {
            if raw.len() < 64 || raw.len() % 2 != 0 {
                anyhow::bail!(
                    "SUNFLOWER_STREAM_PROXY_KEY must be at least 64 hex chars (32 bytes)"
                );
            }
            let mut key = Vec::with_capacity(raw.len() / 2);
            for chunk in raw.as_bytes().chunks_exact(2) {
                let Some(hi) = hex_value(chunk[0]) else {
                    anyhow::bail!(
                        "SUNFLOWER_STREAM_PROXY_KEY must be at least 64 hex chars (32 bytes)"
                    );
                };
                let Some(lo) = hex_value(chunk[1]) else {
                    anyhow::bail!(
                        "SUNFLOWER_STREAM_PROXY_KEY must be at least 64 hex chars (32 bytes)"
                    );
                };
                key.push((hi << 4) | lo);
            }
            Ok(key)
        }
        _ => Ok(random_stream_proxy_key()),
    }
}

pub(crate) fn random_stream_proxy_key() -> Vec<u8> {
    let mut key = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

pub(crate) fn parse_cookie_key_env() -> anyhow::Result<Option<[u8; 32]>> {
    let raw = match env::var("SUNFLOWER_COOKIE_KEY") {
        Ok(raw) if !raw.is_empty() => raw,
        _ => return Ok(None),
    };
    if raw.len() != 64 {
        anyhow::bail!("SUNFLOWER_COOKIE_KEY must be 64 hex chars (32 bytes)");
    }
    let mut key = [0u8; 32];
    for (index, chunk) in raw.as_bytes().chunks_exact(2).enumerate() {
        let Some(hi) = hex_value(chunk[0]) else {
            anyhow::bail!("SUNFLOWER_COOKIE_KEY must be 64 hex chars (32 bytes)");
        };
        let Some(lo) = hex_value(chunk[1]) else {
            anyhow::bail!("SUNFLOWER_COOKIE_KEY must be 64 hex chars (32 bytes)");
        };
        key[index] = (hi << 4) | lo;
    }
    Ok(Some(key))
}

pub(crate) fn configured_cookie_file() -> Option<String> {
    configured_cookie_file_from(env::var("SUNFLOWER_YT_COOKIE_FILE").ok())
}

pub(crate) fn configured_cookie_file_from(value: Option<String>) -> Option<String> {
    Some(non_empty_or(value, ".env.innertube_cookie"))
}

pub(crate) fn stream_proxy_mode() -> String {
    env::var("SUNFLOWER_STREAM_PROXY").unwrap_or_else(|_| "auto".into())
}

pub(crate) fn should_proxy_youtube(mode: &str, cookies_configured: bool) -> bool {
    match mode.trim().to_ascii_lowercase().as_str() {
        "always" => true,
        "never" => false,
        _ => cookies_configured,
    }
}

pub(crate) fn api_rfc3339_seconds(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub(crate) fn is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        == Some("https")
}

pub(crate) fn server_base_url(state: &AppState, headers: &HeaderMap) -> String {
    if !state.public_base_url.is_empty() {
        return state.public_base_url.trim_end_matches('/').to_string();
    }
    let scheme = if is_https(headers) { "https" } else { "http" };
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|host| !host.is_empty())
        .unwrap_or("localhost");
    format!("{scheme}://{host}")
}
