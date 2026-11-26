//! File managers can access all files; other callers use ownership and resource
//! policies.
use crate::app::auth::claims::UserClaims;
use crate::app::files::entity::{File, FileStatus};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Read,
    Edit,
}

#[async_trait::async_trait]
pub trait ResourceAccessPort: Send + Sync {
    /// Missing/unsupported resources return false; policy outages return Busy.
    async fn decide(
        &self,
        caller: &UserClaims,
        resource_type: &str,
        resource_key: &str,
        action: Action,
    ) -> Result<bool>;
}

/// Static composition point for domain policies. No content domains are
/// connected yet.
pub struct DomainResourceAccess;

#[async_trait::async_trait]
impl ResourceAccessPort for DomainResourceAccess {
    async fn decide(&self, _: &UserClaims, _: &str, _: &str, _: Action) -> Result<bool> {
        Ok(false)
    }
}

pub(super) async fn bound(
    policy: &dyn ResourceAccessPort,
    caller: &UserClaims,
    resource_type: Option<&str>,
    resource_key: Option<&str>,
    action: Action,
) -> Result<bool> {
    if caller.has_permission("file.manage") {
        return Ok(true);
    }
    match (resource_type, resource_key) {
        (Some(kind), Some(key)) => policy.decide(caller, kind, key, action).await,
        (None, None) => Ok(false),
        _ => Err(Error::internal("Invalid file binding").at(crate::here!())),
    }
}

/// File management permission takes precedence over uploader and resource
/// access.
pub(super) async fn file(
    policy: &dyn ResourceAccessPort,
    caller: &UserClaims,
    file: &File,
    action: Action,
) -> Result<bool> {
    if caller.has_permission("file.manage") {
        return Ok(true);
    }
    match file.status {
        FileStatus::Pending => Ok(file.uploaded_by == caller.id),
        FileStatus::Deleted if file.uploaded_by == caller.id => Ok(true),
        FileStatus::Attached | FileStatus::Deleted => {
            bound(
                policy,
                caller,
                file.resource_type.as_deref(),
                file.resource_key.as_deref(),
                action,
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    mod contract {
        use uuid::Uuid;

        use super::super::*;
        use crate::app::auth::entity::UserStatus;

        fn pending() -> File {
            File {
                file_id: Uuid::new_v4(),
                group_id: Uuid::new_v4(),
                resource_type: None,
                resource_key: None,
                status: FileStatus::Pending,
                uploaded_by: 10,
                s3_bucket: "bucket".into(),
                s3_object_key: "key".into(),
                original_filename: "report.txt".into(),
                size_bytes: 5,
                content_type: "text/plain".into(),
                created_at: jiff::Timestamp::now(),
                updated_at: jiff::Timestamp::now(),
                deleted_at: None,
            }
        }

        #[tokio::test]
        async fn manager_can_access_foreign_pending_and_attached_files() -> crate::Result<()> {
            let manager = UserClaims::new(20, UserStatus::Active, vec!["file.manage".into()]);
            let mut candidate = pending();
            // 관리자는 업로더나 리소스 정책 허용 여부와 관계없이 파일을
            // 관리한다.
            assert!(file(&DomainResourceAccess, &manager, &candidate, Action::Read).await?);
            assert!(file(&DomainResourceAccess, &manager, &candidate, Action::Edit).await?);
            candidate.status = FileStatus::Attached;
            candidate.resource_type = Some("post".into());
            candidate.resource_key = Some("42".into());
            assert!(file(&DomainResourceAccess, &manager, &candidate, Action::Read).await?);
            assert!(file(&DomainResourceAccess, &manager, &candidate, Action::Edit).await?);
            Ok(())
        }

        #[tokio::test]
        async fn list_permission_does_not_grant_file_access() -> crate::Result<()> {
            let reader = UserClaims::new(20, UserStatus::Active, vec!["file.list.read".into()]);
            let candidate = pending();
            // 목록 조회 권한은 타인의 파일 다운로드·수정 권한을 포함하지
            // 않는다.
            assert!(!file(&DomainResourceAccess, &reader, &candidate, Action::Read).await?);
            assert!(!file(&DomainResourceAccess, &reader, &candidate, Action::Edit).await?);
            Ok(())
        }
    }
}
