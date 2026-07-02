use std::collections::HashMap;

use async_trait::async_trait;
use base64::Engine;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use crypto_secretbox::{
    XSalsa20Poly1305,
    aead::{Aead, KeyInit},
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use sunflower_core::{
    AdminAuditEventResponse, AdminCookieStatusResponse, AdminDeviceResponse,
    AdminLibraryCountsResponse, AdminPairingCodeResponse, AlbumListItemResponse,
    ArtistListItemResponse, DownloadListItemResponse, EventEntryRequest, HomeResponse,
    ImpressionEntryRequest, LikedSong, LocalStatsSnapshot, MediaId, MediaRepository,
    OwnerSetupRequest, PlaylistItemResponse, PlaylistResponse, QueueItem, QueueSession,
    RecommendationCandidate, RecommendationEvent, RecommendationEventRepository,
    RecommendationSnapshot, RecommendationSnapshotRepository, RecommendationSource,
    RegisterDeviceRequest, RegisterDeviceResponse, SearchAlbumResponse, SearchArtistResponse,
    SearchResponse, SearchSongResponse, Song, SongListItemResponse, StorageError, StorageResult,
    TrackStats, device_capabilities, legacy_rfc3339_nano,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedDevice {
    pub user_id: Uuid,
    pub device_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub csrf_secret_hash: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminLoginResult {
    pub token: String,
    pub csrf: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminMe {
    pub user_id: Uuid,
    pub display_name: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyLogRecord {
    pub route: String,
    pub response_hash: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub response_status: Option<i32>,
    pub response_body: Option<Vec<u8>>,
    pub response_content_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyLogInsert<'a> {
    pub key: Uuid,
    pub user_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub route: &'a str,
    pub response_hash: &'a str,
    pub response_status: u16,
    pub response_body: &'a [u8],
    pub response_content_type: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SongFileLookup {
    Missing,
    NotLocal,
    Path(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannedLocalSong {
    pub media_id: String,
    pub title: String,
    pub artist: String,
    pub artist_media_id: String,
    pub album: String,
    pub album_media_id: String,
    pub year: Option<i32>,
    pub local_path: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CachedHome {
    pub home: HomeResponse,
    pub fresh: bool,
}

struct AuditEventInsert<'a> {
    user_id: Option<Uuid>,
    actor_type: &'a str,
    actor_id: &'a str,
    event: &'a str,
    target_type: &'a str,
    target_id: &'a str,
    metadata: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopPlayedArtist {
    pub artist_id: String,
    pub artist_name: String,
    pub play_count: i64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthStoreError {
    #[error("setup_disabled")]
    SetupDisabled,
    #[error("invalid_setup_token")]
    InvalidSetupToken,
    #[error("weak_password")]
    WeakPassword,
    #[error("setup_required")]
    SetupRequired,
    #[error("invalid_password")]
    InvalidPassword,
    #[error("missing_admin_session")]
    MissingAdminSession,
    #[error("invalid_admin_session")]
    InvalidAdminSession,
    #[error("pairing_required")]
    PairingRequired,
    #[error("invalid_pairing_code")]
    InvalidPairingCode,
    #[error("invalid_token")]
    InvalidToken,
    #[error("device_revoked")]
    DeviceRevoked,
    #[error("storage backend error: {0}")]
    Backend(String),
}

const ARGON_TIME: u32 = 1;
const ARGON_MEMORY: u32 = 64 * 1024;
const ARGON_THREADS: u32 = 4;
const ARGON_KEY_LEN: usize = 32;
const ADMIN_SESSION_TTL_DAYS: i64 = 14;
const DEFAULT_PAIRING_TTL_SECONDS: i64 = 10 * 60;
const MAX_PAIRING_TTL_SECONDS: i64 = 60 * 60;
const MIGRATION_ADVISORY_LOCK_KEY: i64 = 0x7375_6e66_6c6f_7765;

fn normalize_pairing_ttl_seconds(ttl_seconds: i64) -> i64 {
    if ttl_seconds <= 0 {
        DEFAULT_PAIRING_TTL_SECONDS
    } else {
        ttl_seconds.min(MAX_PAIRING_TTL_SECONDS)
    }
}

const EMBEDDED_MIGRATIONS: &[(i64, &str, &str)] = &[
    (
        1,
        "0001_init.sql",
        include_str!("../migrations/0001_init.sql"),
    ),
    (
        2,
        "0002_events.sql",
        include_str!("../migrations/0002_events.sql"),
    ),
    (
        3,
        "0003_queue.sql",
        include_str!("../migrations/0003_queue.sql"),
    ),
    (
        4,
        "0004_sync.sql",
        include_str!("../migrations/0004_sync.sql"),
    ),
    (
        5,
        "0005_song_local_path.sql",
        include_str!("../migrations/0005_song_local_path.sql"),
    ),
    (
        6,
        "0006_cookie_health.sql",
        include_str!("../migrations/0006_cookie_health.sql"),
    ),
    (
        7,
        "0007_secure_enrollment.sql",
        include_str!("../migrations/0007_secure_enrollment.sql"),
    ),
    (
        8,
        "0008_event_idempotency.sql",
        include_str!("../migrations/0008_event_idempotency.sql"),
    ),
    (
        9,
        "0009_idempotency_response.sql",
        include_str!("../migrations/0009_idempotency_response.sql"),
    ),
];

impl PostgresStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(database_url: &str) -> StorageResult<Self> {
        let pool = PgPool::connect(database_url).await.map_err(map_backend)?;
        Ok(Self::new(pool))
    }

    pub async fn migrate(&self) -> StorageResult<()> {
        let mut lock_conn = self.pool.acquire().await.map_err(map_backend)?;
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(MIGRATION_ADVISORY_LOCK_KEY)
            .execute(&mut *lock_conn)
            .await
            .map_err(map_backend)?;

        let migrate_result = async {
            self.migrate_legacy_go_schema().await?;
            self.migrate_rust_local_tables().await
        }
        .await;

        let unlock_result = sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(MIGRATION_ADVISORY_LOCK_KEY)
            .execute(&mut *lock_conn)
            .await
            .map_err(map_backend);

        match (migrate_result, unlock_result) {
            (Err(err), _) => Err(err),
            (Ok(()), Err(err)) => Err(err),
            (Ok(()), Ok(_)) => Ok(()),
        }
    }

    pub async fn migrate_minimal(&self) -> StorageResult<()> {
        self.migrate().await
    }

    async fn migrate_legacy_go_schema(&self) -> StorageResult<()> {
        sqlx::raw_sql(
            r#"
            CREATE EXTENSION IF NOT EXISTS pgcrypto;

            CREATE TABLE IF NOT EXISTS goose_db_version (
                id integer PRIMARY KEY GENERATED BY DEFAULT AS IDENTITY,
                version_id bigint NOT NULL,
                is_applied boolean NOT NULL,
                tstamp timestamp NULL DEFAULT now()
            );

            INSERT INTO goose_db_version (version_id, is_applied)
            SELECT 0, true
            WHERE NOT EXISTS (
                SELECT 1 FROM goose_db_version WHERE version_id = 0
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;

        for (version, name, raw_sql) in EMBEDDED_MIGRATIONS {
            let already_applied = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM goose_db_version
                    WHERE version_id = $1 AND is_applied
                )
                "#,
            )
            .bind(version)
            .fetch_one(&self.pool)
            .await
            .map_err(map_backend)?;
            if already_applied {
                continue;
            }

            let up_sql = legacy_goose_up_sql(raw_sql).map_err(|err| {
                StorageError::Backend(format!("parse embedded migration {name}: {err}"))
            })?;
            let mut tx = self.pool.begin().await.map_err(map_backend)?;
            sqlx::raw_sql(&up_sql)
                .execute(&mut *tx)
                .await
                .map_err(map_backend)?;
            sqlx::query(
                r#"
                INSERT INTO goose_db_version (version_id, is_applied)
                VALUES ($1, true)
                "#,
            )
            .bind(version)
            .execute(&mut *tx)
            .await
            .map_err(map_backend)?;
            tx.commit().await.map_err(map_backend)?;
        }

        Ok(())
    }

    async fn migrate_rust_local_tables(&self) -> StorageResult<()> {
        sqlx::raw_sql(
            r#"
            ALTER TABLE IF EXISTS idempotency_log
                ADD COLUMN IF NOT EXISTS response_status integer,
                ADD COLUMN IF NOT EXISTS response_body bytea,
                ADD COLUMN IF NOT EXISTS response_content_type text;

            CREATE TABLE IF NOT EXISTS rust_songs (
                media_id text PRIMARY KEY,
                source_type text NOT NULL,
                title text NOT NULL,
                available boolean NOT NULL,
                payload jsonb NOT NULL,
                updated_at timestamptz NOT NULL DEFAULT now()
            );

            CREATE TABLE IF NOT EXISTS rust_recommendation_events (
                event_id uuid PRIMARY KEY,
                media_id text NOT NULL,
                client_clock bigint NOT NULL,
                occurred_at timestamptz NOT NULL,
                kind text NOT NULL,
                payload jsonb NOT NULL,
                synced_at timestamptz
            );

            CREATE INDEX IF NOT EXISTS idx_rust_recommendation_events_unsynced
                ON rust_recommendation_events (synced_at, client_clock);

            CREATE TABLE IF NOT EXISTS rust_ingested_events (
                user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
                event_id text NOT NULL,
                device_id uuid REFERENCES devices (id) ON DELETE SET NULL,
                created_at timestamptz NOT NULL DEFAULT now(),
                PRIMARY KEY (user_id, event_id)
            );

            CREATE TABLE IF NOT EXISTS rust_recommendation_snapshots (
                snapshot_id uuid PRIMARY KEY,
                model_version text NOT NULL,
                generated_at timestamptz NOT NULL,
                expires_at timestamptz NOT NULL,
                payload jsonb NOT NULL
            );

            CREATE TABLE IF NOT EXISTS rust_like_tombstones (
                user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
                song_media_id text NOT NULL,
                unliked_at timestamptz NOT NULL,
                idempotency_key uuid UNIQUE,
                PRIMARY KEY (user_id, song_media_id)
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(())
    }

    pub async fn owner_configured(&self) -> Result<bool, AuthStoreError> {
        let configured = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM users WHERE admin_password_hash IS NOT NULL
            )
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_auth_backend)?;
        Ok(configured)
    }

    pub async fn setup_owner(
        &self,
        setup_token: &str,
        req: &OwnerSetupRequest,
    ) -> Result<(), AuthStoreError> {
        if self.owner_configured().await? {
            return Err(AuthStoreError::SetupDisabled);
        }

        if !constant_time_eq(req.setup_token.trim(), setup_token.trim()) {
            let _ = self
                .write_audit_event(AuditEventInsert {
                    user_id: None,
                    actor_type: "setup",
                    actor_id: "",
                    event: "owner_setup_failed",
                    target_type: "",
                    target_id: "",
                    metadata: serde_json::json!({"reason": "invalid_setup_token"}),
                })
                .await;
            return Err(AuthStoreError::InvalidSetupToken);
        }

        validate_owner_password(&req.password, setup_token)?;
        let password_hash = hash_owner_password(&req.password)?;
        let mut display_name = req.display_name.trim().to_string();
        if display_name.is_empty() {
            display_name = "Owner".to_string();
        }
        let user_id = self.ensure_owner_user(&display_name).await?;

        sqlx::query(
            r#"
            UPDATE users
            SET display_name = $2,
                admin_password_hash = $3,
                admin_password_updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .bind(&display_name)
        .bind(password_hash)
        .execute(&self.pool)
        .await
        .map_err(map_auth_backend)?;

        let user_id_text = user_id.to_string();
        self.write_audit_event(AuditEventInsert {
            user_id: Some(user_id),
            actor_type: "setup",
            actor_id: "",
            event: "owner_setup_completed",
            target_type: "user",
            target_id: &user_id_text,
            metadata: serde_json::json!({}),
        })
        .await
    }

    pub async fn login_admin(&self, password: &str) -> Result<AdminLoginResult, AuthStoreError> {
        let (user_id, password_hash) = self.first_owner_password_hash().await?;
        match verify_owner_password(&password_hash, password) {
            Ok(true) => {}
            Ok(false) => {
                let _ = self
                    .write_audit_event(AuditEventInsert {
                        user_id: Some(user_id),
                        actor_type: "admin",
                        actor_id: "",
                        event: "admin_login_failed",
                        target_type: "",
                        target_id: "",
                        metadata: serde_json::json!({"reason": "bad_password"}),
                    })
                    .await;
                return Err(AuthStoreError::InvalidPassword);
            }
            Err(err) => return Err(err),
        }

        let token = random_secret("sf_adm_")?;
        let csrf = random_secret("sf_csrf_")?;
        let expires_at = Utc::now() + Duration::days(ADMIN_SESSION_TTL_DAYS);
        let session_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO admin_sessions (user_id, token_hash, csrf_secret_hash, expires_at, last_seen_at)
            VALUES ($1, $2, $3, $4, now())
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(hash_verifier(&token))
        .bind(hash_verifier(&csrf))
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(map_auth_backend)?;

        let session_id_text = session_id.to_string();
        let _ = self
            .write_audit_event(AuditEventInsert {
                user_id: Some(user_id),
                actor_type: "admin_session",
                actor_id: &session_id_text,
                event: "admin_login_succeeded",
                target_type: "admin_session",
                target_id: &session_id_text,
                metadata: serde_json::json!({}),
            })
            .await;

        Ok(AdminLoginResult {
            token,
            csrf,
            expires_at,
        })
    }

    pub async fn verify_admin_session(&self, token: &str) -> Result<AdminSession, AuthStoreError> {
        if token.is_empty() {
            return Err(AuthStoreError::MissingAdminSession);
        }
        let row = sqlx::query(
            r#"
            SELECT id, user_id, csrf_secret_hash, expires_at
            FROM admin_sessions
            WHERE token_hash = $1
              AND revoked_at IS NULL
              AND expires_at > now()
            "#,
        )
        .bind(hash_verifier(token))
        .fetch_optional(&self.pool)
        .await
        .map_err(map_auth_backend)?;

        let Some(row) = row else {
            return Err(AuthStoreError::InvalidAdminSession);
        };
        let session = AdminSession {
            id: row.try_get("id").map_err(map_auth_backend)?,
            user_id: row.try_get("user_id").map_err(map_auth_backend)?,
            csrf_secret_hash: row.try_get("csrf_secret_hash").map_err(map_auth_backend)?,
            expires_at: row.try_get("expires_at").map_err(map_auth_backend)?,
        };
        let _ = sqlx::query("UPDATE admin_sessions SET last_seen_at = now() WHERE id = $1")
            .bind(session.id)
            .execute(&self.pool)
            .await;
        Ok(session)
    }

    pub async fn revoke_admin_session(&self, token: &str) -> Result<(), AuthStoreError> {
        if token.is_empty() {
            return Ok(());
        }
        sqlx::query(
            r#"
            UPDATE admin_sessions
            SET revoked_at = now()
            WHERE token_hash = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(hash_verifier(token))
        .execute(&self.pool)
        .await
        .map_err(map_auth_backend)?;
        Ok(())
    }

    pub async fn admin_me(&self, session: &AdminSession) -> Result<AdminMe, AuthStoreError> {
        let display_name = sqlx::query_scalar::<_, String>(
            r#"
            SELECT display_name FROM users WHERE id = $1
            "#,
        )
        .bind(session.user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_auth_backend)?;
        Ok(AdminMe {
            user_id: session.user_id,
            display_name,
            expires_at: session.expires_at,
        })
    }

    pub async fn create_pairing_code(
        &self,
        session: &AdminSession,
        label: &str,
        ttl_seconds: i64,
        server_url: &str,
    ) -> Result<AdminPairingCodeResponse, AuthStoreError> {
        let ttl_seconds = normalize_pairing_ttl_seconds(ttl_seconds);
        let code = generate_pairing_code()?;
        let expires_at = Utc::now() + Duration::seconds(ttl_seconds);
        let label = label.trim().to_string();
        let pairing_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO pairing_codes
                (user_id, code_hash, label, expires_at, created_by_session_id)
            VALUES
                ($1, $2, nullif($3,''), $4, $5)
            RETURNING id
            "#,
        )
        .bind(session.user_id)
        .bind(hash_token(&code)?)
        .bind(&label)
        .bind(expires_at)
        .bind(session.id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_auth_backend)?;

        let session_id_text = session.id.to_string();
        let pairing_id_text = pairing_id.to_string();
        let _ = self
            .write_audit_event(AuditEventInsert {
                user_id: Some(session.user_id),
                actor_type: "admin_session",
                actor_id: &session_id_text,
                event: "pairing_code_created",
                target_type: "pairing_code",
                target_id: &pairing_id_text,
                metadata: serde_json::json!({
                    "label": label,
                    "ttl_seconds": ttl_seconds,
                }),
            })
            .await;

        Ok(AdminPairingCodeResponse {
            pairing_code: code.clone(),
            pairing_url: pairing_url(server_url, &code),
            expires_at: rfc3339_nano(expires_at),
        })
    }

    pub async fn admin_library_counts(&self) -> StorageResult<AdminLibraryCountsResponse> {
        let songs = count_table(&self.pool, "songs").await?;
        let albums = count_table(&self.pool, "albums").await?;
        let artists = count_table(&self.pool, "artists").await?;
        let playlists = count_table(&self.pool, "playlists").await?;
        Ok(AdminLibraryCountsResponse {
            songs,
            albums,
            artists,
            playlists,
        })
    }

    pub async fn admin_cookie_status(&self) -> StorageResult<AdminCookieStatusResponse> {
        let row = sqlx::query(
            r#"
            SELECT status, checked_at, detail
            FROM cookie_health
            WHERE provider = 'youtube'
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_backend)?;
        let Some(row) = row else {
            return Ok(AdminCookieStatusResponse {
                status: "unknown".to_string(),
                checked_at: None,
                detail: None,
            });
        };
        let checked_at: Option<DateTime<Utc>> = row.try_get("checked_at").map_err(map_backend)?;
        Ok(AdminCookieStatusResponse {
            status: row.try_get("status").map_err(map_backend)?,
            checked_at: checked_at.map(rfc3339_seconds),
            detail: row.try_get("detail").map_err(map_backend)?,
        })
    }

    pub async fn list_admin_devices(&self) -> StorageResult<Vec<AdminDeviceResponse>> {
        let rows = sqlx::query(
            r#"
            SELECT id, coalesce(name,'') AS name, coalesce(platform,'') AS platform,
                   coalesce(token_label,'') AS token_label,
                   created_at, last_seen_at, revoked_at, coalesce(revoked_reason,'') AS revoked_reason
            FROM devices
            ORDER BY coalesce(last_seen_at, created_at) DESC
            LIMIT 100
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                let id: Uuid = row.try_get("id").map_err(map_backend)?;
                let created_at: DateTime<Utc> = row.try_get("created_at").map_err(map_backend)?;
                let last_seen_at: Option<DateTime<Utc>> =
                    row.try_get("last_seen_at").map_err(map_backend)?;
                let revoked_at: Option<DateTime<Utc>> =
                    row.try_get("revoked_at").map_err(map_backend)?;
                Ok(AdminDeviceResponse {
                    id: id.to_string(),
                    name: row.try_get("name").map_err(map_backend)?,
                    platform: row.try_get("platform").map_err(map_backend)?,
                    token_label: row.try_get("token_label").map_err(map_backend)?,
                    created_at: rfc3339_nano(created_at),
                    last_seen_at: last_seen_at.map(rfc3339_nano),
                    revoked_at: revoked_at.map(rfc3339_nano),
                    revoked_reason: row.try_get("revoked_reason").map_err(map_backend)?,
                })
            })
            .collect()
    }

    pub async fn revoke_device(
        &self,
        session: &AdminSession,
        device_id: Uuid,
        reason: &str,
    ) -> Result<(), AuthStoreError> {
        let reason = reason.trim().to_string();
        sqlx::query(
            r#"
            UPDATE devices
            SET revoked_at = coalesce(revoked_at, now()),
                revoked_reason = nullif($2,'')
            WHERE id = $1
            "#,
        )
        .bind(device_id)
        .bind(&reason)
        .execute(&self.pool)
        .await
        .map_err(map_auth_backend)?;

        let session_id_text = session.id.to_string();
        let device_id_text = device_id.to_string();
        self.write_audit_event(AuditEventInsert {
            user_id: Some(session.user_id),
            actor_type: "admin_session",
            actor_id: &session_id_text,
            event: "device_revoked",
            target_type: "device",
            target_id: &device_id_text,
            metadata: serde_json::json!({"reason": reason}),
        })
        .await
    }

    pub async fn record_library_scan_started(
        &self,
        session: &AdminSession,
        job_id: &str,
        root_count: usize,
    ) -> Result<(), AuthStoreError> {
        let session_id_text = session.id.to_string();
        self.write_audit_event(AuditEventInsert {
            user_id: Some(session.user_id),
            actor_type: "admin_session",
            actor_id: &session_id_text,
            event: "library_scan_started",
            target_type: "job",
            target_id: job_id,
            metadata: serde_json::json!({"root_count": root_count}),
        })
        .await
    }

    pub async fn recent_audit_events(
        &self,
        limit: i64,
    ) -> StorageResult<Vec<AdminAuditEventResponse>> {
        let rows = sqlx::query(
            r#"
            SELECT id, actor_type, coalesce(actor_id,'') AS actor_id,
                   event, coalesce(target_type,'') AS target_type,
                   coalesce(target_id,'') AS target_id, metadata, created_at
            FROM audit_events
            ORDER BY created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                let id: Uuid = row.try_get("id").map_err(map_backend)?;
                let metadata: serde_json::Value = row.try_get("metadata").map_err(map_backend)?;
                let created_at: DateTime<Utc> = row.try_get("created_at").map_err(map_backend)?;
                Ok(AdminAuditEventResponse {
                    id: id.to_string(),
                    actor_type: row.try_get("actor_type").map_err(map_backend)?,
                    actor_id: row.try_get("actor_id").map_err(map_backend)?,
                    event: row.try_get("event").map_err(map_backend)?,
                    target_type: row.try_get("target_type").map_err(map_backend)?,
                    target_id: row.try_get("target_id").map_err(map_backend)?,
                    metadata: redact_json(metadata),
                    created_at: rfc3339_nano(created_at),
                })
            })
            .collect()
    }

    pub async fn store_youtube_cookies(
        &self,
        session: &AdminSession,
        key: [u8; 32],
        raw: &[u8],
    ) -> Result<(), AuthStoreError> {
        self.store_youtube_cookies_for_user(session.user_id, key, raw)
            .await?;
        let session_id_text = session.id.to_string();
        self.write_audit_event(AuditEventInsert {
            user_id: Some(session.user_id),
            actor_type: "admin_session",
            actor_id: &session_id_text,
            event: "youtube_cookies_updated",
            target_type: "cookie_store",
            target_id: "youtube",
            metadata: serde_json::json!({"bytes": raw.len()}),
        })
        .await
    }

    pub async fn store_youtube_innertube_token(
        &self,
        session: &AdminSession,
        key: [u8; 32],
        raw: &[u8],
    ) -> Result<(), AuthStoreError> {
        self.store_youtube_innertube_token_for_user(session.user_id, key, raw)
            .await?;
        let session_id_text = session.id.to_string();
        self.write_audit_event(AuditEventInsert {
            user_id: Some(session.user_id),
            actor_type: "admin_session",
            actor_id: &session_id_text,
            event: "youtube_innertube_token_updated",
            target_type: "cookie_store",
            target_id: "youtube_innertube_token",
            metadata: serde_json::json!({"bytes": raw.len()}),
        })
        .await
    }

    pub async fn store_youtube_cookies_for_user(
        &self,
        user_id: Uuid,
        key: [u8; 32],
        raw: &[u8],
    ) -> Result<(), AuthStoreError> {
        self.store_encrypted_secret_for_user(user_id, "youtube", key, raw)
            .await
    }

    pub async fn store_youtube_innertube_token_for_user(
        &self,
        user_id: Uuid,
        key: [u8; 32],
        raw: &[u8],
    ) -> Result<(), AuthStoreError> {
        self.store_encrypted_secret_for_user(user_id, "youtube_innertube_token", key, raw)
            .await
    }

    async fn store_encrypted_secret_for_user(
        &self,
        user_id: Uuid,
        provider: &str,
        key: [u8; 32],
        raw: &[u8],
    ) -> Result<(), AuthStoreError> {
        let mut nonce = [0u8; 24];
        rand::rngs::OsRng
            .try_fill_bytes(&mut nonce)
            .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
        let cipher = XSalsa20Poly1305::new((&key).into());
        let ciphertext = cipher
            .encrypt((&nonce).into(), raw)
            .map_err(|e| AuthStoreError::Backend(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO encrypted_cookies (user_id, provider, ciphertext, nonce, refreshed_at)
            VALUES ($1, $2, $3, $4, now())
            ON CONFLICT (user_id, provider) DO UPDATE
            SET ciphertext = EXCLUDED.ciphertext,
                nonce = EXCLUDED.nonce,
                refreshed_at = now()
            "#,
        )
        .bind(user_id)
        .bind(provider)
        .bind(ciphertext)
        .bind(nonce.to_vec())
        .execute(&self.pool)
        .await
        .map_err(map_auth_backend)?;
        Ok(())
    }

    pub async fn load_first_youtube_cookies(
        &self,
        key: [u8; 32],
    ) -> StorageResult<Option<Vec<u8>>> {
        self.load_first_encrypted_secret("youtube", key).await
    }

    pub async fn load_first_youtube_innertube_token(
        &self,
        key: [u8; 32],
    ) -> StorageResult<Option<Vec<u8>>> {
        self.load_first_encrypted_secret("youtube_innertube_token", key)
            .await
    }

    pub async fn has_youtube_innertube_token(&self) -> StorageResult<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM encrypted_cookies
            WHERE provider = 'youtube_innertube_token'
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(count > 0)
    }

    async fn load_first_encrypted_secret(
        &self,
        provider: &str,
        key: [u8; 32],
    ) -> StorageResult<Option<Vec<u8>>> {
        let Some(row) = sqlx::query(
            r#"
            SELECT ec.ciphertext, ec.nonce
            FROM encrypted_cookies ec
            JOIN users u ON u.id = ec.user_id
            WHERE ec.provider = $1
            ORDER BY u.created_at
            LIMIT 1
            "#,
        )
        .bind(provider)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_backend)?
        else {
            return Ok(None);
        };

        let ciphertext: Vec<u8> = row.try_get("ciphertext").map_err(map_backend)?;
        let nonce: Vec<u8> = row.try_get("nonce").map_err(map_backend)?;
        let cipher = XSalsa20Poly1305::new((&key).into());
        let raw = cipher
            .decrypt(nonce.as_slice().into(), ciphertext.as_slice())
            .map_err(|err| StorageError::Backend(err.to_string()))?;
        Ok(Some(raw))
    }

    pub async fn probe_youtube_cookies(
        &self,
        session: &AdminSession,
    ) -> Result<AdminCookieStatusResponse, AuthStoreError> {
        self.mark_youtube_cookie_probe_requested().await?;

        let session_id_text = session.id.to_string();
        self.write_audit_event(AuditEventInsert {
            user_id: Some(session.user_id),
            actor_type: "admin_session",
            actor_id: &session_id_text,
            event: "youtube_cookies_probe_requested",
            target_type: "cookie_store",
            target_id: "youtube",
            metadata: serde_json::json!({}),
        })
        .await?;

        self.admin_cookie_status().await.map_err(map_auth_backend)
    }

    pub async fn mark_youtube_cookie_probe_requested(&self) -> Result<(), AuthStoreError> {
        sqlx::query(
            r#"
            INSERT INTO cookie_health (provider, status, checked_at, detail)
            VALUES ('youtube', 'unknown', now(), 'manual probe requested')
            ON CONFLICT (provider) DO UPDATE
            SET status = 'unknown',
                checked_at = now(),
                detail = 'manual probe requested'
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(map_auth_backend)?;
        Ok(())
    }

    pub async fn clear_youtube_cookies(
        &self,
        session: &AdminSession,
    ) -> Result<(), AuthStoreError> {
        sqlx::query(
            r#"
            DELETE FROM encrypted_cookies
            WHERE user_id = $1 AND provider = 'youtube'
            "#,
        )
        .bind(session.user_id)
        .execute(&self.pool)
        .await
        .map_err(map_auth_backend)?;
        let _ = sqlx::query("DELETE FROM cookie_health WHERE provider = 'youtube'")
            .execute(&self.pool)
            .await;

        let session_id_text = session.id.to_string();
        self.write_audit_event(AuditEventInsert {
            user_id: Some(session.user_id),
            actor_type: "admin_session",
            actor_id: &session_id_text,
            event: "youtube_cookies_cleared",
            target_type: "cookie_store",
            target_id: "youtube",
            metadata: serde_json::json!({}),
        })
        .await
    }

    pub async fn validate_device_token(
        &self,
        token: &str,
    ) -> Result<AuthenticatedDevice, AuthStoreError> {
        let token_hash = hash_token(token)?;
        let row = sqlx::query(
            r#"
            SELECT id, user_id, revoked_at
            FROM devices
            WHERE token_hash = $1
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_auth_backend)?;

        let Some(row) = row else {
            return Err(AuthStoreError::InvalidToken);
        };

        let device_id: Uuid = row.try_get("id").map_err(map_auth_backend)?;
        let user_id: Uuid = row.try_get("user_id").map_err(map_auth_backend)?;
        let revoked_at: Option<DateTime<Utc>> =
            row.try_get("revoked_at").map_err(map_auth_backend)?;
        if revoked_at.is_some() {
            return Err(AuthStoreError::DeviceRevoked);
        }

        sqlx::query("UPDATE devices SET last_seen_at = now() WHERE id = $1")
            .bind(device_id)
            .execute(&self.pool)
            .await
            .map_err(map_auth_backend)?;

        Ok(AuthenticatedDevice { user_id, device_id })
    }

    pub async fn register_device(
        &self,
        req: &RegisterDeviceRequest,
    ) -> Result<RegisterDeviceResponse, AuthStoreError> {
        let code = normalize_pairing_code(&req.pairing_code);
        if code.is_empty() {
            return Err(AuthStoreError::PairingRequired);
        }

        let mut tx = self.pool.begin().await.map_err(map_auth_backend)?;
        let response = register_device_tx(&mut tx, req, &code).await;
        match response {
            Ok(response) => {
                tx.commit().await.map_err(map_auth_backend)?;
                Ok(response)
            }
            Err(err) => {
                let _ = tx.rollback().await;
                Err(err)
            }
        }
    }

    pub async fn register_device_open(
        &self,
        req: &RegisterDeviceRequest,
    ) -> Result<RegisterDeviceResponse, AuthStoreError> {
        let user_id = ensure_owner_user(&self.pool, "owner").await?;
        let token = generate_device_token()?;
        let token_hash = hash_token(&token)?;
        let name = req.device_name.trim().to_string();
        let platform = req.platform.trim().to_string();
        let device_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO devices (user_id, name, platform, token_hash, token_label)
            VALUES ($1, nullif($2,''), nullif($3,''), $4, nullif($2,''))
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(&name)
        .bind(&platform)
        .bind(token_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(map_auth_backend)?;

        Ok(RegisterDeviceResponse {
            device_id: device_id.to_string(),
            token,
            server_capabilities: device_capabilities()
                .iter()
                .map(|capability| (*capability).to_string())
                .collect(),
        })
    }

    pub async fn find_idempotency_log(
        &self,
        key: Uuid,
    ) -> StorageResult<Option<IdempotencyLogRecord>> {
        let row = sqlx::query(
            r#"
            SELECT route, response_hash, expires_at,
                   response_status, response_body, response_content_type
            FROM idempotency_log
            WHERE key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_backend)?;
        row.map(|row| {
            Ok(IdempotencyLogRecord {
                route: row.try_get("route").map_err(map_backend)?,
                response_hash: row.try_get("response_hash").map_err(map_backend)?,
                expires_at: row.try_get("expires_at").map_err(map_backend)?,
                response_status: row.try_get("response_status").map_err(map_backend)?,
                response_body: row.try_get("response_body").map_err(map_backend)?,
                response_content_type: row.try_get("response_content_type").map_err(map_backend)?,
            })
        })
        .transpose()
    }

    pub async fn insert_idempotency_log(
        &self,
        insert: IdempotencyLogInsert<'_>,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO idempotency_log
                (key, user_id, device_id, route, response_hash,
                 response_status, response_body, response_content_type, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (key) DO NOTHING
            "#,
        )
        .bind(insert.key)
        .bind(insert.user_id)
        .bind(insert.device_id)
        .bind(insert.route)
        .bind(insert.response_hash)
        .bind(i32::from(insert.response_status))
        .bind(insert.response_body)
        .bind(insert.response_content_type)
        .bind(Utc::now() + Duration::hours(24))
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(())
    }

    pub async fn gc_expired_idempotency_log(&self) -> StorageResult<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM idempotency_log
            WHERE expires_at IS NOT NULL AND expires_at < $1
            "#,
        )
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(result.rows_affected())
    }

    async fn ensure_owner_user(&self, display_name: &str) -> Result<Uuid, AuthStoreError> {
        let existing = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id FROM users ORDER BY created_at LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_auth_backend)?;

        if let Some(user_id) = existing {
            return Ok(user_id);
        }

        sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO users (display_name)
            VALUES ($1)
            RETURNING id
            "#,
        )
        .bind(display_name.trim())
        .fetch_one(&self.pool)
        .await
        .map_err(map_auth_backend)
    }

    async fn first_owner_password_hash(&self) -> Result<(Uuid, String), AuthStoreError> {
        let row = sqlx::query(
            r#"
            SELECT id, admin_password_hash
            FROM users
            ORDER BY created_at
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_auth_backend)?;

        let Some(row) = row else {
            return Err(AuthStoreError::SetupRequired);
        };
        let user_id: Uuid = row.try_get("id").map_err(map_auth_backend)?;
        let password_hash: Option<String> = row
            .try_get("admin_password_hash")
            .map_err(map_auth_backend)?;
        let Some(password_hash) = password_hash.filter(|hash| !hash.is_empty()) else {
            return Err(AuthStoreError::SetupRequired);
        };
        Ok((user_id, password_hash))
    }

    async fn write_audit_event(&self, event: AuditEventInsert<'_>) -> Result<(), AuthStoreError> {
        sqlx::query(
            r#"
            INSERT INTO audit_events
                (user_id, actor_type, actor_id, event, target_type, target_id, metadata)
            VALUES ($1, $2, nullif($3,''), $4, nullif($5,''), nullif($6,''), $7)
            "#,
        )
        .bind(event.user_id)
        .bind(event.actor_type)
        .bind(event.actor_id)
        .bind(event.event)
        .bind(event.target_type)
        .bind(event.target_id)
        .bind(event.metadata)
        .execute(&self.pool)
        .await
        .map_err(map_auth_backend)?;
        Ok(())
    }

    pub async fn list_liked_songs(&self, user_id: Uuid) -> StorageResult<Vec<LikedSong>> {
        let rows = sqlx::query(
            r#"
            SELECT s.media_id, s.title, s.duration_ms
            FROM likes l
            JOIN songs s ON s.media_id = l.song_media_id
            WHERE l.user_id = $1 AND s.available = true
            ORDER BY l.liked_at DESC
            LIMIT 200
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                let media_id: String = row.try_get("media_id").map_err(map_backend)?;
                let title: String = row.try_get("title").map_err(map_backend)?;
                let duration_ms: Option<i32> = row.try_get("duration_ms").map_err(map_backend)?;
                Ok(LikedSong {
                    media_id: MediaId::new(media_id),
                    title,
                    duration_ms: duration_ms.unwrap_or_default(),
                })
            })
            .collect()
    }

    pub async fn liked_yt_seed_media_ids(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> StorageResult<Vec<String>> {
        let rows = sqlx::query(
            r#"
            SELECT song_media_id
            FROM likes
            WHERE user_id = $1
            ORDER BY liked_at DESC
            LIMIT $2
            "#,
        )
        .bind(user_id)
        .bind(limit.saturating_mul(4))
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        let limit = usize::try_from(limit.max(0)).unwrap_or_default();
        let mut out = Vec::with_capacity(limit);
        for row in rows {
            let media_id: String = row.try_get("song_media_id").map_err(map_backend)?;
            if media_id.starts_with("yt:") {
                out.push(media_id);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    pub async fn create_queue(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        seed_kind: &str,
        seed_id: &str,
        title: &str,
        items: &[QueueItem],
    ) -> StorageResult<QueueSession> {
        let mut tx = self.pool.begin().await.map_err(map_backend)?;
        let result = create_queue_tx(
            &mut tx, user_id, device_id, seed_kind, seed_id, title, items,
        )
        .await;
        match result {
            Ok(session) => {
                tx.commit().await.map_err(map_backend)?;
                Ok(session)
            }
            Err(err) => {
                let _ = tx.rollback().await;
                Err(err)
            }
        }
    }

    pub async fn get_queue(
        &self,
        queue_id: Uuid,
        user_id: Uuid,
    ) -> StorageResult<Option<QueueSession>> {
        let row = sqlx::query(
            r#"
            SELECT id, seed_kind, seed_id, version, title
            FROM queue_sessions
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(queue_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_backend)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let id: Uuid = row.try_get("id").map_err(map_backend)?;
        let seed_kind: Option<String> = row.try_get("seed_kind").map_err(map_backend)?;
        let seed_id: Option<String> = row.try_get("seed_id").map_err(map_backend)?;
        let title: Option<String> = row.try_get("title").map_err(map_backend)?;
        let version: i64 = row.try_get("version").map_err(map_backend)?;
        let items = self.list_queue_items(id).await?;

        Ok(Some(QueueSession {
            id,
            seed_kind: seed_kind.unwrap_or_default(),
            seed_id: seed_id.unwrap_or_default(),
            title: title.unwrap_or_default(),
            version,
            items,
        }))
    }

    async fn list_queue_items(&self, queue_id: Uuid) -> StorageResult<Vec<QueueItem>> {
        let rows = sqlx::query(
            r#"
            SELECT media_id, source_data
            FROM queue_items
            WHERE queue_id = $1
            ORDER BY position
            "#,
        )
        .bind(queue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                let media_id: String = row.try_get("media_id").map_err(map_backend)?;
                let value: serde_json::Value = row.try_get("source_data").map_err(map_backend)?;
                match serde_json::from_value::<QueueItem>(value) {
                    Ok(item) if !item.media_id.0.is_empty() => Ok(item),
                    _ => Ok(QueueItem {
                        media_id: MediaId::new(media_id),
                        title: String::new(),
                        artists: vec![],
                        duration_ms: 0,
                    }),
                }
            })
            .collect()
    }

    pub async fn song_stream_path(&self, media_id: &str) -> StorageResult<Option<String>> {
        match self.song_file_lookup(media_id).await? {
            SongFileLookup::Path(path) => Ok(Some(path)),
            SongFileLookup::Missing | SongFileLookup::NotLocal => Ok(None),
        }
    }

    pub async fn song_file_lookup(&self, media_id: &str) -> StorageResult<SongFileLookup> {
        let row = sqlx::query(
            r#"
            SELECT local_path
            FROM songs
            WHERE media_id = $1
            "#,
        )
        .bind(media_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_backend)?;

        let Some(row) = row else {
            return Ok(SongFileLookup::Missing);
        };
        let local_path: Option<String> = row.try_get("local_path").map_err(map_backend)?;
        Ok(match local_path.filter(|path| !path.is_empty()) {
            Some(path) => SongFileLookup::Path(path),
            None => SongFileLookup::NotLocal,
        })
    }

    pub async fn list_library_songs(
        &self,
        limit: i64,
        offset: i64,
    ) -> StorageResult<Vec<SongListItemResponse>> {
        let rows = sqlx::query(
            r#"
            SELECT
                s.media_id,
                s.source_type,
                s.title,
                s.duration_ms,
                s.album_id,
                COALESCE(ar.name, '') AS artist_name,
                COALESCE(al.title, '') AS album_title,
                (s.album_id IS NOT NULL) AS has_art
            FROM songs s
            LEFT JOIN artists ar ON ar.media_id = s.primary_artist_id
            LEFT JOIN albums al ON al.media_id = s.album_id
            WHERE s.available = true
            ORDER BY s.title
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                Ok(SongListItemResponse {
                    media_id: row.try_get("media_id").map_err(map_backend)?,
                    source_type: row.try_get("source_type").map_err(map_backend)?,
                    title: row.try_get("title").map_err(map_backend)?,
                    duration_ms: row.try_get("duration_ms").map_err(map_backend)?,
                    album_id: row.try_get("album_id").map_err(map_backend)?,
                    artist_name: row.try_get("artist_name").map_err(map_backend)?,
                    album_title: row.try_get("album_title").map_err(map_backend)?,
                    has_art: row.try_get("has_art").map_err(map_backend)?,
                })
            })
            .collect()
    }

    pub async fn list_library_albums(
        &self,
        limit: i64,
        offset: i64,
    ) -> StorageResult<Vec<AlbumListItemResponse>> {
        let rows = sqlx::query(
            r#"
            SELECT media_id, source_type, title, primary_artist_id, year,
                   available, raw_metadata, created_at
            FROM albums
            WHERE available = true
            ORDER BY title
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                Ok(AlbumListItemResponse {
                    media_id: row.try_get("media_id").map_err(map_backend)?,
                    source_type: row.try_get("source_type").map_err(map_backend)?,
                    title: row.try_get("title").map_err(map_backend)?,
                    primary_artist_id: row.try_get("primary_artist_id").map_err(map_backend)?,
                    year: row.try_get("year").map_err(map_backend)?,
                    available: row.try_get("available").map_err(map_backend)?,
                    raw_metadata: row.try_get("raw_metadata").map_err(map_backend)?,
                    created_at: row.try_get("created_at").map_err(map_backend)?,
                })
            })
            .collect()
    }

    pub async fn list_library_artists(
        &self,
        limit: i64,
        offset: i64,
    ) -> StorageResult<Vec<ArtistListItemResponse>> {
        let rows = sqlx::query(
            r#"
            SELECT media_id, source_type, name, available, raw_metadata, created_at
            FROM artists
            WHERE available = true
            ORDER BY name
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                Ok(ArtistListItemResponse {
                    media_id: row.try_get("media_id").map_err(map_backend)?,
                    source_type: row.try_get("source_type").map_err(map_backend)?,
                    name: row.try_get("name").map_err(map_backend)?,
                    available: row.try_get("available").map_err(map_backend)?,
                    raw_metadata: row.try_get("raw_metadata").map_err(map_backend)?,
                    created_at: row.try_get("created_at").map_err(map_backend)?,
                })
            })
            .collect()
    }

    pub async fn upsert_scanned_local_song(&self, song: &ScannedLocalSong) -> StorageResult<()> {
        let mut tx = self.pool.begin().await.map_err(map_backend)?;
        let artist_id = if song.artist.is_empty() {
            None
        } else {
            sqlx::query(
                r#"
                INSERT INTO artists (media_id, source_type, name, raw_metadata)
                VALUES ($1, 'local', $2, '{}'::jsonb)
                ON CONFLICT (media_id) DO UPDATE SET
                    name = excluded.name,
                    raw_metadata = excluded.raw_metadata
                "#,
            )
            .bind(&song.artist_media_id)
            .bind(&song.artist)
            .execute(&mut *tx)
            .await
            .map_err(map_backend)?;
            Some(song.artist_media_id.as_str())
        };

        let album_id = if song.album.is_empty() {
            None
        } else {
            sqlx::query(
                r#"
                INSERT INTO albums
                    (media_id, source_type, title, primary_artist_id, year, raw_metadata)
                VALUES ($1, 'local', $2, $3, $4, '{}'::jsonb)
                ON CONFLICT (media_id) DO UPDATE SET
                    title = excluded.title,
                    primary_artist_id = excluded.primary_artist_id,
                    year = excluded.year,
                    raw_metadata = excluded.raw_metadata
                "#,
            )
            .bind(&song.album_media_id)
            .bind(&song.album)
            .bind(artist_id)
            .bind(song.year)
            .execute(&mut *tx)
            .await
            .map_err(map_backend)?;
            Some(song.album_media_id.as_str())
        };

        sqlx::query(
            r#"
            INSERT INTO songs
                (media_id, source_type, title, duration_ms, album_id,
                 primary_artist_id, raw_metadata, local_path)
            VALUES ($1, 'local', $2, NULL, $3, $4, '{}'::jsonb, $5)
            ON CONFLICT (media_id) DO UPDATE SET
                title = excluded.title,
                duration_ms = excluded.duration_ms,
                album_id = excluded.album_id,
                primary_artist_id = excluded.primary_artist_id,
                raw_metadata = excluded.raw_metadata,
                local_path = excluded.local_path
            "#,
        )
        .bind(&song.media_id)
        .bind(&song.title)
        .bind(album_id)
        .bind(artist_id)
        .bind(&song.local_path)
        .execute(&mut *tx)
        .await
        .map_err(map_backend)?;

        if let Some(artist_id) = artist_id {
            sqlx::query(
                r#"
                INSERT INTO song_artists (song_media_id, artist_media_id, position)
                VALUES ($1, $2, 0)
                ON CONFLICT (song_media_id, artist_media_id) DO UPDATE SET
                    position = excluded.position
                "#,
            )
            .bind(&song.media_id)
            .bind(artist_id)
            .execute(&mut *tx)
            .await
            .map_err(map_backend)?;
        }

        tx.commit().await.map_err(map_backend)?;
        Ok(())
    }

    pub async fn upsert_download(
        &self,
        device_id: Uuid,
        media_id: &str,
        local_path: &str,
        bytes: i64,
    ) -> StorageResult<()> {
        let bytes = (bytes > 0).then_some(bytes);
        sqlx::query(
            r#"
            INSERT INTO downloaded_tracks
                (device_id, song_media_id, local_path, bytes, completed_at, last_verified_at)
            VALUES ($1, $2, $3, $4, now(), now())
            ON CONFLICT (device_id, song_media_id) DO UPDATE SET
                local_path = excluded.local_path,
                bytes = excluded.bytes,
                completed_at = excluded.completed_at,
                last_verified_at = excluded.last_verified_at
            "#,
        )
        .bind(device_id)
        .bind(media_id)
        .bind(local_path)
        .bind(bytes)
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(())
    }

    pub async fn list_downloads(
        &self,
        device_id: Uuid,
    ) -> StorageResult<Vec<DownloadListItemResponse>> {
        let rows = sqlx::query(
            r#"
            SELECT song_media_id, bytes
            FROM downloaded_tracks
            WHERE device_id = $1
            ORDER BY completed_at DESC NULLS LAST
            "#,
        )
        .bind(device_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                let bytes: Option<i64> = row.try_get("bytes").map_err(map_backend)?;
                Ok(DownloadListItemResponse {
                    media_id: row.try_get("song_media_id").map_err(map_backend)?,
                    bytes: bytes.unwrap_or_default(),
                })
            })
            .collect()
    }

    pub async fn delete_download(&self, device_id: Uuid, media_id: &str) -> StorageResult<()> {
        sqlx::query(
            r#"
            DELETE FROM downloaded_tracks
            WHERE device_id = $1 AND song_media_id = $2
            "#,
        )
        .bind(device_id)
        .bind(media_id)
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(())
    }

    pub async fn local_home_inputs(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        limit: i64,
        hide_explicit: bool,
        hide_video: bool,
    ) -> StorageResult<(Vec<RecommendationCandidate>, LocalStatsSnapshot)> {
        let rows = sqlx::query(
            r#"
            WITH play_stats AS (
                SELECT
                    song_media_id,
                    COUNT(*) FILTER (WHERE kind = 'play')::bigint AS play_count,
                    COUNT(*) FILTER (WHERE kind = 'skip')::bigint AS skip_count,
                    COUNT(*) FILTER (WHERE kind = 'play')::bigint AS completion_count,
                    MAX(occurred_at) AS last_played_at
                FROM play_events
                WHERE user_id = $1
                  AND kind IN ('play', 'skip')
                GROUP BY song_media_id
            ),
            impressions AS (
                SELECT media_id, COUNT(*)::bigint AS impression_count
                FROM recommendation_impressions
                WHERE user_id = $1
                GROUP BY media_id
            )
            SELECT
                s.media_id,
                s.title,
                COALESCE(ar.name, '') AS artist_name,
                s.album_id,
                s.duration_ms,
                s.source_type,
                COALESCE(ps.play_count, 0)::bigint AS play_count,
                COALESCE(ps.skip_count, 0)::bigint AS skip_count,
                COALESCE(ps.completion_count, 0)::bigint AS completion_count,
                COALESCE(im.impression_count, 0)::bigint AS impression_count,
                ps.last_played_at,
                (l.song_media_id IS NOT NULL) AS liked,
                (dt.song_media_id IS NOT NULL AND dt.local_path IS NOT NULL AND dt.local_path <> '') AS downloaded,
                (
                    s.source_type = 'local'
                    OR (dt.song_media_id IS NOT NULL AND dt.local_path IS NOT NULL AND dt.local_path <> '')
                ) AS local_available
            FROM songs s
            LEFT JOIN artists ar ON ar.media_id = s.primary_artist_id
            LEFT JOIN play_stats ps ON ps.song_media_id = s.media_id
            LEFT JOIN impressions im ON im.media_id = s.media_id
            LEFT JOIN likes l ON l.user_id = $1 AND l.song_media_id = s.media_id
            LEFT JOIN downloaded_tracks dt ON dt.device_id = $2 AND dt.song_media_id = s.media_id
            WHERE s.available = true
              AND ($4 = false OR s.explicit = false)
              AND ($5 = false OR s.video_only = false)
              AND (
                ps.song_media_id IS NOT NULL
                OR l.song_media_id IS NOT NULL
                OR s.source_type = 'local'
                OR dt.song_media_id IS NOT NULL
              )
            ORDER BY
                (l.song_media_id IS NOT NULL) DESC,
                COALESCE(ps.play_count, 0) DESC,
                ps.last_played_at DESC NULLS LAST,
                s.title
            LIMIT $3
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(limit)
        .bind(hide_explicit)
        .bind(hide_video)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        let mut candidates = Vec::with_capacity(rows.len());
        let mut stats = Vec::with_capacity(rows.len());
        for row in rows {
            let media_id_raw: String = row.try_get("media_id").map_err(map_backend)?;
            let media_id = MediaId::new(media_id_raw);
            let artist_name: String = row.try_get("artist_name").map_err(map_backend)?;
            let album_id: Option<String> = row.try_get("album_id").map_err(map_backend)?;
            let duration_ms: Option<i32> = row.try_get("duration_ms").map_err(map_backend)?;
            let liked: bool = row.try_get("liked").map_err(map_backend)?;
            let downloaded: bool = row.try_get("downloaded").map_err(map_backend)?;
            let local_available: bool = row.try_get("local_available").map_err(map_backend)?;

            candidates.push(RecommendationCandidate {
                media_id: media_id.clone(),
                title: row.try_get("title").map_err(map_backend)?,
                artists: (!artist_name.is_empty())
                    .then_some(vec![artist_name])
                    .unwrap_or_default(),
                album_id: album_id.map(MediaId::new),
                duration_ms: duration_ms.unwrap_or_default(),
                source: RecommendationSource::Local,
                remote_score: 0.0,
                reason: None,
            });
            stats.push(TrackStats {
                media_id,
                play_count: row.try_get("play_count").map_err(map_backend)?,
                skip_count: row.try_get("skip_count").map_err(map_backend)?,
                completion_count: row.try_get("completion_count").map_err(map_backend)?,
                impression_count: row.try_get("impression_count").map_err(map_backend)?,
                liked,
                downloaded,
                local_available,
                last_played_at: row.try_get("last_played_at").map_err(map_backend)?,
            });
        }

        let recent_rows = sqlx::query(
            r#"
            SELECT song_media_id
            FROM play_events
            WHERE user_id = $1
              AND kind IN ('play', 'skip')
            GROUP BY song_media_id
            ORDER BY MAX(occurred_at) DESC
            LIMIT 10
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;
        let recent = recent_rows
            .into_iter()
            .map(|row| {
                let media_id: String = row.try_get("song_media_id").map_err(map_backend)?;
                Ok(MediaId::new(media_id))
            })
            .collect::<StorageResult<Vec<_>>>()?;

        Ok((
            candidates,
            LocalStatsSnapshot {
                generated_at: Utc::now(),
                tracks: stats,
                recent_media_ids: recent,
                recent_artist_names: vec![],
            },
        ))
    }

    pub async fn cached_home(
        &self,
        user_id: Uuid,
        hide_explicit: bool,
        hide_video: bool,
        hide_shorts: bool,
    ) -> StorageResult<Option<CachedHome>> {
        let row = sqlx::query(
            r#"
            SELECT payload, expires_at
            FROM rec_cache
            WHERE cache_key = $1
            "#,
        )
        .bind(home_cache_key(
            user_id,
            hide_explicit,
            hide_video,
            hide_shorts,
        ))
        .fetch_optional(&self.pool)
        .await
        .map_err(map_backend)?;

        let Some(row) = row else {
            return Ok(None);
        };
        let value: serde_json::Value = row.try_get("payload").map_err(map_backend)?;
        let expires_at: Option<DateTime<Utc>> = row.try_get("expires_at").map_err(map_backend)?;
        let home_value = value.get("home").cloned().unwrap_or(value);
        let mut home: HomeResponse = serde_json::from_value(home_value).map_err(map_backend)?;
        let fresh = expires_at.is_some_and(|expires_at| Utc::now() < expires_at);
        home.stale = !fresh;
        Ok(Some(CachedHome { home, fresh }))
    }

    pub async fn put_home_cache(
        &self,
        user_id: Uuid,
        hide_explicit: bool,
        hide_video: bool,
        hide_shorts: bool,
        home: &HomeResponse,
    ) -> StorageResult<()> {
        let now = Utc::now();
        let payload = serde_json::json!({
            "home": home,
            "generated_at": now,
        });
        sqlx::query(
            r#"
            INSERT INTO rec_cache (cache_key, user_id, payload, generated_at, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (cache_key) DO UPDATE SET
                user_id = excluded.user_id,
                payload = excluded.payload,
                generated_at = excluded.generated_at,
                expires_at = excluded.expires_at
            "#,
        )
        .bind(home_cache_key(
            user_id,
            hide_explicit,
            hide_video,
            hide_shorts,
        ))
        .bind(user_id)
        .bind(payload)
        .bind(now)
        .bind(now + Duration::minutes(30))
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(())
    }

    pub async fn recent_impression_counts(
        &self,
        user_id: Uuid,
    ) -> StorageResult<HashMap<String, i32>> {
        let rows = sqlx::query(
            r#"
            SELECT media_id, COUNT(*)::int AS shows
            FROM recommendation_impressions
            WHERE user_id = $1
              AND shown_at >= now() - interval '24 hours'
              AND media_id IS NOT NULL
              AND media_id <> ''
            GROUP BY media_id
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        let mut counts = HashMap::with_capacity(rows.len());
        for row in rows {
            let media_id: String = row.try_get("media_id").map_err(map_backend)?;
            let shows: i32 = row.try_get("shows").map_err(map_backend)?;
            counts.insert(media_id, shows);
        }
        Ok(counts)
    }

    pub async fn most_played_artists(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> StorageResult<Vec<TopPlayedArtist>> {
        let since = Utc::now() - Duration::days(90);
        let rows = sqlx::query(
            r#"
            SELECT
                s.primary_artist_id AS artist_id,
                COALESCE(ar.name, '') AS artist_name,
                COUNT(*)::bigint AS play_count
            FROM play_events pe
            JOIN songs s ON s.media_id = pe.song_media_id
            LEFT JOIN artists ar ON ar.media_id = s.primary_artist_id
            WHERE pe.user_id = $1
              AND pe.occurred_at > $2
              AND pe.kind = 'play'
              AND s.primary_artist_id IS NOT NULL
            GROUP BY s.primary_artist_id, ar.name
            ORDER BY play_count DESC
            LIMIT $3
            "#,
        )
        .bind(user_id)
        .bind(since)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                Ok(TopPlayedArtist {
                    artist_id: row.try_get("artist_id").map_err(map_backend)?,
                    artist_name: row.try_get("artist_name").map_err(map_backend)?,
                    play_count: row.try_get("play_count").map_err(map_backend)?,
                })
            })
            .collect()
    }

    pub async fn upsert_like(
        &self,
        user_id: Uuid,
        media_id: &str,
        occurred_at: Option<&str>,
        idempotency_key: Uuid,
    ) -> StorageResult<()> {
        let occurred_at = parse_occurred_at_or_now(occurred_at);
        let mut tx = self.pool.begin().await.map_err(map_backend)?;

        let tombstone_at: Option<DateTime<Utc>> = sqlx::query_scalar(
            r#"
            SELECT unliked_at
            FROM rust_like_tombstones
            WHERE user_id = $1 AND song_media_id = $2
            FOR UPDATE
            "#,
        )
        .bind(user_id)
        .bind(media_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_backend)?;
        if tombstone_at.is_some_and(|unliked_at| occurred_at < unliked_at) {
            tx.commit().await.map_err(map_backend)?;
            return Ok(());
        }

        sqlx::query(
            r#"
            DELETE FROM rust_like_tombstones
            WHERE user_id = $1 AND song_media_id = $2 AND unliked_at <= $3
            "#,
        )
        .bind(user_id)
        .bind(media_id)
        .bind(occurred_at)
        .execute(&mut *tx)
        .await
        .map_err(map_backend)?;

        sqlx::query(
            r#"
            INSERT INTO likes (user_id, song_media_id, liked_at, idempotency_key)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, song_media_id) DO UPDATE SET
                liked_at = GREATEST(likes.liked_at, excluded.liked_at)
            "#,
        )
        .bind(user_id)
        .bind(media_id)
        .bind(occurred_at)
        .bind(idempotency_key)
        .execute(&mut *tx)
        .await
        .map_err(map_backend)?;
        tx.commit().await.map_err(map_backend)?;
        Ok(())
    }

    pub async fn delete_like(
        &self,
        user_id: Uuid,
        media_id: &str,
        occurred_at: Option<&str>,
        idempotency_key: Uuid,
    ) -> StorageResult<()> {
        let occurred_at = parse_occurred_at_or_now(occurred_at);
        let mut tx = self.pool.begin().await.map_err(map_backend)?;

        let tombstone_at: Option<DateTime<Utc>> = sqlx::query_scalar(
            r#"
            SELECT unliked_at
            FROM rust_like_tombstones
            WHERE user_id = $1 AND song_media_id = $2
            FOR UPDATE
            "#,
        )
        .bind(user_id)
        .bind(media_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_backend)?;
        if tombstone_at.is_some_and(|unliked_at| occurred_at < unliked_at) {
            tx.commit().await.map_err(map_backend)?;
            return Ok(());
        }

        let liked_at: Option<DateTime<Utc>> = sqlx::query_scalar(
            r#"
            SELECT liked_at
            FROM likes
            WHERE user_id = $1 AND song_media_id = $2
            FOR UPDATE
            "#,
        )
        .bind(user_id)
        .bind(media_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_backend)?;
        if liked_at.is_some_and(|liked_at| occurred_at < liked_at) {
            tx.commit().await.map_err(map_backend)?;
            return Ok(());
        }

        sqlx::query(
            r#"
            DELETE FROM likes
            WHERE user_id = $1 AND song_media_id = $2 AND liked_at <= $3
            "#,
        )
        .bind(user_id)
        .bind(media_id)
        .bind(occurred_at)
        .execute(&mut *tx)
        .await
        .map_err(map_backend)?;

        sqlx::query(
            r#"
            INSERT INTO rust_like_tombstones (user_id, song_media_id, unliked_at, idempotency_key)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, song_media_id) DO UPDATE SET
                unliked_at = GREATEST(rust_like_tombstones.unliked_at, excluded.unliked_at)
            "#,
        )
        .bind(user_id)
        .bind(media_id)
        .bind(occurred_at)
        .bind(idempotency_key)
        .execute(&mut *tx)
        .await
        .map_err(map_backend)?;
        tx.commit().await.map_err(map_backend)?;
        Ok(())
    }

    pub async fn local_search(&self, query: &str, limit: i64) -> StorageResult<SearchResponse> {
        let pattern = format!("%{query}%");
        let song_rows = sqlx::query(
            r#"
            SELECT
                s.media_id,
                s.source_type,
                s.title,
                COALESCE(ar.name, '') AS artist_name,
                s.duration_ms
            FROM songs s
            LEFT JOIN artists ar ON ar.media_id = s.primary_artist_id
            WHERE s.available = true
              AND (s.title ILIKE $1 OR ar.name ILIKE $1)
            ORDER BY s.title
            LIMIT $2
            "#,
        )
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;
        let songs = song_rows
            .into_iter()
            .map(|row| {
                let artist_name: String = row.try_get("artist_name").map_err(map_backend)?;
                let duration_ms: Option<i32> = row.try_get("duration_ms").map_err(map_backend)?;
                Ok(SearchSongResponse {
                    media_id: row.try_get("media_id").map_err(map_backend)?,
                    source: row.try_get("source_type").map_err(map_backend)?,
                    title: row.try_get("title").map_err(map_backend)?,
                    artists: (!artist_name.is_empty())
                        .then_some(vec![artist_name])
                        .unwrap_or_default(),
                    thumbnail_url: None,
                    duration_ms: duration_ms.unwrap_or_default(),
                })
            })
            .collect::<StorageResult<Vec<_>>>()?;

        let album_rows = sqlx::query(
            r#"
            SELECT
                al.media_id,
                al.title,
                COALESCE(ar.name, '') AS artist_name
            FROM albums al
            LEFT JOIN artists ar ON ar.media_id = al.primary_artist_id
            WHERE al.available = true
              AND (al.title ILIKE $1 OR ar.name ILIKE $1)
            ORDER BY al.title
            LIMIT $2
            "#,
        )
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;
        let albums = album_rows
            .into_iter()
            .map(|row| {
                let artist_name: String = row.try_get("artist_name").map_err(map_backend)?;
                Ok(SearchAlbumResponse {
                    browse_id: row.try_get("media_id").map_err(map_backend)?,
                    title: row.try_get("title").map_err(map_backend)?,
                    artists: (!artist_name.is_empty())
                        .then_some(vec![artist_name])
                        .unwrap_or_default(),
                    thumbnail_url: None,
                })
            })
            .collect::<StorageResult<Vec<_>>>()?;

        let artist_rows = sqlx::query(
            r#"
            SELECT media_id, name
            FROM artists
            WHERE available = true AND name ILIKE $1
            ORDER BY name
            LIMIT $2
            "#,
        )
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;
        let artists = artist_rows
            .into_iter()
            .map(|row| {
                Ok(SearchArtistResponse {
                    browse_id: row.try_get("media_id").map_err(map_backend)?,
                    name: row.try_get("name").map_err(map_backend)?,
                    thumbnail_url: None,
                })
            })
            .collect::<StorageResult<Vec<_>>>()?;

        Ok(SearchResponse {
            query: query.to_string(),
            songs,
            albums,
            artists,
            continuation: None,
        })
    }

    pub async fn insert_play_event(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        event: &EventEntryRequest,
    ) -> StorageResult<bool> {
        let occurred_at = event
            .occurred_at
            .as_deref()
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|time| time.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        let queue_id = Uuid::parse_str(&event.queue_id).ok();
        let total_played_ms = (event.total_played_ms > 0).then_some(event.total_played_ms);
        let reason = (!event.reason.is_empty()).then_some(event.reason.as_str());
        let device_id = (device_id != Uuid::nil()).then_some(device_id);

        let mut tx = self.pool.begin().await.map_err(map_backend)?;
        let event_id = event.event_id.as_str();
        if !event_id.is_empty() {
            let claimed = sqlx::query_scalar::<_, i32>(
                r#"
                INSERT INTO rust_ingested_events (user_id, event_id, device_id)
                VALUES ($1, $2, $3)
                ON CONFLICT (user_id, event_id) DO NOTHING
                RETURNING 1
                "#,
            )
            .bind(user_id)
            .bind(event_id)
            .bind(device_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_backend)?
            .is_some();
            if !claimed {
                tx.commit().await.map_err(map_backend)?;
                return Ok(false);
            }
        }

        sqlx::query(
            r#"
            INSERT INTO play_events
                (user_id, device_id, song_media_id, queue_id, kind,
                 occurred_at, total_played_ms, reason)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(&event.media_id)
        .bind(queue_id)
        .bind(&event.kind)
        .bind(occurred_at)
        .bind(total_played_ms)
        .bind(reason)
        .execute(&mut *tx)
        .await
        .map_err(map_backend)?;
        tx.commit().await.map_err(map_backend)?;
        Ok(true)
    }

    pub async fn insert_impression(
        &self,
        user_id: Uuid,
        impression: &ImpressionEntryRequest,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO recommendation_impressions
                (user_id, section_id, source, seed_id, media_id, position)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(user_id)
        .bind(non_empty_str(&impression.section_id))
        .bind(non_empty_str(&impression.source))
        .bind(non_empty_str(&impression.seed_id))
        .bind(non_empty_str(&impression.media_id))
        .bind(Some(impression.position))
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(())
    }

    pub async fn list_playlists(
        &self,
        user_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> StorageResult<Vec<PlaylistResponse>> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, source_type, version
            FROM playlists
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| playlist_from_row(row, vec![]))
            .collect()
    }

    pub async fn create_playlist(
        &self,
        user_id: Uuid,
        title: &str,
    ) -> StorageResult<PlaylistResponse> {
        let row = sqlx::query(
            r#"
            INSERT INTO playlists (user_id, title, source_type, external_id)
            VALUES ($1, $2, 'local', null)
            RETURNING id, title, source_type, version
            "#,
        )
        .bind(user_id)
        .bind(title)
        .fetch_one(&self.pool)
        .await
        .map_err(map_backend)?;
        playlist_from_row(row, vec![])
    }

    pub async fn get_playlist(
        &self,
        user_id: Uuid,
        playlist_id: Uuid,
    ) -> StorageResult<Option<PlaylistResponse>> {
        let row = sqlx::query(
            r#"
            SELECT id, title, source_type, version
            FROM playlists
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(playlist_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_backend)?;

        let Some(row) = row else {
            return Ok(None);
        };
        let items = self.list_playlist_items(playlist_id).await?;
        playlist_from_row(row, items).map(Some)
    }

    pub async fn update_playlist_title(
        &self,
        user_id: Uuid,
        playlist_id: Uuid,
        title: &str,
    ) -> StorageResult<Option<PlaylistResponse>> {
        let row = sqlx::query(
            r#"
            UPDATE playlists
            SET title = $1, version = version + 1
            WHERE id = $2 AND user_id = $3
            RETURNING id, title, source_type, version
            "#,
        )
        .bind(title)
        .bind(playlist_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_backend)?;
        row.map(|row| playlist_from_row(row, vec![])).transpose()
    }

    pub async fn delete_playlist(&self, user_id: Uuid, playlist_id: Uuid) -> StorageResult<()> {
        sqlx::query(
            r#"
            DELETE FROM playlists
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(playlist_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(())
    }

    pub async fn add_playlist_item(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        playlist_id: Uuid,
        media_id: &str,
    ) -> StorageResult<bool> {
        let mut tx = self.pool.begin().await.map_err(map_backend)?;
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM playlists WHERE id = $1 AND user_id = $2
            )
            "#,
        )
        .bind(playlist_id)
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_backend)?;
        if !exists {
            tx.rollback().await.map_err(map_backend)?;
            return Ok(false);
        }

        let position: i32 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(position) + 1, 0)::int
            FROM playlist_items
            WHERE playlist_id = $1
            "#,
        )
        .bind(playlist_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_backend)?;
        sqlx::query(
            r#"
            INSERT INTO playlist_items
                (playlist_id, position, song_media_id, added_by_device_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (playlist_id, position) DO NOTHING
            "#,
        )
        .bind(playlist_id)
        .bind(position)
        .bind(media_id)
        .bind((device_id != Uuid::nil()).then_some(device_id))
        .execute(&mut *tx)
        .await
        .map_err(map_backend)?;
        sqlx::query(
            r#"
            UPDATE playlists
            SET version = version + 1
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(playlist_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(map_backend)?;
        tx.commit().await.map_err(map_backend)?;
        Ok(true)
    }

    pub async fn remove_playlist_item(
        &self,
        user_id: Uuid,
        playlist_id: Uuid,
        media_id: &str,
    ) -> StorageResult<bool> {
        let mut tx = self.pool.begin().await.map_err(map_backend)?;
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM playlists WHERE id = $1 AND user_id = $2
            )
            "#,
        )
        .bind(playlist_id)
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_backend)?;
        if !exists {
            tx.rollback().await.map_err(map_backend)?;
            return Ok(false);
        }

        sqlx::query(
            r#"
            DELETE FROM playlist_items
            WHERE playlist_id = $1 AND song_media_id = $2
            "#,
        )
        .bind(playlist_id)
        .bind(media_id)
        .execute(&mut *tx)
        .await
        .map_err(map_backend)?;
        sqlx::query(
            r#"
            UPDATE playlists
            SET version = version + 1
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(playlist_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(map_backend)?;
        tx.commit().await.map_err(map_backend)?;
        Ok(true)
    }

    async fn list_playlist_items(
        &self,
        playlist_id: Uuid,
    ) -> StorageResult<Vec<PlaylistItemResponse>> {
        let rows = sqlx::query(
            r#"
            SELECT
                pi.position,
                pi.song_media_id,
                COALESCE(s.title, '') AS title,
                COALESCE(ar.name, '') AS artist_name,
                s.album_id,
                s.duration_ms
            FROM playlist_items pi
            JOIN songs s ON s.media_id = pi.song_media_id
            LEFT JOIN artists ar ON ar.media_id = s.primary_artist_id
            WHERE pi.playlist_id = $1
            ORDER BY pi.position
            "#,
        )
        .bind(playlist_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                let album_id: Option<String> = row.try_get("album_id").map_err(map_backend)?;
                let duration_ms: Option<i32> = row.try_get("duration_ms").map_err(map_backend)?;
                Ok(PlaylistItemResponse {
                    position: row.try_get("position").map_err(map_backend)?,
                    media_id: row.try_get("song_media_id").map_err(map_backend)?,
                    title: row.try_get("title").map_err(map_backend)?,
                    artist_name: row.try_get("artist_name").map_err(map_backend)?,
                    album_id: album_id.unwrap_or_default(),
                    duration_ms: duration_ms.unwrap_or_default(),
                })
            })
            .collect()
    }
}

fn playlist_from_row(
    row: sqlx::postgres::PgRow,
    items: Vec<PlaylistItemResponse>,
) -> StorageResult<PlaylistResponse> {
    let id: Uuid = row.try_get("id").map_err(map_backend)?;
    Ok(PlaylistResponse {
        id: id.to_string(),
        title: row.try_get("title").map_err(map_backend)?,
        source_type: row.try_get("source_type").map_err(map_backend)?,
        version: row.try_get("version").map_err(map_backend)?,
        items,
    })
}

fn non_empty_str(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

async fn create_queue_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    device_id: Uuid,
    seed_kind: &str,
    seed_id: &str,
    title: &str,
    items: &[QueueItem],
) -> StorageResult<QueueSession> {
    let items_json = serde_json::to_value(items).map_err(map_backend)?;
    let row = sqlx::query(
        r#"
        INSERT INTO queue_sessions (user_id, device_id, seed_kind, seed_id, title, items)
        VALUES ($1, $2, nullif($3,''), nullif($4,''), nullif($5,''), $6)
        RETURNING id, seed_kind, seed_id, version, title
        "#,
    )
    .bind(user_id)
    .bind((device_id != Uuid::nil()).then_some(device_id))
    .bind(seed_kind)
    .bind(seed_id)
    .bind(title)
    .bind(items_json)
    .fetch_one(&mut **tx)
    .await
    .map_err(map_backend)?;

    let queue_id: Uuid = row.try_get("id").map_err(map_backend)?;
    if !items.is_empty() {
        // Collapse all per-item INSERTs into a single round-trip via UNNEST.
        let mut positions: Vec<i32> = Vec::with_capacity(items.len());
        let mut media_ids: Vec<&str> = Vec::with_capacity(items.len());
        let mut source_data: Vec<String> = Vec::with_capacity(items.len());
        for (position, item) in items.iter().enumerate() {
            positions.push(position as i32);
            media_ids.push(&item.media_id.0);
            source_data.push(serde_json::to_string(item).map_err(map_backend)?);
        }
        sqlx::query(
            r#"
            INSERT INTO queue_items (queue_id, position, media_id, source_data)
            SELECT $1, pos, mid, sd::jsonb
            FROM UNNEST($2::int4[], $3::text[], $4::text[]) AS t(pos, mid, sd)
            ON CONFLICT (queue_id, position) DO UPDATE SET
                media_id = excluded.media_id,
                source_data = excluded.source_data
            "#,
        )
        .bind(queue_id)
        .bind(&positions)
        .bind(&media_ids)
        .bind(&source_data)
        .execute(&mut **tx)
        .await
        .map_err(map_backend)?;
    }

    let seed_kind: Option<String> = row.try_get("seed_kind").map_err(map_backend)?;
    let seed_id: Option<String> = row.try_get("seed_id").map_err(map_backend)?;
    let title: Option<String> = row.try_get("title").map_err(map_backend)?;
    let version: i64 = row.try_get("version").map_err(map_backend)?;

    Ok(QueueSession {
        id: queue_id,
        seed_kind: seed_kind.unwrap_or_default(),
        seed_id: seed_id.unwrap_or_default(),
        title: title.unwrap_or_default(),
        version,
        items: items.to_vec(),
    })
}

async fn register_device_tx(
    tx: &mut Transaction<'_, Postgres>,
    req: &RegisterDeviceRequest,
    code: &str,
) -> Result<RegisterDeviceResponse, AuthStoreError> {
    let code_hash = hash_token(code)?;
    let row = sqlx::query(
        r#"
        SELECT id, user_id, label, expires_at, used_at
        FROM pairing_codes
        WHERE code_hash = $1
        FOR UPDATE
        "#,
    )
    .bind(code_hash)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_auth_backend)?;

    let Some(row) = row else {
        return Err(AuthStoreError::InvalidPairingCode);
    };

    let pairing_id: Uuid = row.try_get("id").map_err(map_auth_backend)?;
    let user_id: Uuid = row.try_get("user_id").map_err(map_auth_backend)?;
    let label: Option<String> = row.try_get("label").map_err(map_auth_backend)?;
    let expires_at: DateTime<Utc> = row.try_get("expires_at").map_err(map_auth_backend)?;
    let used_at: Option<DateTime<Utc>> = row.try_get("used_at").map_err(map_auth_backend)?;
    if used_at.is_some() || Utc::now() > expires_at {
        return Err(AuthStoreError::InvalidPairingCode);
    }

    let token = generate_device_token()?;
    let mut name = req.device_name.trim().to_string();
    if name.is_empty()
        && let Some(label) = label
    {
        name = label;
    }
    let platform = req.platform.trim().to_string();
    let token_hash = hash_token(&token)?;

    let device_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO devices (user_id, name, platform, token_hash, token_label)
        VALUES ($1, nullif($2,''), nullif($3,''), $4, nullif($5,''))
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(&name)
    .bind(&platform)
    .bind(token_hash)
    .bind(&name)
    .fetch_one(&mut **tx)
    .await
    .map_err(map_auth_backend)?;

    sqlx::query(
        r#"
        UPDATE pairing_codes
        SET used_at = now(), used_by_device_id = $1
        WHERE id = $2
        "#,
    )
    .bind(device_id)
    .bind(pairing_id)
    .execute(&mut **tx)
    .await
    .map_err(map_auth_backend)?;

    let metadata = serde_json::json!({
        "platform": platform,
        "client_version": req.client_version,
    });
    sqlx::query(
        r#"
        INSERT INTO audit_events
            (user_id, actor_type, actor_id, event, target_type, target_id, metadata)
        VALUES ($1, 'pairing_code', $2, 'device_paired', 'device', $3, $4)
        "#,
    )
    .bind(user_id)
    .bind(pairing_id.to_string())
    .bind(device_id.to_string())
    .bind(metadata)
    .execute(&mut **tx)
    .await
    .map_err(map_auth_backend)?;

    Ok(RegisterDeviceResponse {
        device_id: device_id.to_string(),
        token,
        server_capabilities: device_capabilities()
            .iter()
            .map(|capability| (*capability).to_string())
            .collect(),
    })
}

async fn ensure_owner_user(pool: &PgPool, display_name: &str) -> Result<Uuid, AuthStoreError> {
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users ORDER BY created_at LIMIT 1")
            .fetch_optional(pool)
            .await
            .map_err(map_auth_backend)?;
    if let Some(user_id) = existing {
        return Ok(user_id);
    }

    sqlx::query_scalar(
        r#"
        INSERT INTO users (display_name)
        VALUES ($1)
        RETURNING id
        "#,
    )
    .bind(display_name.trim())
    .fetch_one(pool)
    .await
    .map_err(map_auth_backend)
}

#[async_trait]
impl MediaRepository for PostgresStore {
    async fn upsert_song(&self, song: &Song) -> StorageResult<()> {
        let payload = serde_json::to_value(song).map_err(map_backend)?;
        sqlx::query(
            r#"
            INSERT INTO rust_songs (media_id, source_type, title, available, payload, updated_at)
            VALUES ($1, $2, $3, $4, $5, now())
            ON CONFLICT (media_id) DO UPDATE SET
                source_type = excluded.source_type,
                title = excluded.title,
                available = excluded.available,
                payload = excluded.payload,
                updated_at = now()
            "#,
        )
        .bind(&song.media_id.0)
        .bind(&song.source_type)
        .bind(&song.title)
        .bind(song.available)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(())
    }

    async fn list_songs(&self, limit: i64, offset: i64) -> StorageResult<Vec<Song>> {
        let rows = sqlx::query(
            r#"
            SELECT payload
            FROM rust_songs
            ORDER BY title, media_id
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                let value: serde_json::Value = row.try_get("payload").map_err(map_backend)?;
                serde_json::from_value(value).map_err(map_backend)
            })
            .collect()
    }
}

#[async_trait]
impl RecommendationEventRepository for PostgresStore {
    async fn append_event(&self, event: &RecommendationEvent) -> StorageResult<()> {
        let payload = serde_json::to_value(event).map_err(map_backend)?;
        let kind = serde_json::to_string(&event.kind)
            .map_err(map_backend)?
            .trim_matches('"')
            .to_string();
        sqlx::query(
            r#"
            INSERT INTO rust_recommendation_events
                (event_id, media_id, client_clock, occurred_at, kind, payload)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(event.event_id)
        .bind(&event.media_id.0)
        .bind(event.client_clock)
        .bind(event.occurred_at)
        .bind(kind)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(())
    }

    async fn unsynced_events(&self, limit: i64) -> StorageResult<Vec<RecommendationEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT payload
            FROM rust_recommendation_events
            WHERE synced_at IS NULL
            ORDER BY client_clock, event_id
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_backend)?;

        rows.into_iter()
            .map(|row| {
                let value: serde_json::Value = row.try_get("payload").map_err(map_backend)?;
                serde_json::from_value(value).map_err(map_backend)
            })
            .collect()
    }

    async fn mark_events_synced(&self, event_ids: &[Uuid]) -> StorageResult<()> {
        sqlx::query(
            r#"
            UPDATE rust_recommendation_events
            SET synced_at = now()
            WHERE event_id = ANY($1)
            "#,
        )
        .bind(event_ids)
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(())
    }
}

#[async_trait]
impl RecommendationSnapshotRepository for PostgresStore {
    async fn put_snapshot(&self, snapshot: &RecommendationSnapshot) -> StorageResult<()> {
        let payload = serde_json::to_value(snapshot).map_err(map_backend)?;
        sqlx::query(
            r#"
            INSERT INTO rust_recommendation_snapshots
                (snapshot_id, model_version, generated_at, expires_at, payload)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (snapshot_id) DO UPDATE SET
                model_version = excluded.model_version,
                generated_at = excluded.generated_at,
                expires_at = excluded.expires_at,
                payload = excluded.payload
            "#,
        )
        .bind(snapshot.snapshot_id)
        .bind(&snapshot.model_version)
        .bind(snapshot.generated_at)
        .bind(snapshot.expires_at)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(map_backend)?;
        Ok(())
    }

    async fn latest_snapshot(&self) -> StorageResult<Option<RecommendationSnapshot>> {
        let row = sqlx::query(
            r#"
            SELECT payload
            FROM rust_recommendation_snapshots
            ORDER BY generated_at DESC, snapshot_id DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_backend)?;

        row.map(|row| {
            let value: serde_json::Value = row.try_get("payload").map_err(map_backend)?;
            serde_json::from_value(value).map_err(map_backend)
        })
        .transpose()
    }
}

fn legacy_goose_up_sql(raw: &str) -> Result<String, &'static str> {
    let mut saw_up = false;
    let mut in_up = false;
    let mut lines = Vec::new();

    for line in raw.lines() {
        match line.trim() {
            "-- +goose Up" => {
                saw_up = true;
                in_up = true;
                continue;
            }
            "-- +goose Down" => break,
            "-- +goose StatementBegin" | "-- +goose StatementEnd" => continue,
            _ => {}
        }

        if in_up {
            lines.push(line);
        }
    }

    if !saw_up {
        return Err("missing -- +goose Up marker");
    }
    let sql = lines.join("\n").trim().to_string();
    if sql.is_empty() {
        return Err("empty up migration");
    }
    Ok(sql)
}

