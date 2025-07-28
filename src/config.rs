use crate::{
    default_backoff, default_error_classifier, default_response_classifier, default_should_retry_error, default_should_retry_response,
    EffectiveStrategy, ErrorStrategy, RetryAttempt,
    RetryReason,
};
use reqwest::{Error as ReqwestError, Response};
use std::collections::HashMap;
use std::time::Duration;

/// Configuration for retry behavior
pub struct RetryConfig {
    /// Maximum number of retry attempts (default fallback)
    pub max_retries: usize,
    /// Base delay for exponential backoff (default fallback)
    pub base_delay: Duration,
    /// Maximum delay between retries (default fallback)
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (default fallback)
    pub backoff_multiplier: f64,
    /// Function to determine if an error should trigger a retry
    pub should_retry: fn(&ReqwestError) -> bool,
    /// Function to determine if a response should trigger a retry
    pub should_retry_response: fn(&Response) -> bool,
    /// Custom backoff calculation function (default fallback)
    pub backoff_fn:
        fn(attempt: usize, base_delay: Duration, multiplier: f64, max_delay: Duration) -> Duration,
    /// Callback called before each retry attempt
    pub on_retry: Option<fn(&RetryAttempt)>,
    /// Callback called when retries are exhausted
    pub on_failure: Option<fn(&RetryAttempt)>,
    /// Error-specific retry strategies
    pub error_strategies: HashMap<RetryReason, ErrorStrategy>,
    /// Function to classify errors into retry reasons
    pub error_classifier: fn(&ReqwestError) -> RetryReason,
    /// Function to classify response statuses into retry reasons
    pub response_classifier: fn(&Response) -> RetryReason,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            should_retry: default_should_retry_error,
            should_retry_response: default_should_retry_response,
            backoff_fn: default_backoff,
            on_retry: None,
            on_failure: None,
            error_strategies: HashMap::new(),
            error_classifier: default_error_classifier,
            response_classifier: default_response_classifier,
        }
    }
}

impl RetryConfig {
    /// Create a new RetryConfig with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum number of retries
    pub fn max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set base delay for exponential backoff
    pub fn base_delay(mut self, delay: Duration) -> Self {
        self.base_delay = delay;
        self
    }

    /// Set maximum delay between retries
    pub fn max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Set backoff multiplier
    pub fn backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    /// Set custom error retry predicate
    pub fn should_retry_error(mut self, predicate: fn(&ReqwestError) -> bool) -> Self {
        self.should_retry = predicate;
        self
    }

    /// Set custom response retry predicate
    pub fn should_retry_response(mut self, predicate: fn(&Response) -> bool) -> Self {
        self.should_retry_response = predicate;
        self
    }

    /// Set custom backoff calculation function
    pub fn backoff_fn(
        mut self,
        backoff_fn: fn(usize, Duration, f64, Duration) -> Duration,
    ) -> Self {
        self.backoff_fn = backoff_fn;
        self
    }

    /// Set callback for retry attempts
    pub fn on_retry(mut self, callback: fn(&RetryAttempt)) -> Self {
        self.on_retry = Some(callback);
        self
    }

    /// Set callback for when retries are exhausted
    pub fn on_failure(mut self, callback: fn(&RetryAttempt)) -> Self {
        self.on_failure = Some(callback);
        self
    }

    /// Set error-specific retry strategy
    pub fn error_strategy(mut self, error_type: RetryReason, strategy: ErrorStrategy) -> Self {
        self.error_strategies.insert(error_type, strategy);
        self
    }

    /// Set custom error classifier
    pub fn error_classifier(mut self, classifier: fn(&ReqwestError) -> RetryReason) -> Self {
        self.error_classifier = classifier;
        self
    }

    /// Set custom response classifier
    pub fn response_classifier(mut self, classifier: fn(&Response) -> RetryReason) -> Self {
        self.response_classifier = classifier;
        self
    }

    /// Get effective strategy for a specific error type
    pub(crate) fn get_effective_strategy(&self, error_type: &RetryReason) -> EffectiveStrategy {
        let strategy = self.error_strategies.get(error_type);

        EffectiveStrategy {
            max_retries: strategy
                .and_then(|s| s.max_retries)
                .unwrap_or(self.max_retries),
            base_delay: strategy
                .and_then(|s| s.base_delay)
                .unwrap_or(self.base_delay),
            max_delay: strategy.and_then(|s| s.max_delay).unwrap_or(self.max_delay),
            backoff_multiplier: strategy
                .and_then(|s| s.backoff_multiplier)
                .unwrap_or(self.backoff_multiplier),
            backoff_fn: strategy
                .and_then(|s| s.backoff_fn)
                .unwrap_or(self.backoff_fn),
        }
    }
}
