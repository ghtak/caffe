use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt as _;

async fn setup() -> anyhow::Result<(axum::Router, Arc<crate::app::Container>)> {
    let container = crate::test_support::container().await?;
    let router = crate::app::http::router(Arc::clone(&container))?;
    Ok((router, container))
}

fn token(container: &crate::app::Container, permissions: &[&str]) -> anyhow::Result<String> {
    let claims = serde_json::json!({"id": 42, "status": "active", "permissions": permissions});
    Ok(container
        .jwt_token_service
        .issue("42", claims.as_object().unwrap().clone())?
        .token)
}

#[tokio::test]
async fn user_role_endpoints_require_authentication_and_their_own_permissions() -> anyhow::Result<()>
{
    let (router, container) = setup().await?;
    let read = token(&container, &["rbac.read"])?;
    let manage = token(&container, &["rbac.user_role.manage"])?;
    for (method, path, denied) in [
        ("GET", "/api/rbac/users/1/roles", &manage),
        ("POST", "/api/rbac/users/1/roles/2", &read),
        ("DELETE", "/api/rbac/users/1/roles/2", &read),
    ] {
        for (auth, expected) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some("Bearer invalid".to_owned()), StatusCode::UNAUTHORIZED),
            (Some(format!("Bearer {denied}")), StatusCode::FORBIDDEN),
        ] {
            let mut request = Request::builder().method(method).uri(path);
            if let Some(auth) = auth {
                request = request.header(header::AUTHORIZATION, auth);
            }
            let response = router.clone().oneshot(request.body(Body::empty())?).await?;
            // 인증과 해당 작업의 권한이 모두 필요하다.
            assert_eq!(response.status(), expected, "{method} {path}");
        }
    }
    Ok(())
}

async fn seed_users_and_roles(container: &crate::app::Container) -> anyhow::Result<()> {
    let mut connection = container.db.conn().await?;
    for schema in [
        include_str!("../../../../../../migrations/0010_user.sql"),
        include_str!("../../../../../../migrations/0012_rbac.sql"),
    ] {
        let schema = schema.replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
        sqlx::raw_sql(sqlx::AssertSqlSafe(schema))
            .execute(connection.handle().conn())
            .await?;
    }
    sqlx::query("INSERT INTO conbira_auth_users (id, nickname) VALUES (101, 'one'), (102, 'two')")
        .execute(connection.handle().conn())
        .await?;
    sqlx::query("INSERT INTO conbira_rbac_roles (id, code, name, description) VALUES (101, 'first', 'First', 'One'), (102, 'second', 'Second', 'Two')")
        .execute(connection.handle().conn()).await?;
    sqlx::query("INSERT INTO conbira_rbac_user_roles VALUES (102, 102)")
        .execute(connection.handle().conn())
        .await?;
    Ok(())
}

