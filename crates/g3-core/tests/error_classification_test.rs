//! Error Classification Integration Tests
//!
//! CHARACTERIZATION: These tests verify the observable behavior of error
//! classification through stable public interfaces.
//!
//! What these tests protect:
//! - Error messages are correctly classified as recoverable/non-recoverable
//! - Specific error types (rate limit, timeout, server error) are detected
//! - Retry delay calculation produces reasonable values
//!
//! What these tests intentionally do NOT assert:
//! - Exact delay values (only ranges and relative behavior)
//! - Internal classification implementation details

use g3_core::error_handling::{
    classify_error, calculate_retry_delay, ErrorType, RecoverableError,
};

// =============================================================================
// Test: Error classification for recoverable errors
// =============================================================================

mod recoverable_error_classification {
    use super::*;

    /// Test rate limit errors are classified as recoverable
    #[test]
    fn test_rate_limit_detected() {
        let error = anyhow::anyhow!("Rate limit exceeded");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::RateLimit)),
            "Rate limit should be recoverable: {:?}", error_type
        );
    }

    /// Test 429 status code is classified as rate limit
    #[test]
    fn test_429_status_detected() {
        let error = anyhow::anyhow!("HTTP 429 Too Many Requests");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::RateLimit)),
            "429 should be rate limit: {:?}", error_type
        );
    }

    /// Test timeout errors are classified as recoverable
    #[test]
    fn test_timeout_detected() {
        let error = anyhow::anyhow!("Request timed out");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::Timeout)),
            "Timeout should be recoverable: {:?}", error_type
        );
    }

    /// Test server errors (5xx) are classified as recoverable
    #[test]
    fn test_server_error_500_detected() {
        let error = anyhow::anyhow!("Server error 500");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::ServerError)),
            "500 should be server error: {:?}", error_type
        );
    }

    /// Test 502 Bad Gateway is classified as server error
    #[test]
    fn test_server_error_502_detected() {
        let error = anyhow::anyhow!("502 Bad Gateway");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::ServerError)),
            "502 should be server error: {:?}", error_type
        );
    }

    /// Test 503 Service Unavailable is classified as server error
    #[test]
    fn test_server_error_503_detected() {
        let error = anyhow::anyhow!("503 Service Unavailable");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::ServerError)),
            "503 should be server error: {:?}", error_type
        );
    }

    /// Test network errors are classified as recoverable
    #[test]
    fn test_network_error_detected() {
        let error = anyhow::anyhow!("Connection refused");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::NetworkError)),
            "Connection refused should be network error: {:?}", error_type
        );
    }

    /// Test connection reset is classified as network error
    #[test]
    fn test_connection_reset_detected() {
        let error = anyhow::anyhow!("Connection reset by peer");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::NetworkError)),
            "Connection reset should be network error: {:?}", error_type
        );
    }

    /// Test "overloaded" is classified as busy
    #[test]
    fn test_model_busy_detected() {
        let error = anyhow::anyhow!("Server is overloaded");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::ModelBusy)),
            "Overloaded should be model busy: {:?}", error_type
        );
    }

    /// Test context length exceeded requires 400 status code
    /// CHARACTERIZATION: The error must contain "400" or "bad request" along with
    /// context length keywords to be classified as ContextLengthExceeded
    #[test]
    fn test_context_length_exceeded_detected() {
        let error = anyhow::anyhow!("400 Bad Request: context_length_exceeded: too many tokens");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::ContextLengthExceeded)),
            "Context length exceeded should be detected: {:?}", error_type
        );
    }

    /// Test token limit exceeded is classified correctly
    /// CHARACTERIZATION: Must contain "token" AND ("limit" OR "exceeded")
    #[test]
    fn test_token_limit_detected() {
        let error = anyhow::anyhow!("token limit exceeded");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::TokenLimit)),
            "Token limit should be detected: {:?}", error_type
        );
    }
}

// =============================================================================
// Test: Error classification for non-recoverable errors
// =============================================================================

mod non_recoverable_error_classification {
    use super::*;

    /// Test invalid API key is non-recoverable
    #[test]
    fn test_invalid_api_key_non_recoverable() {
        let error = anyhow::anyhow!("Invalid API key");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::NonRecoverable),
            "Invalid API key should be non-recoverable: {:?}", error_type
        );
    }

    /// Test authentication failure is non-recoverable
    #[test]
    fn test_auth_failure_non_recoverable() {
        let error = anyhow::anyhow!("Authentication failed");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::NonRecoverable),
            "Auth failure should be non-recoverable: {:?}", error_type
        );
    }

    /// Test generic errors are non-recoverable
    #[test]
    fn test_generic_error_non_recoverable() {
        let error = anyhow::anyhow!("Something went wrong");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::NonRecoverable),
            "Generic error should be non-recoverable: {:?}", error_type
        );
    }

    /// Test 401 Unauthorized is non-recoverable
    #[test]
    fn test_401_non_recoverable() {
        let error = anyhow::anyhow!("401 Unauthorized");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::NonRecoverable),
            "401 should be non-recoverable: {:?}", error_type
        );
    }

    /// Test 403 Forbidden is non-recoverable
    #[test]
    fn test_403_non_recoverable() {
        let error = anyhow::anyhow!("403 Forbidden");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::NonRecoverable),
            "403 should be non-recoverable: {:?}", error_type
        );
    }
}

