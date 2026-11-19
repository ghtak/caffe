pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::http::response::{ApiResponse, ApiResult};
    use crate::app::rbac::entity::Role;
    use crate::app::rbac::repository::role;
    use crate::app::{Container, DatabaseExt as _};

    pub type RoleResponse = Role;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
    ) -> ApiResult<Vec<RoleResponse>> {
        claims.require_permission("rbac.read", crate::here!())?;
        // 단순 목록 조회는 별도 유스케이스 없이 저장소를 호출한다.
        let mut connection = container.db.acquire_conn(crate::here!()).await?;
        Ok(ApiResponse::new(
            role::list(&mut connection.handle()).await?,
        ))
    }
}
