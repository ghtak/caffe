pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::auth::repository::user;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::ApiResult;
    use crate::app::rbac::entity::Role;
    use crate::app::rbac::repository::user_role;
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    pub type UserRoleResponse = Role;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
        ApiPath(user_id): ApiPath<i64>,
    ) -> ApiResult<Vec<UserRoleResponse>> {
        claims.require_permission("rbac.read", crate::here!())?;
        let mut connection = container.db.acquire_conn(crate::here!()).await?;
        let mut handle = connection.handle();
        if user::find_by_id(&mut handle, user_id).await?.is_none() {
            return Err(Error::new(Code::NotFound, "User not found")
                .at(crate::here!())
                .into());
        }
        Ok((user_role::list_by_user_id(&mut handle, user_id).await?).into())
    }
}
