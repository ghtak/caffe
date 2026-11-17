use std::fmt;

/// 클라이언트가 오류를 구분하고 처리하는 기준.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Internal,
    InvalidConfiguration,
}

impl Error {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            at: None,
            cause: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(Code::Internal, message)
    }

    pub fn at(mut self, at: &'static str) -> Self {
        self.at = Some(at);
        self
    }

    pub fn cause(mut self, cause: impl Into<anyhow::Error>) -> Self {
        self.cause = Some(cause.into());
        self
    }
}

/// 앱 오류. `message`는 클라이언트에 전달하고, `at`과 `cause`는 로그에만 기록한다.
#[derive(Debug)]
pub struct Error {
    pub code: Code,
    pub message: String,
    pub at: Option<&'static str>,
    pub cause: Option<anyhow::Error>,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.code)?;
        if let Some(at) = self.at {
            write!(f, " at {at}")?;
        }
        write!(f, ": {}", self.message)?;
        if let Some(cause) = &self.cause {
            write!(f, ": {cause:#}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|cause| cause.as_ref())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
