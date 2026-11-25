pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::http::response::ApiResult;
    use crate::app::rbac::entity::Permission;
    use crate::app::rbac::repository::permission;
    use crate::app::{Container, DatabaseExt as _};

    pub type ResponseData = Vec<Permission>;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
    ) -> ApiResult<ResponseData> {
        claims.require_permission("rbac.read", crate::here!())?;
        // 단순 목록 조회는 별도 유스케이스 없이 저장소를 호출한다.
        let mut connection = container.db.acquire_conn(crate::here!()).await?;
        Ok(permission::list(&mut connection.handle()).await?.into())
    }
}

#[cfg(test)]
mod tests {
    mod contract {
        use std::sync::Arc;

        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode, header};
        use tower::ServiceExt as _;

        async fn setup() -> anyhow::Result<(axum::Router, Arc<crate::app::Container>)> {
            let container = crate::test_support::container().await?;
            let router = crate::app::http::router(Arc::clone(&container))?;
            Ok((router, container))
        }

        #[tokio::test]
        async fn rejects_missing_invalid_and_insufficient_authentication() -> anyhow::Result<()> {
            let (router, container) = setup().await?;
            let issue = |subject: &str, user: serde_json::Value| -> anyhow::Result<String> {
                Ok(container
                    .jwt_token_service
                    .issue(subject, user.as_object().unwrap().clone())?
                    .token)
            };
            let denied = issue(
                "42",
                serde_json::json!({"id": 42, "family_id": "00000000-0000-0000-0000-000000000000", "status": "active", "permissions": []}),
            )?;
            let invalid_claims = issue(
                "43",
                serde_json::json!({"id": 42, "family_id": "00000000-0000-0000-0000-000000000000", "status": "active", "permissions": ["rbac.read"]}),
            )?;
            for (auth, expected) in [
                (None, StatusCode::UNAUTHORIZED),
                (Some("Bearer invalid".to_owned()), StatusCode::UNAUTHORIZED),
                (
                    Some(format!("Bearer {invalid_claims}")),
                    StatusCode::UNAUTHORIZED,
                ),
                (Some(format!("Bearer {denied}")), StatusCode::FORBIDDEN),
            ] {
                let mut request = Request::builder().uri("/api/rbac/permissions");
                if let Some(auth) = auth {
                    request = request.header(header::AUTHORIZATION, auth);
                }
                let response = router.clone().oneshot(request.body(Body::empty())?).await?;
                // 인증과 토큰 권한에 따라 요청을 거부한다.
                assert_eq!(response.status(), expected);
            }
            Ok(())
        }

        #[tokio::test]
        async fn returns_empty_or_id_ordered_permission_list_with_token_permission()
        -> anyhow::Result<()> {
            let (router, container) = setup().await?;
            {
                let mut connection = container.db.conn().await?;
                sqlx::query(
                    "CREATE TEMPORARY TABLE conbira_rbac_permissions (
                        id BIGINT PRIMARY KEY, code TEXT NOT NULL,
                        name TEXT NOT NULL, description TEXT NOT NULL
                    )",
                )
                .execute(connection.handle().conn())
                .await?;
            }
            let claims = serde_json::json!({"id": 42, "family_id": "00000000-0000-0000-0000-000000000000", "status": "active", "permissions": ["rbac.read"]});
            let allowed = container
                .jwt_token_service
                .issue("42", claims.as_object().unwrap().clone())?
                .token;
            let request = || {
                Request::builder()
                    .uri("/api/rbac/permissions")
                    .header(header::AUTHORIZATION, format!("Bearer {allowed}"))
                    .body(Body::empty())
                    .unwrap()
            };
            let response = router.clone().oneshot(request()).await?;
            assert_eq!(response.status(), StatusCode::OK);
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
            // 등록된 권한이 없으면 data는 빈 배열이다.
            assert_eq!(body, serde_json::json!({"code": "OK", "data": []}));
            {
                let mut connection = container.db.conn().await?;
                sqlx::query(
                    "INSERT INTO conbira_rbac_permissions VALUES
                        (20, 'second', 'Second', 'Two'),
                        (10, 'first', 'First', 'One')",
                )
                .execute(connection.handle().conn())
                .await?;
            }
            let response = router.oneshot(request()).await?;
            assert_eq!(response.status(), StatusCode::OK);
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
            // 토큰 권한으로 접근하며 모든 권한의 네 필드를 ID 순서로 반환한다.
            assert_eq!(
                body,
                serde_json::json!({"code": "OK", "data": [
                    {"id": 10, "code": "first", "name": "First", "description": "One"},
                    {"id": 20, "code": "second", "name": "Second", "description": "Two"}
                ]})
            );
            Ok(())
        }
    }
}
