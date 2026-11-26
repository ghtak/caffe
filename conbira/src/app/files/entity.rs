use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum FileStatus {
    Pending,
    Attached,
    Deleted,
}

/// Persisted row. Application validation owns metadata and state consistency.
#[derive(Debug, PartialEq)]
pub struct File {
    pub file_id: Uuid,
    pub group_id: Uuid,
    pub resource_type: Option<String>,
    pub resource_key: Option<String>,
    pub status: FileStatus,
    pub uploaded_by: i64,
    pub s3_bucket: String,
    pub s3_object_key: String,
    pub original_filename: String,
    pub size_bytes: i64,
    pub content_type: String,
    pub created_at: jiff::Timestamp,
    pub updated_at: jiff::Timestamp,
    pub deleted_at: Option<jiff::Timestamp>,
}

impl espr::validate::Validate for File {
    type Error = crate::Error;

    fn validate(&self) -> crate::Result<()> {
        let binding_valid = match (&self.resource_type, &self.resource_key) {
            (None, None) => true,
            (Some(kind), Some(key)) => !kind.trim().is_empty() && !key.trim().is_empty(),
            _ => false,
        };
        if self.size_bytes < 0
            || [
                &self.s3_bucket,
                &self.s3_object_key,
                &self.original_filename,
                &self.content_type,
            ]
            .iter()
            .any(|value| value.trim().is_empty())
            || !binding_valid
            || (self.status == FileStatus::Attached && self.resource_type.is_none())
            || ((self.status == FileStatus::Deleted) != self.deleted_at.is_some())
        {
            return Err(
                crate::Error::new(crate::Code::InvalidRequest, "Invalid file metadata")
                    .at(crate::here!()),
            );
        }
        Ok(())
    }
}

/// Public file metadata. Storage location, binding, and uploader remain
/// internal.
#[derive(Debug, serde::Serialize)]
pub struct FileSummary {
    pub file_id: Uuid,
    pub group_id: Uuid,
    pub status: FileStatus,
    pub original_filename: String,
    pub size_bytes: i64,
    pub content_type: String,
    pub created_at: jiff::Timestamp,
}

impl From<File> for FileSummary {
    fn from(file: File) -> Self {
        Self {
            file_id: file.file_id,
            group_id: file.group_id,
            status: file.status,
            original_filename: file.original_filename,
            size_bytes: file.size_bytes,
            content_type: file.content_type,
            created_at: file.created_at,
        }
    }
}

#[cfg(test)]
#[path = "tests/entity.rs"]
mod tests;
