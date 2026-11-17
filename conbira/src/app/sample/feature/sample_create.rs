pub mod endpoint {
    use axum::Json;
    use axum::extract::rejection::JsonRejection;
    use serde::Deserialize;

    use crate::app::http::response::{ApiResponse, ApiResult};

    #[derive(Debug, Deserialize)]
    pub struct CreateRequest {}

    pub async fn handler(_cmd: Result<Json<CreateRequest>, JsonRejection>) -> ApiResult<()> {
        Ok(ApiResponse::empty())
    }
}

pub mod usecase {

    #[allow(dead_code)]
    pub struct Command {}

    #[allow(dead_code)]
    pub struct Query {}

    #[allow(dead_code)]
    pub async fn execute(_: Command) -> crate::Result<()> { todo!() }
}