// =============================================================================
// Test: Retry delay calculation
// =============================================================================

mod retry_delay_calculation {
    use super::*;
    use std::time::Duration;

    /// Test first retry has reasonable delay
    #[test]
    fn test_first_retry_delay() {
        let delay = calculate_retry_delay(1, false);
        
        // First retry should be around 1-2 seconds (with jitter)
        assert!(delay >= Duration::from_millis(500), "Delay should be at least 500ms: {:?}", delay);
        assert!(delay <= Duration::from_secs(5), "Delay should be at most 5s: {:?}", delay);
    }

    /// Test delays increase with retry count
    #[test]
    fn test_delays_increase() {
        let delay1 = calculate_retry_delay(1, false);
        let delay2 = calculate_retry_delay(2, false);
        let delay3 = calculate_retry_delay(3, false);
        
        // Later retries should generally have longer delays
        // (accounting for jitter, we check the trend)
        assert!(delay2 >= delay1 || delay3 >= delay2, 
            "Delays should generally increase: {:?} -> {:?} -> {:?}", delay1, delay2, delay3);
    }

    /// Test autonomous mode has different delays
    #[test]
    fn test_autonomous_mode_delays() {
        let default_delay = calculate_retry_delay(3, false);
        let autonomous_delay = calculate_retry_delay(3, true);
        
        // Autonomous mode should have longer delays (spread over 10 minutes)
        // But with jitter, we just check they're both reasonable
        assert!(default_delay <= Duration::from_secs(30), 
            "Default delay should be reasonable: {:?}", default_delay);
        assert!(autonomous_delay <= Duration::from_secs(180), 
            "Autonomous delay should be reasonable: {:?}", autonomous_delay);
    }

    /// Test delays are capped at maximum
    #[test]
    fn test_delay_cap() {
        // Even with high retry count, delay should be capped
        let delay = calculate_retry_delay(10, false);
        
        assert!(delay <= Duration::from_secs(15), 
            "Default mode delay should be capped: {:?}", delay);
    }

    /// Test autonomous mode delay cap
    /// CHARACTERIZATION: Autonomous mode uses longer delays spread over 10 minutes
    #[test]
    fn test_autonomous_delay_cap() {
        let delay = calculate_retry_delay(10, true);
        
        // Autonomous mode has longer delays (up to ~200s + jitter)
        assert!(delay <= Duration::from_secs(300), 
            "Autonomous delay should be capped: {:?}", delay);
    }
}

// =============================================================================
// Test: Edge cases and priority
// =============================================================================

mod edge_cases {
    use super::*;

    /// Test error with multiple keywords uses correct priority
    #[test]
    fn test_rate_limit_priority_over_timeout() {
        // Rate limit should take priority
        let error = anyhow::anyhow!("Rate limit exceeded after timeout");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::RateLimit)),
            "Rate limit should take priority: {:?}", error_type
        );
    }

    /// Test case insensitivity
    #[test]
    fn test_case_insensitive_detection() {
        let error = anyhow::anyhow!("RATE LIMIT EXCEEDED");
        let error_type = classify_error(&error);
        
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::RateLimit)),
            "Should detect uppercase: {:?}", error_type
        );
    }

    /// Test empty error message
    #[test]
    fn test_empty_error_message() {
        let error = anyhow::anyhow!("");
        let error_type = classify_error(&error);
        
        // Empty message should be non-recoverable
        assert!(
            matches!(error_type, ErrorType::NonRecoverable),
            "Empty error should be non-recoverable: {:?}", error_type
        );
    }

    /// Test connection timeout is network error (not timeout)
    /// Note: This documents the current behavior where "connection" keyword
    /// takes priority over "timeout"
    #[test]
    fn test_connection_timeout_classification() {
        let error = anyhow::anyhow!("Connection timeout");
        let error_type = classify_error(&error);
        
        // Per memory: "Connection timeout" classifies as NetworkError due to "connection" keyword priority
        assert!(
            matches!(error_type, ErrorType::Recoverable(RecoverableError::NetworkError)),
            "Connection timeout should be network error (per priority): {:?}", error_type
        );
    }
}
