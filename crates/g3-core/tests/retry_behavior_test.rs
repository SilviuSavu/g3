//! Retry Behavior Integration Tests
//!
//! CHARACTERIZATION: These tests verify the observable behavior of retry
//! infrastructure through stable public interfaces.
//!
//! What these tests protect:
//! - RetryConfig construction and presets
//! - RetryResult state transitions
//! - retry_operation behavior with simulated errors
//!
//! What these tests intentionally do NOT assert:
//! - Internal timing details (only that delays occur)
//! - Specific backoff calculations (only that they increase)
//! - Agent internals (tested via execute_with_retry separately)

use g3_core::retry::{RetryConfig, RetryResult, retry_operation};
use g3_core::ContextWindow;
use g3_core::TaskResult;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

// =============================================================================
// Test: RetryConfig presets and customization
// =============================================================================

mod retry_config_presets {
    use super::*;

    /// Test default config values
    #[test]
    fn test_default_config() {
        let config = RetryConfig::default();
        
        assert_eq!(config.max_retries, 3);
        assert!(!config.is_autonomous);
        assert_eq!(config.role_name, "agent");
    }

    /// Test player preset
    #[test]
    fn test_player_preset() {
        let config = RetryConfig::player();
        
        assert_eq!(config.max_retries, 3);
        assert!(config.is_autonomous, "Player should be autonomous");
        assert_eq!(config.role_name, "player");
    }

    /// Test coach preset
    #[test]
    fn test_coach_preset() {
        let config = RetryConfig::coach();
        
        assert_eq!(config.max_retries, 3);
        assert!(config.is_autonomous, "Coach should be autonomous");
        assert_eq!(config.role_name, "coach");
    }

    /// Test planning preset with custom role
    #[test]
    fn test_planning_preset() {
        let config = RetryConfig::planning("reviewer");
        
        assert_eq!(config.max_retries, 3);
        assert!(config.is_autonomous, "Planning should be autonomous");
        assert_eq!(config.role_name, "reviewer");
    }

    /// Test custom max retries
    #[test]
    fn test_custom_max_retries() {
        let config = RetryConfig::player().with_max_retries(10);
        
        assert_eq!(config.max_retries, 10);
        // Other fields should be preserved
        assert!(config.is_autonomous);
        assert_eq!(config.role_name, "player");
    }

    /// Test chaining customizations
    #[test]
    fn test_chained_customization() {
        let config = RetryConfig::default()
            .with_max_retries(5);
        
        assert_eq!(config.max_retries, 5);
        assert!(!config.is_autonomous); // Default is not autonomous
    }
}

// =============================================================================
// Test: RetryResult state handling
// =============================================================================

mod retry_result_states {
    use super::*;

    /// Test success result
    #[test]
    fn test_success_is_success() {
        let ctx = ContextWindow::new(1000);
        let result = RetryResult::Success(TaskResult::new("done".to_string(), ctx));
        
        assert!(result.is_success());
    }

    /// Test max retries reached is not success
    #[test]
    fn test_max_retries_not_success() {
        let result = RetryResult::MaxRetriesReached("timeout".to_string());
        
        assert!(!result.is_success());
    }

    /// Test context length exceeded is not success
    #[test]
    fn test_context_exceeded_not_success() {
        let result = RetryResult::ContextLengthExceeded("too long".to_string());
        
        assert!(!result.is_success());
    }

    /// Test panic is not success
    #[test]
    fn test_panic_not_success() {
        let result = RetryResult::Panic(anyhow::anyhow!("panic occurred"));
        
        assert!(!result.is_success());
    }

    /// Test into_result extracts TaskResult on success
    #[test]
    fn test_into_result_success() {
        let ctx = ContextWindow::new(1000);
        let result = RetryResult::Success(TaskResult::new("done".to_string(), ctx));
        
        let task_result = result.into_result();
        assert!(task_result.is_some());
        assert_eq!(task_result.unwrap().response, "done");
    }

    /// Test into_result returns None on failure
    #[test]
    fn test_into_result_failure() {
        let result = RetryResult::MaxRetriesReached("error".to_string());
        
        let task_result = result.into_result();
        assert!(task_result.is_none());
    }
}

// =============================================================================
// Test: retry_operation behavior
// =============================================================================

mod retry_operation_behavior {
    use super::*;

    /// Test successful operation on first try
    #[tokio::test]
    async fn test_success_first_try() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();
        
        let result = retry_operation(
            "test_op",
            || {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, anyhow::Error>("success")
                }
            },
            3,
            false,
            |_msg| {},
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(call_count.load(Ordering::SeqCst), 1, "Should only call once on success");
    }

    /// Test non-recoverable error fails immediately
    #[tokio::test]
    async fn test_non_recoverable_fails_immediately() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();
        
        let result = retry_operation(
            "test_op",
            || {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Err::<String, _>(anyhow::anyhow!("Invalid API key"))
                }
            },
            3,
            false,
            |_msg| {},
        ).await;
        
        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1, "Non-recoverable should not retry");
    }

    /// Test recoverable error retries up to max
    #[tokio::test]
    async fn test_recoverable_retries_to_max() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();
        
        let result = retry_operation(
            "test_op",
            || {
                let count = call_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    // Rate limit is a recoverable error
                    Err::<String, _>(anyhow::anyhow!("Rate limit exceeded"))
                }
            },
            3, // max retries
            false,
            |_msg| {},
        ).await;
        
        assert!(result.is_err());
        // Should try initial + max_retries times
        assert_eq!(call_count.load(Ordering::SeqCst), 3, "Should retry up to max");
    }

    /// Test recoverable error succeeds on retry
    #[tokio::test]
    async fn test_recoverable_succeeds_on_retry() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();
        
        let result = retry_operation(
            "test_op",
            || {
                let count = call_count_clone.clone();
                async move {
                    let current = count.fetch_add(1, Ordering::SeqCst);
                    if current < 2 {
                        // Fail first two times with recoverable error
                        Err(anyhow::anyhow!("Server error 500"))
                    } else {
                        // Succeed on third try
                        Ok("success after retry")
                    }
                }
            },
            5, // max retries
            false,
            |_msg| {},
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success after retry");
        assert_eq!(call_count.load(Ordering::SeqCst), 3, "Should succeed on third try");
    }

    /// Test print function is called on retry
    #[tokio::test]
    async fn test_print_fn_called_on_retry() {
        let messages = Arc::new(std::sync::Mutex::new(Vec::new()));
        let messages_clone = messages.clone();
        
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();
        
        let _ = retry_operation(
            "test_op",
            || {
                let count = call_count_clone.clone();
                async move {
                    let current = count.fetch_add(1, Ordering::SeqCst);
                    if current < 1 {
                        Err(anyhow::anyhow!("Rate limit exceeded"))
                    } else {
                        Ok("success")
                    }
                }
            },
            3,
            false,
            |msg| {
                messages_clone.lock().unwrap().push(msg.to_string());
            },
        ).await;
        
        let msgs = messages.lock().unwrap();
        assert!(!msgs.is_empty(), "Should have printed retry messages");
        // Should mention the error type
        assert!(msgs.iter().any(|m| m.contains("RateLimit") || m.contains("rate")), 
            "Should mention rate limit in messages: {:?}", msgs);
    }
}
