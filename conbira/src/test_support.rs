//! 테스트 접속 주소는 CAFFE_DATABASE_URL과 CAFFE_CACHE_URL로 재정의할 수
//! 있다. DB 테스트는 `TestContext::run`에서 실행한다.
//! 컨텍스트가 독립 스키마·마이그레이션·설정을 준비하고 성공·오류·패닉 후
//! 스키마를 정리한다. 저장소 테스트는 `context.db`, HTTP 테스트는
//! `context.container()`를 사용한다. Fixture는 시나리오 데이터만 준비한다.

pub(crate) mod files;

use std::future::Future;
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

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_sample.sql"),
    include_str!("../migrations/0010_user.sql"),
    include_str!("../migrations/0011_refresh_token.sql"),
    include_str!("../migrations/0012_rbac.sql"),
    include_str!("../migrations/0013_files.sql"),
    include_str!("../migrations/0014_file_asset.sql"),
];

/// 테스트 설정과 독립 DB 스키마의 수명을 소유한다.
pub(crate) struct TestContext {
    pub db: Arc<crate::app::db::Database>,
    schema: String,
    config: crate::config::Config,
    _dir: TestDir,
}

impl TestContext {
    pub async fn container(&self) -> anyhow::Result<Container> {
        Container::new(&self.config).await.map_err(Into::into)
    }

    async fn new(max_connections: u32) -> anyhow::Result<Self> {
        let mut config = espr::sqlxdb::Config {
            url: std::env::var("CAFFE_DATABASE_URL").unwrap_or_else(|_| {
                "postgres://postgres:postgres@192.168.1.20:5432/conbira_test".into()
            }),
            max_connections,
        };
        let dir = TestDir::new()?;
        let mut app_config = crate::config::Config::load(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml")),
            Some(&dir.0.join("missing.env")),
        )?;
        app_config.cache.url =
            std::env::var("CAFFE_CACHE_URL").unwrap_or_else(|_| "redis://192.168.1.20:6379".into());
        app_config.jwt.ed25519 = dir.jwt_keys()?;
        app_config.files = files::config(Some("http://127.0.0.1:19000".into()));
        app_config.file_asset.s3.access_key_id = "test-access".into();
        app_config.file_asset.s3.secret_access_key = "test-secret".into();
        app_config.file_asset.s3.endpoint = Some("http://127.0.0.1:19000".into());
        let schema = format!("test_{}", uuid::Uuid::new_v4().simple());
        let mut url = url::Url::parse(&config.url)?;
        url.query_pairs_mut()
            .append_pair("options", &format!("-csearch_path={schema}"));
        config.url = url.into();
        let db = crate::app::db::Database::new(&config).await?;
        let mut conn = db.conn().await?;
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(conn.handle().conn())
            .await?;
        drop(conn);
        app_config.database = config;
        let database = Self {
            db: Arc::new(db),
            schema,
            config: app_config,
            _dir: dir,
        };
        let setup = async {
            let mut conn = database.db.conn().await?;
            for migration in MIGRATIONS {
                sqlx::raw_sql(*migration)
                    .execute(conn.handle().conn())
                    .await?;
            }
            anyhow::Ok(())
        }
        .await;
        if let Err(error) = setup {
            database.cleanup().await?;
            return Err(error);
        }
        Ok(database)
    }

    async fn cleanup(&self) -> anyhow::Result<()> {
        let mut conn = self.db.conn().await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA {} CASCADE",
            self.schema
        )))
        .execute(conn.handle().conn())
        .await?;
        Ok(())
    }

    pub async fn run<F, Fut>(max_connections: u32, test: F) -> anyhow::Result<()>
    where
        F: FnOnce(Arc<Self>) -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        use anyhow::Context as _;
        let database = Arc::new(Self::new(max_connections).await?);
        let context = Arc::clone(&database);
        let result = tokio::spawn(async move { test(context).await }).await;
        let cleanup = database
            .cleanup()
            .await
            .context("failed to drop test schema");
        let result = match result {
            Ok(result) => result,
            Err(error) if error.is_panic() => {
                if let Err(error) = cleanup {
                    eprintln!("{error:#}");
                }
                std::panic::resume_unwind(error.into_panic());
            }
            Err(error) => Err(error.into()),
        };
        match (result, cleanup) {
            (Err(error), Err(cleanup)) => Err(error.context(format!("{cleanup:#}"))),
            (Err(error), _) => Err(error),
            (Ok(()), cleanup) => cleanup,
        }
    }
}
