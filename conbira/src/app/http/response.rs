use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::{Code, Error};

/// HTTP 핸들러의 성공 및 오류 응답.
pub type ApiResult<T> = Result<ApiResponse<T>, ApiResponse<()>>;

/// JSON API 응답. 성공에는 선택적 데이터, 오류에는 공개 메시지를 담는다.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    #[serde(skip)]
    status: StatusCode,
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            status: StatusCode::OK,
            code: "OK",
            data: Some(data),
            message: None,
        }
    }
}

impl ApiResponse<()> {
    pub fn empty() -> Self {
        Self {
            status: StatusCode::OK,
            code: "OK",
            data: None,
            message: None,
        }
    }

    fn error(status: StatusCode, code: &'static str, message: String) -> Self {
        Self {
            status,
            code,
            data: None,
            message: Some(message),
        }
    }
}

impl<T> ApiResponse<T> {
    /// 엔드포인트의 맥락에 맞게 기본 HTTP 상태를 변경한다.
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }
}

/// 보유한 상태와 본문을 HTTP 응답으로 변환한다.
impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        let status = self.status;
        match serde_json::to_vec(&self) {
            Ok(body) => {
                (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
            }
            Err(error) => {
                tracing::error!(%error, "failed to serialize HTTP response");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    r#"{"code":"INTERNAL","message":"Internal server error"}"#,
                )
                    .into_response()
            }
        }
    }
}

/// 오류 코드별 기본 HTTP 상태와 공개 문자열.
fn error_mapping(code: Code) -> (StatusCode, &'static str) {
    match code {
        Code::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL"),
        Code::InvalidConfiguration => (StatusCode::INTERNAL_SERVER_ERROR, "INVALID_CONFIGURATION"),
        Code::InvalidRequest => (StatusCode::BAD_REQUEST, "INVALID_REQUEST"),
        Code::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
        Code::Forbidden => (StatusCode::FORBIDDEN, "FORBIDDEN"),
        Code::Busy => (StatusCode::SERVICE_UNAVAILABLE, "BUSY"),
        Code::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND"),
    }
}

impl From<Error> for ApiResponse<()> {
    fn from(error: Error) -> Self {
        tracing::error!(
            error.code = ?error.code,
            error.message = %error.message,
            error.at = error.at,
            error.cause = error.cause.as_ref().map(|cause| format!("{cause:#}")),
            "HTTP request failed"
        );
        let (status, code) = error_mapping(error.code);
        Self::error(status, code, error.message.into())
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use axum::routing::get;
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;

    #[derive(Serialize)]
    struct TestSample {
        id: u32,
    }

    async fn typed_response() -> ApiResponse<TestSample> {
        ApiResponse::new(TestSample { id: 123 })
    }

    async fn typed_result() -> ApiResult<TestSample> {
        let sample: crate::Result<TestSample> = Ok(TestSample { id: 456 });
        Ok(ApiResponse::new(sample?))
    }

    async fn typed_error() -> ApiResult<TestSample> {
        let sample: crate::Result<TestSample> = Err(Error::internal("공개 오류 메시지"));
        Ok(ApiResponse::new(sample?))
    }

    async fn body(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn typed_responses_and_http_results_work_as_axum_handlers() {
        let router = Router::new()
            .route("/response", get(typed_response))
            .route("/success", get(typed_result))
            .route("/error", get(typed_error));

        for (path, status, expected) in [
            (
                "/response",
                StatusCode::OK,
                json!({"code": "OK", "data": {"id": 123}}),
            ),
            (
                "/success",
                StatusCode::OK,
                json!({"code": "OK", "data": {"id": 456}}),
            ),
            (
                "/error",
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"code": "INTERNAL", "message": "공개 오류 메시지"}),
            ),
        ] {
            let response = router
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            // 명시된 반환 타입이 Axum의 HTTP 응답으로 변환된다.
            assert_eq!(response.status(), status);
            assert_eq!(body(response).await, expected);
        }
    }

    #[tokio::test]
    async fn success_responses_include_only_present_data_and_use_selected_status() {
        let response = ApiResponse::new(json!({"id": "123"}))
            .with_status(StatusCode::CREATED)
            .into_response();
        // 지정한 상태와 데이터가 응답에 포함된다.
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            body(response).await,
            json!({"code": "OK", "data": {"id": "123"}})
        );

        let response = ApiResponse::empty().into_response();
        // 데이터 없는 성공은 code만 포함한다.
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body(response).await, json!({"code": "OK"}));
    }

    #[tokio::test]
    async fn errors_include_their_message_without_location_or_cause() {
        for (code, status, public_code) in [
            (
                Code::Internal,
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL",
            ),
            (
                Code::InvalidConfiguration,
                StatusCode::INTERNAL_SERVER_ERROR,
                "INVALID_CONFIGURATION",
            ),
            (
                Code::InvalidRequest,
                StatusCode::BAD_REQUEST,
                "INVALID_REQUEST",
            ),
            (Code::Unauthorized, StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            (Code::Forbidden, StatusCode::FORBIDDEN, "FORBIDDEN"),
            (Code::Busy, StatusCode::SERVICE_UNAVAILABLE, "BUSY"),
            (Code::NotFound, StatusCode::NOT_FOUND, "NOT_FOUND"),
        ] {
            let error = Error::new(code, "공개 오류 메시지")
                .at("secret location")
                .cause(anyhow::anyhow!("secret cause"));
            let response = ApiResponse::from(error).into_response();
            // 코드에 맞는 상태와 공개 메시지를 반환하고 위치와 원인은 제외한다.
            assert_eq!(response.status(), status);
            assert_eq!(
                body(response).await,
                json!({"code": public_code, "message": "공개 오류 메시지"})
            );
        }
    }

    #[tokio::test]
    async fn serialization_failure_returns_safe_internal_error() {
        struct Broken;
        impl Serialize for Broken {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("secret serialization failure"))
            }
        }

        let response = ApiResponse::new(Broken).into_response();
        // 직렬화 오류도 내부 내용을 노출하지 않는 JSON 응답으로 변환한다.
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            body(response).await,
            json!({"code": "INTERNAL", "message": "Internal server error"})
        );
    }
}
