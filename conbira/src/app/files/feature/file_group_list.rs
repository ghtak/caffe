pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use uuid::Uuid;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::files::entity::FileSummary;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::ApiResult;

    pub type ResponseData = Vec<FileSummary>;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiPath(group_id): ApiPath<Uuid>,
    ) -> ApiResult<ResponseData> {
        Ok(super::usecase::execute(
            &container.db,
            container.file_resource_access.as_ref(),
            super::usecase::Query { caller, group_id },
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
    use crate::app::files::entity::{FileStatus, FileSummary};
    use crate::app::files::repository;
    use crate::{Code, Error, Result};

    pub struct Query {
        pub caller: UserClaims,
        pub group_id: Uuid,
    }

    pub async fn execute(
        db: &Database,
        policy: &dyn ResourceAccessPort,
        query: Query,
    ) -> Result<Vec<FileSummary>> {
        let mut connection = db.acquire_conn(crate::here!()).await?;
        let history =
            repository::list_by_group_id(&mut connection.handle(), query.group_id).await?;
        drop(connection);
        if history.is_empty() {
            return Err(Error::new(Code::NotFound, "File group not found").at(crate::here!()));
        }
        let mut visible = Vec::new();
        for file in history {
            let allowed = file.status != FileStatus::Deleted
                && access::file(policy, &query.caller, &file, Action::Read).await?;
            if allowed {
                visible.push(FileSummary::from(file));
            }
        }
        Ok(visible)
    }
}
