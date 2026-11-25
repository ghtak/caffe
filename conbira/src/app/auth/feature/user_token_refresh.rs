pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::ApiResult;

    #[derive(serde::Deserialize)]
    pub struct RequestBody {
        refresh_token: String,
    }
    impl espr::validate::Validate for RequestBody {
        type Error = crate::Error;
        fn validate(&self) -> crate::Result<()> {
            if self.refresh_token.is_empty() {
                return Err(crate::Error::new(
                    crate::Code::InvalidRequest,
                    "Refresh token is required",
                )
                .at(crate::here!()));
            }
            Ok(())
        }
    }
    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<crate::app::auth::service::login_tokens::LoginTokens> {
        Ok(super::usecase::execute(
            &container.db,
            &container.jwt_token_service,
            &container.refresh_token_service,
            &request.refresh_token,
        )
        .await?
        .into())
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::auth::entity::UserStatus;
    use crate::app::auth::repository::user;
    use crate::app::auth::service::login_tokens;
    use crate::app::auth::service::refresh_token::{RefreshTokenService, RotateOutcome};
    use crate::app::container::db::Database;
    use crate::error::ResultExt as _;
    pub async fn execute(
        db: &Database,
        jwt_token_service: &espr::jwt::TokenService,
        refresh_token_service: &RefreshTokenService,
        refresh_token: &str,
    ) -> crate::Result<login_tokens::LoginTokens> {
        let unauthorized = || {
            crate::Error::new(crate::Code::Unauthorized, "Invalid refresh token").at(crate::here!())
        };
        let mut tx = db.begin_tx(crate::here!()).await?;
        let user_id = refresh_token_service
            .find_user_id(&mut tx.handle(), refresh_token)
            .await?
            .ok_or_else(unauthorized)?;
        let outcome = refresh_token_service
            .rotate(&mut tx.handle(), refresh_token)
            .await?;
        let issued = match outcome {
            RotateOutcome::Rotated(issued) => issued,
            _ => {
                tx.commit()
                    .await
                    .map_err_internal("Failed to commit refresh", crate::here!())?;
                return Err(unauthorized());
            }
        };
        let mut handle = tx.handle();
        let user = user::find_by_id(&mut handle, user_id)
            .await?
            .ok_or_else(unauthorized)?;
        if !matches!(user.status, UserStatus::Registered | UserStatus::Active) {
            refresh_token_service
                .revoke(&mut handle, issued.family_id, "user_inactive")
                .await?;
            tx.commit()
                .await
                .map_err_internal("Failed to commit refresh", crate::here!())?;
            return Err(unauthorized());
        }
        let tokens =
            login_tokens::issue_tokens(&mut handle, jwt_token_service, &user, issued, None).await?;
        tx.commit()
            .await
            .map_err_internal("Failed to commit refresh", crate::here!())?;
        Ok(tokens)
    }
}