#[tokio::test]
async fn lists_assigned_roles_in_order_and_reflects_revocation() -> anyhow::Result<()> {
    let (router, container) = setup().await?;
    seed_users_and_roles(&container).await?;
    let read = token(&container, &["rbac.read"])?;
    let manage = token(&container, &["rbac.user_role.manage"])?;
    for (method, path, auth, expected_status, expected) in [
        (
            "GET",
            "/api/rbac/users/101/roles",
            &read,
            StatusCode::OK,
            serde_json::json!({"code":"OK", "data":[]}),
        ),
        (
            "POST",
            "/api/rbac/users/101/roles/102",
            &manage,
            StatusCode::OK,
            serde_json::json!({"code":"OK"}),
        ),
        (
            "POST",
            "/api/rbac/users/101/roles/101",
            &manage,
            StatusCode::OK,
            serde_json::json!({"code":"OK"}),
        ),
        (
            "GET",
            "/api/rbac/users/101/roles",
            &read,
            StatusCode::OK,
            serde_json::json!({"code":"OK", "data":[{"id":101,"code":"first","name":"First","description":"One"},{"id":102,"code":"second","name":"Second","description":"Two"}]}),
        ),
        (
            "DELETE",
            "/api/rbac/users/101/roles/101",
            &manage,
            StatusCode::OK,
            serde_json::json!({"code":"OK"}),
        ),
        (
            "DELETE",
            "/api/rbac/users/101/roles/101",
            &manage,
            StatusCode::NOT_FOUND,
            serde_json::json!({"code":"NOT_FOUND", "message":"User role not found"}),
        ),
        (
            "GET",
            "/api/rbac/users/101/roles",
            &read,
            StatusCode::OK,
            serde_json::json!({"code":"OK", "data":[{"id":102,"code":"second","name":"Second","description":"Two"}]}),
        ),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {auth}"))
            .body(Body::empty())?;
        let response = router.clone().oneshot(request).await?;
        let status = response.status();
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
        // 역할 목록은 ID 순서이며 부여·회수를 반영한다. 이미 회수한 역할은
        // 404를 반환한다.
        assert_eq!(status, expected_status, "{method} {path}");
        assert_eq!(body, expected, "{method} {path}");
    }
    let mut connection = container.db.conn().await?;
    let links: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT user_id, role_id FROM conbira_rbac_user_roles ORDER BY user_id, role_id",
    )
    .fetch_all(connection.handle().conn())
    .await?;
    // 101번 역할만 회수되고 다른 사용자의 역할은 유지된다.
    assert_eq!(links, vec![(101, 102), (102, 102)]);
    Ok(())
}

#[tokio::test]
async fn rejects_duplicate_or_missing_user_role_links_without_changing_them() -> anyhow::Result<()>
{
    let (router, container) = setup().await?;
    seed_users_and_roles(&container).await?;
    {
        let mut connection = container.db.conn().await?;
        sqlx::query("INSERT INTO conbira_rbac_user_roles VALUES (101, 101), (101, 102)")
            .execute(connection.handle().conn())
            .await?;
    }
    let read = token(&container, &["rbac.read"])?;
    let manage = token(&container, &["rbac.user_role.manage"])?;
    for (method, path, auth, expected_status, expected) in [
        (
            "GET",
            "/api/rbac/users/999/roles",
            &read,
            StatusCode::NOT_FOUND,
            serde_json::json!({"code":"NOT_FOUND", "message":"User not found"}),
        ),
        (
            "POST",
            "/api/rbac/users/999/roles/101",
            &manage,
            StatusCode::NOT_FOUND,
            serde_json::json!({"code":"NOT_FOUND", "message":"User or role not found"}),
        ),
        (
            "POST",
            "/api/rbac/users/101/roles/999",
            &manage,
            StatusCode::NOT_FOUND,
            serde_json::json!({"code":"NOT_FOUND", "message":"User or role not found"}),
        ),
        (
            "POST",
            "/api/rbac/users/101/roles/101",
            &manage,
            StatusCode::BAD_REQUEST,
            serde_json::json!({"code":"INVALID_REQUEST", "message":"User already has role"}),
        ),
        (
            "DELETE",
            "/api/rbac/users/999/roles/101",
            &manage,
            StatusCode::NOT_FOUND,
            serde_json::json!({"code":"NOT_FOUND", "message":"User role not found"}),
        ),
        (
            "DELETE",
            "/api/rbac/users/101/roles/999",
            &manage,
            StatusCode::NOT_FOUND,
            serde_json::json!({"code":"NOT_FOUND", "message":"User role not found"}),
        ),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {auth}"))
            .body(Body::empty())?;
        let response = router.clone().oneshot(request).await?;
        let status = response.status();
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
        // 대상·연결이 없으면 404, 이미 부여된 역할이면 400을 반환한다.
        assert_eq!(status, expected_status, "{method} {path}");
        assert_eq!(body, expected, "{method} {path}");
    }
    let mut connection = container.db.conn().await?;
    let links: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT user_id, role_id FROM conbira_rbac_user_roles ORDER BY user_id, role_id",
    )
    .fetch_all(connection.handle().conn())
    .await?;
    // 실패한 요청은 두 사용자의 기존 역할 연결을 변경하지 않는다.
    assert_eq!(links, vec![(101, 101), (101, 102), (102, 102)]);
    Ok(())
}
