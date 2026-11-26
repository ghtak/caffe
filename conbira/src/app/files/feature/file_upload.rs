pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::{Multipart, State};
    use axum::http::StatusCode;
    use tokio::io::{AsyncSeekExt as _, AsyncWriteExt as _};
    use uuid::Uuid;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::files::entity::FileSummary;
    use crate::app::http::response::{ApiResponse, ApiResult};
    use crate::error::ResultExt as _;
    use crate::{Code, Error};

    pub type ResponseData = Vec<FileSummary>;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        multipart: Result<Multipart, axum::extract::multipart::MultipartRejection>,
    ) -> ApiResult<ResponseData> {
        let invalid =
            |at| Error::new(Code::InvalidRequest, "Invalid file multipart request").at(at);
        let mut multipart = multipart.map_err(|_| invalid(crate::here!()))?;
        let mut group_id = None;
        let mut files = Vec::new();
        while let Some(mut field) = multipart
            .next_field()
            .await
            .map_err(|_| invalid(crate::here!()))?
        {
            match field.name() {
                Some("file") => {
                    if files.len() >= container.config.files.max_files_per_upload {
                        return Err(Error::new(Code::InvalidRequest, "Too many files")
                            .at(crate::here!())
                            .into());
                    }
                    let original_filename = field
                        .file_name()
                        .filter(|name| !name.trim().is_empty())
                        .ok_or_else(|| invalid(crate::here!()))?
                        .to_owned();
                    let content_type = field
                        .content_type()
                        .filter(|value| value.parse::<mime::Mime>().is_ok())
                        .ok_or_else(|| invalid(crate::here!()))?
                        .to_owned();
                    let file = tempfile::tempfile()
                        .map_err_internal("Failed to create upload spool", crate::here!())?;
                    let mut spool = tokio::fs::File::from_std(file);
                    let mut size = 0u64;
                    while let Some(chunk) =
                        field.chunk().await.map_err(|_| invalid(crate::here!()))?
                    {
                        size += chunk.len() as u64;
                        if size > container.config.files.max_file_size_bytes {
                            return Err(ApiResponse::from(
                                Error::new(Code::InvalidRequest, "File too large")
                                    .at(crate::here!()),
                            )
                            .with_status(StatusCode::PAYLOAD_TOO_LARGE));
                        }
                        spool
                            .write_all(&chunk)
                            .await
                            .map_err_internal("Failed to spool upload", crate::here!())?;
                    }
                    spool
                        .rewind()
                        .await
                        .map_err_internal("Failed to rewind upload", crate::here!())?;
                    files.push(super::usecase::UploadFile {
                        original_filename,
                        content_type,
                        body: Box::pin(spool),
                    });
                }
                Some("group_id") if group_id.is_none() => {
                    let mut value = Vec::new();
                    while let Some(chunk) =
                        field.chunk().await.map_err(|_| invalid(crate::here!()))?
                    {
                        if value.len() + chunk.len() > 36 {
                            return Err(invalid(crate::here!()).into());
                        }
                        value.extend_from_slice(&chunk);
                    }
                    group_id =
                        Some(Uuid::try_parse_ascii(&value).map_err(|_| invalid(crate::here!()))?);
                }
                _ => return Err(invalid(crate::here!()).into()),
            }
        }
        if files.is_empty() {
            return Err(invalid(crate::here!()).into());
        }
        let file = super::usecase::execute(
            &container.db,
            container.file_storage.as_ref(),
            container.file_resource_access.as_ref(),
            &container.config.files,
            super::usecase::Command {
                caller,
                group_id,
                files,
            },
        )
        .await?;
        Ok(ApiResponse::from(file).with_status(StatusCode::CREATED))
    }
}

pub mod usecase {
    use std::pin::Pin;

    use espr::validate::Validate as _;
    use tokio::io::AsyncRead;
    use uuid::Uuid;

    use crate::app::DatabaseExt as _;
    use crate::app::auth::claims::UserClaims;
    use crate::app::container::db::{Database, Handle};
    use crate::app::files::access::{self, Action, ResourceAccessPort};
    use crate::app::files::entity::{File, FileStatus, FileSummary};
    use crate::app::files::s3::{ObjectLocation, PutObject, S3ObjectPort};
    use crate::app::files::{Config, repository};
    use crate::{Code, Error, Result};

    pub struct Command {
        pub caller: UserClaims,
        pub group_id: Option<Uuid>,
        pub files: Vec<UploadFile>,
    }

    pub struct UploadFile {
        pub original_filename: String,
        pub content_type: String,
        pub body: Pin<Box<dyn AsyncRead + Send>>,
    }

