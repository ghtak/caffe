pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::files::entity::FileSummary;
    use crate::app::files::repository;
    use crate::app::http::extract::ApiQuery;
    use crate::app::http::response::ApiResult;
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    #[derive(serde::Deserialize, espr_derive::Validate)]
    pub struct RequestQuery {
        pub name: Option<String>,
        pub page: Option<u32>,
        pub page_size: Option<u32>,
    }

    #[derive(serde::Serialize)]
    pub struct ResponseData {
        pub items: Vec<FileSummary>,
        pub page: u32,
        pub page_size: u32,
        pub has_next: bool,
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiQuery(request): ApiQuery<RequestQuery>,
    ) -> ApiResult<ResponseData> {
        caller.require_permission("file.list.read", crate::here!())?;
        let page = request.page.unwrap_or(1);
        let page_size = request.page_size.unwrap_or(20);
        if page == 0 || !(1..=100).contains(&page_size) {
            return Err(
                Error::new(Code::InvalidRequest, "Invalid page or page size")
                    .at(crate::here!())
                    .into(),
            );
        }
        let mut connection = container.db.acquire_conn(crate::here!()).await?;
        let mut files = repository::list(
            &mut connection.handle(),
            request.name.as_deref().unwrap_or("").trim(),
            i64::from(page_size) + 1,
            i64::from(page - 1) * i64::from(page_size),
        )
        .await?;
        let has_next = files.len() > page_size as usize;
        files.truncate(page_size as usize);
        Ok(ResponseData {
            items: files.into_iter().map(FileSummary::from).collect(),
            page,
            page_size,
            has_next,
        }
        .into())
    }
}
