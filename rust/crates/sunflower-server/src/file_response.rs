use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StreamFileError {
    NotFound,
    InvalidRange { len: u64 },
    Internal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HashFileError {
    NotFound,
    Internal,
}

pub(crate) fn hash_file(path: &str) -> Result<(String, u64), HashFileError> {
    let mut file = fs::File::open(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            HashFileError::NotFound
        } else {
            HashFileError::Internal
        }
    })?;
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 32 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| HashFileError::Internal)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes += read as u64;
    }
    let digest = hasher.finalize();
    Ok((hex_lower_bytes(&digest), bytes))
}

pub(crate) fn hex_lower_bytes(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(TABLE[(byte >> 4) as usize] as char);
        out.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    out
}

pub(crate) fn serve_static_bytes(
    bytes: &'static [u8],
    content_type: &'static str,
    range_header: Option<&str>,
) -> Response {
    let len = bytes.len() as u64;
    let range = match range_header {
        Some(raw) => match parse_single_range(raw, len) {
            Some(range) => Some(range),
            None => return range_not_satisfiable(len),
        },
        None => None,
    };
    let (status, start, end) = match range {
        Some((start, end)) => (StatusCode::PARTIAL_CONTENT, start, end),
        None => (StatusCode::OK, 0, len.saturating_sub(1)),
    };
    let body_len = if len == 0 { 0 } else { end - start + 1 };
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, body_len.to_string());
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    let body = if body_len == 0 {
        Body::empty()
    } else {
        Body::from(bytes[start as usize..=end as usize].to_vec())
    };
    builder
        .body(body)
        .unwrap_or_else(|_| legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"))
}

pub(crate) async fn serve_local_file(
    path: &str,
    range_header: Option<&str>,
) -> Result<Response, StreamFileError> {
    let metadata = tokio::fs::metadata(path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            StreamFileError::NotFound
        } else {
            StreamFileError::Internal
        }
    })?;
    if !metadata.is_file() {
        return Err(StreamFileError::NotFound);
    }
    let len = metadata.len();
    let range = match range_header {
        Some(raw) => {
            Some(parse_single_range(raw, len).ok_or(StreamFileError::InvalidRange { len })?)
        }
        None => None,
    };
    let (status, start, end) = match range {
        Some((start, end)) => (StatusCode::PARTIAL_CONTENT, start, end),
        None => (StatusCode::OK, 0, len.saturating_sub(1)),
    };
    let body_len = if len == 0 { 0 } else { end - start + 1 };
    let mut file = tokio::fs::File::open(path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            StreamFileError::NotFound
        } else {
            StreamFileError::Internal
        }
    })?;
    if start > 0 {
        file.seek(SeekFrom::Start(start))
            .await
            .map_err(|_| StreamFileError::Internal)?;
    }

    let mut builder = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, body_len.to_string());
    if let Ok(modified) = metadata.modified() {
        builder = builder.header(header::LAST_MODIFIED, http_date(modified));
    }
    if let Some(content_type) = content_type_for_path(path) {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    builder
        .body(stream_file_body(file, body_len))
        .map_err(|_| StreamFileError::Internal)
}

pub(crate) fn stream_file_body(file: tokio::fs::File, body_len: u64) -> Body {
    if body_len == 0 {
        return Body::empty();
    }

    let chunks = stream::try_unfold((file, body_len), |(mut file, remaining)| async move {
        if remaining == 0 {
            return Ok(None);
        }
        let chunk_len = remaining.min(FILE_STREAM_CHUNK_SIZE as u64) as usize;
        let mut chunk = vec![0u8; chunk_len];
        let read = file.read(&mut chunk).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "file ended before advertised content length",
            ));
        }
        chunk.truncate(read);
        let remaining = remaining.saturating_sub(read as u64);
        Ok(Some((Bytes::from(chunk), (file, remaining))))
    });

    Body::from_stream(chunks)
}

pub(crate) fn parse_single_range(raw: &str, len: u64) -> Option<(u64, u64)> {
    let spec = raw.strip_prefix("bytes=")?;
    if spec.contains(',') || len == 0 {
        return None;
    }
    let (start_raw, end_raw) = spec.split_once('-')?;
    if start_raw.is_empty() {
        let suffix_len = end_raw.parse::<u64>().ok()?;
        if suffix_len == 0 {
            return None;
        }
        let start = len.saturating_sub(suffix_len);
        return Some((start, len - 1));
    }
    let start = start_raw.parse::<u64>().ok()?;
    if start >= len {
        return None;
    }
    let end = if end_raw.is_empty() {
        len - 1
    } else {
        end_raw.parse::<u64>().ok()?.min(len - 1)
    };
    (start <= end).then_some((start, end))
}

pub(crate) fn content_type_for_path(path: &str) -> Option<&'static str> {
    match FsPath::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("mp3") => Some("audio/mpeg"),
        Some("flac") => Some("audio/flac"),
        Some("m4a") => Some("audio/mp4"),
        Some("ogg" | "opus") => Some("audio/ogg"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        _ => None,
    }
}

pub(crate) fn http_date(time: SystemTime) -> String {
    let time = DateTime::<Utc>::from(time);
    time.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

pub(crate) fn range_not_satisfiable(len: u64) -> Response {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{len}"))
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header("x-content-type-options", "nosniff")
        .body(Body::from("invalid range: failed to overlap\n"))
        .unwrap_or_else(|_| legacy_json_error(StatusCode::RANGE_NOT_SATISFIABLE, "invalid_range"))
}