fn map_backend<E: std::error::Error>(err: E) -> StorageError {
    StorageError::Backend(err.to_string())
}

fn parse_occurred_at_or_now(occurred_at: Option<&str>) -> DateTime<Utc> {
    occurred_at
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|time| time.with_timezone(&Utc))
        .unwrap_or_else(Utc::now)
}

fn map_auth_backend<E: std::error::Error>(err: E) -> AuthStoreError {
    AuthStoreError::Backend(err.to_string())
}

async fn count_table(pool: &PgPool, table: &str) -> StorageResult<i64> {
    let sql = match table {
        "songs" => "SELECT count(*) FROM songs",
        "albums" => "SELECT count(*) FROM albums",
        "artists" => "SELECT count(*) FROM artists",
        "playlists" => "SELECT count(*) FROM playlists",
        _ => return Err(StorageError::Backend("unsupported count table".into())),
    };
    sqlx::query_scalar(sql)
        .fetch_one(pool)
        .await
        .map_err(map_backend)
}

pub fn normalize_pairing_code(code: &str) -> String {
    let code = code.trim().to_uppercase().replace(['-', ' '], "");
    if code.len() == 8 {
        format!("{}-{}", &code[..4], &code[4..])
    } else {
        code
    }
}

pub fn hash_token(token: &str) -> Result<String, AuthStoreError> {
    let mut out = [0u8; 32];
    let params = argon2::Params::new(ARGON_MEMORY, ARGON_TIME, ARGON_THREADS, Some(out.len()))
        .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon
        .hash_password_into(token.as_bytes(), b"sunflower-tok-v1", &mut out)
        .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
    Ok(hex_lower(&out))
}

