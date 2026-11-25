pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;

    use crate::app::Container;
    use crate::app::http::response::ApiResult;
    use crate::{Code, Error};

    pub async fn handler(
        State(container): State<Arc<Container>>,
    ) -> ApiResult<super::usecase::Output> {
        let oidc = container.google_oidc_service.as_ref().ok_or_else(|| {
            Error::new(Code::NotFound, "Google login is disabled").at(crate::here!())
        })?;
        let output = super::usecase::execute(&container.cache, oidc).await?;
        Ok(output.into())
    }
}

pub mod usecase {
    use std::time::Duration;

    use espr::cache::Cache;
    use openidconnect::CsrfToken;
    use openidconnect::url::Url;

    use crate::app::auth::service::google_oidc::GoogleOidcService;
    use crate::{Code, Error, Result};

    pub(in crate::app::auth::feature) fn state_key(state: &str, browser_proof: &str) -> String {
        format!(
            "conbira:auth:google-oidc:{}:{state}{browser_proof}",
            state.len()
        )
    }

    #[derive(serde::Serialize)]
    pub struct Output {
        pub authorization_url: Url,
        pub state: String,
        pub browser_proof: String,
    }

    pub async fn execute(cache: &Cache, oidc: &GoogleOidcService) -> Result<Output> {
        let login = oidc.begin_login();
        let state = login.state.value().to_owned();
        let browser_proof = CsrfToken::new_random().secret().to_owned();
        let key = state_key(&state, &browser_proof);
        let value =
            serde_json::to_string(&login.state).expect("Google login state is JSON-serializable");
        cache
            .set_with_ttl(key, value, Duration::from_secs(300))
            .await
            .map_err(|_| {
                Error::new(Code::Busy, "Google login is temporarily unavailable").at(crate::here!())
            })?;
        Ok(Output {
            authorization_url: login.authorization_url,
            state,
            browser_proof,
        })
    }
}
