#[derive(serde::Deserialize, Debug, Clone)]
pub(crate) struct Config {
    pub(crate) trace: espr::tracing::Config,
    pub(crate) https: Option<espr::http::Config>,
    pub(crate) jwt: Option<espr::jwt::Config>,
}

pub(crate) struct Container {
    _config: Config,
}

impl Container {
    pub async fn new(config: &Config) -> espr::Result<Self> {
        Ok(Self {
            _config: config.clone(),
        })
    }
}
