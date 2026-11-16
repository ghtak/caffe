use std::borrow::Cow;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("validation failed: {0}")]
    Validation(Cow<'static, str>),

    #[error("invalid parameter: {0}")]
    InvalidParameter(Cow<'static, str>),

    #[error("illegal state: {0}")]
    IllegalState(Cow<'static, str>),

    #[error("not found: {0}")]
    NotFound(Cow<'static, str>),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("config error: {0}")]
    ConfigError(#[from] ::config::ConfigError),

    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

pub type Result<R> = std::result::Result<R, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anyhow_error_converts_with_context() {
        fn fallible_operation() -> anyhow::Result<()> {
            Err(anyhow::anyhow!("root cause").context("operation failed"))
        }

        fn operation() -> super::Result<()> {
            fallible_operation()?;
            Ok(())
        }

        let Error::Internal(error) = operation().unwrap_err() else {
            panic!("expected an internal error");
        };
        let chain: Vec<_> = error.chain().map(ToString::to_string).collect();
        assert_eq!(chain, ["operation failed", "root cause"]);
    }
}
