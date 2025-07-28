// Custom backoff functions for common use cases
use std::time::Duration;

/// Linear backoff: delay = base_delay * attempt
pub fn linear(
    attempt: usize,
    base_delay: Duration,
    _multiplier: f64,
    max_delay: Duration,
) -> Duration {
    let delay_ms =
        (base_delay.as_millis() as u64 * attempt as u64).min(max_delay.as_millis() as u64);
    Duration::from_millis(delay_ms)
}

/// Fixed delay backoff: always use base_delay
pub fn fixed(
    attempt: usize,
    base_delay: Duration,
    _multiplier: f64,
    _max_delay: Duration,
) -> Duration {
    // Allow first few attempts to be immediate, then use fixed delay
    if attempt == 0 {
        Duration::from_millis(0)
    } else {
        base_delay
    }
}

/// Exponential with jitter to avoid thundering herd
pub fn exponential_jitter(
    attempt: usize,
    base_delay: Duration,
    multiplier: f64,
    max_delay: Duration,
) -> Duration {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Create pseudo-random jitter based on attempt number
    let mut hasher = DefaultHasher::new();
    attempt.hash(&mut hasher);
    let jitter_factor = (hasher.finish() % 1000) as f64 / 1000.0; // 0.0 to 1.0

    let exponential_delay = base_delay.as_millis() as f64 * multiplier.powi(attempt as i32);
    let jittered_delay = exponential_delay * (0.5 + jitter_factor * 0.5); // 50% to 100% of calculated delay
    let delay_ms = jittered_delay.min(max_delay.as_millis() as f64) as u64;
    Duration::from_millis(delay_ms)
}

/// Fibonacci backoff: delay follows fibonacci sequence
pub fn fibonacci(
    attempt: usize,
    base_delay: Duration,
    _multiplier: f64,
    max_delay: Duration,
) -> Duration {
    fn fib(n: usize) -> u64 {
        match n {
            0 => 0,
            1 => 1,
            _ => {
                let mut a = 0u64;
                let mut b = 1u64;
                for _ in 2..=n {
                    let temp = a.saturating_add(b);
                    a = b;
                    b = temp;
                }
                b
            }
        }
    }

    let fib_multiplier = fib(attempt + 1).max(1); // Start from fib(1) = 1
    let delay_ms =
        (base_delay.as_millis() as u64 * fib_multiplier).min(max_delay.as_millis() as u64);
    Duration::from_millis(delay_ms)
}
