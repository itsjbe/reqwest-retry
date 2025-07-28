use crate::config::RetryConfig;
use crate::error::RetryError;
use crate::{EffectiveStrategy, RetryAttempt, RetryReason};
use pin_project_lite::pin_project;
use reqwest::{Error as ReqwestError, Response};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{sleep, Sleep};

pin_project! {
    /// Future that handles the retry logic
    pub struct RetryFuture {
        request_builder: Option<reqwest::RequestBuilder>,
        config: RetryConfig,
        attempts: usize,
        current_error_type: Option<RetryReason>,
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
    pub(crate) fn new(request_builder: reqwest::RequestBuilder, config: RetryConfig) -> Self {
        Self {
            request_builder: Some(request_builder),
            config,
            attempts: 0,
            current_error_type: None,
            state: RetryState::Ready,
        }
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
                    // Get effective strategy for current error type
                    let strategy = if let Some(error_type) = this.current_error_type.as_ref() {
                        this.config.get_effective_strategy(error_type)
                    } else {
                        // First attempt, use default strategy
                        EffectiveStrategy {
                            max_retries: this.config.max_retries,
                            base_delay: this.config.base_delay,
                            max_delay: this.config.max_delay,
                            backoff_multiplier: this.config.backoff_multiplier,
                            backoff_fn: this.config.backoff_fn,
                        }
                    };

                    // Check if we've exceeded max retries for this error type
                    if *this.attempts > strategy.max_retries {
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
                            // Classify the response error
                            let error_type = (this.config.response_classifier)(&response);

                            // Check if response indicates we should retry
                            if (this.config.should_retry_response)(&response) {
                                let strategy = this.config.get_effective_strategy(&error_type);

                                if *this.attempts < strategy.max_retries {
                                    *this.attempts += 1;
                                    *this.current_error_type = Some(error_type.clone());

                                    // Calculate delay using error-specific strategy
                                    let delay = (strategy.backoff_fn)(
                                        *this.attempts,
                                        strategy.base_delay,
                                        strategy.backoff_multiplier,
                                        strategy.max_delay,
                                    );

                                    // Call retry callback if provided
                                    if let Some(on_retry) = this.config.on_retry {
                                        let retry_info = RetryAttempt {
                                            attempt: *this.attempts,
                                            max_attempts: strategy.max_retries + 1,
                                            delay,
                                            error: None,
                                            response_status: Some(response.status().as_u16()),
                                            error_type: error_type.clone(),
                                        };
                                        on_retry(&retry_info);
                                    }

                                    next_state = Some(RetryState::Sleeping {
                                        sleep: sleep(delay),
                                    });
                                    should_continue = true;
                                    Poll::Pending // Will be overridden by continue
                                } else {
                                    Poll::Ready(Ok(response))
                                }
                            } else {
                                Poll::Ready(Ok(response))
                            }
                        }
                        Poll::Ready(Err(error)) => {
                            // Classify the error
                            let error_type = (this.config.error_classifier)(&error);

                            // Check if this error should trigger a retry
                            if !(this.config.should_retry)(&error) {
                                Poll::Ready(Err(RetryError::NonRetryableError(error)))
                            } else {
                                let strategy = this.config.get_effective_strategy(&error_type);

                                if *this.attempts >= strategy.max_retries {
                                    // Call failure callback if provided
                                    if let Some(on_failure) = this.config.on_failure {
                                        let retry_info = RetryAttempt {
                                            attempt: *this.attempts,
                                            max_attempts: strategy.max_retries + 1,
                                            delay: Duration::from_secs(0),
                                            error: Some(error.to_string()),
                                            response_status: None,
                                            error_type: error_type.clone(),
                                        };
                                        on_failure(&retry_info);
                                    }
                                    Poll::Ready(Err(RetryError::RequestError(error)))
                                } else {
                                    *this.attempts += 1;
                                    *this.current_error_type = Some(error_type.clone());

                                    // Calculate delay using error-specific strategy
                                    let delay = (strategy.backoff_fn)(
                                        *this.attempts,
                                        strategy.base_delay,
                                        strategy.backoff_multiplier,
                                        strategy.max_delay,
                                    );

                                    // Call retry callback if provided
                                    if let Some(on_retry) = this.config.on_retry {
                                        let retry_info = RetryAttempt {
                                            attempt: *this.attempts,
                                            max_attempts: strategy.max_retries + 1,
                                            delay,
                                            error: Some(error.to_string()),
                                            response_status: None,
                                            error_type: error_type.clone(),
                                        };
                                        on_retry(&retry_info);
                                    }

                                    next_state = Some(RetryState::Sleeping {
                                        sleep: sleep(delay),
                                    });
                                    should_continue = true;
                                    Poll::Pending // Will be overridden by continue
                                }
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
