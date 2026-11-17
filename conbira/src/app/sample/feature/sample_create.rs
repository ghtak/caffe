pub mod endpoint {
    use espr_derive::Validate;
    use serde::Deserialize;

    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::{ApiResponse, ApiResult};

    #[derive(Debug, Deserialize, Validate)]
    pub struct CreateRequest {}

    pub async fn handler(ApiJson(_request): ApiJson<CreateRequest>) -> ApiResult<()> {
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
