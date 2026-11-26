pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use uuid::Uuid;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::{ApiResponse, ApiResult};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiPath(file_id): ApiPath<Uuid>,
    ) -> ApiResult<()> {
        super::usecase::execute(
            &container.db,
            container.file_storage.as_ref(),
            container.file_resource_access.as_ref(),
            super::usecase::Command { caller, file_id },
        )
        .await?;
        Ok(ApiResponse::empty())
    }
}

pub mod usecase {
    use uuid::Uuid;

    use crate::app::DatabaseExt as _;
    use crate::app::auth::claims::UserClaims;
    use crate::app::container::db::Database;
    use crate::app::files::access::{self, Action, ResourceAccessPort};
    use crate::app::files::entity::FileStatus;
    use crate::app::files::repository;
    use crate::app::files::s3::{ObjectLocation, S3ObjectPort};
    use crate::{Code, Error, Result};

    pub struct Command {
        pub caller: UserClaims,
        pub file_id: Uuid,
    }

    pub async fn execute(
        db: &Database,
        storage: &dyn S3ObjectPort,
        policy: &dyn ResourceAccessPort,
        command: Command,
    ) -> Result<()> {
        let file_id = command.file_id;
        let mut tx = db.begin_tx(crate::here!()).await?;
        let file = repository::find_by_id_for_update(&mut tx.handle(), file_id)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "File not found").at(crate::here!()))?;
        let allowed = access::file(policy, &command.caller, &file, Action::Edit).await?;
        if !allowed {
            return Err(Error::new(Code::Forbidden, "File access denied").at(crate::here!()));
        }
        if file.status == FileStatus::Deleted {
            return Ok(());
        }
        repository::mark_deleted(&mut tx.handle(), file_id, jiff::Timestamp::now()).await?;
        tx.commit().await.map_err(|cause| {
            tracing::warn!(%file_id, "Delete commit failed; retaining object for later cleanup");
            Error::internal("File transaction commit failed")
                .at(crate::here!())
                .cause(cause)
        })?;
        let location = ObjectLocation {
            bucket: file.s3_bucket,
            key: file.s3_object_key,
        };
        if let Err(error) = storage.delete(&location).await {
            tracing::warn!(%file_id, ?error, "Physical deletion failed after logical commit");
        }
        Ok(())
    }
}
