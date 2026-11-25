use crate::app::DatabaseExt as _;
use crate::app::auth::claims::UserClaims;
use crate::app::auth::entity::UserStatus;
use crate::app::auth::repository::user;
use crate::app::auth::service::refresh_token::RefreshTokenService;
use crate::app::container::db::Database;
use crate::app::rbac::repository::{permission, user_role};
use crate::error::ResultExt as _;

#[derive(serde::Serialize)]
pub struct LoginTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub user: crate::app::auth::entity::UserInfo,
}

pub async fn issue(
    db: &Database,
    jwt_token_service: &espr::jwt::TokenService,
    refresh_token_service: &RefreshTokenService,
    user_id: i64,
    email: String,
) -> crate::Result<LoginTokens> {
    let (user, roles, permissions) = {
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
        let roles = user_role::list_by_user_id(&mut handle, user.id)
            .await?
            .into_iter()
            .map(|role| role.code)
            .collect();
        let permissions = permission::find_by_user_id(&mut handle, user.id)
            .await?
            .into_iter()
            .map(|permission| permission.code)
            .collect();
        (user, roles, permissions)
    };
    let claims = UserClaims::new(user.id, user.status, roles, permissions).try_into()?;
    let access = jwt_token_service
        .issue(&user.id.to_string(), claims)
        .map_err_internal("Failed to issue access token", crate::here!())?;

    let refresh = {
        let mut tx = db.begin_tx(crate::here!()).await?;
        let refresh = refresh_token_service
            .issue(&mut tx.handle(), user.id)
            .await?;
        tx.commit()
            .await
            .map_err_internal("Failed to commit login", crate::here!())?;
        refresh
    };

    Ok(LoginTokens {
        access_token: access.token,
        refresh_token: refresh.token,
        user: crate::app::auth::entity::UserInfo {
            id: user.id,
            nickname: user.nickname,
            status: user.status,
            email,
            created_at: user.created_at,
        },
    })
}
