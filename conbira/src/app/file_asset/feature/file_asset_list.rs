pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::auth::claims::UserClaims;
    use crate::app::http::extract::ApiQuery;
    use crate::app::http::response::ApiResult;
    use crate::app::{Container, DatabaseExt as _};
    use crate::{Code, Error};

    #[derive(serde::Deserialize, espr_derive::Validate)]
    pub struct RequestQuery {
        pub page: Option<u32>,
        pub page_size: Option<u32>,
    }
    #[derive(serde::Serialize)]
    pub struct ResponseData {
        items: Vec<super::super::super::repository::FileAssetSummary>,
        page: u32,
        page_size: u32,
        has_next: bool,
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiQuery(query): ApiQuery<RequestQuery>,
    ) -> ApiResult<ResponseData> {
        caller.require_permission("file_asset.list.read", crate::here!())?;
        let page = query.page.unwrap_or(1);
        let page_size = query.page_size.unwrap_or(20);
        if page == 0 || !(1..=100).contains(&page_size) {
            return Err(
                Error::new(Code::InvalidRequest, "Invalid page or page size")
                    .at(crate::here!())
                    .into(),
            );
        }
        let mut conn = container.db.acquire_conn(crate::here!()).await?;
        let mut items = super::super::super::repository::list(
            &mut conn.handle(),
            i64::from(page_size) + 1,
            i64::from(page - 1) * i64::from(page_size),
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
