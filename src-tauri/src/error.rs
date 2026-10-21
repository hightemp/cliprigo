use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("network error: {0}")]
    Network(String),
    #[error("GitHub rate limit reached, try again later")]
    RateLimited,
    #[error("checksum mismatch for {0}")]
    Checksum(String),
    #[error("unsupported platform: {0}")]
    UnsupportedPlatform(String),
    #[error("invalid proxy URL: {0}")]
    InvalidProxy(String),
    #[error("file not found: {0}")]
    FileMissing(String),
    #[error("component not installed: {0}")]
    ComponentsMissing(String),
    #[error("{message}")]
    Ytdlp {
        kind: crate::ytdlp::errors::ErrorKind,
        message: String,
    },
    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// Stable identifier the UI uses to pick a translated message.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Db(_) => "db",
            Self::Io(_) => "io",
            Self::Json(_) => "json",
            Self::Network(_) => "network",
            Self::RateLimited => "rate_limited",
            Self::Checksum(_) => "checksum",
            Self::UnsupportedPlatform(_) => "unsupported_platform",
            Self::ComponentsMissing(_) => "components_missing",
            Self::FileMissing(_) => "file_missing",
            Self::InvalidProxy(_) => "invalid_proxy",
            Self::Ytdlp { kind, .. } => kind.code(),
            Self::Other(_) => "other",
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        Self::Network(e.to_string())
    }
}

/// Sent to the frontend as `{ code, message }`.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
