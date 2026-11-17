use std::fmt;

/// 클라이언트가 오류를 구분하고 처리하는 기준.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Internal,
    InvalidConfiguration,
    InvalidRequest,
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

    pub fn internal(message: impl Into<String>) -> Self { Self::new(Code::Internal, message) }

    pub fn at(mut self, at: &'static str) -> Self {
        self.at = Some(at);
        self
    }

    pub fn cause(mut self, cause: impl Into<anyhow::Error>) -> Self {
        self.cause = Some(cause.into());
        self
    }
}

/// 작업을 완료할 수 없을 때 호출자에게 전달하는 실패 결과.
///
/// 내부 오류는 원인과 필요한 맥락을 보존해 전달하고, 작업 실패의 의미를
/// 판단할 수 있는 곳에서 이 타입으로 변환한다. 동일한 실패를 상위 작업에
/// 전달할 때는 불필요하게 다시 감싸지 않는다.
/// 최종 출력 경계에서 `code`와 `message`를 사용자에게 전달하며,
/// `at`과 `cause`는 로그에만 기록한다.
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

// Validate 검증 대응
impl From<std::convert::Infallible> for Error {
    fn from(value: std::convert::Infallible) -> Self { match value {} }
}

pub type Result<T> = std::result::Result<T, Error>;
