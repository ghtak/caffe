pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::board::entity::PostSummary;
    use crate::app::board::repository::post;
    use crate::app::http::extract::ApiQuery;
    use crate::app::http::response::ApiResult;
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    #[derive(serde::Deserialize, espr_derive::Validate)]
    pub struct RequestQuery {
        /// 제목 또는 본문 LIKE 부분 검색. %와 _는 SQL 와일드카드.
        pub search: Option<String>,
        pub author_id: Option<i64>,
        pub board_slug: String,
        pub page: Option<u32>,
        pub page_size: Option<u32>,
    }

    #[derive(serde::Serialize)]
    pub struct ResponseData {
        pub items: Vec<PostSummary>,
        pub page: u32,
        pub page_size: u32,
        pub has_next: bool,
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiQuery(request): ApiQuery<RequestQuery>,
    ) -> ApiResult<ResponseData> {
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
        let mut items = post::search(
            &mut connection.handle(),
            &post::Criteria {
                search: request.search.as_deref(),
                author_id: request.author_id,
                board_slug: &request.board_slug,
                limit: i64::from(page_size) + 1,
                offset: i64::from(page - 1) * i64::from(page_size),
            },
        )
        .await?;
        let has_next = items.len() > page_size as usize;
        items.truncate(page_size as usize);
        Ok(ResponseData {
            items,
            page,
            page_size,
            has_next,
        }
        .into())
    }
}
