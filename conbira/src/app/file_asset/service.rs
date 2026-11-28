use std::time::Duration;

use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::presigning::PresigningConfig;
use uuid::Uuid;

use crate::app::DatabaseExt as _;
use crate::app::container::db::{Database, Handle};
use crate::app::file_asset::config::Config;
use crate::error::ResultExt as _;
use crate::{Code, Error};

pub struct FileAssetService {
    config: Config,
    s3: aws_sdk_s3::Client,
}

/// Asset lifecycle states; domain callers determine permitted transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAssetStatus {
    Pending,
    Attached,
    Deleted,
}

impl FileAssetStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Attached => "attached",
            Self::Deleted => "deleted",
        }
    }
}

#[derive(serde::Serialize)]
pub struct DownloadTicket {
    pub download_url: String,
    pub headers: Vec<(String, String)>,
    pub expires_in_seconds: u64,
}

pub struct UploadRequest {
    pub uploaded_by: i64,
    pub original_filename: String,
    pub size_bytes: i64,
    pub content_type: String,
}

#[derive(serde::Serialize)]
pub struct UploadTicket {
    pub id: Uuid,
    pub upload_url: String,
    pub headers: Vec<(String, String)>,
    pub expires_in_seconds: u64,
}

impl FileAssetService {
    /// Removes expired objects and their records. Failed objects remain
    /// eligible for a later run.
    pub async fn cleanup(&self, db: &Database) -> crate::Result<()> {
        let pending_secs = i64::try_from(self.config.pending_ttl_secs)
            .map_err_internal("Invalid pending retention", crate::here!())?;
        let deleted_secs = i64::try_from(self.config.deleted_retention_secs)
            .map_err_internal("Invalid deleted retention", crate::here!())?;
        let mut conn = db.acquire_conn(crate::here!()).await?;
        let ids =
            super::repository::cleanup_candidates(&mut conn.handle(), pending_secs, deleted_secs)
                .await?;
        drop(conn);
        for id in ids {
            let mut tx = db.begin_tx(crate::here!()).await?;
            let asset = super::repository::lock_cleanup_candidate(
                &mut tx.handle(),
                id,
                pending_secs,
                deleted_secs,
            )
            .await?;
            let Some(asset) = asset else { continue };
            if let Err(error) = self
                .s3
                .delete_object()
                .bucket(&asset.s3_bucket)
                .key(&asset.s3_object_key)
                .send()
                .await
            {
                tracing::error!(%id, %error, "File asset cleanup failed");
                continue;
            }
            super::repository::remove(&mut tx.handle(), id).await?;
            tx.commit()
                .await
                .map_err_internal("Failed to commit file asset cleanup", crate::here!())?;
        }
        Ok(())
    }
    pub fn new(config: Config) -> crate::Result<Self> {
        let s3 = &config.s3;
        if s3.access_key_id.trim().is_empty()
            || s3.secret_access_key.trim().is_empty()
            || s3.region.trim().is_empty()
            || s3.bucket.trim().is_empty()
            || s3.key_prefix.trim().is_empty()
            || config.max_size_bytes == 0
            || config.upload_url_ttl_secs == 0
            || config.upload_url_ttl_secs > 604_800
            || config.download_url_ttl_secs == 0
            || config.download_url_ttl_secs > 604_800
        {
            return Err(Error::new(
                Code::InvalidConfiguration,
                "Invalid file asset configuration",
            )
            .at(crate::here!()));
        }
        let credentials = Credentials::new(
            &s3.access_key_id,
            &s3.secret_access_key,
            None,
            None,
            "file-assets",
        );
        let mut builder = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .credentials_provider(credentials)
            .region(Region::new(s3.region.clone()));
        if let Some(endpoint) = &s3.endpoint {
            let url = url::Url::parse(endpoint).map_err(|_| {
                Error::new(Code::InvalidConfiguration, "Invalid S3 endpoint").at(crate::here!())
            })?;
            let local = match url.host() {
                Some(url::Host::Domain("localhost")) => true,
                Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_private(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                _ => false,
            };
            if !(url.scheme() == "https" || (url.scheme() == "http" && local))
                || url.host().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(
                    Error::new(Code::InvalidConfiguration, "Invalid S3 endpoint")
                        .at(crate::here!()),
                );
            }
            builder = builder.endpoint_url(endpoint).force_path_style(true);
        }
        Ok(Self {
            s3: aws_sdk_s3::Client::from_conf(builder.build()),
            config,
        })
    }

