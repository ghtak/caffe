pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use espr_derive::Validate;
    use serde::Deserialize;

    use crate::app::Container;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::{ApiResponse, ApiResult};
    use crate::app::sample::entity::Sample;

    #[derive(Debug, Deserialize, Validate)]
    pub struct CreateRequest {
        pub name: String,
    }

    pub type CreateResponse = Sample;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiJson(request): ApiJson<CreateRequest>,
    ) -> ApiResult<CreateResponse> {
        let sample = super::usecase::execute(
            &container.db,
            super::usecase::Command { name: request.name },
        )
        .await?;
        Ok(ApiResponse::new(sample))
    }
}

pub mod usecase {
    use crate::app::container::db::Database;
    use crate::app::sample::entity::Sample;
    use crate::app::sample::repository::{self, NewSample};

    pub struct Command {
        pub name: String,
    }

    pub async fn execute(db: &Database, command: Command) -> crate::Result<Sample> {
        let mut connection = db.conn().await.map_err(|error| {
            crate::Error::internal("Failed to acquire sample connection")
                .at("sample.create")
                .cause(error)
        })?;
        let mut handle = connection.handle();
        let sample = NewSample { name: command.name };
        repository::insert(&mut handle, &sample).await
    }
}
