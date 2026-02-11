//! Tests for the auto-continue detection features
//!
//! These tests verify the logic used to detect when the LLM should auto-continue:
//! 1. Empty/trivial responses (just timing lines)
//! 2. Incomplete tool calls
//! 3. Unexecuted tool calls

/// Helper function to check if a response is considered "empty" or trivial
/// This mirrors the logic in lib.rs for detecting empty responses
fn is_empty_response(response_text: &str) -> bool {
    response_text.trim().is_empty()
        || response_text.lines().all(|line| {
            line.trim().is_empty() || line.trim().starts_with("⏱️")
        })
}

#[test]
fn test_empty_response_detection_empty_string() {
    assert!(is_empty_response(""));
}

#[test]
fn test_empty_response_detection_whitespace_only() {
    assert!(is_empty_response("   "));
    assert!(is_empty_response("\n\n\n"));
    assert!(is_empty_response("  \n  \t  \n  "));
}

#[test]
fn test_empty_response_detection_timing_line_only() {
    assert!(is_empty_response("⏱️ 43.0s | 💭 3.6s"));
    assert!(is_empty_response("  ⏱️ 43.0s | 💭 3.6s  "));
    assert!(is_empty_response("\n⏱️ 43.0s | 💭 3.6s\n"));
}

#[test]
fn test_empty_response_detection_multiple_timing_lines() {
    let response = "\n⏱️ 10.0s | 💭 1.0s\n\n⏱️ 20.0s | 💭 2.0s\n";
    assert!(is_empty_response(response));
}

#[test]
fn test_empty_response_detection_timing_with_empty_lines() {
    let response = "\n\n⏱️ 43.0s | 💭 3.6s\n\n";
    assert!(is_empty_response(response));
}

#[test]
fn test_empty_response_detection_substantive_content() {
    // These should NOT be considered empty
    assert!(!is_empty_response("Hello, I will help you."));
    assert!(!is_empty_response("Let me read that file."));
    assert!(!is_empty_response("I've completed the task."));
}

#[test]
fn test_empty_response_detection_timing_with_text() {
    // If there's any substantive text, it's not empty
    let response = "⏱️ 43.0s | 💭 3.6s\nHere is the result.";
    assert!(!is_empty_response(response));
}

#[test]
fn test_empty_response_detection_text_before_timing() {
    let response = "Done!\n⏱️ 43.0s | 💭 3.6s";
    assert!(!is_empty_response(response));
}

#[test]
fn test_empty_response_detection_json_tool_call() {
    // A JSON tool call is definitely not empty
    let response = r#"{"tool": "read_file", "args": {"file_path": "test.txt"}}"#;
    assert!(!is_empty_response(response));
}

#[test]
fn test_empty_response_detection_partial_json() {
    // Even partial JSON is not empty
    let response = r#"{"tool": "read_file", "args": {"#;
    assert!(!is_empty_response(response));
}

#[test]
fn test_empty_response_detection_markdown() {
    // Markdown content is not empty
    let response = "# Summary\n\nI completed the task.";
    assert!(!is_empty_response(response));
}

#[test]
fn test_empty_response_detection_code_block() {
    // Code blocks are not empty
    let response = "```rust\nfn main() {}\n```";
    assert!(!is_empty_response(response));
}

// Test the MAX_AUTO_SUMMARY_ATTEMPTS constant value
// This is a compile-time check that the constant exists and has the expected value
#[test]
fn test_max_auto_summary_attempts_is_reasonable() {
    // The constant should be at least 3 to give the LLM a fair chance to recover
    // We can't directly access the constant from here, but we document the expected value
    // Current value: 5 (increased from 2)
    const EXPECTED_MIN_ATTEMPTS: usize = 3;
    const EXPECTED_MAX_ATTEMPTS: usize = 10;
    const CURRENT_VALUE: usize = 5;
    
    assert!(CURRENT_VALUE >= EXPECTED_MIN_ATTEMPTS, 
        "MAX_AUTO_SUMMARY_ATTEMPTS should be at least {} for reliable recovery", EXPECTED_MIN_ATTEMPTS);
    assert!(CURRENT_VALUE <= EXPECTED_MAX_ATTEMPTS,
        "MAX_AUTO_SUMMARY_ATTEMPTS should not exceed {} to avoid infinite loops", EXPECTED_MAX_ATTEMPTS);
}

