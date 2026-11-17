use espr::cache;

use crate::config::Config;
use crate::error::ResultExt;

pub mod db {
    pub type Database = espr::sqlxdb::Database<sqlx::Postgres>;
    pub type Handle<'a> = espr::sqlxdb::Handle<'a, sqlx::Postgres>;
    pub type QueryBuilder = sqlx::QueryBuilder<sqlx::Postgres>;
}

pub struct Container {
    pub config: Config,
    pub db: db::Database,
    pub cache: espr::cache::Cache,
    pub http_server: espr::http::Server,
}

impl Container {
    pub async fn new(config: &Config) -> crate::Result<Self> {
        let db = db::Database::new(&config.database)
            .await
            .internal("Failed to initialize database", "app.container.new")?;

        let cache = cache::Cache::new(&config.cache)
            .await
            .internal("Failed to initialize cache", "app.container.new")?;

        let http_server = espr::http::Server::new(&config.http)
            .await
            .internal("Failed to initialize http", "app.container.new")?;

        Ok(Self {
            config: config.clone(),
            db,
            cache,
            http_server,
        })
    }
}