    async fn authorized_group(
        policy: &dyn ResourceAccessPort,
        caller: &UserClaims,
        handle: &mut Handle<'_>,
        group_id: Uuid,
    ) -> Result<repository::GroupHistory> {
        let history = repository::find_group(handle, group_id)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "File group not found").at(crate::here!()))?;
        let allowed = if caller.has_permission("file.manage") {
            true
        } else if history.resource_type.is_none() && history.resource_key.is_none() {
            history.uploaded_by.contains(&caller.id)
        } else {
            access::bound(
                policy,
                caller,
                history.resource_type.as_deref(),
                history.resource_key.as_deref(),
                Action::Edit,
            )
            .await?
        };
        if !allowed {
            return Err(Error::new(Code::Forbidden, "File group access denied").at(crate::here!()));
        }
        Ok(history)
    }

    async fn compensate(storage: &dyn S3ObjectPort, files: &[(File, u64)]) {
        for (file, _) in files {
            let location = ObjectLocation {
                bucket: file.s3_bucket.clone(),
                key: file.s3_object_key.clone(),
            };
            if let Err(error) = storage.delete(&location).await {
                tracing::warn!(file_id = %file.file_id, ?error, "Upload compensation failed");
            }
        }
    }

    pub async fn execute(
        db: &Database,
        storage: &dyn S3ObjectPort,
        policy: &dyn ResourceAccessPort,
        config: &Config,
        command: Command,
    ) -> Result<Vec<FileSummary>> {
        if command.files.is_empty() {
            return Err(Error::new(Code::InvalidRequest, "No files to upload").at(crate::here!()));
        }
        if let Some(group_id) = command.group_id {
            let mut connection = db.acquire_conn(crate::here!()).await?;
            authorized_group(policy, &command.caller, &mut connection.handle(), group_id).await?;
        }
        let group_id = command.group_id.unwrap_or_else(Uuid::new_v4);
        let mut uploaded = Vec::new();
        for upload in command.files {
            let file_id = Uuid::new_v4();
            let location = ObjectLocation {
                bucket: config.s3.bucket.clone(),
                key: format!("{}/{file_id}", config.s3.key_prefix),
            };
            let stored = match storage
                .put(PutObject {
                    location: location.clone(),
                    content_type: upload.content_type.clone(),
                    body: upload.body,
                    max_bytes: config.max_file_size_bytes,
                })
                .await
            {
                Ok(stored) => stored,
                Err(error) => {
                    compensate(storage, &uploaded).await;
                    return Err(error.into());
                }
            };
            let now = jiff::Timestamp::now();
            uploaded.push((
                File {
                    file_id,
                    group_id,
                    resource_type: None,
                    resource_key: None,
                    status: FileStatus::Pending,
                    uploaded_by: command.caller.id,
                    s3_bucket: location.bucket,
                    s3_object_key: location.key,
                    original_filename: upload.original_filename,
                    size_bytes: 0,
                    content_type: upload.content_type,
                    created_at: now,
                    updated_at: now,
                    deleted_at: None,
                },
                stored.size_bytes,
            ));
        }
        let mut tx = match db.begin_tx(crate::here!()).await {
            Ok(tx) => tx,
            Err(error) => {
                compensate(storage, &uploaded).await;
                return Err(error);
            }
        };
        let registration = async {
            let (resource_type, resource_key) = if command.group_id.is_some() {
                let group =
                    authorized_group(policy, &command.caller, &mut tx.handle(), group_id).await?;
                (group.resource_type, group.resource_key)
            } else {
                (None, None)
            };
            let mut files = Vec::with_capacity(uploaded.len());
            for (file, size_bytes) in &mut uploaded {
                file.resource_type = resource_type.clone();
                file.resource_key = resource_key.clone();
                file.size_bytes = i64::try_from(*size_bytes)
                    .map_err(|_| Error::internal("Invalid stored file size").at(crate::here!()))?;
                file.validate()?;
                files.push(repository::insert(&mut tx.handle(), file).await?.into());
            }
            Ok(files)
        }
        .await;
        let files = match registration {
            Ok(files) => files,
            Err(error) => {
                match tx.rollback().await {
                    Ok(()) => compensate(storage, &uploaded).await,
                    Err(cause) => {
                        tracing::warn!(%group_id, %cause, "Upload rollback unknown; retaining objects")
                    }
                }
                return Err(error);
            }
        };
        tx.commit().await.map_err(|cause| {
            tracing::warn!(%group_id, "Upload commit failed; retaining objects for later cleanup");
            Error::internal("File transaction commit failed")
                .at(crate::here!())
                .cause(cause)
        })?;
        Ok(files)
    }
}
