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
    pub async fn new(config: &Config) -> crate::Result<Self> {
        let db = db::Database::new(&config.database).await.map_err(|error| {
            crate::Error::internal("Failed to initialize database")
                .at("app.container.new")
                .cause(error)
        })?;

        Ok(Self {
            config: config.clone(),
            db,
        })
    }
}
