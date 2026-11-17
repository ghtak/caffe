use crate::config::Config;

pub mod db {
    pub type Database = espr::sqlxdb::Database<sqlx::Postgres>;
    pub type Handle<'a> = espr::sqlxdb::Handle<'a, sqlx::Postgres>;
    pub type QueryBuilder = sqlx::QueryBuilder<sqlx::Postgres>;
}

pub struct Container {
    pub config: Config,
    pub db: db::Database,
}

impl Container {
    pub async fn new(config: &Config) -> espr::Result<Self> {
        let db = db::Database::new(&config.database).await?;

        Ok(Self {
            config: config.clone(),
            db,
        })
    }
}
