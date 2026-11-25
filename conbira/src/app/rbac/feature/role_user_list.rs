pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::auth::entity::UserStatus;
    use crate::app::auth::repository::user::Criteria;
    use crate::app::http::extract::{ApiPath, ApiQuery};
    use crate::app::http::response::ApiResult;
    use crate::app::rbac::repository::{role, user_role};
    use crate::app::utils::trim_nonempty;
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    #[derive(Debug, serde::Deserialize, espr_derive::Validate)]
    pub struct RequestQuery {
        pub name: Option<String>,
        pub email: Option<String>,
        pub status: Option<UserStatus>,
        pub page: Option<u32>,
        pub page_size: Option<u32>,
    }

    #[derive(serde::Serialize)]
    pub struct UserDto {
        pub id: i64,
        pub nickname: String,
        pub emails: Vec<String>,
        pub status: UserStatus,
        pub created_at: jiff::Timestamp,
    }

    #[derive(serde::Serialize)]
    pub struct ResponseData {
        pub items: Vec<UserDto>,
        pub page: u32,
        pub page_size: u32,
        pub has_next: bool,
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
        ApiPath(role_id): ApiPath<i64>,
        ApiQuery(query): ApiQuery<RequestQuery>,
    ) -> ApiResult<ResponseData> {
        claims.require_permissions(&["rbac.read", "auth.user.read"], crate::here!())?;
        let page = query.page.unwrap_or(1);
        let page_size = query.page_size.unwrap_or(20);
        if page == 0 || !(1..=100).contains(&page_size) {
            return Err(
                Error::new(Code::InvalidRequest, "Invalid page or page size")
                    .at(crate::here!())
                    .into(),
            );
        }
        let mut connection = container.db.acquire_conn(crate::here!()).await?;
        let mut handle = connection.handle();
        if role::find_by_id(&mut handle, role_id).await?.is_none() {
            return Err(Error::new(Code::NotFound, "Role not found")
                .at(crate::here!())
                .into());
        }
        let mut users = user_role::search_by_role_id(
            &mut handle,
            role_id,
            &Criteria {
                name: trim_nonempty(query.name.as_deref()),
                email: trim_nonempty(query.email.as_deref()),
                status: query.status,
                limit: i64::from(page_size) + 1,
                offset: i64::from(page - 1) * i64::from(page_size),
            },
        )
        .await?;
        let has_next = users.len() > page_size as usize;
        users.truncate(page_size as usize);
        Ok(ResponseData {
            items: users
                .into_iter()
                .map(|user| UserDto {
                    id: user.id,
                    nickname: user.nickname,
                    emails: user.emails,
                    status: user.status,
                    created_at: user.created_at,
                })
                .collect(),
            page,
            page_size,
            has_next,
        }
        .into())
    }
}
