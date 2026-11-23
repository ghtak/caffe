use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt as _;

async fn setup() -> anyhow::Result<(axum::Router, Arc<crate::app::Container>)> {
    let dir = std::env::temp_dir().join(format!("conbira-user-role-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir)?;
    let mut config = crate::config::Config::load(
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml")),
        Some(&dir.join("missing.env")),
    )?;
    config.database = crate::test_support::database_config(1);
    config.cache.url = crate::test_support::cache_url();
    config.jwt.ed25519.key_path = dir.join("private.pem");
    config.jwt.ed25519.public_key_path = dir.join("public.pem");
    espr::jwt::ed25519::generate(&config.jwt.ed25519)?;
    let container = Arc::new(crate::app::Container::new(&config).await?);
    std::fs::remove_dir_all(dir)?;
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

#[tokio::test]
async fn lists_assigns_and_revokes_roles_and_reports_invalid_relationships() -> anyhow::Result<()> {
    let (router, container) = setup().await?;
    {
        let mut connection = container.db.conn().await?;
        for schema in [
            include_str!("../../../../migrations/0010_user.sql"),
            include_str!("../../../../migrations/0012_rbac.sql"),
        ] {
            let schema = schema.replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
            sqlx::raw_sql(sqlx::AssertSqlSafe(schema))
                .execute(connection.handle().conn())
                .await?;
        }
        sqlx::query(
            "INSERT INTO conbira_auth_users (id, nickname) VALUES (101, 'one'), (102, 'two')",
        )
        .execute(connection.handle().conn())
        .await?;
        sqlx::query("INSERT INTO conbira_rbac_roles (id, code, name, description) VALUES (101, 'first', 'First', 'One'), (102, 'second', 'Second', 'Two')")
            .execute(connection.handle().conn()).await?;
        sqlx::query("INSERT INTO conbira_rbac_user_roles VALUES (102, 102)")
            .execute(connection.handle().conn())
            .await?;
    }
    let read = token(&container, &["rbac.read"])?;
    let manage = token(&container, &["rbac.user_role.manage"])?;
    for (method, path, auth, status, expected) in [
        (
            "GET",
            "/api/rbac/users/999/roles",
            &read,
            StatusCode::NOT_FOUND,
            serde_json::json!({"code":"NOT_FOUND", "message":"User not found"}),
        ),
        (
            "GET",
            "/api/rbac/users/101/roles",
            &read,
            StatusCode::OK,
            serde_json::json!({"code":"OK", "data":[]}),
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
            "POST",
            "/api/rbac/users/101/roles/101",
            &manage,
            StatusCode::BAD_REQUEST,
            serde_json::json!({"code":"INVALID_REQUEST", "message":"User already has role"}),
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
        // 각 요청의 상태와 응답은 계약에 일치하며 목록은 해당 사용자만
        // 포함한다.
        assert_eq!(response.status(), status, "{method} {path}");
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
        assert_eq!(body, expected, "{method} {path}");
    }
    let mut connection = container.db.conn().await?;
    let links: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT user_id, role_id FROM conbira_rbac_user_roles ORDER BY user_id, role_id",
    )
    .fetch_all(connection.handle().conn())
    .await?;
    // 회수와 실패한 요청은 다른 사용자의 연결을 변경하지 않는다.
    assert_eq!(links, vec![(101, 102), (102, 102)]);
    Ok(())
}