    /// Issues a GET URL for a pending or attached asset. The caller checks
    /// access.
    pub async fn prepare_download(
        &self,
        handle: &mut Handle<'_>,
        id: Uuid,
    ) -> crate::Result<DownloadTicket> {
        let asset = super::repository::find_download_asset(handle, id)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "File asset not found").at(crate::here!()))?;
        let presigning =
            PresigningConfig::expires_in(Duration::from_secs(self.config.download_url_ttl_secs))
                .map_err_internal("Invalid download URL expiration", crate::here!())?;
        let filename = percent_encoding::utf8_percent_encode(
            &asset.original_filename,
            percent_encoding::NON_ALPHANUMERIC,
        );
        let signed = self
            .s3
            .get_object()
            .bucket(asset.s3_bucket)
            .key(asset.s3_object_key)
            .response_content_disposition(format!("attachment; filename*=UTF-8''{filename}"))
            .presigned(presigning)
            .await
            .map_err_internal("Failed to sign download URL", crate::here!())?;
        Ok(DownloadTicket {
            download_url: signed.uri().to_owned(),
            headers: signed
                .headers()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
            expires_in_seconds: self.config.download_url_ttl_secs,
        })
    }

    /// Changes state only when it matches `expected`; false means missing or
    /// changed. The caller owns authorization and lifecycle transition rules.
    pub async fn change_status(
        &self,
        handle: &mut Handle<'_>,
        id: Uuid,
        expected: FileAssetStatus,
        target: FileAssetStatus,
    ) -> crate::Result<bool> {
        if expected == FileAssetStatus::Pending
            && target == FileAssetStatus::Attached
            && !self.verify_pending_upload(handle, id, None).await?
        {
            return Ok(false);
        }
        super::repository::change_status(handle, id, expected.as_str(), target.as_str()).await
    }

    pub async fn attach_owned(
        &self,
        handle: &mut Handle<'_>,
        id: Uuid,
        user_id: i64,
    ) -> crate::Result<bool> {
        if !self
            .verify_pending_upload(handle, id, Some(user_id))
            .await?
        {
            return Ok(false);
        }
        super::repository::attach_owned(handle, id, user_id).await
    }

    async fn verify_pending_upload(
        &self,
        handle: &mut Handle<'_>,
        id: Uuid,
        owner: Option<i64>,
    ) -> crate::Result<bool> {
        let Some(asset) = super::repository::find_pending(handle, id, owner).await? else {
            return Ok(false);
        };
        let object = self
            .s3
            .head_object()
            .bucket(asset.s3_bucket)
            .key(asset.s3_object_key)
            .send()
            .await;
        let object = match object {
            Ok(object) => object,
            Err(error)
                if error
                    .raw_response()
                    .is_some_and(|response| response.status().as_u16() == 404) =>
            {
                return Err(
                    Error::new(Code::InvalidRequest, "File upload is missing").at(crate::here!())
                );
            }
            Err(error) => {
                return Err(error).map_err_internal("Failed to verify file upload", crate::here!());
            }
        };
        if object.content_length() != Some(asset.size_bytes) {
            return Err(
                Error::new(Code::InvalidRequest, "File upload size does not match")
                    .at(crate::here!()),
            );
        }
        Ok(true)
    }

    /// Marks an asset for cleanup, including assets in the attached state.
    pub async fn delete(&self, handle: &mut Handle<'_>, id: Uuid) -> crate::Result<()> {
        if super::repository::mark_deleted(handle, id).await? {
            return Ok(());
        }
        Err(Error::new(Code::NotFound, "File asset not found").at(crate::here!()))
    }

    /// Issues a PUT URL and records a pending asset using the caller's database
    /// handle.
    pub async fn prepare_upload(
        &self,
        handle: &mut Handle<'_>,
        request: UploadRequest,
    ) -> crate::Result<UploadTicket> {
        if request.original_filename.trim().is_empty()
            || request.content_type.parse::<mime::Mime>().is_err()
            || request.size_bytes < 0
            || request.size_bytes as u64 > self.config.max_size_bytes
            || (!self.config.allowed_content_types.is_empty()
                && !self
                    .config
                    .allowed_content_types
                    .contains(&request.content_type))
        {
            return Err(
                Error::new(Code::InvalidRequest, "Invalid file upload metadata").at(crate::here!()),
            );
        }
        let id = Uuid::new_v4();
        let object_key = format!("{}/{id}", self.config.s3.key_prefix.trim_end_matches('/'));
        let presigning =
            PresigningConfig::expires_in(Duration::from_secs(self.config.upload_url_ttl_secs))
                .map_err_internal("Invalid upload URL expiration", crate::here!())?;
        let signed = self
            .s3
            .put_object()
            .bucket(&self.config.s3.bucket)
            .key(&object_key)
            .content_type(&request.content_type)
            .content_length(request.size_bytes)
            .presigned(presigning)
            .await
            .map_err_internal("Failed to sign upload URL", crate::here!())?;
        super::repository::insert(
            handle,
            &super::repository::NewAsset {
                id,
                uploaded_by: request.uploaded_by,
                bucket: &self.config.s3.bucket,
                object_key: &object_key,
                filename: &request.original_filename,
                size_bytes: request.size_bytes,
                content_type: &request.content_type,
            },
        )
        .await?;
        Ok(UploadTicket {
            id,
            upload_url: signed.uri().to_owned(),
            headers: signed
                .headers()
                .filter(|(name, _)| !name.eq_ignore_ascii_case("content-length"))
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
            expires_in_seconds: self.config.upload_url_ttl_secs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::file_asset::config::S3Config;

    fn config() -> Config {
        Config {
            s3: S3Config {
                access_key_id: "access".into(),
                secret_access_key: "secret".into(),
                endpoint: Some("http://127.0.0.1:9000".into()),
                region: "us-east-1".into(),
                bucket: "assets".into(),
                key_prefix: "uploads".into(),
            },
            max_size_bytes: 1024,
            upload_url_ttl_secs: 600,
            download_url_ttl_secs: 180,
            pending_ttl_secs: 86400,
            deleted_retention_secs: 604800,
            allowed_content_types: vec!["text/plain".into()],
        }
    }

    mod regression {
        use super::*;

        #[tokio::test]
        async fn missing_upload_cannot_be_attached() -> anyhow::Result<()> {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            let server = tokio::spawn(async move {
                let router =
                    axum::Router::new().fallback(|| async { axum::http::StatusCode::NOT_FOUND });
                axum::serve(listener, router).await.unwrap();
            });
            let result = crate::test_support::TestContext::run(2, move |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('writer') RETURNING id").fetch_one(conn.handle().conn()).await?;
                let id = Uuid::new_v4();
                sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type) VALUES ($1, 'pending', $2, 'bucket', 'missing', 'file', 4, 'text/plain')").bind(id).bind(user).execute(conn.handle().conn()).await?;
                let mut config = config();
                config.s3.endpoint = Some(format!("http://{address}"));
                let service = FileAssetService::new(config)?;
                assert_eq!(service.attach_owned(&mut conn.handle(), id, user).await.unwrap_err().code, Code::InvalidRequest);
                let status: String = sqlx::query_scalar("SELECT status FROM conbira_file_assets WHERE id = $1").bind(id).fetch_one(conn.handle().conn()).await?;
                assert_eq!(status, "pending");
                Ok(())
            }).await;
            server.abort();
            result
        }
    }

    mod contract {
        use super::*;

        #[tokio::test]
        async fn issues_put_url_and_persists_pending_asset() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut connection = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql"))
                    .execute(connection.handle().conn()).await?;
                let user_id: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('writer') RETURNING id")
                    .fetch_one(connection.handle().conn()).await?;
                let service = FileAssetService::new(config())?;
                let ticket = service.prepare_upload(&mut connection.handle(), UploadRequest {
                    uploaded_by: user_id, original_filename: "report.txt".into(),
                    size_bytes: 4, content_type: "text/plain".into(),
                }).await?;
                let url = url::Url::parse(&ticket.upload_url)?;
                let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
                // URL과 DB 행은 같은 자산을 가리키며 요청한 콘텐츠 타입으로 서명된다.
                assert_eq!(url.path(), format!("/assets/uploads/{}", ticket.id));
                assert_eq!(query.get("X-Amz-Expires").map(String::as_str), Some("600"));
                assert!(query.contains_key("X-Amz-Signature"));
                let signed_headers = query.get("X-Amz-SignedHeaders").unwrap();
                assert!(signed_headers.split(';').any(|name| name == "content-length"));
                assert!(signed_headers.split(';').any(|name| name == "content-type"));
                assert!(ticket.headers.iter().any(|(name, value)| name.eq_ignore_ascii_case("content-type") && value == "text/plain"));
                let row: (String, i64, String, String, i64, String) = sqlx::query_as("SELECT status, uploaded_by, s3_bucket, s3_object_key, size_bytes, content_type FROM conbira_file_assets WHERE id=$1")
                    .bind(ticket.id).fetch_one(connection.handle().conn()).await?;
                assert_eq!(row, ("pending".into(), user_id, "assets".into(), format!("uploads/{}", ticket.id), 4, "text/plain".into()));
                Ok(())
            }).await
        }

        #[tokio::test]
        async fn rejects_invalid_file_metadata_without_creating_asset() -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut connection = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql"))
                    .execute(connection.handle().conn())
                    .await?;
                let service = FileAssetService::new(config())?;
                let result = service
                    .prepare_upload(
                        &mut connection.handle(),
                        UploadRequest {
                            uploaded_by: 1,
                            original_filename: "large.txt".into(),
                            size_bytes: 1025,
                            content_type: "text/plain".into(),
                        },
                    )
                    .await;
                assert_eq!(result.err().unwrap().code, Code::InvalidRequest);
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conbira_file_assets")
                    .fetch_one(connection.handle().conn())
                    .await?;
                assert_eq!(count, 0);
                Ok(())
            })
            .await
        }
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::app::file_asset::config::S3Config;

    fn service() -> crate::Result<FileAssetService> {
        FileAssetService::new(Config {
            s3: S3Config {
                access_key_id: "access".into(),
                secret_access_key: "secret".into(),
                endpoint: Some("http://127.0.0.1:9000".into()),
                region: "us-east-1".into(),
                bucket: "configured-bucket".into(),
                key_prefix: "uploads".into(),
            },
            max_size_bytes: 1024,
            upload_url_ttl_secs: 600,
            download_url_ttl_secs: 180,
            pending_ttl_secs: 86400,
            deleted_retention_secs: 604800,
            allowed_content_types: vec![],
        })
    }

    mod contract {
        use super::*;

        #[tokio::test]
        async fn signs_download_from_stored_location_and_rejects_deleted_assets()
        -> anyhow::Result<()> {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('writer') RETURNING id").fetch_one(conn.handle().conn()).await?;
                let id = Uuid::new_v4();
                sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type) VALUES ($1, 'attached', $2, 'original-bucket', 'path/file', '한글.txt', 5, 'text/plain')").bind(id).bind(user).execute(conn.handle().conn()).await?;
                let service = service()?;
                let ticket = service.prepare_download(&mut conn.handle(), id).await?;
                let url = url::Url::parse(&ticket.download_url)?;
                let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
                assert_eq!(url.path(), "/original-bucket/path/file");
                assert_eq!(query.get("X-Amz-Expires").map(String::as_str), Some("180"));
                assert_eq!(query.get("response-content-disposition").map(String::as_str), Some("attachment; filename*=UTF-8''%ED%95%9C%EA%B8%80%2Etxt"));
                assert!(service.change_status(&mut conn.handle(), id, FileAssetStatus::Attached, FileAssetStatus::Deleted).await?);
                assert_eq!(service.prepare_download(&mut conn.handle(), id).await.err().unwrap().code, Code::NotFound);
                assert_eq!(service.prepare_download(&mut conn.handle(), Uuid::new_v4()).await.err().unwrap().code, Code::NotFound);
                Ok(())
            }).await
        }

        #[tokio::test]
        async fn updates_only_expected_status_and_maintains_deleted_timestamp() -> anyhow::Result<()>
        {
            crate::test_support::TestContext::run(2, |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('writer') RETURNING id").fetch_one(conn.handle().conn()).await?;
                let id = Uuid::new_v4();
                sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type) VALUES ($1, 'pending', $2, 'bucket', 'key', 'name', 0, 'text/plain')").bind(id).bind(user).execute(conn.handle().conn()).await?;
                let (service, _server) = context.file_asset_service(0).await?;
                assert!(!service.change_status(&mut conn.handle(), id, FileAssetStatus::Attached, FileAssetStatus::Deleted).await?);
                assert!(service.change_status(&mut conn.handle(), id, FileAssetStatus::Pending, FileAssetStatus::Attached).await?);
                assert!(service.change_status(&mut conn.handle(), id, FileAssetStatus::Attached, FileAssetStatus::Deleted).await?);
                let deleted: bool = sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM conbira_file_assets WHERE id=$1").bind(id).fetch_one(conn.handle().conn()).await?;
                assert!(deleted);
                assert!(service.change_status(&mut conn.handle(), id, FileAssetStatus::Deleted, FileAssetStatus::Attached).await?);
                let cleared: bool = sqlx::query_scalar("SELECT deleted_at IS NULL FROM conbira_file_assets WHERE id=$1").bind(id).fetch_one(conn.handle().conn()).await?;
                assert!(cleared);
                Ok(())
            }).await
        }
    }
}

