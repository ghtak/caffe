pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::auth::entity::UserStatus;
    use crate::app::http::extract::ApiQuery;
    use crate::app::http::response::ApiResult;
    use crate::{Code, Error};

    #[derive(serde::Serialize)]
    pub struct UserDto {
        pub id: i64,
        pub nickname: String,
        pub emails: Vec<String>,
        pub status: UserStatus,
        pub created_at: jiff::Timestamp,
    }

    #[derive(Debug, serde::Deserialize, espr_derive::Validate)]
    pub struct RequestQuery {
        pub name: Option<String>,
        pub email: Option<String>,
        pub status: Option<UserStatus>,
        pub page: Option<u32>,
        pub page_size: Option<u32>,
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
        ApiQuery(request): ApiQuery<RequestQuery>,
    ) -> ApiResult<ResponseData> {
        claims.require_permission("auth.user.read", crate::here!())?;

        let page = request.page.unwrap_or(1);
        let page_size = request.page_size.unwrap_or(20);
        if page == 0 || !(1..=100).contains(&page_size) {
            return Err(
                Error::new(Code::InvalidRequest, "Invalid page or page size")
                    .at(crate::here!())
                    .into(),
            );
        }

        let result = super::usecase::execute(
            &container.db,
            super::usecase::Query {
                name: request.name,
                email: request.email,
                status: request.status,
                page,
                page_size,
            },
        )
        .await?;

        Ok(ResponseData {
            items: result
                .items
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
            has_next: result.has_next,
        }
        .into())
    }
}

pub mod usecase {

    use crate::app::DatabaseExt as _;
    use crate::app::auth::entity::UserStatus;
    use crate::app::auth::repository::user;
    use crate::app::container::db::Database;
    use crate::app::utils::trim_nonempty;

    pub struct Query {
        pub name: Option<String>,
        pub email: Option<String>,
        pub status: Option<UserStatus>,
        pub page: u32,
        pub page_size: u32,
    }

    pub struct Output {
        pub items: Vec<user::SearchUser>,
        pub has_next: bool,
    }

    pub async fn execute(db: &Database, query: Query) -> crate::Result<Output> {
        let mut connection = db.acquire_conn(crate::here!()).await?;
        let mut items = user::search(
            &mut connection.handle(),
            &user::Criteria {
                name: trim_nonempty(query.name.as_deref()),
                email: trim_nonempty(query.email.as_deref()),
                status: query.status,
                limit: i64::from(query.page_size) + 1,
                offset: i64::from(query.page - 1) * i64::from(query.page_size),
            },
        )
        .await?;
        let has_next = items.len() > query.page_size as usize;
        items.truncate(query.page_size as usize);
        Ok(Output { items, has_next })
    }
}

#[cfg(test)]
mod tests {
    mod contract {
        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode, header};
        use tower::ServiceExt as _;

        #[tokio::test]
        async fn search_requires_permission_and_returns_only_public_fields_in_id_order()
        -> anyhow::Result<()> {
            crate::test_support::TestContext::run(1, |context| async move {
            let container = std::sync::Arc::new(context.container().await?);
            let router = crate::app::http::router(container.clone())?;
            let mut connection = container.db.conn().await?;
            sqlx::query(
                "INSERT INTO conbira_auth_users (id, nickname) VALUES (2, 'second'), (1, 'first')",
            )
            .execute(connection.handle().conn())
            .await?;
            sqlx::query("INSERT INTO conbira_auth_user_identities (user_id, provider, provider_subject, email) VALUES (1, 'local', '1', 'first@example.com'), (1, 'google', 'google-1', 'other@example.com'), (1, 'google', 'google-2', 'first@example.com')")
                .execute(connection.handle().conn()).await?;
            drop(connection);

            for (roles, permissions, expected) in [
                (vec!["admin"], vec![], StatusCode::FORBIDDEN),
                (vec!["user"], vec!["auth.user.read"], StatusCode::OK),
            ] {
                let claims = serde_json::json!({"id": 42, "status": "active", "roles": roles, "permissions": permissions});
                let token = container
                    .jwt_token_service
                    .issue("42", claims.as_object().unwrap().clone())?
                    .token;
                let response = router
                    .clone()
                    .oneshot(
                        Request::get("/api/auth/users")
                            .header(header::AUTHORIZATION, format!("Bearer {token}"))
                            .body(Body::empty())?,
                    )
                    .await?;
                // 조회 권한이 있는 사용자만 목록을 조회할 수 있다.
                assert_eq!(response.status(), expected);
                if expected == StatusCode::OK {
                    let body: serde_json::Value =
                        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
                    let users = body["data"]["items"].as_array().unwrap();
                    assert_eq!(users.len(), 2);
                    assert_eq!(users[0]["nickname"], "first");
                    assert_eq!(users[1]["nickname"], "second");
                    assert_eq!(users[0].as_object().unwrap().len(), 5);
                    assert_eq!(
                        users[0]["emails"],
                        serde_json::json!(["first@example.com", "other@example.com"])
                    );
                    assert_eq!(users[1]["emails"], serde_json::json!([]));
                }
            }

            let claims = serde_json::json!({"id": 42, "status": "active", "roles": ["admin"], "permissions": ["auth.user.read"]});
            let token = container
                .jwt_token_service
                .issue("42", claims.as_object().unwrap().clone())?
                .token;

            for invalid_query in [
                "page=0",
                "page_size=0",
                "page_size=101",
                "page=0&page_size=101",
            ] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::get(format!("/api/auth/users?{invalid_query}"))
                            .header(header::AUTHORIZATION, format!("Bearer {token}"))
                            .body(Body::empty())?,
                    )
                    .await?;
                // 허용 범위를 벗어난 페이지 번호나 페이지 크기는 거부한다.
                assert_eq!(
                    response.status(),
                    StatusCode::BAD_REQUEST,
                    "expected 400 for query: {invalid_query}"
                );
            }

            for (query, expected, has_next) in [
                ("name=s&status=registered&page=1&page_size=1", "first", true),
                (
                    "name=s&status=registered&page=2&page_size=1",
                    "second",
                    false,
                ),
                (
                    "name=fir&status=registered&page=1&page_size=1",
                    "first",
                    false,
                ),
            ] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::get(format!("/api/auth/users?{query}"))
                            .header(header::AUTHORIZATION, format!("Bearer {token}"))
                            .body(Body::empty())?,
                    )
                    .await?;
                assert_eq!(response.status(), StatusCode::OK);
                let body: serde_json::Value =
                    serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
                // 검색 결과는 ID 순으로 나뉘며 다음 페이지 존재 여부를
                // 제공한다.
                assert_eq!(body["data"]["items"][0]["nickname"], expected);
                assert_eq!(body["data"]["has_next"], has_next);
            }

            let response = router
                .clone()
                .oneshot(
                    Request::get("/api/auth/users?status=active")
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .body(Body::empty())?,
                )
                .await?;
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
            // 일치하는 상태가 없으면 빈 목록을 반환한다.
            assert_eq!(body["data"]["items"], serde_json::json!([]));

            let response = router
                .oneshot(
                    Request::get("/api/auth/users?email=OTHER%40EXAMPLE")
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .body(Body::empty())?,
                )
                .await?;
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
            // 어느 식별 정보의 이메일이든 대소문자 구분 없이 검색된다.
            assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
            assert_eq!(body["data"]["items"][0]["nickname"], "first");
            Ok(())

}).await
        }
    }
}
