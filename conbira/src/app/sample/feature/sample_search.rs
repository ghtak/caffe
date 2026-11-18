pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use espr_derive::Validate;
    use serde::{Deserialize, Serialize};

    use crate::app::Container;
    use crate::app::http::extract::ApiQuery;
    use crate::app::http::response::{ApiResponse, ApiResult};
    use crate::app::sample::entity::Sample;

    #[derive(Debug, Deserialize, Validate)]
    pub struct SearchRequest {
        /// 이름 부분 검색. 생략하면 전체 조회.
        pub name: Option<String>,
        /// 1부터 시작하는 페이지 번호. 생략하면 1.
        pub page: Option<u32>,
        /// 페이지당 건수. 생략하면 20, 최대 100.
        pub page_size: Option<u32>,
    }

    #[derive(Debug, Serialize)]
    pub struct SearchResponse {
        pub items: Vec<Sample>,
        pub page: u32,
        pub page_size: u32,
        pub has_next: bool,
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        ApiQuery(request): ApiQuery<SearchRequest>,
    ) -> ApiResult<SearchResponse> {
        let page = request.page.unwrap_or(1);
        let page_size = request.page_size.unwrap_or(20);
        let result = super::usecase::execute(
            &container.db,
            super::usecase::Query {
                name: request.name,
                page,
                page_size,
            },
        )
        .await?;
        Ok(ApiResponse::new(SearchResponse {
            items: result.items,
            page,
            page_size,
            has_next: result.has_next,
        }))
    }
}

pub mod usecase {
    use crate::app::DatabaseExt as _;
    use crate::app::container::db::Database;
    use crate::app::sample::entity::Sample;
    use crate::app::sample::repository;
    use crate::{Code, Error};

    pub struct Query {
        pub name: Option<String>,
        pub page: u32,
        pub page_size: u32,
    }

    pub struct SearchResult {
        pub items: Vec<Sample>,
        pub has_next: bool,
    }

    pub async fn execute(db: &Database, query: Query) -> crate::Result<SearchResult> {
        if query.page == 0 || !(1..=100).contains(&query.page_size) {
            return Err(
                Error::new(Code::InvalidRequest, "Invalid page or page size").at(crate::here!()),
            );
        }
        let offset = i64::from(query.page - 1) * i64::from(query.page_size);
        let mut connection = db.acquire_conn(crate::here!()).await?;
        let mut items = repository::search(
            &mut connection.handle(),
            &repository::Criteria {
                name: query.name.as_deref(),
                limit: i64::from(query.page_size) + 1,
                offset,
            },
        )
        .await?;
        let has_next = items.len() > query.page_size as usize;
        items.truncate(query.page_size as usize);
        Ok(SearchResult { items, has_next })
    }
}