fn validate_owner_password(password: &str, setup_token: &str) -> Result<(), AuthStoreError> {
    let trimmed = password.trim();
    if trimmed.is_empty() || trimmed.len() < 12 {
        return Err(AuthStoreError::WeakPassword);
    }
    if !setup_token.is_empty() && constant_time_eq(trimmed, setup_token.trim()) {
        return Err(AuthStoreError::WeakPassword);
    }
    Ok(())
}

fn hash_owner_password(password: &str) -> Result<String, AuthStoreError> {
    let mut salt = [0u8; 16];
    rand::rngs::OsRng
        .try_fill_bytes(&mut salt)
        .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
    let mut out = [0u8; ARGON_KEY_LEN];
    let params = argon2::Params::new(ARGON_MEMORY, ARGON_TIME, ARGON_THREADS, Some(out.len()))
        .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon
        .hash_password_into(password.as_bytes(), &salt, &mut out)
        .map_err(|e| AuthStoreError::Backend(e.to_string()))?;

    Ok(format!(
        "$argon2id$v=19$m={},t={},p={}${}${}",
        ARGON_MEMORY,
        ARGON_TIME,
        ARGON_THREADS,
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(salt),
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(out)
    ))
}

fn verify_owner_password(phc: &str, password: &str) -> Result<bool, AuthStoreError> {
    let parts: Vec<&str> = phc.split('$').collect();
    if parts.len() != 6 || parts[1] != "argon2id" {
        return Err(AuthStoreError::Backend("unsupported password hash".into()));
    }
    let mut memory = None;
    let mut time = None;
    let mut threads = None;
    for param in parts[3].split(',') {
        let Some((key, value)) = param.split_once('=') else {
            return Err(AuthStoreError::Backend("invalid password params".into()));
        };
        let parsed = value
            .parse::<u32>()
            .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
        match key {
            "m" => memory = Some(parsed),
            "t" => time = Some(parsed),
            "p" => threads = Some(parsed),
            _ => {}
        }
    }
    let salt = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(parts[4])
        .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
    let want = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(parts[5])
        .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
    let mut out = vec![0u8; want.len()];
    let params = argon2::Params::new(
        memory.ok_or_else(|| AuthStoreError::Backend("missing memory param".into()))?,
        time.ok_or_else(|| AuthStoreError::Backend("missing time param".into()))?,
        threads.ok_or_else(|| AuthStoreError::Backend("missing threads param".into()))?,
        Some(out.len()),
    )
    .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon
        .hash_password_into(password.as_bytes(), &salt, &mut out)
        .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
    Ok(constant_time_bytes_eq(&out, &want))
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    constant_time_bytes_eq(left, right)
}

