use thiserror::Error;

#[derive(Debug, Error)]
pub enum PcsoError {
    #[error("invalid date `{0}`: {1}")]
    InvalidDate(String, String),

    #[error("invalid date range: from {from} is after to {to}")]
    InvalidRange { from: String, to: String },

    #[error("browser error: {0}")]
    Browser(String),

    #[error("timed out waiting for {0}")]
    Timeout(String),

    #[error("S3 upload failed for key `{key}`: {source}")]
    S3 {
        key: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("date {date} failed after {attempts} attempts; re-run with `--from {date}` to resume. Last error: {source}")]
    DateAborted {
        date: String,
        attempts: u32,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

pub type Result<T> = std::result::Result<T, PcsoError>;
