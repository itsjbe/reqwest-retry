use reqwest::{Error as ReqwestError, Response, StatusCode};
use std::time::Duration;

mod config;
mod error;
mod retry_future;
mod trait_impl;
pub use config::RetryConfig;
pub use error::RetryError;
pub use retry_future::RetryFuture;
pub use trait_impl::RetryExt;
pub mod backoff;
pub mod predicates;

// Example usage documentation
#[cfg(doc)]
mod examples;

#[cfg(test)]
mod tests;

/// Information about the current retry attempt
#[derive(Debug, Clone)]
pub struct RetryAttempt {
    /// Current attempt number (0-based)
    pub attempt: usize,
    /// Total number of attempts that will be made
    pub max_attempts: usize,
    /// Delay that will be applied before this retry
    pub delay: Duration,
    /// The error that triggered this retry (if any)
    pub error: Option<String>,
    /// The response status that triggered this retry (if any)
    pub response_status: Option<u16>,
    /// The type of error that triggered this retry
    pub error_type: RetryReason,
}

/// The reason why a retry is being attempted
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RetryReason {
    /// Network-level error (connection, timeout, etc.)
    NetworkError,
    /// HTTP server error (5xx status codes)
    ServerError,
    /// Rate limiting (429 status code)
    RateLimit,
    /// Request error (malformed request, etc.)
    RequestError,
    /// Custom error type defined by user
    Custom(String),
}

/// Error-specific retry strategy
#[derive(Clone)]
pub struct ErrorStrategy {
    /// Maximum retries for this error type
    pub max_retries: Option<usize>,
    /// Custom backoff function for this error type
    pub backoff_fn: Option<fn(usize, Duration, f64, Duration) -> Duration>,
    /// Base delay override for this error type
    pub base_delay: Option<Duration>,
    /// Max delay override for this error type
    pub max_delay: Option<Duration>,
    /// Backoff multiplier override for this error type
    pub backoff_multiplier: Option<f64>,
}

impl Default for ErrorStrategy {
    fn default() -> Self {
        Self {
            max_retries: None,
            backoff_fn: None,
            base_delay: None,
            max_delay: None,
            backoff_multiplier: None,
        }
    }
}

/// Default implementation for determining if an error should trigger a retry
fn default_should_retry_error(error: &ReqwestError) -> bool {
    // Retry on network errors, timeouts, and some server errors
    error.is_timeout()
        || error.is_connect()
        || error.is_request()
        || (error.status().map_or(false, |s| s.is_server_error()))
}

/// Default implementation for determining if a response should trigger a retry
fn default_should_retry_response(response: &Response) -> bool {
    // Retry on server errors and some client errors
    let status = response.status();
    status.is_server_error() || status == 429 // Too Many Requests
}

/// Default response classifier
fn default_response_classifier(response: &Response) -> RetryReason {
    let status = response.status();
    if status.is_server_error() {
        RetryReason::ServerError
    } else if status == StatusCode::TOO_MANY_REQUESTS {
        RetryReason::RateLimit
    } else {
        RetryReason::RequestError
    }
}

/// Default error classifier for reqwest errors
fn default_error_classifier(error: &ReqwestError) -> RetryReason {
    if error.is_timeout() || error.is_connect() {
        RetryReason::NetworkError
    } else if error.is_request() {
        RetryReason::RequestError
    } else if let Some(status) = error.status() {
        if status.is_server_error() {
            RetryReason::ServerError
        } else if status == StatusCode::TOO_MANY_REQUESTS {
            RetryReason::RateLimit
        } else {
            RetryReason::RequestError
        }
    } else {
        RetryReason::NetworkError
    }
}
/// Default exponential backoff calculation
fn default_backoff(
    attempt: usize,
    base_delay: Duration,
    multiplier: f64,
    max_delay: Duration,
) -> Duration {
    let exponential_delay = base_delay.as_millis() as f64 * multiplier.powi(attempt as i32);
    let delay_ms = exponential_delay.min(max_delay.as_millis() as f64) as u64;
    Duration::from_millis(delay_ms)
}

impl ErrorStrategy {
    /// Create a new ErrorStrategy
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum retries for this error type
    pub fn max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    /// Set custom backoff function for this error type
    pub fn backoff_fn(
        mut self,
        backoff_fn: fn(usize, Duration, f64, Duration) -> Duration,
    ) -> Self {
        self.backoff_fn = Some(backoff_fn);
        self
    }

    /// Set base delay for this error type
    pub fn base_delay(mut self, delay: Duration) -> Self {
        self.base_delay = Some(delay);
        self
    }

    /// Set max delay for this error type
    pub fn max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = Some(delay);
        self
    }

    /// Set backoff multiplier for this error type
    pub fn backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = Some(multiplier);
        self
    }
}

/// Internal struct to hold the effective strategy for an error type
struct EffectiveStrategy {
    max_retries: usize,
    base_delay: Duration,
    max_delay: Duration,
    backoff_multiplier: f64,
    backoff_fn: fn(usize, Duration, f64, Duration) -> Duration,
}
