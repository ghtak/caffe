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
            .at(crate::here!()));
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
                        .at(crate::here!())
                })?
                .map_err_internal("Password hash limiter is closed", crate::here!())?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation()
        })
        .await
        .map_err_internal("Password hash task failed", crate::here!())?
        .map_err_internal("Password hash operation failed", crate::here!())
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

    mod contract {
        use super::*;

        #[test]
        fn rejects_zero_capacity_or_timeout() {
            for (capacity, timeout) in [(0, 1), (1, 0)] {
                // 용량과 대기 제한은 양수여야 한다.
                assert!(
                    matches!(service(capacity, timeout), Err(error) if error.code == crate::Code::InvalidConfiguration)
                );
            }
        }

        #[tokio::test]
        async fn generated_hash_verifies_only_the_original_password() -> anyhow::Result<()> {
            let service = service(1, 1)?;
            let hash = service.hash("correct".into()).await?;
            // 생성된 해시는 올바른 비밀번호만 검증한다.
            assert!(service.verify("correct".into(), hash.clone()).await?);
            assert!(!service.verify("wrong".into(), hash).await?);
            Ok(())
        }

        #[tokio::test]
        async fn cancelled_caller_keeps_capacity_until_blocking_work_finishes() -> anyhow::Result<()>
        {
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

    mod edge {
        use super::*;

        #[test]
        fn unrepresentable_capacity_is_rejected() {
            let result = service(tokio::sync::Semaphore::MAX_PERMITS + 1, 1);
            // 세마포어의 표현 한계를 넘는 용량은 설정 오류를 반환한다.
            assert!(
                matches!(result, Err(error) if error.code == crate::Code::InvalidConfiguration)
            );
        }

        #[tokio::test]
        async fn verify_reports_malformed_hash_as_internal_error() -> anyhow::Result<()> {
            let service = service(1, 1)?;
            // 저장된 해시가 잘못되면 내부 오류를 반환한다.
            assert!(
                matches!(service.verify("correct".into(), "invalid".into()).await, Err(error) if error.code == crate::Code::Internal)
            );
            Ok(())
        }
    }
}
