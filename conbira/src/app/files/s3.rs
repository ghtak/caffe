use std::pin::Pin;
use std::time::{Duration, UNIX_EPOCH};

use aws_sdk_s3::config::retry::RetryConfig;
use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use espr::validate::Validate as _;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};

use super::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectLocation {
    pub bucket: String,
    pub key: String,
}

#[derive(Debug)]
pub enum StorageError {
    InvalidObjectRequest,
    FileTooLarge,
    StorageUnavailable(anyhow::Error),
    StorageConfiguration(anyhow::Error),
}

impl From<StorageError> for crate::Error {
    fn from(error: StorageError) -> Self {
        use crate::{Code, Error};
        match error {
            StorageError::InvalidObjectRequest | StorageError::FileTooLarge => {
                Error::internal("Invalid object storage request")
            }
            StorageError::StorageUnavailable(cause) => {
                Error::new(Code::Busy, "Object storage unavailable").cause(cause)
            }
            StorageError::StorageConfiguration(cause) => Error::new(
                Code::InvalidConfiguration,
                "Object storage configuration error",
            )
            .cause(cause),
        }
        .at(crate::here!())
    }
}

/// Transport-neutral streaming input. No HTTP body or SDK type crosses this
/// port.
pub struct PutObject {
    pub location: ObjectLocation,
    pub content_type: String,
    pub body: Pin<Box<dyn AsyncRead + Send>>,
    pub max_bytes: u64,
}

#[derive(Debug)]
pub struct StoredObject {
    pub location: ObjectLocation,
    pub size_bytes: u64,
}

#[derive(Debug)]
pub enum ObjectHead {
    Missing,
    Present {
        size_bytes: u64,
        content_type: String,
    },
}

pub struct PresignGet {
    pub location: ObjectLocation,
    pub original_filename: String,
    pub ttl_seconds: u64,
    pub now: jiff::Timestamp,
}

#[derive(Debug)]
pub struct PresignedGet {
    pub url: String,
    pub expires_at: jiff::Timestamp,
}

#[async_trait::async_trait]
pub trait S3ObjectPort: Send + Sync {
    /// Enforce measured size before completing storage; refuse existing keys.
    async fn put(&self, params: PutObject) -> Result<StoredObject, StorageError>;
    /// Distinguish missing objects, permission/configuration errors, and
    /// outages.
    async fn head(&self, location: &ObjectLocation) -> Result<ObjectHead, StorageError>;
    /// Local signing only; safely encode the original filename in disposition.
    async fn presign_get(&self, params: PresignGet) -> Result<PresignedGet, StorageError>;
    /// Exactly one DeleteObject request per call, including failures; missing
    /// is OK.
    async fn delete(&self, location: &ObjectLocation) -> Result<(), StorageError>;
}

/// S3 SDK adapter using the explicitly configured access key pair.
pub struct S3ObjectAdapter {
    client: aws_sdk_s3::Client,
}

impl S3ObjectAdapter {
    /// Intended Container assembly boundary; must not contact S3 during
    /// startup.
    pub async fn new(config: &Config) -> crate::Result<Self> {
        config.validate()?;
        let mut builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(config.s3.region.clone()))
            .credentials_provider(Credentials::new(
                config.s3.access_key_id.clone(),
                config.s3.secret_access_key.clone(),
                None,
                None,
                "conbira-files",
            ))
            .retry_config(RetryConfig::standard().with_max_attempts(1))
            .timeout_config(
                aws_sdk_s3::config::timeout::TimeoutConfig::builder()
                    .operation_timeout(Duration::from_secs(60))
                    .build(),
            )
            .request_checksum_calculation(
                aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired,
            );
        if let Some(endpoint) = &config.s3.endpoint {
            builder = builder.endpoint_url(endpoint).force_path_style(true);
        }
        Ok(Self {
            client: aws_sdk_s3::Client::from_conf(builder.build()),
        })
    }
}

// A single mapping keeps SDK failure classification consistent across
// operations.
fn storage_error(status: Option<u16>, error: impl Into<anyhow::Error>) -> StorageError {
    match status {
        Some(409 | 412) => StorageError::InvalidObjectRequest,
        Some(400..=499) if !matches!(status, Some(408 | 429)) => {
            StorageError::StorageConfiguration(error.into())
        }
        _ => StorageError::StorageUnavailable(error.into()),
    }
}

