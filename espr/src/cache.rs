use std::time::Duration;

use deadpool_redis::redis::{self, AsyncConnectionConfig, FromRedisValue, ToRedisArgs};
use serde::Deserialize;

use crate::{Error, Result};

#[derive(Clone, Deserialize)]
pub struct Config {
    pub url: String,
    pub max_connections: usize,
    pub connect_timeout_ms: u64,
    pub response_timeout_ms: u64,
    pub pool_wait_timeout_ms: u64,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .field("connect_timeout_ms", &self.connect_timeout_ms)
            .field("response_timeout_ms", &self.response_timeout_ms)
            .field("pool_wait_timeout_ms", &self.pool_wait_timeout_ms)
            .finish()
    }
}

#[derive(Clone)]
pub struct Cache {
    pool: deadpool_redis::Pool,
}

impl Cache {
    pub async fn new(config: &Config) -> Result<Self> {
        if config.max_connections == 0 {
            return Err(Error::InvalidParameter(
                "max_connections must be greater than zero".into(),
            ));
        }
        if config.connect_timeout_ms == 0
            || config.response_timeout_ms == 0
            || config.pool_wait_timeout_ms == 0
        {
            return Err(Error::InvalidParameter(
                "cache timeouts must be greater than zero".into(),
            ));
        }
        let connection_config = AsyncConnectionConfig::new()
            .set_connection_timeout(Some(Duration::from_millis(config.connect_timeout_ms)))
            .set_response_timeout(Some(Duration::from_millis(config.response_timeout_ms)));
        let manager =
            deadpool_redis::Manager::new_with_config(config.url.as_str(), connection_config)?;
        let pool = deadpool_redis::Pool::builder(manager)
            .max_size(config.max_connections)
            .wait_timeout(Some(Duration::from_millis(config.pool_wait_timeout_ms)))
            .runtime(deadpool_redis::Runtime::Tokio1)
            .build()
            .map_err(|error| Error::Internal(anyhow::Error::new(error)))?;
        let cache = Self { pool };
        cache.ping().await?;
        Ok(cache)
    }

    /// Checks that a pooled connection can reach the cache server.
    pub async fn ping(&self) -> Result<()> {
        let mut conn = self.pool.get().await?;
        redis::cmd("PING").query_async::<String>(&mut conn).await?;
        Ok(())
    }

    /// Returns `None` when the key is absent or expired.
    pub async fn get<K, V>(&self, key: K) -> Result<Option<V>>
    where
        K: ToRedisArgs + Send + Sync,
        V: FromRedisValue,
    {
        let mut conn = self.pool.get().await?;
        Ok(redis::cmd("GET").arg(key).query_async(&mut conn).await?)
    }

    /// Stores a value without an expiration time.
    pub async fn set<K, V>(&self, key: K, value: V) -> Result<()>
    where
        K: ToRedisArgs + Send + Sync,
        V: ToRedisArgs + Send + Sync,
    {
        let mut conn = self.pool.get().await?;
        redis::cmd("SET")
            .arg(key)
            .arg(value)
            .query_async::<()>(&mut conn)
            .await?;
        Ok(())
    }

    /// Stores a value with a positive expiration time in whole milliseconds.
    pub async fn set_with_ttl<K, V>(&self, key: K, value: V, ttl: Duration) -> Result<()>
    where
        K: ToRedisArgs + Send + Sync,
        V: ToRedisArgs + Send + Sync,
    {
        let millis = u64::try_from(ttl.as_millis())
            .ok()
            .filter(|&millis| millis > 0 && millis <= i64::MAX as u64)
            .ok_or_else(|| {
                Error::InvalidParameter(
                    "cache TTL must be between 1 and i64::MAX milliseconds".into(),
                )
            })?;
        let mut conn = self.pool.get().await?;
        redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("PX")
            .arg(millis)
            .query_async::<()>(&mut conn)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_zero_pool_size() {
        let mut config = crate::test_support::cache_config();
        config.max_connections = 0;
        assert!(matches!(
            Cache::new(&config).await,
            Err(Error::InvalidParameter(_))
        ));
    }

    #[tokio::test]
    async fn rejects_zero_timeouts() {
        let mut config = crate::test_support::cache_config();
        config.connect_timeout_ms = 0;
        assert!(matches!(
            Cache::new(&config).await,
            Err(Error::InvalidParameter(_))
        ));
        config.connect_timeout_ms = 1_000;
        config.response_timeout_ms = 0;
        assert!(matches!(
            Cache::new(&config).await,
            Err(Error::InvalidParameter(_))
        ));
        config.response_timeout_ms = 1_000;
        config.pool_wait_timeout_ms = 0;
        assert!(matches!(
            Cache::new(&config).await,
            Err(Error::InvalidParameter(_))
        ));
    }

    #[tokio::test]
    async fn reports_pool_errors_as_cache_errors() {
        let manager = deadpool_redis::Manager::new("redis://127.0.0.1/").unwrap();
        let pool = deadpool_redis::Pool::builder(manager).build().unwrap();
        pool.close();
        let cache = Cache { pool };

        assert!(matches!(
            cache.get::<_, String>("key").await,
            Err(Error::CachePoolError(deadpool_redis::PoolError::Closed))
        ));
    }

    #[tokio::test]
    async fn stores_and_reads_values() -> Result<()> {
        let cache = Cache::new(&crate::test_support::cache_config()).await?;
        cache.ping().await?;
        let key = format!("espr:test:{}:values", std::process::id());

        let missing: Option<String> = cache.get(&key).await?;
        assert_eq!(missing, None);
        cache.set(&key, "hello").await?;
        let stored: Option<String> = cache.get(&key).await?;
        assert_eq!(stored.as_deref(), Some("hello"));

        let mut conn = cache.pool.get().await?;
        redis::cmd("DEL")
            .arg(&key)
            .query_async::<()>(&mut conn)
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn expires_values() -> Result<()> {
        let cache = Cache::new(&crate::test_support::cache_config()).await?;
        let key = format!("espr:test:{}:expiry", std::process::id());

        cache
            .set_with_ttl(&key, b"value", Duration::from_secs(1))
            .await?;
        let stored: Option<Vec<u8>> = cache.get(&key).await?;
        assert_eq!(stored.as_deref(), Some(b"value".as_slice()));
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let expired: Option<Vec<u8>> = cache.get(&key).await?;
        assert_eq!(expired, None);
        Ok(())
    }
}
