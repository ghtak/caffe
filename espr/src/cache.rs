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

    /// Atomically removes a key and returns its value, if it still exists.
    pub async fn take<K, V>(&self, key: K) -> Result<Option<V>>
    where
        K: ToRedisArgs + Send + Sync,
        V: FromRedisValue,
    {
        let mut conn = self.pool.get().await?;
        Ok(redis::cmd("GETDEL").arg(key).query_async(&mut conn).await?)
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

    fn cache_with_closed_pool() -> Cache {
        let manager = deadpool_redis::Manager::new("redis://127.0.0.1/").unwrap();
        let pool = deadpool_redis::Pool::builder(manager).build().unwrap();
        pool.close();
        Cache { pool }
    }

    mod contract {
        use super::*;

        #[tokio::test]
        async fn set_stores_a_readable_value_without_expiration() -> Result<()> {
            let cache = Cache::new(&crate::test_support::cache_config()).await?;
            cache.ping().await?;
            let key = format!("espr:test:{}:values", crate::test_support::unique_id());

            let missing: Option<String> = cache.get(&key).await?;

            // A key that has not been stored returns None.
            assert_eq!(missing, None);

            cache.set(&key, "hello").await?;
            let stored: Option<String> = cache.get(&key).await?;
            let mut conn = cache.pool.get().await?;
            let ttl: i64 = redis::cmd("PTTL").arg(&key).query_async(&mut conn).await?;
            redis::cmd("DEL")
                .arg(&key)
                .query_async::<()>(&mut conn)
                .await?;

            // The stored value is readable, and Redis reports -1 for a key
            // without expiration.
            assert_eq!(stored.as_deref(), Some("hello"));
            assert_eq!(ttl, -1);
            Ok(())
        }

        #[tokio::test]
        async fn set_with_ttl_returns_value_before_expiry_and_none_after() -> Result<()> {
            let cache = Cache::new(&crate::test_support::cache_config()).await?;
            let key = format!("espr:test:{}:expiry", crate::test_support::unique_id());
            let ttl = Duration::from_secs(2);

            let started = std::time::Instant::now();
            cache.set_with_ttl(&key, b"value", ttl).await?;
            let remaining_ms: i64 = {
                let mut conn = cache.pool.get().await?;
                redis::cmd("PTTL").arg(&key).query_async(&mut conn).await?
            };
            let elapsed = started.elapsed();
            let minimum_ttl = ttl.saturating_sub(elapsed + Duration::from_millis(1));
            let remaining = Duration::from_millis(
                u64::try_from(remaining_ms).expect("Redis should report a remaining TTL"),
            );

            // Redis stores the requested TTL, allowing for elapsed time and
            // millisecond rounding.
            assert!(
                !remaining.is_zero() && (minimum_ttl..=ttl).contains(&remaining),
                "expected a positive remaining TTL between {minimum_ttl:?} and {ttl:?}, got {remaining:?}"
            );

            let stored: Option<Vec<u8>> = cache.get(&key).await?;

            // Before the TTL elapses, the original value is readable.
            assert_eq!(stored.as_deref(), Some(b"value".as_slice()));

            let expiration = tokio::time::timeout(Duration::from_secs(5), async {
                while cache.get::<_, Vec<u8>>(&key).await?.is_some() {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Ok::<_, Error>(())
            })
            .await;

            // The cache removes the key within the test's expiration deadline.
            expiration.expect("cache key did not expire within five seconds")?;
            Ok(())
        }

        #[tokio::test]
        async fn set_with_ttl_rejects_durations_below_one_millisecond() {
            let cache = cache_with_closed_pool();
            for ttl in [Duration::ZERO, Duration::from_micros(999)] {
                let result = cache.set_with_ttl("key", "value", ttl).await;

                // A TTL below one millisecond is rejected before using the
                // pool.
                assert!(
                    matches!(result, Err(Error::InvalidParameter(_))),
                    "TTL: {ttl:?}"
                );
            }
        }
    }

    mod edge {
        use super::*;

        #[tokio::test]
        async fn set_with_ttl_rejects_milliseconds_outside_the_signed_range() {
            let cache = cache_with_closed_pool();
            let ttl = Duration::from_millis(i64::MAX as u64 + 1);
            let result = cache.set_with_ttl("key", "value", ttl).await;

            // An unrepresentable TTL is rejected before using the pool.
            assert!(matches!(result, Err(Error::InvalidParameter(_))));
        }

        #[tokio::test]
        async fn new_returns_invalid_parameter_for_zero_pool_size() {
            let config = Config {
                max_connections: 0,
                ..crate::test_support::cache_config()
            };
            let result = Cache::new(&config).await;

            // Zero connections is rejected as an invalid parameter.
            assert!(matches!(result, Err(Error::InvalidParameter(_))));
        }

        #[tokio::test]
        async fn new_returns_invalid_parameter_for_each_zero_timeout() {
            let config = crate::test_support::cache_config();
            for (field, config) in [
                (
                    "connect_timeout_ms",
                    Config {
                        connect_timeout_ms: 0,
                        ..config.clone()
                    },
                ),
                (
                    "response_timeout_ms",
                    Config {
                        response_timeout_ms: 0,
                        ..config.clone()
                    },
                ),
                (
                    "pool_wait_timeout_ms",
                    Config {
                        pool_wait_timeout_ms: 0,
                        ..config
                    },
                ),
            ] {
                let result = Cache::new(&config).await;

                // Each zero timeout is rejected as an invalid parameter.
                assert!(matches!(result, Err(Error::InvalidParameter(_))), "{field}");
            }
        }

        #[tokio::test]
        async fn get_returns_cache_pool_error_when_pool_is_closed() {
            let cache = cache_with_closed_pool();
            let result = cache.get::<_, String>("key").await;

            // Reading through a closed pool returns CachePoolError(Closed).
            assert!(matches!(
                result,
                Err(Error::CachePoolError(deadpool_redis::PoolError::Closed))
            ));
        }
    }
}
