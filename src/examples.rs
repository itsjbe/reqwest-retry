// Example usage documentation

use super::*;
use crate::{predicates, RetryConfig, RetryError};
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

/// Advanced usage with custom configuration
async fn advanced_usage() -> Result<reqwest::Response, RetryError> {
    let config = RetryConfig::new()
        .max_retries(5)
        .base_delay(Duration::from_millis(500))
        .max_delay(Duration::from_secs(30))
        .backoff_multiplier(1.5)
        .should_retry_error(predicates::network_errors_only)
        .should_retry_response(predicates::server_errors_and_rate_limit);

    let response = Client::new()
        .post("https://api.example.com/upload")
        .json(&serde_json::json!({"key": "value"}))
        .or_retry_with(config)
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
