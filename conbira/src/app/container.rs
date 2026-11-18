use espr::cache;
use espr::hash::PasswordHasher as _;

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
    pub password_hasher: std::sync::Arc<espr::hash::argon2::Hasher>,
    pub dummy_password_hash: String,
    pub jwt_token_service: espr::jwt::TokenService,
    pub refresh_token_service: crate::app::auth::service::RefreshTokenService,
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
        let dummy_hasher = std::sync::Arc::clone(&password_hasher);
        let dummy_password_hash =
            tokio::task::spawn_blocking(move || dummy_hasher.hash("dummy login password"))
                .await
                .internal(
                    "Failed to run dummy password hashing task",
                    "app.container.new",
                )?
                .internal("Failed to hash dummy password", "app.container.new")?;

        let jwt_token_service = espr::jwt::TokenService::new(&config.jwt)
            .internal("Failed to initialize jwt", "app.container.new")?;

        let refresh_token_service =
            crate::app::auth::service::RefreshTokenService::new(config.auth.clone());

        Ok(Self {
            config: config.clone(),
            db,
            cache,
            http_server,
            password_hasher,
            dummy_password_hash,
            jwt_token_service,
            refresh_token_service,
        })
    }
}
