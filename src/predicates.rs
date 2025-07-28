use reqwest::{Error as ReqwestError, Response, StatusCode};

/// Retry only on network errors (no server errors)
pub fn network_errors_only(error: &ReqwestError) -> bool {
    error.is_timeout() || error.is_connect()
}

/// Retry on all errors except client errors (4xx)
pub fn except_client_errors(error: &ReqwestError) -> bool {
    if let Some(status) = error.status() {
        !status.is_client_error()
    } else {
        true // Network errors, timeouts, etc.
    }
}

/// Custom response predicate for specific status codes
pub fn retry_on_status(codes: &'static [StatusCode]) -> impl Fn(&Response) -> bool {
    move |response: &Response| codes.contains(&response.status())
}

/// Retry on server errors and rate limiting
pub fn server_errors_and_rate_limit(response: &Response) -> bool {
    let status = response.status();
    status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS
}
