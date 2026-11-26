//! File metadata persistence. The caller owns transaction boundaries.

use espr::validate::Validate as _;
use uuid::Uuid;

use crate::Result;
use crate::app::container::db::Handle;
use crate::app::files::entity::{File, FileStatus};
use crate::error::ResultExt as _;

// Nullable timestamps follow auth::repository::refresh_token's row mapping.
#[derive(sqlx::FromRow)]
struct FileRow {
    file_id: Uuid,
    group_id: Uuid,
    resource_type: Option<String>,
    resource_key: Option<String>,
    status: FileStatus,
    uploaded_by: i64,
    s3_bucket: String,
    s3_object_key: String,
    original_filename: String,
    size_bytes: i64,
    content_type: String,
    created_at: jiff_sqlx::Timestamp,
    updated_at: jiff_sqlx::Timestamp,
    deleted_at: Option<jiff_sqlx::Timestamp>,
}

impl From<FileRow> for File {
    fn from(row: FileRow) -> Self {
        Self {
            file_id: row.file_id,
            group_id: row.group_id,
            resource_type: row.resource_type,
            resource_key: row.resource_key,
            status: row.status,
            uploaded_by: row.uploaded_by,
            s3_bucket: row.s3_bucket,
            s3_object_key: row.s3_object_key,
            original_filename: row.original_filename,
            size_bytes: row.size_bytes,
            content_type: row.content_type,
            created_at: row.created_at.into(),
            updated_at: row.updated_at.into(),
            deleted_at: row.deleted_at.map(Into::into),
        }
    }
}

/// Group history projection, including deleted files and their uploaders.
/// find_group returns None only when the group has never been issued.
#[derive(Debug, sqlx::FromRow)]
pub struct GroupHistory {
    pub group_id: Uuid,
    pub resource_type: Option<String>,
    pub resource_key: Option<String>,
    pub uploaded_by: Vec<i64>,
}

pub async fn find_group(handle: &mut Handle<'_>, group_id: Uuid) -> Result<Option<GroupHistory>> {
    let files = list_by_group_id(handle, group_id).await?;
    if files.is_empty() {
        return Ok(None);
    }
    let mut history = GroupHistory {
        group_id,
        resource_type: None,
        resource_key: None,
        uploaded_by: Vec::new(),
    };
    for file in files {
        file.validate().map_err(|cause| {
            crate::Error::internal("Invalid stored file metadata")
                .at(crate::here!())
                .cause(cause)
        })?;
        if let (Some(kind), Some(key)) = (file.resource_type, file.resource_key) {
            if history.resource_type.is_some()
                && (history.resource_type.as_ref() != Some(&kind)
                    || history.resource_key.as_ref() != Some(&key))
            {
                return Err(
                    crate::Error::internal("Conflicting file group bindings").at(crate::here!())
                );
            }
            history.resource_type = Some(kind);
            history.resource_key = Some(key);
        }
        history.uploaded_by.push(file.uploaded_by);
    }
    history.uploaded_by.sort_unstable();
    history.uploaded_by.dedup();
    Ok(Some(history))
}

pub async fn mark_attached(
    handle: &mut Handle<'_>,
    group_id: Uuid,
    file_ids: &[Uuid],
    resource_type: &str,
    resource_key: &str,
    now: jiff::Timestamp,
) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE conbira_files SET status='attached', resource_type=$3, resource_key=$4, updated_at=$5
         WHERE group_id=$1 AND file_id=ANY($2) AND status='pending'",
    )
    .bind(group_id).bind(file_ids).bind(resource_type).bind(resource_key)
    .bind(jiff_sqlx::Timestamp::from(now))
    .execute(handle.conn()).await
    .map_err_internal("Failed to attach files", crate::here!())?;
    Ok(result.rows_affected())
}

