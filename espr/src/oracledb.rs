use std::sync::Arc;

use serde::Deserialize;

use crate::Result;

#[derive(Clone, Deserialize)]
pub struct Config {
    pub username: String,
    pub password: String,
    pub connect_string: String,
    pub max_connections: usize,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("connect_string", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .finish()
    }
}

#[derive(Clone)]
pub struct Database {
    pool: Arc<oracledb::Pool>,
}

impl Database {
    /// Create the pool; database connectivity is checked on acquisition.
    pub async fn new(config: &Config) -> Result<Self> {
        let config = config.clone();
        tokio::task::spawn_blocking(move || {
            let pool_config = oracledb::PoolConfig::default()
                .set_credentials(&config.username, &config.password)
                .set_connect_string(&config.connect_string)?
                .set_max_connections(config.max_connections);
            let pool = oracledb::create_pool(pool_config)?;
            Ok(Self {
                pool: Arc::new(pool),
            })
        })
        .await
        .map_err(anyhow::Error::new)?
    }

    /// Run database work on a blocking thread using the shared pool.
    /// Acquire connections and commit or roll back transactions inside the
    /// closure. Dropping a connection rolls back any pending transaction.
    /// Once started, the blocking closure continues even if its future is
    /// dropped.
    pub async fn interact<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&oracledb::Pool) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let pool = Arc::clone(&self.pool);
        tokio::task::spawn_blocking(move || f(&pool))
            .await
            .map_err(anyhow::Error::new)?
    }
}

#[cfg(test)]
mod tests;
