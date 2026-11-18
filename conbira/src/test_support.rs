//! 테스트 환경 변수: TESTENV_DATABASE_URL (기본값:
//! postgres://test:test@192.168.1.20:35432/test).

pub(crate) fn database_config(max_connections: u32) -> espr::sqlxdb::Config {
    espr::sqlxdb::Config {
        url: std::env::var("TESTENV_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://test:test@192.168.1.20:35432/test".to_string()),
        max_connections,
    }
}
