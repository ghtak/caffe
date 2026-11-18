use std::sync::Arc;
use std::time::Duration;

use espr::hash::PasswordHasher as _;

use crate::error::ResultExt as _;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub max_concurrency: usize,
    pub wait_timeout_seconds: u64,
}

/// 비밀번호 해싱과 검증의 동시 실행을 제한한다.
pub struct PasswordHashService {
    hasher: Arc<espr::hash::argon2::Hasher>,
    permits: Arc<tokio::sync::Semaphore>,
    wait_timeout: Duration,
}

impl PasswordHashService {
    pub fn new(hasher: Arc<espr::hash::argon2::Hasher>, config: &Config) -> crate::Result<Self> {
        if config.max_concurrency == 0
            || config.max_concurrency > tokio::sync::Semaphore::MAX_PERMITS
            || config.wait_timeout_seconds == 0
        {
            return Err(crate::Error::new(
                crate::Code::InvalidConfiguration,
                "Invalid password hash concurrency or wait timeout",
            )
            .at("auth.password_hash.config"));
        }
        Ok(Self {
            hasher,
            permits: Arc::new(tokio::sync::Semaphore::new(config.max_concurrency)),
            wait_timeout: Duration::from_secs(config.wait_timeout_seconds),
        })
    }

    pub async fn hash(&self, password: String) -> crate::Result<String> {
        let hasher = Arc::clone(&self.hasher);
        self.run(move || hasher.hash(&password)).await
    }

    pub async fn verify(&self, password: String, hash: String) -> crate::Result<bool> {
        let hasher = Arc::clone(&self.hasher);
        self.run(move || hasher.verify(&password, &hash)).await
    }

    async fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce() -> espr::Result<T> + Send + 'static,
    ) -> crate::Result<T> {
        let permit =
            tokio::time::timeout(self.wait_timeout, Arc::clone(&self.permits).acquire_owned())
                .await
                .map_err(|_| {
                    crate::Error::new(crate::Code::Busy, "Password hashing is busy")
                        .at("auth.password_hash")
                })?
                .internal("Password hash limiter is closed", "auth.password_hash")?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation()
        })
        .await
        .internal("Password hash task failed", "auth.password_hash")?
        .internal("Password hash operation failed", "auth.password_hash")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(
        max_concurrency: usize,
        wait_timeout_seconds: u64,
    ) -> crate::Result<PasswordHashService> {
        let hasher = Arc::new(espr::hash::argon2::Hasher::new(&Default::default()).unwrap());
        PasswordHashService::new(
            hasher,
            &Config {
                max_concurrency,
                wait_timeout_seconds,
            },
        )
    }

    #[test]
    fn rejects_invalid_capacity_and_timeout() {
        for (capacity, timeout) in [(0, 1), (1, 0), (tokio::sync::Semaphore::MAX_PERMITS + 1, 1)] {
            assert!(
                matches!(service(capacity, timeout), Err(error) if error.code == crate::Code::InvalidConfiguration)
            );
        }
    }

    #[tokio::test]
    async fn hashes_and_verifies_password_and_reports_invalid_hash() -> anyhow::Result<()> {
        let service = service(1, 1)?;
        let hash = service.hash("correct".into()).await?;
        // 생성된 해시는 올바른 비밀번호만 검증한다.
        assert!(service.verify("correct".into(), hash.clone()).await?);
        assert!(!service.verify("wrong".into(), hash).await?);
        assert!(
            matches!(service.verify("correct".into(), "invalid".into()).await, Err(error) if error.code == crate::Code::Internal)
        );
        Ok(())
    }

    #[tokio::test]
    async fn cancelled_caller_keeps_capacity_until_blocking_work_finishes() -> anyhow::Result<()> {
        let service = Arc::new(service(1, 1)?);
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let running = {
            let service = Arc::clone(&service);
            tokio::spawn(async move {
                service
                    .run(move || {
                        let _ = started_tx.send(());
                        let _ = release_rx.recv();
                        Ok(())
                    })
                    .await
            })
        };
        started_rx.await?;
        running.abort();
        let (hash, verify) = tokio::join!(
            service.hash("password".into()),
            service.verify("password".into(), "invalid".into())
        );
        // 호출자가 취소되어도 실행 중인 작업이 허용량을 보유해 두 작업 모두
        // 대기 초과된다.
        assert!(matches!(hash, Err(error) if error.code == crate::Code::Busy));
        assert!(matches!(verify, Err(error) if error.code == crate::Code::Busy));
        release_tx.send(())?;
        // blocking 작업이 끝나면 다시 해싱을 수행할 수 있다.
        let hash = service.hash("password".into()).await?;
        assert!(service.verify("password".into(), hash).await?);
        Ok(())
    }
}
