use super::*;
use crate::error::RetryError;
use reqwest::Client;
use std::time::Duration;

/// Basic usage with default retry configuration
async fn basic_usage() -> Result<reqwest::Response, RetryError> {
    let response = Client::new()
        .get("https://api.example.com/data")
        .or_retry()
        .await?;

    Ok(response)
}

/// Advanced usage with custom configuration and logging
async fn advanced_usage_with_logging() -> Result<reqwest::Response, RetryError> {
    let config = RetryConfig::new()
        .max_retries(5)
        .base_delay(Duration::from_millis(500))
        .max_delay(Duration::from_secs(30))
        .backoff_multiplier(1.5)
        .should_retry_error(predicates::network_errors_only)
        .should_retry_response(predicates::server_errors_and_rate_limit)
        .backoff_fn(backoff::exponential_jitter)
        .on_retry(|attempt| {
            println!(
                "Retrying request (attempt {}/{}) after {}ms delay. Reason: {}",
                attempt.attempt,
                attempt.max_attempts,
                attempt.delay.as_millis(),
                attempt
                    .error
                    .as_deref()
                    .or_else(|| attempt
                        .response_status
                        .map(|s| format!("HTTP {}", s).as_str()))
                    .unwrap_or("unknown")
            );
        })
        .on_failure(|attempt| {
            eprintln!(
                "Request failed after {} attempts. Final error: {}",
                attempt.max_attempts,
                attempt.error.as_deref().unwrap_or("unknown")
            );
        });

    let response = Client::new()
        .post("https://api.example.com/upload")
        .json(&serde_json::json!({"key": "value"}))
        .or_retry_with(config)
        .await?;

    Ok(response)
}

/// Usage with different backoff strategies
async fn different_backoff_strategies() -> Result<reqwest::Response, RetryError> {
    // Linear backoff
    let linear_config = RetryConfig::new().backoff_fn(backoff::linear);

    // Fixed delay
    let fixed_config = RetryConfig::new().backoff_fn(backoff::fixed);

    // Fibonacci backoff
    let fibonacci_config = RetryConfig::new().backoff_fn(backoff::fibonacci);

    // Exponential with jitter (recommended for production)
    let jitter_config = RetryConfig::new().backoff_fn(backoff::exponential_jitter);

    let response = Client::new()
        .get("https://api.example.com/data")
        .or_retry_with(jitter_config)
        .await?;

    Ok(response)
}

/// Usage with POST request and JSON body
async fn post_with_retry() -> Result<reqwest::Response, RetryError> {
    let request_body = serde_json::json!({
        "name": "example",
        "data": "some data"
    });

    let response = Client::default()
        .post("http://localhost:8080/api/uploads/init")
        .json(&request_body)
        .header("Authorization", "Bearer token123")
        .or_retry_with(
            RetryConfig::new()
                .max_retries(3)
                .base_delay(Duration::from_millis(200)),
        )
        .await?;

    Ok(response)
}

/// Usage with error-specific retry strategies
async fn error_specific_strategies() -> Result<reqwest::Response, RetryError> {
    let config = RetryConfig::new()
        // Rate limits: more aggressive retries with longer delays
        .error_strategy(
            RetryReason::RateLimit,
            ErrorStrategy::new()
                .max_retries(10)
                .base_delay(Duration::from_secs(1))
                .max_delay(Duration::from_secs(60))
                .backoff_fn(backoff::exponential_jitter),
        )
        // Network errors: quick retries
        .error_strategy(
            RetryReason::NetworkError,
            ErrorStrategy::new()
                .max_retries(5)
                .base_delay(Duration::from_millis(100))
                .backoff_fn(backoff::linear),
        )
        // Server errors: moderate retries
        .error_strategy(
            RetryReason::ServerError,
            ErrorStrategy::new()
                .max_retries(3)
                .base_delay(Duration::from_millis(500))
                .backoff_fn(backoff::fibonacci),
        )
        .on_retry(|attempt| {
            println!(
                "Retrying {:?} (attempt {}/{}) after {}ms",
                attempt.error_type,
                attempt.attempt,
                attempt.max_attempts,
                attempt.delay.as_millis()
            );
        });

    let response = Client::new()
        .post("https://api.example.com/upload")
        .json(&serde_json::json!({"key": "value"}))
        .or_retry_with(config)
        .await?;

    Ok(response)
}

/// Usage with custom error classification
async fn custom_error_classification() -> Result<reqwest::Response, RetryError> {
    // Custom error classifier that recognizes specific API errors
    let custom_classifier = |error: &ReqwestError| -> RetryReason {
        if let Some(status) = error.status() {
            match status.as_u16() {
                503 => RetryReason::Custom("MaintenanceMode".to_string()),
                422 => RetryReason::Custom("ValidationError".to_string()),
                _ => default_error_classifier(error),
            }
        } else {
            default_error_classifier(error)
        }
    };

    let config = RetryConfig::new()
        .error_classifier(custom_classifier)
        .error_strategy(
            RetryReason::Custom("MaintenanceMode".to_string()),
            ErrorStrategy::new()
                .max_retries(20) // Wait longer for maintenance to complete
                .base_delay(Duration::from_secs(30))
                .backoff_fn(backoff::fixed),
        )
        .error_strategy(
            RetryReason::Custom("ValidationError".to_string()),
            ErrorStrategy::new().max_retries(0), // Don't retry validation errors
        );

    let response = Client::new()
        .get("https://api.example.com/data")
        .or_retry_with(config)
        .await?;

    Ok(response)
}
