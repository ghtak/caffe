pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::auth::entity::UserStatus;
    use crate::app::http::extract::{ApiJson, ApiPath};
    use crate::app::http::response::{ApiResponse, ApiResult};

    #[derive(Debug, serde::Deserialize, espr_derive::Validate)]
    pub struct RequestBody {
        pub status: UserStatus,
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
        ApiPath(id): ApiPath<i64>,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<()> {
        claims.require_permission("auth.user.manage", crate::here!())?;
        super::usecase::execute(
            &container.db,
            super::usecase::Command::new(id, request.status),
        )
        .await?;
        Ok(ApiResponse::empty())
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::auth::entity::UserStatus;
    use crate::app::auth::repository::user;
    use crate::app::container::db::Database;

    pub struct Command {
        pub id: i64,
        pub status: UserStatus,
    }

    impl Command {
        pub fn new(id: i64, status: UserStatus) -> Self { Self { id, status } }
    }

    pub async fn execute(db: &Database, command: Command) -> crate::Result<()> {
        let mut connection = db.acquire_conn(crate::here!()).await?;
        if !user::update_status(&mut connection.handle(), command.id, command.status).await? {
            return Err(
                crate::Error::new(crate::Code::NotFound, "User not found").at(crate::here!())
            );
        }
        Ok(())
    }
}
