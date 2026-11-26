use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt as _;

#[tokio::test]
async fn role_writes_require_manage_permission() -> anyhow::Result<()> {
    crate::test_support::TestContext::run(1, |context| async move {
        let container = Arc::new(context.container().await?);
        let router = crate::app::http::router(Arc::clone(&container))?;
        let claims =
            serde_json::json!({"id": 42, "status": "active", "permissions": ["rbac.read"]});
        let token = container
            .jwt_token_service
            .issue("42", claims.as_object().unwrap().clone())?
            .token;
        for (method, path, body) in [
            (
                "POST",
                "/api/rbac/roles",
                r#"{"code":"test","name":"Test","description":""}"#,
            ),
            ("DELETE", "/api/rbac/roles/1", ""),
            ("POST", "/api/rbac/roles/1/permissions/2", ""),
            ("DELETE", "/api/rbac/roles/1/permissions/2", ""),
        ] {
            for (authorization, expected) in [
                (None, StatusCode::UNAUTHORIZED),
                (Some("Bearer invalid".to_owned()), StatusCode::UNAUTHORIZED),
                (Some(format!("Bearer {token}")), StatusCode::FORBIDDEN),
            ] {
                let mut request = Request::builder()
                    .method(method)
                    .uri(path)
                    .header(header::CONTENT_TYPE, "application/json");
                if let Some(authorization) = authorization {
                    request = request.header(header::AUTHORIZATION, authorization);
                }
                let response = router
                    .clone()
                    .oneshot(request.body(Body::from(body))?)
                    .await?;
                // 인증이 없거나 관리 권한이 없으면 쓰기를 거부한다.
                assert_eq!(response.status(), expected);
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn adds_and_removes_role_permissions_and_rejects_invalid_relationships() -> anyhow::Result<()>
{
    crate::test_support::TestContext::run(1, |context| async move {
    let container = Arc::new(context.container().await?);
let router = crate::app::http::router(Arc::clone(&container))?;
    {
        let mut connection = container.db.conn().await?;
        for sql in [
            "TRUNCATE conbira_rbac_roles, conbira_rbac_permissions CASCADE",



            "INSERT INTO conbira_rbac_roles (id, code, name) VALUES (1, 'test-role', 'Test')",
            "INSERT INTO conbira_rbac_permissions (id, code, name) VALUES (2, 'test.permission', 'Test')",
        ] {
            sqlx::query(sql).execute(connection.handle().conn()).await?;
        }
    }
    let claims =
        serde_json::json!({"id": 42, "status": "active", "permissions": ["rbac.role.manage"]});
    let token = container
        .jwt_token_service
        .issue("42", claims.as_object().unwrap().clone())?
        .token;
    for (method, path, expected, message) in [
        (
            "POST",
            "/api/rbac/roles/9/permissions/2",
            StatusCode::NOT_FOUND,
            Some("Role or permission not found"),
        ),
        (
            "POST",
            "/api/rbac/roles/1/permissions/9",
            StatusCode::NOT_FOUND,
            Some("Role or permission not found"),
        ),
        (
            "POST",
            "/api/rbac/roles/1/permissions/2",
            StatusCode::OK,
            None,
        ),
        (
            "POST",
            "/api/rbac/roles/1/permissions/2",
            StatusCode::BAD_REQUEST,
            Some("Role already has permission"),
        ),
        (
            "DELETE",
            "/api/rbac/roles/1/permissions/2",
            StatusCode::OK,
            None,
        ),
        (
            "DELETE",
            "/api/rbac/roles/1/permissions/2",
            StatusCode::NOT_FOUND,
            Some("Role permission not found"),
        ),
        (
            "DELETE",
            "/api/rbac/roles/9/permissions/2",
            StatusCode::NOT_FOUND,
            Some("Role permission not found"),
        ),
        (
            "DELETE",
            "/api/rbac/roles/1/permissions/9",
            StatusCode::NOT_FOUND,
            Some("Role permission not found"),
        ),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())?;
        let response = router.clone().oneshot(request).await?;
        // 각 관계 변경 결과와 공개 오류가 계약과 일치한다.
        assert_eq!(response.status(), expected, "{method} {path}");
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
        if let Some(message) = message {
            assert_eq!(body["message"], message);
        } else {
            assert_eq!(body, serde_json::json!({"code": "OK"}));
        }
        let mut connection = container.db.conn().await?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conbira_rbac_role_permissions")
            .fetch_one(connection.handle().conn())
            .await?;
        // 성공한 추가 이후부터 성공한 제거 전까지 관계 하나만 유지한다.
        let expected_count = if method == "POST"
            && (expected == StatusCode::OK || expected == StatusCode::BAD_REQUEST)
        {
            1
        } else {
            0
        };
        assert_eq!(count, expected_count);
    }
    Ok(())

}).await
}

#[tokio::test]
async fn creates_and_deletes_roles_but_rejects_assigned_role() -> anyhow::Result<()> {
    crate::test_support::TestContext::run(1, |context| async move {
    let container = Arc::new(context.container().await?);
let router = crate::app::http::router(Arc::clone(&container))?;
    let claims =
        serde_json::json!({"id": 42, "status": "active", "permissions": ["rbac.role.manage"]});
    let token = container
        .jwt_token_service
        .issue("42", claims.as_object().unwrap().clone())?
        .token;
    let create = Request::builder()
        .method("POST")
        .uri("/api/rbac/roles")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"code":"test.manage","name":"Test","description":"Test role"}"#,
        ))?;
    let response = router.clone().oneshot(create).await?;
    // 생성된 역할을 ID와 함께 반환한다.
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
    let id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(
        body,
        serde_json::json!({"code": "OK", "data": {"id": id, "code": "test.manage", "name": "Test", "description": "Test role"}})
    );
    let duplicate = Request::builder()
        .method("POST")
        .uri("/api/rbac/roles")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"code":"test.manage","name":"Duplicate","description":""}"#,
        ))?;
    let response = router.clone().oneshot(duplicate).await?;
    // 중복 코드는 클라이언트 오류로 반환하고 기존 역할을 유지한다.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
    assert_eq!(
        body,
        serde_json::json!({"code": "INVALID_REQUEST", "message": "Role code already exists"})
    );
    {
        let mut connection = container.db.conn().await?;
        let name: String = sqlx::query_scalar("SELECT name FROM conbira_rbac_roles WHERE id = $1")
            .bind(id)
            .fetch_one(connection.handle().conn())
            .await?;
        assert_eq!(name, "Test");
        sqlx::query("INSERT INTO conbira_rbac_role_permissions (permission_id, role_id) VALUES (1, $1)")
            .bind(id)
            .execute(connection.handle().conn())
            .await?;
    }
    let delete = || {
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/rbac/roles/{id}"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    let response = router.clone().oneshot(delete()).await?;
    // 권한에 연결된 역할은 삭제되지 않는다.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    {
        let mut connection = container.db.conn().await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM conbira_rbac_roles WHERE id = $1)")
                .bind(id)
                .fetch_one(connection.handle().conn())
                .await?;
        assert!(exists);
        sqlx::query("DELETE FROM conbira_rbac_role_permissions WHERE role_id = $1")
            .bind(id)
            .execute(connection.handle().conn())
            .await?;
        sqlx::query("INSERT INTO conbira_auth_users (id, nickname) VALUES (42, 'assigned user')")
            .execute(connection.handle().conn()).await?;
        sqlx::query("INSERT INTO conbira_rbac_user_roles VALUES (42, $1)")
            .bind(id)
            .execute(connection.handle().conn())
            .await?;
    }
    // 사용자에 연결된 역할도 삭제되지 않는다.
    assert_eq!(
        router.clone().oneshot(delete()).await?.status(),
        StatusCode::BAD_REQUEST
    );
    {
        let mut connection = container.db.conn().await?;
        sqlx::query("DELETE FROM conbira_rbac_user_roles WHERE role_id = $1")
            .bind(id)
            .execute(connection.handle().conn())
            .await?;
    }
    let response = router.clone().oneshot(delete()).await?;
    // 연결을 해제하면 삭제되며 반복 삭제는 대상을 찾지 못한다.
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
    assert_eq!(body, serde_json::json!({"code": "OK"}));
    assert_eq!(
        router.oneshot(delete()).await?.status(),
        StatusCode::NOT_FOUND
    );
    Ok(())

}).await
}

