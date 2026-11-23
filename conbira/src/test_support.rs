//! 테스트 접속 주소는 CAFFE_DATABASE_URL과 CAFFE_CACHE_URL로 재정의할 수
//! 있다.

pub(crate) fn database_config(max_connections: u32) -> espr::sqlxdb::Config {
    espr::sqlxdb::Config {
        url: std::env::var("CAFFE_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@192.168.1.20:5432/conbira_test".into()
        }),
        max_connections,
    }
}

pub(crate) fn cache_url() -> String {
    std::env::var("CAFFE_CACHE_URL").unwrap_or_else(|_| "redis://192.168.1.20:6379".into())
}