#[async_trait::async_trait]
impl S3ObjectPort for S3ObjectAdapter {
    async fn put(&self, params: PutObject) -> Result<StoredObject, StorageError> {
        if params.location.bucket.trim().is_empty()
            || params.location.key.trim().is_empty()
            || params.content_type.parse::<mime::Mime>().is_err()
            || params.max_bytes == 0
            || params.max_bytes > i64::MAX as u64
        {
            return Err(StorageError::InvalidObjectRequest);
        }
        let temporary = tempfile::NamedTempFile::new()
            .map_err(|e| StorageError::StorageUnavailable(e.into()))?;
        let file = temporary
            .reopen()
            .map_err(|e| StorageError::StorageUnavailable(e.into()))?;
        let mut spool = tokio::fs::File::from_std(file);
        let mut limited = params.body.take(params.max_bytes + 1);
        let size_bytes = tokio::io::copy(&mut limited, &mut spool)
            .await
            .map_err(|e| StorageError::StorageUnavailable(e.into()))?;
        if size_bytes > params.max_bytes {
            return Err(StorageError::FileTooLarge);
        }
        spool
            .flush()
            .await
            .map_err(|e| StorageError::StorageUnavailable(e.into()))?;
        let body = ByteStream::from_path(temporary.path())
            .await
            .map_err(|e| StorageError::StorageUnavailable(e.into()))?;
        self.client
            .put_object()
            .bucket(&params.location.bucket)
            .key(&params.location.key)
            .content_type(params.content_type)
            .content_length(size_bytes as i64)
            .if_none_match("*")
            .body(body)
            .send()
            .await
            .map_err(|e| storage_error(e.raw_response().map(|r| r.status().as_u16()), e))?;
        Ok(StoredObject {
            location: params.location,
            size_bytes,
        })
    }

    async fn head(&self, location: &ObjectLocation) -> Result<ObjectHead, StorageError> {
        match self
            .client
            .head_object()
            .bucket(&location.bucket)
            .key(&location.key)
            .send()
            .await
        {
            Ok(output) => Ok(ObjectHead::Present {
                size_bytes: output
                    .content_length()
                    .and_then(|size| u64::try_from(size).ok())
                    .ok_or_else(|| {
                        StorageError::StorageUnavailable(anyhow::anyhow!(
                            "Missing or invalid S3 content length"
                        ))
                    })?,
                content_type: output
                    .content_type()
                    .ok_or_else(|| {
                        StorageError::StorageUnavailable(anyhow::anyhow!("Missing S3 content type"))
                    })?
                    .into(),
            }),
            Err(error)
                if error
                    .raw_response()
                    .is_some_and(|r| r.status().as_u16() == 404) =>
            {
                Ok(ObjectHead::Missing)
            }
            Err(error) => Err(storage_error(
                error.raw_response().map(|r| r.status().as_u16()),
                error,
            )),
        }
    }

    async fn presign_get(&self, params: PresignGet) -> Result<PresignedGet, StorageError> {
        if !(1..=604800).contains(&params.ttl_seconds)
            || params.location.bucket.trim().is_empty()
            || params.location.key.trim().is_empty()
            || params.original_filename.trim().is_empty()
        {
            return Err(StorageError::InvalidObjectRequest);
        }
        let seconds = u64::try_from(params.now.as_second())
            .map_err(|_| StorageError::InvalidObjectRequest)?;
        let start = UNIX_EPOCH
            .checked_add(Duration::from_secs(seconds))
            .ok_or(StorageError::InvalidObjectRequest)?;
        let expires_at = params
            .now
            .checked_add(jiff::SignedDuration::from_secs(params.ttl_seconds as i64))
            .map_err(|_| StorageError::InvalidObjectRequest)?;
        let signing = PresigningConfig::builder()
            .start_time(start)
            .expires_in(Duration::from_secs(params.ttl_seconds))
            .build()
            .map_err(|_| StorageError::InvalidObjectRequest)?;
        let filename = percent_encoding::utf8_percent_encode(
            &params.original_filename,
            percent_encoding::NON_ALPHANUMERIC,
        );
        let request = self
            .client
            .get_object()
            .bucket(params.location.bucket)
            .key(params.location.key)
            .response_content_disposition(format!("attachment; filename*=UTF-8''{filename}"))
            .presigned(signing)
            .await
            .map_err(|error| StorageError::StorageConfiguration(error.into()))?;
        Ok(PresignedGet {
            url: request.uri().to_owned(),
            expires_at,
        })
    }

    async fn delete(&self, location: &ObjectLocation) -> Result<(), StorageError> {
        match self
            .client
            .delete_object()
            .bucket(&location.bucket)
            .key(&location.key)
            .send()
            .await
        {
            Ok(_) => Ok(()),
            Err(error)
                if error
                    .raw_response()
                    .is_some_and(|r| r.status().as_u16() == 404) =>
            {
                Ok(())
            }
            Err(error) => Err(storage_error(
                error.raw_response().map(|r| r.status().as_u16()),
                error,
            )),
        }
    }
}

#[cfg(test)]
#[path = "tests/s3.rs"]
mod tests;
