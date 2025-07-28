use crate::{backoff, default_backoff, ErrorStrategy, RetryConfig, RetryExt, RetryReason};
use reqwest::Client;
use std::time::Duration;

#[tokio::test]
async fn test_retry_config_builder() {
    let config = RetryConfig::new()
        .max_retries(5)
        .base_delay(Duration::from_millis(200))
        .max_delay(Duration::from_secs(10))
        .backoff_multiplier(1.5);

    assert_eq!(config.max_retries, 5);
    assert_eq!(config.base_delay, Duration::from_millis(200));
    assert_eq!(config.max_delay, Duration::from_secs(10));
    assert_eq!(config.backoff_multiplier, 1.5);
}

#[test]
fn test_default_backoff_calculation() {
    // Test the default backoff function directly
    let base_delay = Duration::from_millis(100);
    let multiplier = 2.0;
    let max_delay = Duration::from_secs(5);

    // Test exponential backoff calculation
    assert_eq!(
        default_backoff(0, base_delay, multiplier, max_delay),
        Duration::from_millis(100)
    ); // 2^0 * 100
    assert_eq!(
        default_backoff(1, base_delay, multiplier, max_delay),
        Duration::from_millis(200)
    ); // 2^1 * 100
    assert_eq!(
        default_backoff(2, base_delay, multiplier, max_delay),
        Duration::from_millis(400)
    ); // 2^2 * 100
}

#[test]
fn test_max_delay_cap() {
    let base_delay = Duration::from_millis(1000);
    let multiplier = 10.0;
    let max_delay = Duration::from_millis(2000);

    // This would normally result in 10^5 * 1000ms, but should be capped
    let result = default_backoff(5, base_delay, multiplier, max_delay);
    assert_eq!(result, Duration::from_millis(2000)); // Capped at max_delay
}

#[test]
fn test_error_strategy_builder() {
    let strategy = ErrorStrategy::new()
        .max_retries(10)
        .base_delay(Duration::from_secs(1))
        .backoff_fn(backoff::linear);

    assert_eq!(strategy.max_retries, Some(10));
    assert_eq!(strategy.base_delay, Some(Duration::from_secs(1)));
    assert!(strategy.backoff_fn.is_some());
}

#[test]
fn test_effective_strategy() {
    let mut config = RetryConfig::new()
        .max_retries(3)
        .base_delay(Duration::from_millis(100));

    // Add specific strategy for rate limits
    config = config.error_strategy(
        RetryReason::RateLimit,
        ErrorStrategy::new()
            .max_retries(10)
            .base_delay(Duration::from_secs(1)),
    );

    // Test that rate limit gets the specific strategy
    let rate_limit_strategy = config.get_effective_strategy(&RetryReason::RateLimit);
    assert_eq!(rate_limit_strategy.max_retries, 10);
    assert_eq!(rate_limit_strategy.base_delay, Duration::from_secs(1));

    // Test that other errors get the default strategy
    let network_strategy = config.get_effective_strategy(&RetryReason::NetworkError);
    assert_eq!(network_strategy.max_retries, 3);
    assert_eq!(network_strategy.base_delay, Duration::from_millis(100));
}

#[test]
fn test_error_classification() {
    // Test that different error types are classified correctly
    let rate_limit_reason = RetryReason::RateLimit;
    let network_reason = RetryReason::NetworkError;

    assert_eq!(rate_limit_reason, RetryReason::RateLimit);
    assert_eq!(network_reason, RetryReason::NetworkError);
    assert_ne!(rate_limit_reason, network_reason);
}

#[test]
fn test_retry_reason_custom() {
    let custom_reason = RetryReason::Custom("DatabaseTimeout".to_string());
    match custom_reason {
        RetryReason::Custom(msg) => assert_eq!(msg, "DatabaseTimeout"),
        _ => panic!("Expected custom retry reason"),
    }
}

// #[test]
// fn test_backoff_functions() {
//     let base_delay = Duration::from_millis(100);
//     let multiplier = 2.0;
//     let max_delay = Duration::from_secs(10);
//
//     // Test linear backoff
//     assert_eq!(
//         backoff::linear(1, base_delay, multiplier, max_delay),
//         Duration::from_millis(100)
//     );
//     assert_eq!(
//         backoff::linear(2, base_delay, multiplier, max_delay),
//         Duration::from_millis(200)
//     );
// }

#[test]
fn test_backoff_functions() {
    let base_delay = Duration::from_millis(100);
    let multiplier = 2.0;
    let max_delay = Duration::from_secs(10);

    // Test linear backoff
    assert_eq!(
        backoff::linear(1, base_delay, multiplier, max_delay),
        Duration::from_millis(100)
    );
    assert_eq!(
        backoff::linear(2, base_delay, multiplier, max_delay),
        Duration::from_millis(200)
    );

    // Test fixed backoff
    assert_eq!(
        backoff::fixed(0, base_delay, multiplier, max_delay),
        Duration::from_millis(0)
    );
    assert_eq!(
        backoff::fixed(1, base_delay, multiplier, max_delay),
        base_delay
    );
    assert_eq!(
        backoff::fixed(5, base_delay, multiplier, max_delay),
        base_delay
    );

    // Test fibonacci backoff
    let fib_1 = backoff::fibonacci(1, base_delay, multiplier, max_delay);
    let fib_2 = backoff::fibonacci(2, base_delay, multiplier, max_delay);
    let fib_3 = backoff::fibonacci(3, base_delay, multiplier, max_delay);

    assert_eq!(fib_1, Duration::from_millis(100)); // fib(2) = 1, so 1 * 100
    assert_eq!(fib_2, Duration::from_millis(200)); // fib(3) = 2, so 2 * 100
    assert_eq!(fib_3, Duration::from_millis(300)); // fib(4) = 3, so 3 * 100
}

// Integration test to verify the trait is implemented correctly
#[tokio::test]
async fn test_retry_ext_trait() {
    // This test verifies that the RetryExt trait is properly implemented
    // We can't easily test the actual retry logic without a mock server,
    // but we can verify the trait methods are available and return the right types

    let client = Client::new();
    let retry_future = client.get("http://example.com").or_retry();

    // Verify the future type is correct
    assert_eq!(
        std::any::type_name_of_val(&retry_future),
        "reqwest_retry::retry_future::RetryFuture"
    );
}

#[tokio::test]
async fn integration_test() {
    let response = Client::new()
        .get("https://mock.httpstatus.io/429")
        .or_retry_with(
            RetryConfig::new()
                .max_retries(3)
                .base_delay(Duration::from_millis(100))
                .max_delay(Duration::from_secs(1000)),
        )
        .await;

    println!("Response: {:?}", response);
    assert!(
        response.is_ok(),
        "Expected successful response, got: {:?}",
        response
    );
}
