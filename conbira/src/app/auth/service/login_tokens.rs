use crate::app::DatabaseExt as _;
use crate::app::auth::claims::UserClaims;
use crate::app::auth::entity::{User, UserStatus};
use crate::app::auth::repository::user;
use crate::app::auth::service::refresh_token::RefreshTokenService;
use crate::app::container::db::{Database, Handle};
use crate::app::rbac::repository::{permission, user_role};
use crate::error::ResultExt as _;

#[derive(serde::Serialize)]
pub struct LoginTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub claims: UserClaims,
}

pub async fn issue(
    db: &Database,
    jwt_token_service: &espr::jwt::TokenService,
    refresh_token_service: &RefreshTokenService,
    user_id: i64,
) -> crate::Result<LoginTokens> {
    let user = {
        let mut connection = db.acquire_conn(crate::here!()).await?;
        let mut handle = connection.handle();
        let user = user::find_by_id(&mut handle, user_id)
            .await?
            .ok_or_else(|| {
                crate::Error::internal("User for identity not found").at(crate::here!())
            })?;
        if !matches!(user.status, UserStatus::Registered | UserStatus::Active) {
            return Err(crate::Error::new(
                crate::Code::Unauthorized,
                "Invalid credentials or user status",
            )
            .at(crate::here!()));
        }
        user
    };
    let tokens = {
        let mut tx = db.begin_tx(crate::here!()).await?;
        let refresh = refresh_token_service
            .issue(&mut tx.handle(), user.id)
            .await?;
        let tokens =
            issue_tokens(&mut tx.handle(), jwt_token_service, &user, refresh.token).await?;
        tx.commit()
            .await
            .map_err_internal("Failed to commit login", crate::here!())?;
        tokens
    };
    Ok(tokens)
}

pub async fn issue_tokens(
    handle: &mut Handle<'_>,
    jwt_token_service: &espr::jwt::TokenService,
    user: &User,
    refresh_token: String,
) -> crate::Result<LoginTokens> {
    let roles = user_role::list_by_user_id(handle, user.id)
        .await?
        .into_iter()
        .map(|role| role.code)
        .collect();
    let permissions = permission::find_by_user_id(handle, user.id)
        .await?
        .into_iter()
        .map(|permission| permission.code)
        .collect();
    let claims = UserClaims::new(user.id, user.status, roles, permissions);
    let issued = jwt_token_service
        .issue(&user.id.to_string(), claims.clone().try_into()?)
        .map_err_internal("Failed to issue access token", crate::here!())?;
    Ok(LoginTokens {
        access_token: issued.token,
        refresh_token,
        expires_at: issued.expires_at.as_second(),
        claims,
    })
}