fn constant_time_bytes_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn hash_verifier(secret: &str) -> String {
    let hash = Sha256::digest(secret.as_bytes());
    hex_lower(&hash)
}

pub fn verify_admin_csrf(session: &AdminSession, token: &str) -> bool {
    !token.is_empty() && hash_verifier(token) == session.csrf_secret_hash
}

pub fn generate_device_token() -> Result<String, AuthStoreError> {
    let mut raw = [0u8; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut raw)
        .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
    Ok(format!(
        "sf_dev_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
    ))
}

fn random_secret(prefix: &str) -> Result<String, AuthStoreError> {
    let mut raw = [0u8; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut raw)
        .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
    Ok(format!(
        "{}{}",
        prefix,
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
    ))
}

fn generate_pairing_code() -> Result<String, AuthStoreError> {
    let mut raw = [0u8; 5];
    rand::rngs::OsRng
        .try_fill_bytes(&mut raw)
        .map_err(|e| AuthStoreError::Backend(e.to_string()))?;
    let encoded = base32_no_padding_5_bytes(raw);
    Ok(format!("{}-{}", &encoded[..4], &encoded[4..8]))
}

fn base32_no_padding_5_bytes(raw: [u8; 5]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let value = ((raw[0] as u64) << 32)
        | ((raw[1] as u64) << 24)
        | ((raw[2] as u64) << 16)
        | ((raw[3] as u64) << 8)
        | raw[4] as u64;
    let mut out = String::with_capacity(8);
    for shift in (0..40).step_by(5).rev() {
        let index = ((value >> shift) & 0x1f) as usize;
        out.push(ALPHABET[index] as char);
    }
    out
}

