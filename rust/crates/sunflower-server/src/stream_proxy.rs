use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    body::Body,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::TryStreamExt;
use sha2::{Digest, Sha256};

const DEFAULT_TTL: Duration = Duration::from_secs(15 * 60);
const HMAC_BLOCK_SIZE: usize = 64;

#[derive(Clone)]
pub struct ProxySigner {
    key: Vec<u8>,
    ttl: Duration,
}

impl ProxySigner {
    pub fn new(key: Vec<u8>) -> Self {
        Self {
            key,
            ttl: DEFAULT_TTL,
        }
    }

    pub fn sign(&self, target: &str) -> String {
        self.sign_until(
            target,
            SystemTime::now()
                .checked_add(self.ttl)
                .unwrap_or(SystemTime::now()),
        )
    }

    pub fn sign_until(&self, target: &str, exp: SystemTime) -> String {
        let exp = exp
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default();
        let payload = format!("{{\"u\":{},\"e\":{exp}}}", go_json_string(target));
        let body = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        format!("{body}.{}", self.mac(&body))
    }

    pub fn verify(&self, token: &str) -> Result<String, ProxyTokenError> {
        let (body, sig) = token.split_once('.').ok_or(ProxyTokenError)?;
        let expected = self.mac(body);
        if body.is_empty()
            || sig.is_empty()
            || !constant_time_eq(sig.as_bytes(), expected.as_bytes())
        {
            return Err(ProxyTokenError);
        }
        let raw = URL_SAFE_NO_PAD.decode(body).map_err(|_| ProxyTokenError)?;
        let payload: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|_| ProxyTokenError)?;
        let url = payload
            .get("u")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .ok_or(ProxyTokenError)?;
        let exp = payload
            .get("e")
            .and_then(|value| value.as_i64())
            .ok_or(ProxyTokenError)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default();
        if now > exp {
            return Err(ProxyTokenError);
        }
        Ok(url.to_string())
    }

    fn mac(&self, body: &str) -> String {
        URL_SAFE_NO_PAD.encode(hmac_sha256(&self.key, body.as_bytes()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProxyTokenError;

#[derive(Clone)]
pub struct StreamProxy {
    signer: ProxySigner,
    client: reqwest::Client,
}

impl StreamProxy {
    pub fn new(signer: ProxySigner) -> Self {
        Self {
            signer,
            client: Self::client_builder()
                .build()
                .expect("stream proxy reqwest client"),
        }
    }

    fn client_builder() -> reqwest::ClientBuilder {
        reqwest::Client::builder().redirect(reqwest::redirect::Policy::custom(|attempt| {
            let Some(next) = attempt.url().host_str() else {
                return attempt.error("streamproxy: redirect without host blocked");
            };
            if allowed_host(next) {
                attempt.follow()
            } else {
                attempt.error("streamproxy: redirect to disallowed host blocked")
            }
        }))
    }

    pub fn sign(&self, target: &str) -> String {
        self.signer.sign(target)
    }

    pub fn sign_until(&self, target: &str, exp: SystemTime) -> String {
        self.signer.sign_until(target, exp)
    }

    pub async fn serve(&self, token: Option<&str>, headers: &HeaderMap) -> Response {
        let target = match token.and_then(|token| self.signer.verify(token).ok()) {
            Some(target) => target,
            None => return plain_json_error(StatusCode::FORBIDDEN, "invalid_token"),
        };
        let Ok(url) = reqwest::Url::parse(&target) else {
            return plain_json_error(StatusCode::FORBIDDEN, "forbidden_target");
        };
        let scheme_allowed = matches!(url.scheme(), "http" | "https");
        let host_allowed = url.host_str().is_some_and(allowed_host);
        if !scheme_allowed || !host_allowed {
            return plain_json_error(StatusCode::FORBIDDEN, "forbidden_target");
        }

        let mut request = self.client.get(url);
        if let Some(range) = headers
            .get(header::RANGE)
            .and_then(|value| value.to_str().ok())
        {
            request = request.header(header::RANGE, range);
        }
        let upstream = match request.send().await {
            Ok(response) => response,
            Err(_) => return plain_json_error(StatusCode::BAD_GATEWAY, "upstream_error"),
        };

        let status =
            StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let mut builder = Response::builder().status(status);
        for key in [
            header::CONTENT_TYPE,
            header::CONTENT_LENGTH,
            header::CONTENT_RANGE,
            header::ACCEPT_RANGES,
            header::LAST_MODIFIED,
        ] {
            if let Some(value) = upstream.headers().get(&key) {
                builder = builder.header(key, value);
            }
        }
        let stream = upstream.bytes_stream().map_err(std::io::Error::other);
        builder
            .body(Body::from_stream(stream))
            .unwrap_or_else(|_| plain_json_error(StatusCode::BAD_GATEWAY, "upstream_error"))
    }
}

pub fn allowed_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "googlevideo.com"
        || host.ends_with(".googlevideo.com")
        || host == "youtube.com"
        || host.ends_with(".youtube.com")
}

fn plain_json_error(status: StatusCode, code: &str) -> Response {
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

fn go_json_string(value: &str) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "\"\"".to_string())
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&left, &right) in a.iter().zip(b.iter()) {
        diff |= left ^ right;
    }
    diff == 0
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; HMAC_BLOCK_SIZE];
    if key.len() > HMAC_BLOCK_SIZE {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut outer = [0x5c; HMAC_BLOCK_SIZE];
    let mut inner = [0x36; HMAC_BLOCK_SIZE];
    for i in 0..HMAC_BLOCK_SIZE {
        outer[i] ^= key_block[i];
        inner[i] ^= key_block[i];
    }

    let mut inner_hasher = Sha256::new();
    inner_hasher.update(inner);
    inner_hasher.update(data);
    let inner_digest = inner_hasher.finalize();

    let mut outer_hasher = Sha256::new();
    outer_hasher.update(outer);
    outer_hasher.update(inner_digest);
    outer_hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn allowed_host_matches_go_contract() {
        let cases = [
            ("googlevideo.com", true),
            ("r1---sn-abc.googlevideo.com", true),
            ("www.youtube.com", true),
            ("youtube.com", true),
            ("evil.com", false),
            ("googlevideo.com.evil.com", false),
            ("127.0.0.1", false),
            ("GoogleVideo.COM", true),
            ("R1---SN-abc.GoogleVideo.com", true),
            ("", false),
        ];
        for (host, want) in cases {
            assert_eq!(allowed_host(host), want, "host {host}");
        }
    }

    #[test]
    fn signer_round_trips_and_rejects_tampering() {
        let signer = ProxySigner::new(b"test-key-0123456789".to_vec());
        let target = "https://r1---sn-abc.googlevideo.com/videoplayback?expire=123";
        let token = signer.sign(target);
        assert_eq!(signer.verify(&token).unwrap(), target);

        let mut bad = token.clone().into_bytes();
        let last = bad.last_mut().unwrap();
        *last = if *last == b'A' { b'B' } else { b'A' };
        let bad = String::from_utf8(bad).unwrap();
        assert!(signer.verify(&bad).is_err());
        assert!(signer.verify("").is_err());
        assert!(signer.verify("nodot").is_err());
    }

    #[test]
    fn signer_payload_matches_go_json_contract() {
        let signer = ProxySigner::new(b"key".to_vec());
        let target = "https://r1.googlevideo.com/videoplayback?expire=123&itag=251<>&";
        let token = signer.sign_until(target, UNIX_EPOCH + Duration::from_secs(2_000_000_001));
        let (body, _) = token.split_once('.').unwrap();
        let raw = URL_SAFE_NO_PAD.decode(body).unwrap();
        assert_eq!(
            String::from_utf8(raw).unwrap(),
            r#"{"u":"https://r1.googlevideo.com/videoplayback?expire=123\u0026itag=251\u003c\u003e\u0026","e":2000000001}"#
        );
        assert_eq!(signer.verify(&token).unwrap(), target);
    }

    #[test]
    fn signer_rejects_wrong_key() {
        let token =
            ProxySigner::new(b"key-a".to_vec()).sign("https://x.googlevideo.com/videoplayback");
        assert!(ProxySigner::new(b"key-b".to_vec()).verify(&token).is_err());
    }

    #[test]
    fn signer_rejects_expired_token() {
        let signer = ProxySigner::new(b"key".to_vec());
        let token = signer.sign_until("https://x.googlevideo.com/a", UNIX_EPOCH);
        assert!(signer.verify(&token).is_err());
    }

    #[tokio::test]
    async fn proxy_rejects_invalid_token() {
        let proxy = StreamProxy::new(ProxySigner::new(b"k".to_vec()));
        let response = proxy.serve(Some("garbage"), &HeaderMap::new()).await;
        assert_plain_json_error(response, StatusCode::FORBIDDEN, "invalid_token").await;
    }

    #[tokio::test]
    async fn proxy_rejects_bad_host() {
        let signer = ProxySigner::new(b"k".to_vec());
        let token = signer.sign("https://evil.example.com/videoplayback");
        let proxy = StreamProxy::new(signer);
        let response = proxy.serve(Some(&token), &HeaderMap::new()).await;
        assert_plain_json_error(response, StatusCode::FORBIDDEN, "forbidden_target").await;
    }

    #[tokio::test]
    async fn proxy_forwards_range_and_headers_to_allowed_host() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let n = socket.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase();
            assert!(req.contains("range: bytes=2-5"));
            let response = concat!(
                "HTTP/1.1 206 Partial Content\r\n",
                "Content-Type: audio/mp4\r\n",
                "Content-Length: 4\r\n",
                "Content-Range: bytes 2-5/20\r\n",
                "Accept-Ranges: bytes\r\n",
                "\r\n",
                "2345"
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let signer = ProxySigner::new(b"k".to_vec());
        let token = signer.sign("http://r1.googlevideo.com/videoplayback");
        let proxy = StreamProxy {
            signer,
            client: reqwest::Client::builder()
                .resolve("r1.googlevideo.com", addr)
                .build()
                .unwrap(),
        };
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, "bytes=2-5".parse().unwrap());
        let response = proxy.serve(Some(&token), &headers).await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 2-5/20"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"2345");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn proxy_blocks_redirect_to_disallowed_host_like_go_client() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let _ = socket.read(&mut buf).await.unwrap();
            let response = concat!(
                "HTTP/1.1 302 Found\r\n",
                "Location: http://127.0.0.1/latest/meta-data\r\n",
                "Content-Length: 0\r\n",
                "\r\n"
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let signer = ProxySigner::new(b"k".to_vec());
        let token = signer.sign("http://r1.googlevideo.com/videoplayback");
        let proxy = StreamProxy {
            signer,
            client: StreamProxy::client_builder()
                .resolve("r1.googlevideo.com", addr)
                .build()
                .unwrap(),
        };

        let response = proxy.serve(Some(&token), &HeaderMap::new()).await;
        assert_plain_json_error(response, StatusCode::BAD_GATEWAY, "upstream_error").await;
        server.await.unwrap();
    }

    async fn assert_plain_json_error(response: Response, status: StatusCode, code: &str) {
        assert_eq!(response.status(), status);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], format!("{{\"error\":\"{code}\"}}\n").as_bytes());
    }
}
