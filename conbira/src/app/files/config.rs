use espr::validate::Validate;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub s3: S3Config,
    pub pending_ttl_seconds: u64,
    pub cleanup_interval_seconds: u64,
    pub download_url_ttl_seconds: u64,
    pub max_file_size_bytes: u64,
    pub max_files_per_upload: usize,
}

#[derive(Clone, serde::Deserialize)]
pub struct S3Config {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub bucket: String,
    pub region: String,
    /// None selects the regional AWS endpoint. HTTP is allowed only for
    /// loopback hosts and private IPv4 addresses.
    pub endpoint: Option<String>,
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

impl Validate for Config {
    type Error = crate::Error;

    fn validate(&self) -> crate::Result<()> {
        let invalid = |at| {
            crate::Error::new(
                crate::Code::InvalidConfiguration,
                "Invalid files configuration",
            )
            .at(at)
        };
        if [
            &self.s3.access_key_id,
            &self.s3.secret_access_key,
            &self.s3.bucket,
            &self.s3.region,
            &self.s3.key_prefix,
        ]
        .iter()
        .any(|value| value.trim().is_empty())
            || self.pending_ttl_seconds == 0
            || self.cleanup_interval_seconds == 0
            || !(1..=604800).contains(&self.download_url_ttl_seconds)
            || self.max_files_per_upload == 0
            || self.max_file_size_bytes == 0
            || self.max_file_size_bytes > i64::MAX as u64
        {
            return Err(invalid(crate::here!()));
        }
        if let Some(endpoint) = &self.s3.endpoint {
            let url = url::Url::parse(endpoint).map_err(|_| invalid(crate::here!()))?;
            let local = match url.host() {
                Some(url::Host::Domain("localhost")) => true,
                Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_private(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                _ => false,
            };
            if url.host().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || !(url.scheme() == "https" || (url.scheme() == "http" && local))
            {
                return Err(invalid(crate::here!()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
