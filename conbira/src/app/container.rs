use crate::config::Config;

pub struct Container {
    _config: Config,
}

impl Container {
    pub async fn new(config: &Config) -> espr::Result<Self> {
        Ok(Self {
            _config: config.clone(),
        })
    }
}
