use crate::config::RetryConfig;
use crate::retry_future::RetryFuture;

/// Extension trait for reqwest::RequestBuilder to add retry functionality
pub trait RetryExt {
    /// Add retry functionality with default configuration
    fn or_retry(self) -> RetryFuture;

    /// Add retry functionality with custom configuration
    fn or_retry_with(self, config: RetryConfig) -> RetryFuture;
}

impl RetryExt for reqwest::RequestBuilder {
    fn or_retry(self) -> RetryFuture {
        RetryFuture::new(self, RetryConfig::default())
    }

    fn or_retry_with(self, config: RetryConfig) -> RetryFuture {
        RetryFuture::new(self, config)
    }
}