fn pairing_url(server_url: &str, code: &str) -> String {
    format!(
        "sunflower://pair?code={}&server={}",
        query_escape(code),
        query_escape(server_url.trim_end_matches('/'))
    )
}

fn home_cache_key(
    user_id: Uuid,
    hide_explicit: bool,
    hide_video: bool,
    hide_shorts: bool,
) -> String {
    format!(
        "home:{}:{}",
        user_id,
        filters_hash(hide_explicit, hide_video, hide_shorts)
    )
}

fn filters_hash(hide_explicit: bool, hide_video: bool, hide_shorts: bool) -> char {
    let mut value = 0u8;
    if hide_explicit {
        value |= 1;
    }
    if hide_video {
        value |= 2;
    }
    if hide_shorts {
        value |= 4;
    }
    char::from(b'a' + value)
}

fn query_escape(raw: &str) -> String {
    let mut out = String::new();
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn redact_json(mut value: serde_json::Value) -> serde_json::Value {
    redact_json_value(&mut value);
    value
}

fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                let lower = key.to_ascii_lowercase();
                if lower.contains("password")
                    || lower.contains("token")
                    || lower.contains("cookie")
                    || lower.contains("code")
                    || lower.contains("secret")
                {
                    *child = serde_json::Value::String("[redacted]".to_string());
                } else {
                    redact_json_value(child);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json_value(item);
            }
        }
        _ => {}
    }
}

