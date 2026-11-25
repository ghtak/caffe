pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::http::response::{ApiResponse, ApiResult};

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
    ) -> ApiResult<()> {
        super::usecase::execute(&container.db, &container.refresh_token_service, claims).await?;
        Ok(ApiResponse::empty())
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::auth::claims::UserClaims;
    use crate::app::auth::service::refresh_token::RefreshTokenService;
    use crate::app::container::db::Database;
    use crate::error::ResultExt as _;

    pub async fn execute(
        db: &Database,
        refresh_token_service: &RefreshTokenService,
        claims: UserClaims,
    ) -> crate::Result<()> {
        let mut tx = db.begin_tx(crate::here!()).await?;
        refresh_token_service
            .revoke(&mut tx.handle(), claims.family_id, "logout")
            .await?;
        tx.commit()
            .await
            .map_err_internal("Failed to commit logout", crate::here!())?;
        Ok(())
    }
}
