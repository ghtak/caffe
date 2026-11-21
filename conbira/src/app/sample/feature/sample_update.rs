pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use espr_derive::Validate;
    use serde::Deserialize;

    use crate::app::Container;
    use crate::app::http::extract::{ApiJson, ApiPath};
    use crate::app::http::response::ApiResult;
    use crate::app::sample::entity::Sample;

    #[derive(Debug, Deserialize, Validate)]
    pub struct RequestBody {
        pub name: String,
    }

    pub type ResponseData = Sample;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiPath(id): ApiPath<i64>,
        ApiJson(request): ApiJson<RequestBody>,
    ) -> ApiResult<ResponseData> {
        let sample = super::usecase::execute(
            &container.db,
            super::usecase::Command {
                id,
                name: request.name,
            },
        )
        .await?;
        Ok(sample.into())
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::container::db::Database;
    use crate::app::sample::entity::Sample;
    use crate::app::sample::repository;
    use crate::{Code, Error};

    pub struct Command {
        pub id: i64,
        pub name: String,
    }

    pub async fn execute(db: &Database, command: Command) -> crate::Result<Sample> {
        let mut connection = db.acquire_conn(crate::here!()).await?;
        let mut handle = connection.handle();
        let mut sample = repository::find_by_id(&mut handle, command.id)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "Sample not found").at(crate::here!()))?;
        sample.name = command.name;
        repository::update(&mut handle, &sample)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "Sample not found").at(crate::here!()))
    }
}
