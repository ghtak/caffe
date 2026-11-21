pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use espr_derive::Validate;
    use serde::Deserialize;

    use crate::app::Container;
    use crate::app::http::extract::ApiJson;
    use crate::app::http::response::ApiResult;
    use crate::app::sample::entity::Sample;

    #[derive(Debug, Deserialize, Validate)]
    pub struct RequestBody {
        pub name: String,
    }

    pub type ResponseData = Sample;

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiJson(body): ApiJson<RequestBody>,
    ) -> ApiResult<ResponseData> {
        let sample =
            super::usecase::execute(&container.db, super::usecase::Command { name: body.name })
                .await?;
        Ok(sample.into())
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::container::db::Database;
    use crate::app::sample::entity::Sample;
    use crate::app::sample::repository::{self, NewSample};

    pub struct Command {
        pub name: String,
    }

    pub async fn execute(db: &Database, command: Command) -> crate::Result<Sample> {
        let mut connection = db.acquire_conn(crate::here!()).await?;
        let mut handle = connection.handle();
        let sample = NewSample { name: command.name };
        repository::insert(&mut handle, &sample).await
    }
}
