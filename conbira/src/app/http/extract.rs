use axum::extract::{FromRequest, Json, Request};
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;

use super::response::ApiResponse;
use crate::{Code, Error};

/// 공통 API 오류 응답을 사용하는 JSON 요청 본문.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(request, state)
            .await
            .map_err(|error| {
                let error = Error::new(Code::InvalidRequest, "Invalid request body")
                    .at("http.extract.json")
                    .cause(error);
                ApiResponse::from(error).into_response()
            })?;
        Ok(Self(value))
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use serde::Deserialize;
    use serde_json::json;
    use tower::ServiceExt;

    use super::ApiJson;

    #[derive(Deserialize)]
    struct Input {
        value: u32,
    }

    async fn handler(ApiJson(input): ApiJson<Input>) -> String { input.value.to_string() }

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
