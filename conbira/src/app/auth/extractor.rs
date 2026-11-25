use std::sync::Arc;

use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};

use crate::app::Container;
use crate::app::auth::claims::{AuthClaims, UserClaims};

impl FromRequestParts<Arc<Container>> for UserClaims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<Container>,
    ) -> Result<Self, Self::Rejection> {
        <Self as OptionalFromRequestParts<Arc<Container>>>::from_request_parts(parts, state)
            .await?
            .ok_or_else(|| bearer_rejection("Authentication required", crate::here!()))
    }
}

impl OptionalFromRequestParts<Arc<Container>> for UserClaims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<Container>,
    ) -> Result<Option<Self>, Self::Rejection> {
        let claims = <AuthClaims as OptionalFromRequestParts<Arc<Container>>>::from_request_parts(
            parts, state,
        )
        .await?;
        claims
            .map(UserClaims::try_from)
            .transpose()
            .map_err(|_| bearer_rejection("Invalid access token", crate::here!()))
    }
}

impl FromRequestParts<Arc<Container>> for AuthClaims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<Container>,
    ) -> Result<Self, Self::Rejection> {
        <Self as OptionalFromRequestParts<Arc<Container>>>::from_request_parts(parts, state)
            .await?
            .ok_or_else(|| bearer_rejection("Authentication required", crate::here!()))
    }
}

impl OptionalFromRequestParts<Arc<Container>> for AuthClaims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<Container>,
    ) -> Result<Option<Self>, Self::Rejection> {
        extract_claims(&parts.headers, &state.jwt_token_service)
            .map(|claims| claims.map(Self))
            .map_err(|()| bearer_rejection("Invalid access token", crate::here!()))
    }
}

