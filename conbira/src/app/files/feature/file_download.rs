pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use uuid::Uuid;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::ApiResult;

    pub type ResponseData = super::usecase::Output;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiPath(file_id): ApiPath<Uuid>,
    ) -> ApiResult<ResponseData> {
        Ok(super::usecase::execute(
            &container.db,
            container.file_storage.as_ref(),
            container.file_resource_access.as_ref(),
            &container.config.files,
            super::usecase::Query { caller, file_id },
        )
        .await?
        .into())
    }
}

pub mod usecase {
    use uuid::Uuid;

    use crate::app::DatabaseExt as _;
    use crate::app::auth::claims::UserClaims;
    use crate::app::container::db::Database;
    use crate::app::files::access::{self, Action, ResourceAccessPort};
    use crate::app::files::entity::FileStatus;
    use crate::app::files::s3::{ObjectHead, ObjectLocation, PresignGet, S3ObjectPort};
    use crate::app::files::{Config, repository};
    use crate::{Code, Error, Result};

    pub struct Query {
        pub caller: UserClaims,
        pub file_id: Uuid,
    }

    #[derive(serde::Serialize)]
    pub struct Output {
        pub url: String,
        pub expires_at: jiff::Timestamp,
    }

    pub async fn execute(
        db: &Database,
        storage: &dyn S3ObjectPort,
        policy: &dyn ResourceAccessPort,
        config: &Config,
        query: Query,
    ) -> Result<Output> {
        let mut connection = db.acquire_conn(crate::here!()).await?;
        let file = repository::find_by_id(&mut connection.handle(), query.file_id)
            .await?
            .filter(|file| file.status != FileStatus::Deleted)
            .ok_or_else(|| Error::new(Code::NotFound, "File not found").at(crate::here!()))?;
        drop(connection);
        let allowed = access::file(policy, &query.caller, &file, Action::Read).await?;
        if !allowed {
            return Err(Error::new(Code::Forbidden, "File access denied").at(crate::here!()));
        }
        let location = ObjectLocation {
            bucket: file.s3_bucket,
            key: file.s3_object_key,
        };
        if matches!(storage.head(&location).await?, ObjectHead::Missing) {
            return Err(Error::new(Code::NotFound, "File object not found").at(crate::here!()));
        }
        let signed = storage
            .presign_get(PresignGet {
                location,
                original_filename: file.original_filename,
                ttl_seconds: config.download_url_ttl_seconds,
                now: jiff::Timestamp::now(),
            })
            .await?;
        Ok(Output {
            url: signed.url,
            expires_at: signed.expires_at,
        })
    }
}
