pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::ApiResult;

    #[derive(serde::Deserialize)]
    pub struct RequestBody {
        pub(crate) refresh_token: String,
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
    use crate::app::auth::service::refresh_token::RefreshTokenService;
    use crate::app::container::db::Database;
    use crate::error::ResultExt as _;
    fn renewal_required(remaining: jiff::SignedDuration) -> Option<bool> {
        (remaining > jiff::SignedDuration::ZERO)
            .then_some(remaining <= jiff::SignedDuration::from_secs(72 * 60 * 60))
    }

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
        let mut handle = tx.handle();
        let record = refresh_token_service
            .find_valid(&mut handle, refresh_token)
            .await?
            .ok_or_else(unauthorized)?;
        let user = user::find_by_id(&mut handle, record.user_id)
            .await?
            .ok_or_else(unauthorized)?;
        if !matches!(user.status, UserStatus::Registered | UserStatus::Active) {
            return Err(unauthorized());
        }
        let now = jiff::Timestamp::now();
        let remaining = record.expires_at.duration_since(now);
        let renew = renewal_required(remaining).ok_or_else(unauthorized)?;
        let issued = if renew {
            // 기존 토큰은 유지하고 신규 토큰을 전달
            refresh_token_service
                .issue(&mut handle, user.id)
                .await?
                .token
        } else {
            refresh_token.to_owned()
        };
        let tokens =
            login_tokens::issue_tokens(&mut handle, jwt_token_service, &user, issued).await?;
        tx.commit()
            .await
            .map_err_internal("Failed to commit refresh", crate::here!())?;
        Ok(tokens)
    }

    #[cfg(test)]
    mod tests {
        mod contract {
            use super::super::renewal_required;
            #[test]
            fn renewal_boundary_is_inclusive_and_expired_is_rejected() {
                let boundary = jiff::SignedDuration::from_secs(72 * 60 * 60);
                assert_eq!(
                    renewal_required(boundary + jiff::SignedDuration::from_nanos(1)),
                    Some(false)
                );
                assert_eq!(renewal_required(boundary), Some(true));
                assert_eq!(
                    renewal_required(jiff::SignedDuration::from_nanos(1)),
                    Some(true)
                );
                assert_eq!(renewal_required(jiff::SignedDuration::ZERO), None);
            }
        }
    }
}
