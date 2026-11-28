#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub s3: S3Config,
    pub max_size_bytes: u64,
    /// 업로드용 Presigned URL 만료 시간 (초) - 예: 600 (10분)
    #[serde(default = "default_upload_url_ttl_secs")]
    pub upload_url_ttl_secs: u64,
    /// 다운로드용 Presigned URL 만료 시간 (초) - 예: 180 (3분)
    #[serde(default = "default_download_url_ttl_secs")]
    pub download_url_ttl_secs: u64,
    /// 미연결(pending) 고아 파일 만료 기준 시간 (초) - 예: 86400 (24시간)
    #[serde(default = "default_pending_ttl_secs")]
    pub pending_ttl_secs: u64,
    /// 논리 삭제(deleted) 후 S3 물리 삭제 전 유예 시간 (초) - 예: 604800 (7일)
    #[serde(default = "default_deleted_retention_secs")]
    pub deleted_retention_secs: u64,
    /// 업로드 허용 MIME 타입 목록 (비어있을 경우 제한 없음 또는 기본 정책 적용)
    #[serde(default)]
    pub allowed_content_types: Vec<String>,
}

fn default_upload_url_ttl_secs() -> u64 { 600 }
fn default_download_url_ttl_secs() -> u64 { 180 }
fn default_pending_ttl_secs() -> u64 { 86_400 }
fn default_deleted_retention_secs() -> u64 { 604_800 }

#[derive(Clone, serde::Deserialize)]
pub struct S3Config {
    pub access_key_id: String,
    pub secret_access_key: String,
    /// None selects the regional AWS endpoint. HTTP is allowed only for
    /// loopback hosts and private IPv4 addresses.
    pub endpoint: Option<String>,
    pub region: String,
    pub bucket: String,
    pub key_prefix: String,
}

impl std::fmt::Debug for S3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Config")
            .field("access_key_id", &"[REDACTED]")
            .field("secret_access_key", &"[REDACTED]")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("endpoint", &self.endpoint)
            .field("key_prefix", &self.key_prefix)
            .finish()
    }
}
