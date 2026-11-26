pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::auth::repository::user::DetailUser;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::ApiResult;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        claims: UserClaims,
        ApiPath(id): ApiPath<i64>,
    ) -> ApiResult<DetailUser> {
        claims.require_permission("auth.user.read", crate::here!())?;
        Ok(super::usecase::execute(&container.db, id).await?.into())
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::auth::repository::user::{self, DetailUser};
    use crate::app::container::db::Database;
    use crate::{Code, Error};

    pub async fn execute(db: &Database, id: i64) -> crate::Result<DetailUser> {
        let mut connection = db.acquire_conn(crate::here!()).await?;
        user::find_detail_by_id(&mut connection.handle(), id)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "User not found").at(crate::here!()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt as _;

    use crate::app::Container;

    async fn fixture() -> anyhow::Result<Arc<Container>> {
        let container = crate::test_support::container().await?;
        let mut connection = container.db.conn().await?;
        for statement in [
            "CREATE TEMPORARY TABLE conbira_auth_users (id BIGINT PRIMARY KEY, nickname TEXT NOT NULL, status TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP, updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TEMPORARY TABLE conbira_auth_user_identities (user_id BIGINT NOT NULL, email TEXT NOT NULL, password_hash TEXT)",
            "INSERT INTO conbira_auth_users (id, nickname, status) VALUES (1, 'first', 'active'), (2, 'second', 'registered'), (3, 'third', 'inactive'), (4, 'fourth', 'withdrawn')",
            "INSERT INTO conbira_auth_user_identities VALUES (1, 'z@example.com', 'secret'), (1, 'a@example.com', 'secret'), (1, 'z@example.com', 'secret')",
        ] {
            sqlx::query(statement)
                .execute(connection.handle().conn())
                .await?;
        }
        drop(connection);
        Ok(container)
    }

    async fn get(
        container: Arc<Container>,
        id: &str,
        permissions: Option<Vec<&str>>,
    ) -> anyhow::Result<(StatusCode, serde_json::Value)> {
        let mut request = Request::get(format!("/api/auth/users/{id}"));
        if let Some(permissions) = permissions {
            let claims =
                serde_json::json!({"id": 42, "status": "active", "permissions": permissions});
            let token = container
                .jwt_token_service
                .issue("42", claims.as_object().unwrap().clone())?
                .token;
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = crate::app::http::router(container)?
            .oneshot(request.body(Body::empty())?)
            .await?;
        let status = response.status();
        let body = serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
        Ok((status, body))
    }

    mod edge {
        use super::*;

        #[tokio::test]
        async fn storage_failure_is_internal_instead_of_not_found() -> anyhow::Result<()> {
            let container = fixture().await?;
            let mut connection = container.db.conn().await?;
            sqlx::query("ALTER TABLE conbira_auth_user_identities DROP COLUMN email")
                .execute(connection.handle().conn())
                .await?;
            drop(connection);
            let (status, body) = get(container, "1", Some(vec!["auth.user.read"])).await?;
            // 조회 실패는 미존재로 오인하지 않고 SQL 원인을 공개하지 않는다.
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(body["code"], "INTERNAL");
            assert!(body.get("data").is_none());
            assert!(body.get("cause").is_none());
            assert_eq!(body["message"], "Failed to read user detail");
            Ok(())
        }
    }

    mod contract {
        use super::*;

        #[tokio::test]
        async fn returns_only_public_fields_with_sorted_distinct_emails() -> anyhow::Result<()> {
            let container = fixture().await?;
            let (status, body) = get(container, "1", Some(vec!["auth.user.read"])).await?;
            // 지정한 사용자의 공개 필드만 반환하고 이메일을 중복 제거·정렬한다.
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["code"], "OK");
            assert_eq!(body["data"].as_object().unwrap().len(), 6);
            assert_eq!(body["data"]["id"], 1);
            assert_eq!(
                body["data"]["emails"],
                serde_json::json!(["a@example.com", "z@example.com"])
            );
            Ok(())
        }

        #[tokio::test]
        async fn reads_all_statuses_including_users_without_identities() -> anyhow::Result<()> {
            let container = fixture().await?;
            for (id, expected) in [("2", "registered"), ("3", "inactive"), ("4", "withdrawn")] {
                let (status, body) =
                    get(container.clone(), id, Some(vec!["auth.user.read"])).await?;
                // 연결 정보가 없어도 저장된 상태와 빈 이메일 배열을 반환한다.
                assert_eq!(status, StatusCode::OK);
                assert_eq!(body["data"]["status"], expected);
                assert_eq!(body["data"]["emails"], serde_json::json!([]));
            }
            Ok(())
        }

        #[tokio::test]
        async fn rejects_missing_permission_before_revealing_user_existence() -> anyhow::Result<()>
        {
            let container = crate::test_support::container().await?;
            for id in ["1", "-1"] {
                let (status, body) = get(container.clone(), id, Some(vec![])).await?;
                // DB 준비 없이도 권한 거부가 우선하며 존재 여부를 노출하지
                // 않는다.
                assert_eq!(status, StatusCode::FORBIDDEN);
                assert_eq!(body["code"], "FORBIDDEN");
            }
            Ok(())
        }

        #[tokio::test]
        async fn returns_not_found_for_missing_user() -> anyhow::Result<()> {
            let (status, body) = get(fixture().await?, "-1", Some(vec!["auth.user.read"])).await?;
            // 미존재는 빈 성공 응답 대신 404이며 data를 포함하지 않는다.
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body["code"], "NOT_FOUND");
            assert!(body.get("data").is_none());
            Ok(())
        }

        #[tokio::test]
        async fn rejects_unauthenticated_request() -> anyhow::Result<()> {
            let (status, _) = get(crate::test_support::container().await?, "1", None).await?;
            // 토큰 없는 상세 요청은 인증 실패다.
            assert_eq!(status, StatusCode::UNAUTHORIZED);
            Ok(())
        }
    }
}
