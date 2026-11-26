use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt as _;

#[tokio::test]
async fn role_detail_lists_permissions_and_paginated_users() -> anyhow::Result<()> {
    crate::test_support::TestContext::run(1, |context| async move {
    let container = std::sync::Arc::new(context.container().await?);
    let router = crate::app::http::router(container.clone())?;
    {
        let mut connection = container.db.conn().await?;
        for sql in [
            "INSERT INTO conbira_rbac_roles (id, code, name, description) VALUES (101, 'operator', 'Operator', 'Test')",
            "INSERT INTO conbira_rbac_permissions (id, code, name, description) VALUES (102, 'item.read', 'Read', 'Read items')",
            "INSERT INTO conbira_rbac_role_permissions (role_id, permission_id) VALUES (101, 102)",
            "INSERT INTO conbira_auth_users (id, nickname) VALUES (1, 'Alpha'), (2, 'Beta'), (3, 'Other')",
            "INSERT INTO conbira_auth_user_identities (user_id, provider, provider_subject, email) VALUES (1, 'local', '1', 'alpha@example.com'), (2, 'local', '2', 'beta@example.com')",
            "INSERT INTO conbira_rbac_user_roles (user_id, role_id) VALUES (1, 101), (2, 101)",
        ] {
            sqlx::query(sql).execute(connection.handle().conn()).await?;
        }
    }
    let claims = serde_json::json!({"id": 42, "status": "active", "permissions": ["rbac.read", "auth.user.read"]});
    let token = container
        .jwt_token_service
        .issue("42", claims.as_object().unwrap().clone())?
        .token;
    for (path, expected) in [
        (
            "/api/rbac/roles/101",
            serde_json::json!({"id":101,"code":"operator","name":"Operator","description":"Test"}),
        ),
        (
            "/api/rbac/roles/101/permissions",
            serde_json::json!([{"id":102,"code":"item.read","name":"Read","description":"Read items"}]),
        ),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::get(path)
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await?)?;
        assert_eq!(body["data"], expected);
    }
    let response = router
        .clone()
        .oneshot(
            Request::get("/api/rbac/roles/101/users?page_size=1&email=example.com")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await?)?;
    assert_eq!(body["data"]["items"][0]["nickname"], "Alpha");
    assert_eq!(body["data"]["has_next"], true);
    let response = router
        .clone()
        .oneshot(
            Request::get("/api/rbac/roles/101/users/search?page_size=2")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await?)?;
    assert_eq!(body["data"]["items"][0]["assigned"], true);
    assert_eq!(body["data"]["has_next"], true);
    let response = router
        .clone()
        .oneshot(
            Request::get("/api/rbac/roles/101/users/search?page=2&page_size=2")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await?)?;
    assert_eq!(body["data"]["items"][0]["nickname"], "Other");
    assert_eq!(body["data"]["items"][0]["assigned"], false);
    assert_eq!(body["data"]["has_next"], false);
    let response = router
        .clone()
        .oneshot(
            Request::get("/api/rbac/roles/101/users/search?name=Other&page_size=1")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8192).await?)?;
    assert_eq!(body["data"]["items"][0]["nickname"], "Other");
    assert_eq!(body["data"]["items"][0]["assigned"], false);
    assert_eq!(body["data"]["has_next"], false);
    for path in [
        "/api/rbac/roles/9",
        "/api/rbac/roles/9/permissions",
        "/api/rbac/roles/9/users",
        "/api/rbac/roles/9/users/search",
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::get(path)
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    Ok(())

}).await
}

#[tokio::test]
async fn role_user_list_requires_both_read_permissions() -> anyhow::Result<()> {
    crate::test_support::TestContext::run(1, |context| async move {
        let container = std::sync::Arc::new(context.container().await?);
        let router = crate::app::http::router(container.clone())?;
        for permissions in [vec!["rbac.read"], vec!["auth.user.read"]] {
            let claims =
                serde_json::json!({"id": 42, "status": "active", "permissions": permissions});
            let token = container
                .jwt_token_service
                .issue("42", claims.as_object().unwrap().clone())?
                .token;
            let response = router
                .clone()
                .oneshot(
                    Request::get("/api/rbac/roles/1/users")
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .body(Body::empty())?,
                )
                .await?;
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn role_user_search_requires_both_read_permissions() -> anyhow::Result<()> {
    crate::test_support::TestContext::run(1, |context| async move {
        let container = std::sync::Arc::new(context.container().await?);
        let router = crate::app::http::router(container.clone())?;
        for permissions in [vec!["rbac.read"], vec!["auth.user.read"]] {
            let claims =
                serde_json::json!({"id": 42, "status": "active", "permissions": permissions});
            let token = container
                .jwt_token_service
                .issue("42", claims.as_object().unwrap().clone())?
                .token;
            let response = router
                .clone()
                .oneshot(
                    Request::get("/api/rbac/roles/1/users/search")
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .body(Body::empty())?,
                )
                .await?;
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        Ok(())
    })
    .await
}
