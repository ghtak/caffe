pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::http::extract::ApiPath;
    use crate::app::http::response::{ApiResponse, ApiResult};
    use crate::app::sample::entity::Sample;

    pub type ReadResponse = Sample;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiPath(id): ApiPath<i64>,
    ) -> ApiResult<ReadResponse> {
        Ok(ApiResponse::new(
            super::usecase::execute(&container.db, id).await?,
        ))
    }
}

pub mod usecase {
    use crate::app::container::db::Database;
    use crate::app::sample::entity::Sample;
    use crate::app::sample::repository;
    use crate::error::ResultExt as _;
    use crate::{Code, Error};

    pub async fn execute(db: &Database, id: i64) -> crate::Result<Sample> {
        let mut connection = db
            .conn()
            .await
            .internal("Failed to acquire sample connection", "sample.read")?;
        repository::find_by_id(&mut connection.handle(), id)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "Sample not found").at("sample.read"))
    }
}
