use espr::{cache, sqlxdb};

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
    pub password_hash_service: crate::app::auth::service::password_hash::PasswordHashService,
    pub dummy_password_hash: String,
    pub jwt_token_service: espr::jwt::TokenService,
    pub refresh_token_service: crate::app::auth::service::refresh_token::RefreshTokenService,
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

        let password_hasher = std::sync::Arc::new(
            espr::hash::argon2::Hasher::new(&config.argon2)
                .internal("Failed to initialize password hasher", "app.container.new")?,
        );
        let password_hash_service =
            crate::app::auth::service::password_hash::PasswordHashService::new(
                password_hasher,
                &config.auth.password_hash,
            )?;
        let dummy_password_hash = password_hash_service
            .hash("dummy login password".into())
            .await?;

        let jwt_token_service = espr::jwt::TokenService::new(&config.jwt)
            .internal("Failed to initialize jwt", "app.container.new")?;

        let refresh_token_service =
            crate::app::auth::service::refresh_token::RefreshTokenService::new(
                config.auth.refresh_token.clone(),
            );

        Ok(Self {
            config: config.clone(),
            db,
            cache,
            http_server,
            password_hash_service,
            dummy_password_hash,
            jwt_token_service,
            refresh_token_service,
        })
    }
}

#[async_trait::async_trait]
pub trait DatabaseExt {
    async fn acquire_conn(&self, at: &'static str) -> crate::Result<sqlxdb::Conn<sqlx::Postgres>>;
    async fn begin_tx(&self, at: &'static str) -> crate::Result<sqlxdb::Tx<sqlx::Postgres>>;
}

#[async_trait::async_trait]
impl DatabaseExt for db::Database {
    async fn acquire_conn(&self, at: &'static str) -> crate::Result<sqlxdb::Conn<sqlx::Postgres>> {
        self.conn()
            .await
            .internal("Failed to get db connection", at)
    }

    async fn begin_tx(&self, at: &'static str) -> crate::Result<sqlxdb::Tx<sqlx::Postgres>> {
        self.tx().await.internal("Failed to get db transaction", at)
    }
}
