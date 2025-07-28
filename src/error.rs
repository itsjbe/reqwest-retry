use reqwest::Error as ReqwestError;
use thiserror::Error;

/// Errors that can occur during retry operations
#[derive(Error, Debug)]
pub enum RetryError {
    #[error("Maximum retry attempts exceeded")]
    MaxRetriesExceeded,
    #[error("Request failed with non-retryable error: {0}")]
    NonRetryableError(ReqwestError),
    #[error("Request failed: {0}")]
    RequestError(ReqwestError),
    #[error("Cannot clone request builder - request body may not be cloneable")]
    RequestBuilderCloneError,
    #[error("Request builder not available")]
    RequestBuilderNotAvailable,
}
