use std::path::Path;

#[derive(serde::Deserialize, Debug, Clone)]
pub struct Config {
    pub trace: espr::tracing::Config,
    pub https: Option<espr::http::Config>,
    pub jwt: Option<espr::jwt::Config>,
}

impl Config {
    pub fn load(path: &Path, env_file: Option<&Path>) -> espr::Result<Self> {
        let env_file = env_file.unwrap_or_else(|| Path::new(".env"));
        match dotenvy::from_path(env_file) {
            Ok(_) => Ok(()),
            Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(())
            }
            Err(error) => Err(espr::Error::Internal(error.into())),
        }?;
        espr::config::load(path, None)
    }
}