pub(super) fn bearer_rejection(message: &'static str, at: &'static str) -> Response {
    let mut response = crate::app::http::response::ApiResponse::<()>::from(
        crate::Error::new(crate::Code::Unauthorized, message).at(at),
    )
    .into_response();
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

fn extract_claims(
    headers: &HeaderMap,
    service: &espr::jwt::TokenService,
) -> Result<Option<espr::jwt::Claims>, ()> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    let value = value.to_str().map_err(|_| ())?;
    let (scheme, token) = value.split_once(' ').ok_or(())?;
    let token = token.trim_start_matches(' ');
    if !scheme.eq_ignore_ascii_case("Bearer")
        || token.is_empty()
        || token.bytes().any(|b| b.is_ascii_whitespace())
    {
        return Err(());
    }
    service.verify(token).map(Some).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    mod contract {
        use std::sync::Arc;

        use axum::body::{Body, to_bytes};
        use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};
        use tower::ServiceExt as _;

        use super::super::extract_claims;
        use crate::app::auth::claims::{AuthClaims, UserClaims};

        #[test]
        fn optional_claims_can_be_used_as_a_handler_extractor() {
            async fn handler(_: Option<AuthClaims>) {}
            // Axum 핸들러에서 선택적 인증 추출기로 사용 가능하다.
            let _: axum::Router<Arc<crate::app::Container>> =
                axum::Router::new().route("/", axum::routing::get(handler));
        }

        #[test]
        fn optional_claims_only_accepts_valid_bearer_tokens() -> anyhow::Result<()> {
            let service = crate::test_support::jwt_service("test", "test", 900)?;
            let token = service.issue("user-1", Default::default())?.token;
            let mut headers = HeaderMap::new();
            // 헤더가 없는 요청은 익명 요청으로 처리한다.
            assert!(extract_claims(&headers, &service).unwrap().is_none());
            headers.insert(header::AUTHORIZATION, format!("bearer {token}").parse()?);
            // 유효한 토큰만 검증된 사용자 클레임을 반환한다.
            assert_eq!(
                extract_claims(&headers, &service).unwrap().unwrap().sub,
                "user-1"
            );
            for value in [
                "Basic abc",
                "Bearer",
                "Bearer ",
                "Bearer invalid",
                "Bearer a b",
            ] {
                headers.insert(header::AUTHORIZATION, HeaderValue::from_str(value)?);
                assert!(extract_claims(&headers, &service).is_err(), "{value}");
            }
            headers.insert(header::AUTHORIZATION, format!("Bearer {token}").parse()?);
            headers.append(
                header::AUTHORIZATION,
                HeaderValue::from_static("Bearer invalid"),
            );
            // 중복 인증 헤더는 하나를 임의로 선택하지 않고 거부한다.
            assert!(extract_claims(&headers, &service).is_err());
            Ok(())
        }
        async fn user_claims_router() -> anyhow::Result<(axum::Router, Arc<crate::app::Container>)>
        {
            let container = crate::test_support::container().await?;

            async fn required(user: UserClaims) -> String { user.id.to_string() }
            async fn optional(user: Option<UserClaims>) -> String {
                user.map_or_else(|| "anonymous".into(), |user| user.id.to_string())
            }
            let router = axum::Router::new()
                .route("/required", axum::routing::get(required))
                .route("/optional", axum::routing::get(optional))
                .with_state(Arc::clone(&container));
            Ok((router, container))
        }

        #[tokio::test]
        async fn user_claims_without_header_are_required_or_anonymous() -> anyhow::Result<()> {
            let (router, _) = user_claims_router().await?;
            let required = router
                .clone()
                .oneshot(Request::builder().uri("/required").body(Body::empty())?)
                .await?;
            // 필수 추출은 인증이 없으면 Bearer 챌린지와 401을 반환한다.
            assert_eq!(required.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(required.headers()[header::WWW_AUTHENTICATE], "Bearer");

            let optional = router
                .oneshot(Request::builder().uri("/optional").body(Body::empty())?)
                .await?;
            // 선택 추출은 인증이 없으면 익명으로 처리한다.
            assert_eq!(optional.status(), StatusCode::OK);
            assert_eq!(
                to_bytes(optional.into_body(), 4096).await?.as_ref(),
                b"anonymous"
            );
            Ok(())
        }

        #[tokio::test]
        async fn user_claims_extractors_accept_matching_user() -> anyhow::Result<()> {
            let (router, container) = user_claims_router().await?;
            let user = serde_json::json!({"id": 42, "family_id": "00000000-0000-0000-0000-000000000000", "status": "active"});
            let token = container
                .jwt_token_service
                .issue("42", user.as_object().unwrap().clone())?
                .token;
            for path in ["/required", "/optional"] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri(path)
                            .header(header::AUTHORIZATION, format!("Bearer {token}"))
                            .body(Body::empty())?,
                    )
                    .await?;
                // 두 추출기 모두 검증된 사용자 ID를 전달한다.
                assert_eq!(response.status(), StatusCode::OK, "{path}");
                assert_eq!(
                    to_bytes(response.into_body(), 4096).await?.as_ref(),
                    b"42",
                    "{path}"
                );
            }
            Ok(())
        }

        #[tokio::test]
        async fn user_claims_extractors_reject_invalid_tokens_and_user_claims() -> anyhow::Result<()>
        {
            let (router, container) = user_claims_router().await?;
            for (subject, user) in [
                (
                    "43",
                    serde_json::json!({"id": 42, "family_id": "00000000-0000-0000-0000-000000000000", "status": "active"}),
                ),
                ("42", serde_json::json!({})),
                ("42", serde_json::json!({"id": 42, "status": "unknown"})),
                ("42", serde_json::json!({"id": "42", "status": "active"})),
            ] {
                let token = container
                    .jwt_token_service
                    .issue(subject, user.as_object().unwrap().clone())?
                    .token;
                for path in ["/required", "/optional"] {
                    let response = router
                        .clone()
                        .oneshot(
                            Request::builder()
                                .uri(path)
                                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                                .body(Body::empty())?,
                        )
                        .await?;
                    // 불일치하거나 잘못된 사용자 클레임은 Bearer 챌린지와 401을
                    // 반환한다.
                    assert_eq!(
                        response.status(),
                        StatusCode::UNAUTHORIZED,
                        "{path}: {user}"
                    );
                    assert_eq!(
                        response.headers()[header::WWW_AUTHENTICATE],
                        "Bearer",
                        "{path}: {user}"
                    );
                }
            }
            for path in ["/required", "/optional"] {
                let response = router
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri(path)
                            .header(header::AUTHORIZATION, "Bearer invalid")
                            .body(Body::empty())?,
                    )
                    .await?;
                // 잘못된 토큰은 Bearer 챌린지와 401을 반환한다.
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
                assert_eq!(
                    response.headers()[header::WWW_AUTHENTICATE],
                    "Bearer",
                    "{path}"
                );
            }
            Ok(())
        }
    }
}