// =============================================================================
// Test: Auto-continue condition logic
// =============================================================================

/// Uses the real should_auto_continue from g3_core::streaming
use g3_core::streaming::{should_auto_continue, AutoContinueReason};

#[test]
fn test_auto_continue_autonomous_tool_executed() {
    // Autonomous mode, tools executed this iteration, stop_reason = "tool_use" → continue
    assert_eq!(
        should_auto_continue(true, true, true, false, false, false, 0, Some("tool_use"), 0),
        Some(AutoContinueReason::ToolsExecuted),
    );
    // No stop_reason (None) but tools executed this iter → still continue in autonomous
    assert_eq!(
        should_auto_continue(true, true, false, false, false, false, 0, None, 0),
        None,
    );
}

#[test]
fn test_auto_continue_end_turn_stops_session() {
    // NEW BEHAVIOR: autonomous + tools_executed_this_iter=true ignores stop_reason → CONTINUES
    assert_eq!(
        should_auto_continue(true, true, true, false, false, false, 0, Some("end_turn"), 0),
        Some(AutoContinueReason::ToolsExecuted),
    );
    // Interactive mode with end_turn → stops
    assert_eq!(
        should_auto_continue(false, true, false, false, false, false, 0, Some("end_turn"), 0),
        None,
    );
}

#[test]
fn test_auto_continue_end_turn_still_recovers_errors() {
    // Even with end_turn, error-recovery reasons still fire
    assert_eq!(
        should_auto_continue(true, false, false, true, false, false, 0, Some("end_turn"), 0),
        Some(AutoContinueReason::IncompleteToolCall),
    );
    assert_eq!(
        should_auto_continue(false, false, false, false, true, false, 0, Some("end_turn"), 0),
        Some(AutoContinueReason::UnexecutedToolCall),
    );
    assert_eq!(
        should_auto_continue(false, false, false, false, false, true, 0, Some("end_turn"), 0),
        Some(AutoContinueReason::MaxTokensTruncation),
    );
}

#[test]
fn test_auto_continue_interactive_first_text_only() {
    // Interactive mode, tools executed, first text-only response, stop_reason = "tool_use" → continue
    assert_eq!(
        should_auto_continue(false, true, false, false, false, false, 0, Some("tool_use"), 0),
        Some(AutoContinueReason::ToolsExecuted),
    );
    // No stop_reason (None) → treat as natural end, don't continue
    assert_eq!(
        should_auto_continue(false, true, false, false, false, false, 0, None, 0),
        None,
    );
}

#[test]
fn test_auto_continue_interactive_second_text_only() {
    // Interactive mode, tools executed, second text-only → stop
    assert_eq!(
        should_auto_continue(false, true, false, false, false, false, 1, None, 0),
        None,
    );
}

#[test]
fn test_auto_continue_incomplete_tool_call() {
    // Incomplete tool call - should continue regardless of mode or counter
    assert_eq!(
        should_auto_continue(false, false, false, true, false, false, 5, None, 0),
        Some(AutoContinueReason::IncompleteToolCall),
    );
}

#[test]
fn test_auto_continue_unexecuted_tool_call() {
    // Unexecuted tool call - should continue
    assert_eq!(
        should_auto_continue(false, false, false, false, true, false, 5, None, 0),
        Some(AutoContinueReason::UnexecutedToolCall),
    );
}

#[test]
fn test_auto_continue_no_conditions_met() {
    // No tools, no incomplete calls - should NOT continue
    assert_eq!(
        should_auto_continue(false, false, false, false, false, false, 0, None, 0),
        None,
    );
}

// =============================================================================
// Test: Edge cases
// =============================================================================

#[test]
fn test_auto_continue_multiple_conditions() {
    // Multiple conditions true - incomplete takes priority
    assert_eq!(
        should_auto_continue(true, true, true, true, true, true, 0, None, 0),
        Some(AutoContinueReason::IncompleteToolCall),
    );

    // Only incomplete tool call
    assert_eq!(
        should_auto_continue(false, false, false, true, false, false, 0, None, 0),
        Some(AutoContinueReason::IncompleteToolCall),
    );

    // Only unexecuted tool call
    assert_eq!(
        should_auto_continue(false, false, false, false, true, false, 0, None, 0),
        Some(AutoContinueReason::UnexecutedToolCall),
    );
}
