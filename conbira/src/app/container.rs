use espr::{cache, sqlxdb};

use crate::app::auth::service::google_oidc::GoogleOidcService;
use crate::config::Config;
use crate::error::ResultExt;

pub mod db {
    pub type Database = espr::sqlxdb::Database<sqlx::Postgres>;
    pub type Handle<'a> = espr::sqlxdb::Handle<'a, sqlx::Postgres>;
    pub type QueryBuilder = sqlx::QueryBuilder<sqlx::Postgres>;
}

pub struct Container {
    pub config: Config,
    pub file_storage: std::sync::Arc<dyn crate::app::files::s3::S3ObjectPort>,
    pub file_resource_access: std::sync::Arc<dyn crate::app::files::access::ResourceAccessPort>,
    pub db: std::sync::Arc<db::Database>,
    pub cache: espr::cache::Cache,
    pub http_server: espr::http::Server,
    pub password_hash_service: crate::app::auth::service::password_hash::PasswordHashService,
    pub dummy_password_hash: String,
    pub jwt_token_service: espr::jwt::TokenService,
    pub refresh_token_service: crate::app::auth::service::refresh_token::RefreshTokenService,
    pub google_oidc_service: Option<GoogleOidcService>,
}

impl Container {
    pub async fn new(config: &Config) -> crate::Result<Self> {
        let file_storage =
            std::sync::Arc::new(crate::app::files::s3::S3ObjectAdapter::new(&config.files).await?);
        let db = std::sync::Arc::new(
            db::Database::new(&config.database)
                .await
                .map_err_internal("Failed to initialize database", crate::here!())?,
        );

        let policy_db = std::sync::Arc::new(
            db::Database::new(&config.database)
                .await
                .map_err_internal("Failed to initialize file policy database", crate::here!())?,
        );

        let cache = cache::Cache::new(&config.cache)
            .await
            .map_err_internal("Failed to initialize cache", crate::here!())?;

        let http_server = espr::http::Server::new(&config.http)
            .await
            .map_err_internal("Failed to initialize http", crate::here!())?;

        let password_hasher = std::sync::Arc::new(
            espr::hash::argon2::Hasher::new(&config.argon2)
                .map_err_internal("Failed to initialize password hasher", crate::here!())?,
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
            .map_err_internal("Failed to initialize jwt", crate::here!())?;

        let refresh_token_service =
            crate::app::auth::service::refresh_token::RefreshTokenService::new(
                config.auth.refresh_token.clone(),
            );
        let google_oidc_service = match &config.auth.google_oidc {
            Some(config) => Some(GoogleOidcService::new(config).await?),
            None => None,
        };

        Ok(Self {
            config: config.clone(),
            file_storage,
            file_resource_access: std::sync::Arc::new(crate::app::board::PostFileAccess {
                db: policy_db,
            }),
            db,
            cache,
            http_server,
            password_hash_service,
            dummy_password_hash,
            jwt_token_service,
            refresh_token_service,
            google_oidc_service,
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
            .map_err_internal("Failed to get db connection", at)
    }

    async fn begin_tx(&self, at: &'static str) -> crate::Result<sqlxdb::Tx<sqlx::Postgres>> {
        self.tx()
            .await
            .map_err_internal("Failed to begin db transaction", at)
    }
}
