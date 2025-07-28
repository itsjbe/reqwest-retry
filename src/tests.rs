use super::*;
use reqwest::Client;

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

#[tokio::test]
async fn test_delay_calculation() {
    let config = RetryConfig::new()
        .base_delay(Duration::from_millis(100))
        .backoff_multiplier(2.0)
        .max_delay(Duration::from_secs(5));

    let future = RetryFuture::new(Client::new().get("http://example.com"), config);

    // Test exponential backoff calculation
    assert_eq!(future.calculate_delay(), Duration::from_millis(100)); // 2^0 * 100

    let mut future_attempt_1 = future;
    future_attempt_1.attempts = 1;
    assert_eq!(
        future_attempt_1.calculate_delay(),
        Duration::from_millis(200)
    ); // 2^1 * 100

    let mut future_attempt_2 = future_attempt_1;
    future_attempt_2.attempts = 2;
    assert_eq!(
        future_attempt_2.calculate_delay(),
        Duration::from_millis(400)
    ); // 2^2 * 100
}

#[tokio::test]
async fn test_max_delay_cap() {
    let config = RetryConfig::new()
        .base_delay(Duration::from_millis(1000))
        .backoff_multiplier(10.0)
        .max_delay(Duration::from_millis(2000));

    let mut future = RetryFuture::new(Client::new().get("http://example.com"), config);

    future.attempts = 5; // This would normally result in 10^5 * 1000ms
    assert_eq!(future.calculate_delay(), Duration::from_millis(2000)); // Capped at max_delay
}

#[tokio::test]
async fn integration_test() {
    let response = Client::new()
        .get("https://mock.httpstatus.io/429")
        .or_retry_with_config(
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