pub async fn mark_detached(
    handle: &mut Handle<'_>,
    group_id: Uuid,
    file_ids: &[Uuid],
    resource_type: &str,
    resource_key: &str,
    now: jiff::Timestamp,
) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE conbira_files SET status='deleted', updated_at=$5, deleted_at=$5
         WHERE group_id=$1 AND file_id=ANY($2) AND status='attached'
         AND resource_type=$3 AND resource_key=$4",
    )
    .bind(group_id)
    .bind(file_ids)
    .bind(resource_type)
    .bind(resource_key)
    .bind(jiff_sqlx::Timestamp::from(now))
    .execute(handle.conn())
    .await
    .map_err_internal("Failed to detach files", crate::here!())?;
    Ok(result.rows_affected())
}

pub async fn find_by_id(handle: &mut Handle<'_>, file_id: Uuid) -> Result<Option<File>> {
    sqlx::query_as::<_, FileRow>("SELECT * FROM conbira_files WHERE file_id = $1")
        .bind(file_id)
        .fetch_optional(handle.conn())
        .await
        .map(|row| row.map(Into::into))
        .map_err_internal("Failed to find file", crate::here!())
}

/// Administrative metadata list. Storage location and uploader are not exposed.
pub async fn list(
    handle: &mut Handle<'_>,
    name: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<File>> {
    sqlx::query_as::<_, FileRow>(
        "SELECT * FROM conbira_files WHERE status <> 'deleted'
         AND strpos(lower(original_filename), lower($1)) > 0
         ORDER BY created_at DESC, file_id DESC LIMIT $2 OFFSET $3",
    )
    .bind(name)
    .bind(limit)
    .bind(offset)
    .fetch_all(handle.conn())
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err_internal("Failed to list files", crate::here!())
}

pub async fn list_by_group_id(handle: &mut Handle<'_>, group_id: Uuid) -> Result<Vec<File>> {
    sqlx::query_as::<_, FileRow>(
        "SELECT * FROM conbira_files WHERE group_id = $1 ORDER BY created_at, file_id",
    )
    .bind(group_id)
    .fetch_all(handle.conn())
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err_internal("Failed to list file group history", crate::here!())
}

pub async fn insert(handle: &mut Handle<'_>, file: &File) -> Result<File> {
    sqlx::query_as::<_, FileRow>(
        "INSERT INTO conbira_files
         (file_id, group_id, resource_type, resource_key, status, uploaded_by, s3_bucket,
          s3_object_key, original_filename, size_bytes, content_type, created_at, updated_at, deleted_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING *")
        .bind(file.file_id).bind(file.group_id).bind(&file.resource_type).bind(&file.resource_key)
        .bind(file.status).bind(file.uploaded_by).bind(&file.s3_bucket).bind(&file.s3_object_key)
        .bind(&file.original_filename).bind(file.size_bytes).bind(&file.content_type)
        .bind(jiff_sqlx::Timestamp::from(file.created_at))
        .bind(jiff_sqlx::Timestamp::from(file.updated_at))
        .bind(file.deleted_at.map(jiff_sqlx::Timestamp::from))
        .fetch_one(handle.conn()).await.map(Into::into)
        .map_err_internal("Failed to insert file", crate::here!())
}

/// Lock this file while the caller authorizes and marks it deleted.
pub async fn find_by_id_for_update(handle: &mut Handle<'_>, file_id: Uuid) -> Result<Option<File>> {
    sqlx::query_as::<_, FileRow>("SELECT * FROM conbira_files WHERE file_id = $1 FOR UPDATE")
        .bind(file_id)
        .fetch_optional(handle.conn())
        .await
        .map(|row| row.map(Into::into))
        .map_err_internal("Failed to find file for deletion", crate::here!())
}

/// The caller holds the file row lock and owns commit/rollback.
pub async fn mark_deleted(
    handle: &mut Handle<'_>,
    file_id: Uuid,
    now: jiff::Timestamp,
) -> Result<()> {
    sqlx::query("UPDATE conbira_files SET status = 'deleted', updated_at = $2, deleted_at = $2 WHERE file_id = $1")
        .bind(file_id).bind(jiff_sqlx::Timestamp::from(now))
        .execute(handle.conn()).await
        .map_err_internal("Failed to delete file metadata", crate::here!())?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/repository.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/registration.rs"]
mod registration_tests;
