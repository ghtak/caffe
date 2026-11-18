use axum::extract::{FromRequest, FromRequestParts, Json, Path, Query, Request};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;

use super::response::ApiResponse;
use crate::{Code, Error};

/// 공통 API 오류 응답을 사용하는 쿼리 파라미터.
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + espr::validate::Validate,
    T::Error: Into<Error>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|error| {
                ApiResponse::from(
                    Error::new(Code::InvalidRequest, "Invalid request query")
                        .at(crate::here!())
                        .cause(error),
                )
                .into_response()
            })?;
        value
            .validate()
            .map_err(|error| ApiResponse::from(error.into()).into_response())?;
        Ok(Self(value))
    }
}

/// 공통 API 오류 응답을 사용하는 경로 파라미터.
pub struct ApiPath<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(value) = Path::<T>::from_request_parts(parts, state)
            .await
            .map_err(|error| {
                let mapped = if error.status().is_server_error() {
                    Error::internal("Internal server error")
                } else {
                    Error::new(Code::InvalidRequest, "Invalid request path")
                };
                let error = mapped.at(crate::here!()).cause(error);
                ApiResponse::from(error).into_response()
            })?;
        Ok(Self(value))
    }
}

/// 공통 API 오류 응답을 사용하는 JSON 요청 본문.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + espr::validate::Validate,
    T::Error: Into<Error>,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(request, state)
            .await
            .map_err(|error| {
                let error = Error::new(Code::InvalidRequest, "Invalid request body")
                    .at(crate::here!())
                    .cause(error);
                ApiResponse::from(error).into_response()
            })?;

        value
            .validate()
            .map_err(|error| ApiResponse::from(error.into()).into_response())?;

        Ok(Self(value))
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use espr_derive::Validate;
    use serde::Deserialize;
    use serde_json::json;
    use tower::ServiceExt;

    use super::{ApiJson, ApiPath, ApiQuery};

    #[derive(Deserialize, Validate)]
    struct Input {
        value: u32,
    }

    async fn handler(ApiJson(input): ApiJson<Input>) -> String { input.value.to_string() }

    #[tokio::test]
    async fn invalid_query_returns_common_error_response() {
        #[derive(Deserialize, Validate)]
        struct Search {
            page: u32,
        }
        let router = Router::new().route(
            "/",
            get(|ApiQuery(query): ApiQuery<Search>| async move { query.page.to_string() }),
        );
        let response = router
            .oneshot(Request::get("/?page=invalid").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // 숫자가 아닌 페이지는 공통 요청 오류를 반환한다.
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            json!({"code": "INVALID_REQUEST", "message": "Invalid request query"})
        );
    }

    #[tokio::test]
    async fn path_extracts_valid_id_and_returns_common_error_for_invalid_id() {
        let router = Router::new().route(
            "/{id}",
            get(|ApiPath(id): ApiPath<i64>| async move { id.to_string() }),
        );

        let response = router
            .clone()
            .oneshot(Request::get("/42").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // 숫자 ID가 핸들러에 전달된다.
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            &to_bytes(response.into_body(), 4096).await.unwrap()[..],
            b"42"
        );

        let response = router
            .oneshot(Request::get("/invalid").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // 변환할 수 없는 ID는 공통 오류 응답을 반환한다.
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            json!({"code": "INVALID_REQUEST", "message": "Invalid request path"})
        );
    }

    #[tokio::test]
    async fn path_configuration_errors_return_internal_error() {
        let router = Router::new().route("/", get(|ApiPath(_): ApiPath<i64>| async { "ok" }));
        let response = router
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // 서버의 경로 설정 오류는 클라이언트 요청 오류로 취급하지 않는다.
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            json!({"code": "INTERNAL", "message": "Internal server error"})
        );
    }

    #[derive(Deserialize)]
    struct CheckedInput {
        name: String,
    }

    impl espr::validate::Validate for CheckedInput {
        type Error = crate::Error;

        fn validate(&self) -> Result<(), Self::Error> {
            if self.name.is_empty() {
                return Err(crate::Error::new(
                    crate::Code::InvalidRequest,
                    "Name is required",
                ));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn query_validation_failure_returns_request_error() {
        let router = Router::new().route(
            "/",
            get(|ApiQuery(_): ApiQuery<CheckedInput>| async { "ok" }),
        );
        let response = router
            .oneshot(Request::get("/?name=").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // 쿼리 검증 오류의 코드와 공개 메시지가 응답에 전달된다.
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            json!({"code": "INVALID_REQUEST", "message": "Name is required"})
        );
    }

    #[tokio::test]
    async fn validation_failure_returns_request_error() {
        let router = Router::new().route(
            "/",
            post(|ApiJson(_): ApiJson<CheckedInput>| async { "ok" }),
        );
        let request = Request::post("/")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":""}"#))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        // 검증 오류의 코드와 공개 메시지가 응답에 전달된다.
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            json!({"code": "INVALID_REQUEST", "message": "Name is required"})
        );
    }

    #[tokio::test]
    async fn valid_json_is_extracted_for_the_handler() {
        let router = Router::new().route("/", post(handler));
        let request = Request::post("/")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"value":42}"#))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        // 추출된 값이 핸들러에 전달된다.
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            &to_bytes(response.into_body(), 4096).await.unwrap()[..],
            b"42"
        );
    }

    #[tokio::test]
    async fn invalid_json_requests_return_bad_request_with_common_error_body() {
        let router = Router::new().route("/", post(handler));
        for (body, content_type) in [
            ("{", "application/json"),
            (r#"{"value":"x"}"#, "application/json"),
            ("{}", "text/plain"),
        ] {
            let request = Request::post("/")
                .header("content-type", content_type)
                .body(Body::from(body))
                .unwrap();
            let response = router.clone().oneshot(request).await.unwrap();
            // 문법, 데이터 타입, 콘텐츠 타입 오류 모두 같은 응답을 반환한다.
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                json!({"code": "INVALID_REQUEST", "message": "Invalid request body"})
            );
        }
    }
}
