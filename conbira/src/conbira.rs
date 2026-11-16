#[derive(serde::Deserialize, Debug)]
pub(crate) struct Config {
    pub(crate) trace: espr::tracing::Config,
}
