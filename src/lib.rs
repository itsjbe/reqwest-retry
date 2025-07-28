use chrono::Utc;
use pin_project_lite::pin_project;
use reqwest::{Error as ReqwestError, RequestBuilder, Response, StatusCode};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use thiserror::Error;
use tokio::time::{sleep, Sleep};

#[cfg(doc)]
mod examples;
pub mod predicates;
mod tests;

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

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: usize,
    /// Base delay for exponential backoff
    pub base_delay: Duration,
    /// Maximum delay for exponential backoff
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Function to determine if an error should trigger a retry
    pub should_retry: fn(&ReqwestError) -> bool,
    /// Function to determine if a response should trigger a retry
    pub should_retry_response: fn(&reqwest::Response) -> bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 0,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(30),
            backoff_multiplier: 2.0,
            should_retry: default_should_retry_error,
            should_retry_response: default_should_retry_response,
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
    let should_retry = status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS; // Too Many Requests
    let now = Utc::now().to_rfc3339();
    println!(
        "{now}, Response status: {} -> should retry: {}",
        status, should_retry
    );
    should_retry
}

/// Extension trait for reqwest::RequestBuilder to add retry functionality
pub trait RetryExt {
    /// add retry functionality with default configuration
    fn or_retry(self) -> RetryFuture;

    /// Add retry functionality with custom configuration
    fn or_retry_with_config(self, config: RetryConfig) -> RetryFuture;
}

impl RetryExt for reqwest::RequestBuilder {
    fn or_retry(self) -> RetryFuture {
        // RetryFuture::new(self, RetryConfig::default())
        RetryExt::or_retry_with_config(self, RetryConfig::default())
    }

    fn or_retry_with_config(self, config: RetryConfig) -> RetryFuture {
        println!("Using retry config: {:?}", config);
        RetryFuture::new(self, config)
    }
}

pin_project! {
    /// Future that handles the retry logic
    pub struct RetryFuture {
        request_builder: Option<reqwest::RequestBuilder>,
        config: RetryConfig,
        attempts: usize,
        #[pin]
        state: RetryState,
    }
}

pin_project! {
    #[project = RetryStateProj]
    enum RetryState {
        Ready,
        Requesting {
            #[pin]
            future: Pin<Box<dyn Future<Output = Result<Response, ReqwestError>> + Send>>,
        },
        Sleeping {
             #[pin]
            sleep: Sleep,
        },
        Done,
    }
}

impl RetryFuture {
    fn new(request_builder: RequestBuilder, config: RetryConfig) -> Self {
        Self {
            request_builder: Some(request_builder),
            config,
            attempts: 0,
            state: RetryState::Ready,
        }
    }

    fn calculate_delay(&self) -> Duration {
        let exp_delay = self.config.base_delay.as_millis() as f64
            * self.config.backoff_multiplier.powi(self.attempts as i32);

        let delay_millis = exp_delay.min(self.config.max_delay.as_millis() as f64) as u64;
        Duration::from_millis(delay_millis)
    }
}

impl Future for RetryFuture {
    type Output = Result<Response, RetryError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            // We need to handle state transitions outside the match to avoid borrow conflicts
            let mut next_state: Option<RetryState> = None;
            let mut should_continue = false;

            let poll_result = match this.state.as_mut().project() {
                RetryStateProj::Ready => {
                    // Check if we've exceeded max retries
                    if *this.attempts > this.config.max_retries {
                        return Poll::Ready(Err(RetryError::MaxRetriesExceeded));
                    }

                    // Clone the request for this attempt
                    let request = match this.request_builder.as_ref() {
                        Some(builder) => match builder.try_clone() {
                            Some(cloned) => cloned,
                            None => {
                                return Poll::Ready(Err(RetryError::RequestBuilderCloneError));
                            }
                        },
                        None => {
                            return Poll::Ready(Err(RetryError::RequestBuilderNotAvailable));
                        }
                    };

                    // Prepare to transition to Requesting state
                    let future = Box::pin(request.send());
                    next_state = Some(RetryState::Requesting { future });
                    should_continue = true;
                    Poll::Pending // Will be overridden by continue
                }

                RetryStateProj::Requesting { future } => {
                    match future.poll(cx) {
                        Poll::Ready(Ok(response)) => {
                            // Check if response indicates we should retry
                            if (this.config.should_retry_response)(&response)
                                && *this.attempts < this.config.max_retries
                            {
                                *this.attempts += 1;
                                // Calculate delay inline
                                let exponential_delay = this.config.base_delay.as_millis() as f64
                                    * this.config.backoff_multiplier.powi(*this.attempts as i32);
                                let delay_ms = exponential_delay
                                    .min(this.config.max_delay.as_millis() as f64)
                                    as u64;
                                let delay = Duration::from_millis(delay_ms);

                                next_state = Some(RetryState::Sleeping {
                                    sleep: sleep(delay),
                                });
                                should_continue = true;
                                Poll::Pending // Will be overridden by continue
                            } else {
                                Poll::Ready(Ok(response))
                            }
                        }
                        Poll::Ready(Err(error)) => {
                            // Check if this error should trigger a retry
                            if !(this.config.should_retry)(&error) {
                                Poll::Ready(Err(RetryError::NonRetryableError(error)))
                            } else if *this.attempts >= this.config.max_retries {
                                Poll::Ready(Err(RetryError::RequestError(error)))
                            } else {
                                *this.attempts += 1;
                                // Calculate delay inline
                                let exponential_delay = this.config.base_delay.as_millis() as f64
                                    * this.config.backoff_multiplier.powi(*this.attempts as i32);
                                let delay_ms = exponential_delay
                                    .min(this.config.max_delay.as_millis() as f64)
                                    as u64;
                                let delay = Duration::from_millis(delay_ms);

                                next_state = Some(RetryState::Sleeping {
                                    sleep: sleep(delay),
                                });
                                should_continue = true;
                                Poll::Pending // Will be overridden by continue
                            }
                        }
                        Poll::Pending => Poll::Pending,
                    }
                }

                RetryStateProj::Sleeping { sleep } => {
                    match sleep.poll(cx) {
                        Poll::Ready(()) => {
                            next_state = Some(RetryState::Ready);
                            should_continue = true;
                            Poll::Pending // Will be overridden by continue
                        }
                        Poll::Pending => Poll::Pending,
                    }
                }

                RetryStateProj::Done => {
                    panic!("RetryFuture polled after completion");
                }
            };

            // Apply state transition if needed
            if let Some(new_state) = next_state {
                this.state.set(new_state);
            }

            if should_continue {
                continue;
            } else {
                return poll_result;
            }
        }
    }
}