mod list {
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt as _;

    #[tokio::test]
    async fn rejects_missing_invalid_and_insufficient_authentication() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(1, |context| async move {
            let container = Arc::new(context.container().await?);
            let router = crate::app::http::router(Arc::clone(&container))?;
            let issue = |subject: &str, user: serde_json::Value| -> anyhow::Result<String> {
                Ok(container
                    .jwt_token_service
                    .issue(subject, user.as_object().unwrap().clone())?
                    .token)
            };
            let denied = issue(
                "42",
                serde_json::json!({"id": 42, "status": "active", "permissions": []}),
            )?;
            let invalid_claims = issue(
                "43",
                serde_json::json!({"id": 42, "status": "active", "permissions": ["rbac.read"]}),
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
                let mut request = Request::builder().uri("/api/rbac/roles");
                if let Some(auth) = auth {
                    request = request.header(header::AUTHORIZATION, auth);
                }
                let response = router.clone().oneshot(request.body(Body::empty())?).await?;
                // 인증과 토큰 권한에 따라 요청을 거부한다.
                assert_eq!(response.status(), expected);
            }
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn returns_empty_or_id_ordered_role_list_with_token_permission() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(1, |context| async move {
            let container = Arc::new(context.container().await?);
            let router = crate::app::http::router(Arc::clone(&container))?;
            {
                let mut connection = container.db.conn().await?;
                sqlx::query("TRUNCATE conbira_rbac_roles CASCADE")
                    .execute(connection.handle().conn())
                    .await?;
            }
            let claims =
                serde_json::json!({"id": 42, "status": "active", "permissions": ["rbac.read"]});
            let allowed = container
                .jwt_token_service
                .issue("42", claims.as_object().unwrap().clone())?
                .token;
            let request = || {
                Request::builder()
                    .uri("/api/rbac/roles")
                    .header(header::AUTHORIZATION, format!("Bearer {allowed}"))
                    .body(Body::empty())
                    .unwrap()
            };
            let response = router.clone().oneshot(request()).await?;
            assert_eq!(response.status(), StatusCode::OK);
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
            // 등록된 역할이 없으면 data는 빈 배열이다.
            assert_eq!(body, serde_json::json!({"code": "OK", "data": []}));
            {
                let mut connection = container.db.conn().await?;
                sqlx::query(
                    "INSERT INTO conbira_rbac_roles VALUES
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
            // 토큰 권한으로 접근하며 모든 역할의 네 필드를 ID 순서로 반환한다.
            assert_eq!(
                body,
                serde_json::json!({"code": "OK", "data": [
                    {"id": 10, "code": "first", "name": "First", "description": "One"},
                    {"id": 20, "code": "second", "name": "Second", "description": "Two"}
                ]})
            );
            Ok(())
        })
        .await
    }
}
