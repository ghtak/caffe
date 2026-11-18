//! 테스트 환경 변수와 기본값:
//! - TESTENV_DATABASE_URL: postgres://test:test@192.168.1.20:35432/test
//! - TESTENV_ORACLE_USERNAME / TESTENV_ORACLE_PASSWORD: test / test
//! - TESTENV_ORACLE_CONNECT_STRING: 192.168.1.20:31521/FREEPDB1
//! - TESTENV_CACHE_URL: redis://192.168.1.20:36379

pub(crate) fn database_config(max_connections: u32) -> crate::sqlxdb::Config {
    crate::sqlxdb::Config {
        url: std::env::var("TESTENV_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://test:test@192.168.1.20:35432/test".to_string()),
        max_connections,
    }
}

pub(crate) fn oracle_config() -> crate::oracledb::Config {
    crate::oracledb::Config {
        username: std::env::var("TESTENV_ORACLE_USERNAME").unwrap_or_else(|_| "test".into()),
        password: std::env::var("TESTENV_ORACLE_PASSWORD").unwrap_or_else(|_| "test".into()),
        connect_string: std::env::var("TESTENV_ORACLE_CONNECT_STRING")
            .unwrap_or_else(|_| "192.168.1.20:31521/FREEPDB1".into()),
        max_connections: 10,
    }
}

pub(crate) fn cache_config() -> crate::cache::Config {
    crate::cache::Config {
        url: std::env::var("TESTENV_CACHE_URL")
            .unwrap_or_else(|_| "redis://192.168.1.20:36379".into()),
        max_connections: 2,
        connect_timeout_ms: 1_000,
        response_timeout_ms: 1_000,
        pool_wait_timeout_ms: 1_000,
    }
}
