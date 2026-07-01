use crate::*;

pub(crate) async fn admin_session_from_headers(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(AdminSession, Option<String>), Response> {
    let token = cookie_value(headers, ADMIN_COOKIE_NAME)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| legacy_json_error(StatusCode::UNAUTHORIZED, "missing_admin_session"))?;
    let Some(store) = &state.store else {
        return Err(legacy_json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
        ));
    };
    let session = store
        .verify_admin_session(&token)
        .await
        .map_err(admin_auth_error_response)?;
    Ok((session, cookie_value(headers, ADMIN_CSRF_COOKIE_NAME)))
}

pub(crate) async fn admin_html_session_from_headers(
    state: &AppState,
    headers: &HeaderMap,
    method: &Method,
) -> Result<(AdminSession, Option<String>), Response> {
    let Some(token) = cookie_value(headers, ADMIN_COOKIE_NAME).filter(|token| !token.is_empty())
    else {
        return Err(redirect_found_for_method("/admin/login", method));
    };
    let Some(store) = &state.store else {
        return Err(redirect_found_for_method("/admin/login", method));
    };
    match store.verify_admin_session(&token).await {
        Ok(session) => Ok((session, cookie_value(headers, ADMIN_CSRF_COOKIE_NAME))),
        Err(_) => Err(redirect_found_for_method("/admin/login", method)),
    }
}

pub(crate) async fn admin_form_session(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
    body: &Bytes,
) -> Result<(AdminSession, ParsedForm), Response> {
    let (session, _) = admin_html_session_from_headers(state, headers, &Method::POST).await?;
    let form = parse_request_form(&Method::POST, headers, query, body);
    if form.invalid {
        return Err(admin_html_error(StatusCode::BAD_REQUEST, "Invalid form"));
    }
    if verify_admin_csrf(&session, &admin_form_csrf_token(headers, &form)) {
        Ok((session, form))
    } else {
        Err(admin_html_error(
            StatusCode::FORBIDDEN,
            "Invalid CSRF token",
        ))
    }
}

pub(crate) async fn admin_action_session(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
    body: &Bytes,
) -> ResponseResult<AdminSession> {
    let (session, _) = admin_html_session_from_headers(state, headers, &Method::POST).await?;
    if let Some(token) = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .filter(|token| !token.is_empty())
    {
        return verify_admin_csrf_response(session, token);
    }
    let form = parse_request_form(&Method::POST, headers, query, body);
    if form.invalid {
        return Err(Box::new(admin_html_error(
            StatusCode::BAD_REQUEST,
            "Invalid form",
        )));
    }
    verify_admin_csrf_response(session, form_value(&form, "csrf_token").trim())
}

