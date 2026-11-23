//! 테스트 환경 변수와 기본값:
//! - CAFFE_DATABASE_URL:
//!   postgres://postgres:postgres@192.168.1.20:5432/espr_test
//! - CAFFE_CACHE_URL: redis://192.168.1.20:6379
//! - CAFFE_ORACLE_USERNAME / CAFFE_ORACLE_PASSWORD: oracle / oracle
//! - CAFFE_ORACLE_CONNECT_STRING: 192.168.1.20:31521/FREEPDB1

pub(crate) fn database_config(max_connections: u32) -> crate::sqlxdb::Config {
    crate::sqlxdb::Config {
        url: std::env::var("CAFFE_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@192.168.1.20:5432/espr_test".into()),
        max_connections,
    }
}

/// `tests/docker/docker-compose.yml`의 Oracle 서비스 주석을 해제하면 기본값으로
/// 접속할 수 있다. 다른 서버는 `CAFFE_ORACLE_*`로 지정한다.
pub(crate) fn oracle_config() -> crate::oracledb::Config {
    crate::oracledb::Config {
        username: std::env::var("CAFFE_ORACLE_USERNAME").unwrap_or_else(|_| "oracle".into()),
        password: std::env::var("CAFFE_ORACLE_PASSWORD").unwrap_or_else(|_| "oracle".into()),
        connect_string: std::env::var("CAFFE_ORACLE_CONNECT_STRING")
            .unwrap_or_else(|_| "192.168.1.20:31521/FREEPDB1".into()),
        max_connections: 10,
    }
}

pub(crate) fn cache_config() -> crate::cache::Config {
    crate::cache::Config {
        url: std::env::var("CAFFE_CACHE_URL")
            .unwrap_or_else(|_| "redis://192.168.1.20:6379".into()),
        max_connections: 2,
        connect_timeout_ms: 1_000,
        response_timeout_ms: 1_000,
        pool_wait_timeout_ms: 1_000,
    }
}