#[cfg(test)]
mod cleanup_tests {
    mod contract {
        use axum::Router;
        use axum::http::StatusCode;
        use axum::routing::delete;

        use crate::app::file_asset::config::{Config, S3Config};
        use crate::app::file_asset::service::FileAssetService;

        #[tokio::test]
        async fn removes_only_expired_assets_and_keeps_failed_s3_deletions() -> anyhow::Result<()> {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            let server = tokio::spawn(async move {
                let router =
                    Router::new().route(
                        "/{bucket}/{key}",
                        delete(
                            |axum::extract::Path((_, key)): axum::extract::Path<(
                                String,
                                String,
                            )>| async move {
                                if key == "fail" {
                                    StatusCode::INTERNAL_SERVER_ERROR
                                } else {
                                    StatusCode::NO_CONTENT
                                }
                            },
                        ),
                    );
                axum::serve(listener, router).await.unwrap();
            });
            let result = crate::test_support::TestContext::run(2, move |context| async move {
                let mut conn = context.db.conn().await?;
                sqlx::raw_sql(include_str!("../../../migrations/0014_file_asset.sql")).execute(conn.handle().conn()).await?;
                let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('cleanup') RETURNING id").fetch_one(conn.handle().conn()).await?;
                let old_pending = uuid::Uuid::new_v4();
                let old_deleted = uuid::Uuid::new_v4();
                let fresh = uuid::Uuid::new_v4();
                let attached = uuid::Uuid::new_v4();
                let failed = uuid::Uuid::new_v4();
                for (id, status, key, old) in [(old_pending, "pending", "pending", true), (old_deleted, "deleted", "deleted", true), (fresh, "pending", "fresh", false), (attached, "attached", "attached", true), (failed, "deleted", "fail", true)] {
                    sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type, created_at, deleted_at) VALUES ($1, $2, $3, 'bucket', $4, 'file.txt', 1, 'text/plain', CURRENT_TIMESTAMP - CASE WHEN $5 THEN INTERVAL '2 days' ELSE INTERVAL '0 seconds' END, CASE WHEN $2 = 'deleted' THEN CURRENT_TIMESTAMP - INTERVAL '2 days' ELSE NULL END)").bind(id).bind(status).bind(user).bind(key).bind(old).execute(conn.handle().conn()).await?;
                }
                drop(conn);
                let service = FileAssetService::new(Config { s3: S3Config { access_key_id: "access".into(), secret_access_key: "secret".into(), endpoint: Some(format!("http://{address}")), region: "us-east-1".into(), bucket: "bucket".into(), key_prefix: "uploads".into() }, max_size_bytes: 1024, upload_url_ttl_secs: 600, download_url_ttl_secs: 180, pending_ttl_secs: 86400, deleted_retention_secs: 86400, allowed_content_types: vec![] })?;
                service.cleanup(&context.db).await?;
                let mut conn = context.db.conn().await?;
                let remaining: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM conbira_file_assets").fetch_all(conn.handle().conn()).await?;
                assert_eq!(remaining.len(), 3);
                assert!(remaining.contains(&fresh) && remaining.contains(&attached) && remaining.contains(&failed));
                Ok(())
            }).await;
            server.abort();
            result
        }
    }
}
