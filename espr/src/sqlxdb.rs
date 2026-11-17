use serde::Deserialize;
use sqlx::{Acquire, Connection as _};

use crate::{Error, Result};

#[derive(Clone, Deserialize)]
pub struct Config {
    pub url: String,
    pub max_connections: u32,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .finish()
    }
}

pub struct Database<DB: sqlx::Database> {
    inner: sqlx::Pool<DB>,
}

impl<DB: sqlx::Database> Database<DB> {
    pub async fn new(config: &Config) -> Result<Self> {
        if config.url.trim().is_empty() {
            return Err(Error::InvalidParameter(
                "database URL must not be blank".into(),
            ));
        }
        if config.max_connections == 0 {
            return Err(Error::InvalidParameter(
                "max_connections must be greater than zero".into(),
            ));
        }
        let inner = sqlx::pool::PoolOptions::<DB>::new()
            .max_connections(config.max_connections)
            .connect(&config.url)
            .await?;
        Ok(Self { inner })
    }

    pub async fn conn(&self) -> Result<Conn<DB>> {
        Ok(Conn {
            inner: self.inner.acquire().await?,
        })
    }

    pub async fn tx(&self) -> Result<Tx<'static, DB>> {
        Ok(Tx {
            inner: self.inner.begin().await?,
        })
    }
}

pub struct Conn<DB: sqlx::Database> {
    inner: sqlx::pool::PoolConnection<DB>,
}

pub struct Tx<'c, DB: sqlx::Database> {
    inner: sqlx::Transaction<'c, DB>,
}

pub struct Handle<'c, DB: sqlx::Database> {
    inner: &'c mut DB::Connection,
}

impl<DB: sqlx::Database> Conn<DB> {
    /// Begin a transaction borrowing this connection.
    pub async fn begin(&mut self) -> Result<Tx<'_, DB>> {
        Ok(Tx {
            inner: self.inner.as_mut().begin().await?,
        })
    }

    pub fn handle(&mut self) -> Handle<'_, DB> {
        Handle {
            inner: self.inner.as_mut(),
        }
    }
}

impl<DB: sqlx::Database> Tx<'_, DB> {
    pub async fn begin(&mut self) -> Result<Tx<'_, DB>> {
        Ok(Tx {
            inner: Acquire::begin(&mut self.inner).await?,
        })
    }

    pub async fn commit(self) -> Result<()> {
        self.inner.commit().await?;
        Ok(())
    }

    pub async fn rollback(self) -> Result<()> {
        self.inner.rollback().await?;
        Ok(())
    }

    pub fn handle(&mut self) -> Handle<'_, DB> {
        Handle {
            inner: &mut self.inner,
        }
    }
}

impl<DB: sqlx::Database> Handle<'_, DB> {
    pub fn conn(&mut self) -> &mut DB::Connection { self.inner }
}

#[cfg(test)]
mod tests;