fn rfc3339_seconds(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn rfc3339_nano(time: DateTime<Utc>) -> String {
    legacy_rfc3339_nano(time)
}

fn hex_lower(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(TABLE[(byte >> 4) as usize] as char);
        out.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sunflower_core::RegisterDeviceRequest;

    #[test]
    fn embedded_migrations_extract_only_up_sql() {
        let expected_versions = (1_i64..=EMBEDDED_MIGRATIONS.len() as i64).collect::<Vec<_>>();
        assert_eq!(
            EMBEDDED_MIGRATIONS
                .iter()
                .map(|(version, _, _)| *version)
                .collect::<Vec<_>>(),
            expected_versions
        );

        let first = legacy_goose_up_sql(EMBEDDED_MIGRATIONS[0].2).unwrap();
        assert!(first.contains("CREATE TABLE users"));
        assert!(first.contains("CREATE TABLE playlist_items"));
        assert!(!first.contains("-- +goose Down"));
        assert!(!first.contains("DROP TABLE"));

        for (_, name, raw) in EMBEDDED_MIGRATIONS {
            let up = legacy_goose_up_sql(raw).unwrap();
            assert!(!up.is_empty(), "{name} extracted an empty Up migration");
            assert!(
                !up.contains("-- +goose Statement"),
                "{name} leaked goose markers into SQL"
            );
        }
    }

    #[test]
    fn pairing_code_normalization_matches_go() {
        assert_eq!(normalize_pairing_code(" abcd efgh "), "ABCD-EFGH");
        assert_eq!(normalize_pairing_code("abcd-efgh"), "ABCD-EFGH");
        assert_eq!(normalize_pairing_code("abc"), "ABC");
        assert_eq!(normalize_pairing_code(""), "");
    }

    #[test]
    fn device_tokens_match_go_shape() {
        let token = generate_device_token().unwrap();
        assert!(token.starts_with("sf_dev_"));
        assert_eq!(token.len(), "sf_dev_".len() + 43);
        assert!(!token.contains('='));
    }

    #[test]
    fn token_hash_is_deterministic_argon2id_hex() {
        let first = hash_token("sf_dev_test").unwrap();
        let second = hash_token("sf_dev_test").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn owner_password_policy_and_hash_match_go_shape() {
        assert_eq!(
            validate_owner_password("short", "sunflower-test-setup-token").unwrap_err(),
            AuthStoreError::WeakPassword
        );
        assert_eq!(
            validate_owner_password("sunflower-test-setup-token", "sunflower-test-setup-token",)
                .unwrap_err(),
            AuthStoreError::WeakPassword
        );
        validate_owner_password("sunflower owner password", "sunflower-test-setup-token").unwrap();

        let phc = hash_owner_password("sunflower owner password").unwrap();
        assert!(phc.starts_with("$argon2id$v=19$m=65536,t=1,p=4$"));
        let parts: Vec<_> = phc.split('$').collect();
        assert_eq!(parts.len(), 6);
        assert_eq!(parts[4].len(), 22);
        assert_eq!(parts[5].len(), 43);
        assert!(!parts[4].contains('='));
        assert!(!parts[5].contains('='));
        assert!(verify_owner_password(&phc, "sunflower owner password").unwrap());
        assert!(!verify_owner_password(&phc, "wrong password").unwrap());
    }

    #[test]
    fn admin_session_and_pairing_helpers_match_go_shapes() {
        let admin = random_secret("sf_adm_").unwrap();
        let csrf = random_secret("sf_csrf_").unwrap();
        assert!(admin.starts_with("sf_adm_"));
        assert!(csrf.starts_with("sf_csrf_"));
        assert_eq!(admin.len(), "sf_adm_".len() + 43);
        assert_eq!(csrf.len(), "sf_csrf_".len() + 43);
        assert_eq!(hash_verifier("secret").len(), 64);

        assert_eq!(base32_no_padding_5_bytes([0, 0, 0, 0, 0]), "AAAAAAAA");
        let code = generate_pairing_code().unwrap();
        assert_eq!(code.len(), 9);
        assert_eq!(&code[4..5], "-");
        assert!(
            code.chars()
                .all(|c| c == '-' || c.is_ascii_uppercase() || ('2'..='7').contains(&c))
        );
        assert_eq!(
            pairing_url("http://localhost:8080/", "ABCD-EFGH"),
            "sunflower://pair?code=ABCD-EFGH&server=http%3A%2F%2Flocalhost%3A8080"
        );
        assert_eq!(normalize_pairing_ttl_seconds(0), 10 * 60);
        assert_eq!(normalize_pairing_ttl_seconds(-1), 10 * 60);
        assert_eq!(normalize_pairing_ttl_seconds(599), 599);
        assert_eq!(normalize_pairing_ttl_seconds(60 * 60), 60 * 60);
        assert_eq!(normalize_pairing_ttl_seconds(60 * 60 + 1), 60 * 60);
        assert_eq!(normalize_pairing_ttl_seconds(2_147_483_648), 60 * 60);
    }

    #[test]
    fn rfc3339_helpers_match_go_time_json_contract() {
        let cases = [
            ("2026-07-01T00:00:00Z", "2026-07-01T00:00:00Z"),
            ("2026-07-01T00:00:00.100000000Z", "2026-07-01T00:00:00.1Z"),
            ("2026-07-01T00:00:00.120000000Z", "2026-07-01T00:00:00.12Z"),
            ("2026-07-01T00:00:00.123000000Z", "2026-07-01T00:00:00.123Z"),
            (
                "2026-07-01T00:00:00.123400000Z",
                "2026-07-01T00:00:00.1234Z",
            ),
            (
                "2026-07-01T00:00:00.123456000Z",
                "2026-07-01T00:00:00.123456Z",
            ),
            (
                "2026-07-01T00:00:00.123456700Z",
                "2026-07-01T00:00:00.1234567Z",
            ),
            (
                "2026-07-01T00:00:00.123456789Z",
                "2026-07-01T00:00:00.123456789Z",
            ),
        ];

        for (raw, expected) in cases {
            let time = DateTime::parse_from_rfc3339(raw)
                .unwrap()
                .with_timezone(&Utc);
            assert_eq!(rfc3339_nano(time), expected, "{raw}");
        }

        let subsecond = DateTime::parse_from_rfc3339("2026-07-01T00:00:00.123456789Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(rfc3339_seconds(subsecond), "2026-07-01T00:00:00Z");
    }

    #[test]
    fn audit_metadata_redaction_matches_go_keys() {
        let redacted = redact_json(serde_json::json!({
            "password": "pw",
            "nested": {
                "csrf_secret": "secret",
                "safe": "value"
            },
            "items": [
                {"pairing_code": "ABCD-EFGH"},
                {"count": 1}
            ]
        }));
        assert_eq!(
            redacted,
            serde_json::json!({
                "password": "[redacted]",
                "nested": {
                    "csrf_secret": "[redacted]",
                    "safe": "value"
                },
                "items": [
                    {"pairing_code": "[redacted]"},
                    {"count": 1}
                ]
            })
        );
    }

    #[tokio::test]
    async fn postgres_auth_round_trips_against_legacy_schema_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };

        let store = PostgresStore::connect(&database_url).await.unwrap();
        let user_id = Uuid::new_v4();
        let pairing_id = Uuid::new_v4();
        let compact_code = user_id.simple().to_string()[..8].to_uppercase();
        let code = format!("{}-{}", &compact_code[..4], &compact_code[4..]);
        let code_hash = hash_token(&code).unwrap();

        sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
            .bind(user_id)
            .bind("Rust Auth Test")
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            INSERT INTO pairing_codes (id, user_id, code_hash, label, expires_at)
            VALUES ($1, $2, $3, $4, now() + interval '10 minutes')
            "#,
        )
        .bind(pairing_id)
        .bind(user_id)
        .bind(code_hash)
        .bind("Test Phone")
        .execute(&store.pool)
        .await
        .unwrap();

        let response = store
            .register_device(&RegisterDeviceRequest {
                device_name: String::new(),
                platform: "android".into(),
                client_version: "test".into(),
                pairing_code: compact_code.to_lowercase(),
            })
            .await
            .unwrap();
        assert!(response.token.starts_with("sf_dev_"));
        assert_eq!(
            response.server_capabilities,
            vec![
                "auth.pairing.v1".to_string(),
                "library.v1".to_string(),
                "recs.v1".to_string(),
                "stream.proxy".to_string(),
                "ws.now_playing".to_string(),
            ]
        );

        let authenticated = store.validate_device_token(&response.token).await.unwrap();
        assert_eq!(authenticated.user_id, user_id);
        assert_eq!(authenticated.device_id.to_string(), response.device_id);

        let reused = store
            .register_device(&RegisterDeviceRequest {
                device_name: String::new(),
                platform: String::new(),
                client_version: String::new(),
                pairing_code: code,
            })
            .await
            .unwrap_err();
        assert!(matches!(reused, AuthStoreError::InvalidPairingCode));

        sqlx::query("DELETE FROM audit_events WHERE user_id = $1 OR actor_id = $2")
            .bind(user_id)
            .bind(pairing_id.to_string())
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&store.pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn postgres_local_home_inputs_ignore_non_playback_events_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };

        let store = PostgresStore::connect(&database_url).await.unwrap();
        store.migrate().await.unwrap();

        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let song_id = format!("local:stats-{}", user_id.simple());
        let pause_only_id = format!("local:pause-{}", user_id.simple());
        let pathless_local_id = format!("local:pathless-{}", user_id.simple());
        let downloaded_remote_id = format!("yt:downloaded-{}", user_id.simple());

        sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
            .bind(user_id)
            .bind("Rust Local Stats Test")
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            INSERT INTO devices (id, user_id, name, platform, token_hash)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind("Stats Phone")
        .bind("test")
        .bind("token-hash")
        .execute(&store.pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO songs (media_id, source_type, title, available, local_path)
            VALUES
                ($1, 'local', 'Stats Song', true, '/music/stats.mp3'),
                ($2, 'local', 'Pause Only Song', true, '/music/pause.mp3'),
                ($3, 'local', 'Pathless Local Song', true, NULL),
                ($4, 'yt', 'Downloaded Remote Song', true, NULL)
            "#,
        )
        .bind(&song_id)
        .bind(&pause_only_id)
        .bind(&pathless_local_id)
        .bind(&downloaded_remote_id)
        .execute(&store.pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO downloaded_tracks (device_id, song_media_id, local_path, bytes, completed_at, last_verified_at)
            VALUES ($1, $2, '/downloads/remote.m4a', 1234, now(), now())
            "#,
        )
        .bind(device_id)
        .bind(&downloaded_remote_id)
        .execute(&store.pool)
        .await
        .unwrap();

        let now = Utc::now();
        for (media_id, kind, occurred_at) in [
            (song_id.as_str(), "play", now - Duration::minutes(3)),
            (song_id.as_str(), "skip", now - Duration::minutes(2)),
            (song_id.as_str(), "pause", now - Duration::minutes(1)),
            (pause_only_id.as_str(), "pause", now),
        ] {
            store
                .insert_play_event(
                    user_id,
                    device_id,
                    &EventEntryRequest {
                        event_id: String::new(),
                        kind: kind.to_string(),
                        media_id: media_id.to_string(),
                        queue_id: String::new(),
                        occurred_at: Some(rfc3339_nano(occurred_at)),
                        total_played_ms: 30_000,
                        duration_ms: 180_000,
                        reason: String::new(),
                    },
                )
                .await
                .unwrap();
        }

        let (_candidates, snapshot) = store
            .local_home_inputs(user_id, device_id, 10, false, false)
            .await
            .unwrap();

        let stats_song = snapshot
            .tracks
            .iter()
            .find(|track| track.media_id.0 == song_id)
            .expect("stats song should be present");
        assert_eq!(stats_song.play_count, 1);
        assert_eq!(stats_song.skip_count, 1);
        assert_eq!(stats_song.completion_count, 1);
        assert!(stats_song.last_played_at.is_some());

        let pause_only = snapshot
            .tracks
            .iter()
            .find(|track| track.media_id.0 == pause_only_id)
            .expect("local pause-only song should be present");
        assert_eq!(pause_only.play_count, 0);
        assert_eq!(pause_only.skip_count, 0);
        assert_eq!(pause_only.completion_count, 0);
        assert!(pause_only.last_played_at.is_none());

        let pathless_local = snapshot
            .tracks
            .iter()
            .find(|track| track.media_id.0 == pathless_local_id)
            .expect("pathless local song should be present");
        assert!(!pathless_local.downloaded);
        assert!(pathless_local.local_available);

        let downloaded_remote = snapshot
            .tracks
            .iter()
            .find(|track| track.media_id.0 == downloaded_remote_id)
            .expect("downloaded remote song should be present");
        assert!(downloaded_remote.downloaded);
        assert!(downloaded_remote.local_available);
        assert_eq!(
            snapshot.recent_media_ids,
            vec![MediaId::new(song_id.clone())]
        );

        sqlx::query("DELETE FROM downloaded_tracks WHERE device_id = $1")
            .bind(device_id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM play_events WHERE user_id = $1")
            .bind(user_id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM devices WHERE id = $1")
            .bind(device_id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM songs WHERE media_id IN ($1, $2, $3, $4)")
            .bind(&song_id)
            .bind(&pause_only_id)
            .bind(&pathless_local_id)
            .bind(&downloaded_remote_id)
            .execute(&store.pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn postgres_most_played_artists_ignore_non_play_events_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };

        let store = PostgresStore::connect(&database_url).await.unwrap();
        store.migrate().await.unwrap();

        let user_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let play_artist_id = format!("yt:play-artist-{}", user_id.simple());
        let noise_artist_id = format!("yt:noise-artist-{}", user_id.simple());
        let play_song_id = format!("yt:play-song-{}", user_id.simple());
        let noise_song_id = format!("yt:noise-song-{}", user_id.simple());

        sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
            .bind(user_id)
            .bind("Rust Artist Stats Test")
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query(
            r#"
            INSERT INTO devices (id, user_id, name, platform, token_hash)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .bind("Artist Stats Phone")
        .bind("test")
        .bind("token-hash")
        .execute(&store.pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO artists (media_id, source_type, name)
            VALUES
                ($1, 'yt', 'Played Artist'),
                ($2, 'yt', 'Noise Artist')
            "#,
        )
        .bind(&play_artist_id)
        .bind(&noise_artist_id)
        .execute(&store.pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO songs (media_id, source_type, title, primary_artist_id, available)
            VALUES
                ($1, 'yt', 'Played Song', $2, true),
                ($3, 'yt', 'Noise Song', $4, true)
            "#,
        )
        .bind(&play_song_id)
        .bind(&play_artist_id)
        .bind(&noise_song_id)
        .bind(&noise_artist_id)
        .execute(&store.pool)
        .await
        .unwrap();

        let now = Utc::now();
        for (media_id, kind, occurred_at) in [
            (play_song_id.as_str(), "play", now - Duration::minutes(5)),
            (play_song_id.as_str(), "play", now - Duration::minutes(4)),
            (noise_song_id.as_str(), "skip", now - Duration::minutes(3)),
            (noise_song_id.as_str(), "pause", now - Duration::minutes(2)),
            (
                noise_song_id.as_str(),
                "complete",
                now - Duration::minutes(1),
            ),
        ] {
            store
                .insert_play_event(
                    user_id,
                    device_id,
                    &EventEntryRequest {
                        event_id: String::new(),
                        kind: kind.to_string(),
                        media_id: media_id.to_string(),
                        queue_id: String::new(),
                        occurred_at: Some(rfc3339_nano(occurred_at)),
                        total_played_ms: 30_000,
                        duration_ms: 180_000,
                        reason: String::new(),
                    },
                )
                .await
                .unwrap();
        }

        let artists = store.most_played_artists(user_id, 10).await.unwrap();
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].artist_id, play_artist_id);
        assert_eq!(artists[0].artist_name, "Played Artist");
        assert_eq!(artists[0].play_count, 2);

        sqlx::query("DELETE FROM play_events WHERE user_id = $1")
            .bind(user_id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM devices WHERE id = $1")
            .bind(device_id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM songs WHERE media_id IN ($1, $2)")
            .bind(&play_song_id)
            .bind(&noise_song_id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM artists WHERE media_id IN ($1, $2)")
            .bind(&play_artist_id)
            .bind(&noise_artist_id)
            .execute(&store.pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn postgres_likes_resolve_unlikes_by_occurred_at_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };

        let store = PostgresStore::connect(&database_url).await.unwrap();
        store.migrate().await.unwrap();

        let user_id = Uuid::new_v4();
        let song_id = format!("yt:like-lww-{}", user_id.simple());
        sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
            .bind(user_id)
            .bind("Rust Like LWW Test")
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO songs (media_id, source_type, title) VALUES ($1, 'yt', $2)")
            .bind(&song_id)
            .bind("Like LWW Song")
            .execute(&store.pool)
            .await
            .unwrap();

        store
            .upsert_like(
                user_id,
                &song_id,
                Some("2026-07-01T00:10:00Z"),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        store
            .delete_like(
                user_id,
                &song_id,
                Some("2026-07-01T00:05:00Z"),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        let like_count_after_old_unlike: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM likes WHERE user_id = $1 AND song_media_id = $2",
        )
        .bind(user_id)
        .bind(&song_id)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(like_count_after_old_unlike, 1);

        store
            .delete_like(
                user_id,
                &song_id,
                Some("2026-07-01T00:20:00Z"),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        let like_count_after_new_unlike: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM likes WHERE user_id = $1 AND song_media_id = $2",
        )
        .bind(user_id)
        .bind(&song_id)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(like_count_after_new_unlike, 0);

        store
            .upsert_like(
                user_id,
                &song_id,
                Some("2026-07-01T00:15:00Z"),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        let like_count_after_old_like: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM likes WHERE user_id = $1 AND song_media_id = $2",
        )
        .bind(user_id)
        .bind(&song_id)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(like_count_after_old_like, 0);

        store
            .upsert_like(
                user_id,
                &song_id,
                Some("2026-07-01T00:20:00Z"),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        let like_count_after_tie_like: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM likes WHERE user_id = $1 AND song_media_id = $2",
        )
        .bind(user_id)
        .bind(&song_id)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(like_count_after_tie_like, 1);
        let tombstone_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM rust_like_tombstones WHERE user_id = $1 AND song_media_id = $2",
        )
        .bind(user_id)
        .bind(&song_id)
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(tombstone_count, 0);

        sqlx::query("DELETE FROM likes WHERE user_id = $1")
            .bind(user_id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM rust_like_tombstones WHERE user_id = $1")
            .bind(user_id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&store.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM songs WHERE media_id = $1")
            .bind(&song_id)
            .execute(&store.pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn postgres_migrate_bootstraps_legacy_and_rust_tables_when_enabled() {
        if std::env::var("SUNFLOWER_RUN_PG_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };

        let store = PostgresStore::connect(&database_url).await.unwrap();
        store.migrate().await.unwrap();

        for table in [
            "users",
            "devices",
            "songs",
            "play_events",
            "queue_sessions",
            "rec_cache",
            "downloaded_tracks",
            "admin_sessions",
            "pairing_codes",
            "audit_events",
            "rust_songs",
            "rust_recommendation_events",
            "rust_ingested_events",
            "rust_recommendation_snapshots",
            "rust_like_tombstones",
        ] {
            let found: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
                .bind(table)
                .fetch_one(&store.pool)
                .await
                .unwrap();
            assert_eq!(found.as_deref(), Some(table), "{table} missing");
        }

        for version in 1_i64..=EMBEDDED_MIGRATIONS.last().unwrap().0 {
            let applied = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM goose_db_version
                    WHERE version_id = $1 AND is_applied
                )
                "#,
            )
            .bind(version)
            .fetch_one(&store.pool)
            .await
            .unwrap();
            assert!(
                applied,
                "embedded migration {version} was not marked applied"
            );
        }
    }
}
