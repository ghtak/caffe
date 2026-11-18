use std::path::Path;

use crate::error::ResultExt as _;

#[derive(serde::Deserialize, Debug, Clone)]
pub struct Config {
    pub trace: espr::tracing::Config,
    pub database: espr::sqlxdb::Config,
    pub cache: espr::cache::Config,
    pub http: espr::http::Config,
    pub jwt: espr::jwt::Config,
    pub argon2: espr::hash::argon2::Config,
    pub auth: crate::app::auth::Config,
}

impl Config {
    pub fn load(path: &Path, env_file: Option<&Path>) -> crate::Result<Self> {
        let env_file = env_file.unwrap_or_else(|| Path::new(".env"));
        match dotenvy::from_path(env_file) {
            Ok(_) => Ok(()),
            Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(())
            }
            Err(error) => Err(crate::Error::internal("Failed to load environment file")
                .at("config.load.env")
                .cause(error)),
        }?;
        espr::config::load(path, Some("CONBIRA"))
            .internal("Failed to load configuration", "config.load")
    }
}