pub(crate) fn verify_admin_csrf_response(
    session: AdminSession,
    token: &str,
) -> ResponseResult<AdminSession> {
    if verify_admin_csrf(&session, token) {
        Ok(session)
    } else {
        Err(Box::new(admin_html_error(
            StatusCode::FORBIDDEN,
            "Invalid CSRF token",
        )))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AdminCsrfCheck {
    pub(crate) form_body_consumed: bool,
}

pub(crate) type ResponseResult<T> = Result<T, Box<Response>>;

impl AdminCsrfCheck {
    pub(crate) fn body_after_middleware(self, body: Bytes) -> Bytes {
        if self.form_body_consumed {
            Bytes::new()
        } else {
            body
        }
    }
}

pub(crate) fn require_admin_csrf(
    session: &AdminSession,
    headers: &HeaderMap,
    query: Option<&str>,
    body: Option<&Bytes>,
) -> ResponseResult<AdminCsrfCheck> {
    let csrf = admin_api_csrf_token(headers, query, body);
    if verify_admin_csrf(session, &csrf.token) {
        Ok(AdminCsrfCheck {
            form_body_consumed: csrf.form_body_consumed,
        })
    } else {
        Err(Box::new(legacy_json_error(
            StatusCode::FORBIDDEN,
            "invalid_csrf",
        )))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AdminApiCsrfToken {
    pub(crate) token: String,
    pub(crate) form_body_consumed: bool,
}

pub(crate) fn admin_api_csrf_token(
    headers: &HeaderMap,
    query: Option<&str>,
    body: Option<&Bytes>,
) -> AdminApiCsrfToken {
    if let Some(token) = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .filter(|token| !token.is_empty())
    {
        return AdminApiCsrfToken {
            token: token.to_string(),
            form_body_consumed: false,
        };
    }

    if is_form_urlencoded(headers)
        && let Some(form) = body.map(|body| parse_form(body))
    {
        return AdminApiCsrfToken {
            token: form_value_opt(&form, "csrf_token")
                .or_else(|| query_param(query.unwrap_or_default(), "csrf_token"))
                .unwrap_or_default(),
            form_body_consumed: true,
        };
    }

    AdminApiCsrfToken {
        token: query_param(query.unwrap_or_default(), "csrf_token").unwrap_or_default(),
        form_body_consumed: false,
    }
}
pub(crate) async fn authorize(
    headers: &HeaderMap,
    uri: &Uri,
    state: &AppState,
) -> Result<AuthenticatedDevice, Response> {
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty())
        .map(Cow::Borrowed)
        .or_else(|| query_token(uri.query()).map(Cow::Owned));

    let Some(token) = token else {
        return Err(legacy_http_error(StatusCode::UNAUTHORIZED, "missing_token"));
    };

    match state.auth_mode {
        #[cfg(test)]
        AuthMode::AllowAllForContractTests => Ok(AuthenticatedDevice {
            user_id: uuid::Uuid::nil(),
            device_id: uuid::Uuid::nil(),
        }),
        AuthMode::Database => {
            let Some(store) = &state.store else {
                return Err(legacy_http_error(StatusCode::UNAUTHORIZED, "invalid_token"));
            };
            store
                .validate_device_token(&token)
                .await
                .map_err(auth_error_response)
        }
        AuthMode::RejectAllTokens => {
            Err(legacy_http_error(StatusCode::UNAUTHORIZED, "invalid_token"))
        }
    }
}

pub(crate) fn authorized_device_id(id: &str, auth: &AuthenticatedDevice) -> ResponseResult<Uuid> {
    let path_id = Uuid::parse_str(id)
        .map_err(|_| Box::new(legacy_json_error(StatusCode::BAD_REQUEST, "invalid_id")))?;
    if path_id != auth.device_id {
        return Err(Box::new(legacy_json_error(
            StatusCode::FORBIDDEN,
            "forbidden",
        )));
    }
    Ok(path_id)
}

pub(crate) fn parse_playlist_id(id: &str) -> ResponseResult<Uuid> {
    Uuid::parse_str(id)
        .map_err(|_| Box::new(legacy_json_error(StatusCode::BAD_REQUEST, "invalid_id")))
}

pub(crate) fn parse_uuid_v7(raw: &str) -> Option<Uuid> {
    let key = Uuid::parse_str(raw).ok()?;
    (key.get_version_num() == 7).then_some(key)
}

pub(crate) fn idempotency_key_from_headers(headers: &HeaderMap) -> Uuid {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_uuid_v7)
        .unwrap_or_else(Uuid::now_v7)
}

pub(crate) fn required_idempotency_key(headers: &HeaderMap) -> ResponseResult<Uuid> {
    let Some(raw) = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
    else {
        return Err(Box::new(legacy_json_error(
            StatusCode::BAD_REQUEST,
            "invalid_idempotency_key",
        )));
    };
    parse_uuid_v7(raw).ok_or_else(|| {
        Box::new(legacy_json_error(
            StatusCode::BAD_REQUEST,
            "invalid_idempotency_key",
        ))
    })
}

pub(crate) async fn run_idempotent<Fut>(
    state: &AppState,
    headers: &HeaderMap,
    uri: &Uri,
    method: &str,
    auth: &AuthenticatedDevice,
    future: Fut,
) -> Response
where
    Fut: Future<Output = Response>,
{
    let key = match required_idempotency_key(headers) {
        Ok(key) => key,
        Err(response) => return *response,
    };
    let Some(store) = &state.store else {
        return future.await;
    };
    let route = format!("{method} {}", legacy_url_path(uri.path()));
    if let Ok(Some(record)) = store.find_idempotency_log(key).await {
        if record.route != route
            || record
                .expires_at
                .is_some_and(|expires_at| expires_at <= Utc::now())
        {
            return legacy_json_error(StatusCode::CONFLICT, "conflict");
        }
        return idempotent_replay_response(record);
    }

    let response = future.await;
    if !response.status().is_success() {
        return response;
    }

    record_idempotent_response(
        store,
        IdempotencyLogIdentity {
            key,
            user_id: Some(auth.user_id),
            device_id: Some(auth.device_id),
            route: &route,
        },
        response,
    )
    .await
}

pub(crate) struct IdempotencyLogIdentity<'a> {
    pub(crate) key: Uuid,
    pub(crate) user_id: Option<Uuid>,
    pub(crate) device_id: Option<Uuid>,
    pub(crate) route: &'a str,
}

pub(crate) async fn record_idempotent_response(
    store: &PostgresStore,
    identity: IdempotencyLogIdentity<'_>,
    response: Response,
) -> Response {
    if !response.status().is_success() {
        return response;
    }

    let (parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => return legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    let replay_body = legacy_wire_body_for_hash(&parts.headers, &bytes);
    let hash = hex_lower_bytes(&Sha256::digest(&replay_body));
    let response_status = parts.status.as_u16();
    let response_content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let _ = store
        .insert_idempotency_log(IdempotencyLogInsert {
            key: identity.key,
            user_id: identity.user_id,
            device_id: identity.device_id,
            route: identity.route,
            response_hash: &hash,
            response_status,
            response_body: &replay_body,
            response_content_type,
        })
        .await;
    Response::from_parts(parts, Body::from(bytes))
}

pub(crate) fn idempotent_replay_response(record: IdempotencyLogRecord) -> Response {
    if let (Some(status), Some(body)) = (record.response_status, record.response_body) {
        let status = u16::try_from(status)
            .ok()
            .and_then(|status| StatusCode::from_u16(status).ok())
            .unwrap_or(StatusCode::OK);
        let mut builder = Response::builder()
            .status(status)
            .header("Idempotent-Replay", "true");
        if let Some(content_type) = record.response_content_type {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }
        return builder
            .body(Body::from(body))
            .unwrap_or_else(|_| legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"));
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotent-Replay", "true")
        .body(Body::from(r#"{"idempotent_replay":true}"#))
        .unwrap_or_else(|_| legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal"))
}

pub(crate) fn legacy_wire_body_for_hash(headers: &HeaderMap, bytes: &[u8]) -> Vec<u8> {
    if bytes.is_empty() || !is_legacy_json_encoded_response(headers) {
        return bytes.to_vec();
    }
    let mut body = escape_legacy_json_html_bytes(bytes);
    if !body.ends_with(b"\n") {
        body.push(b'\n');
    }
    body
}

pub(crate) fn legacy_url_path(path: &str) -> String {
    percent_decode_path(path).unwrap_or_else(|| path.to_string())
}
pub(crate) fn auth_error_response(err: AuthStoreError) -> Response {
    match err {
        AuthStoreError::SetupDisabled => legacy_json_error(StatusCode::FORBIDDEN, "setup_disabled"),
        AuthStoreError::InvalidSetupToken => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_setup_token")
        }
        AuthStoreError::WeakPassword => legacy_json_error(StatusCode::BAD_REQUEST, "weak_password"),
        AuthStoreError::SetupRequired => legacy_json_error(StatusCode::FORBIDDEN, "setup_required"),
        AuthStoreError::InvalidPassword => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_password")
        }
        AuthStoreError::MissingAdminSession => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "missing_admin_session")
        }
        AuthStoreError::InvalidAdminSession => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_admin_session")
        }
        AuthStoreError::PairingRequired => {
            legacy_json_error(StatusCode::FORBIDDEN, "pairing_required")
        }
        AuthStoreError::InvalidPairingCode => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_pairing_code")
        }
        AuthStoreError::InvalidToken => {
            legacy_http_error(StatusCode::UNAUTHORIZED, "invalid_token")
        }
        AuthStoreError::DeviceRevoked => {
            legacy_http_error(StatusCode::UNAUTHORIZED, "device_revoked")
        }
        AuthStoreError::Backend(_) => {
            legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
        }
    }
}

pub(crate) fn admin_auth_error_response(err: AuthStoreError) -> Response {
    match err {
        AuthStoreError::SetupRequired => legacy_json_error(StatusCode::FORBIDDEN, "setup_required"),
        AuthStoreError::InvalidPassword => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_password")
        }
        AuthStoreError::MissingAdminSession => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "missing_admin_session")
        }
        AuthStoreError::InvalidAdminSession => {
            legacy_json_error(StatusCode::UNAUTHORIZED, "invalid_admin_session")
        }
        AuthStoreError::Backend(_) => {
            legacy_json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
        }
        other => auth_error_response(other),
    }
}
