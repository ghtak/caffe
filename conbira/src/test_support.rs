//! 테스트 접속 주소는 CAFFE_DATABASE_URL과 CAFFE_CACHE_URL로 재정의할 수
//! 있다.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::Container;

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!("conbira-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn jwt_keys(&self) -> espr::Result<espr::jwt::ed25519::Config> {
        let keys = espr::jwt::ed25519::Config {
            key_path: self.0.join("private.pem"),
            public_key_path: self.0.join("public.pem"),
        };
        espr::jwt::ed25519::generate(&keys)?;
        Ok(keys)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

pub(crate) fn jwt_service(
    issuer: &str,
    audience: &str,
    access_token_ttl_seconds: u64,
) -> anyhow::Result<espr::jwt::TokenService> {
    let dir = TestDir::new()?;
    Ok(espr::jwt::TokenService::new(&espr::jwt::Config {
        issuer: issuer.into(),
        audience: audience.into(),
        access_token_ttl_seconds,
        ed25519: dir.jwt_keys()?,
    })?)
}

pub(crate) async fn container() -> anyhow::Result<Arc<Container>> {
    let dir = TestDir::new()?;
    let mut config = crate::config::Config::load(
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml")),
        Some(&dir.0.join("missing.env")),
    )?;
    config.database = database_config(1);
    config.cache.url = cache_url();
    config.jwt.ed25519 = dir.jwt_keys()?;
    Ok(Arc::new(Container::new(&config).await?))
}

pub(crate) fn database_config(max_connections: u32) -> espr::sqlxdb::Config {
    espr::sqlxdb::Config {
        url: std::env::var("CAFFE_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@192.168.1.20:5432/conbira_test".into()
        }),
        max_connections,
    }
}

fn cache_url() -> String {
    std::env::var("CAFFE_CACHE_URL").unwrap_or_else(|_| "redis://192.168.1.20:6379".into())
}
