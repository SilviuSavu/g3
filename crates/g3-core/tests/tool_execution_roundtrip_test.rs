//! Tool Execution Round-Trip Integration Tests
//!
//! CHARACTERIZATION: These tests verify that tools execute correctly through
//! the Agent interface, testing the full round-trip from tool call to result.
//!
//! What these tests protect:
//! - File operations (read, write, str_replace) work end-to-end
//! - Shell command execution produces expected output
//! - TODO operations persist correctly
//! - Error handling for invalid inputs
//!
//! What these tests intentionally do NOT assert:
//! - Internal implementation details of tools
//! - Specific formatting of success messages (only key content)
//! - UI writer behavior (uses NullUiWriter)

use g3_core::ui_writer::NullUiWriter;
use g3_core::{Agent, ToolCall};
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

// =============================================================================
// Test Helpers
// =============================================================================

/// Create a test agent in a temporary directory
async fn create_test_agent(temp_dir: &TempDir) -> Agent<NullUiWriter> {
    std::env::set_current_dir(temp_dir.path()).unwrap();
    let config = g3_config::Config::default();
    let ui_writer = NullUiWriter;
    Agent::new(config, ui_writer).await.unwrap()
}

/// Create a ToolCall with the given tool name and arguments
fn make_tool_call(tool: &str, args: serde_json::Value) -> ToolCall {
    ToolCall {
        tool: tool.to_string(),
        args,
    }
}

// =============================================================================
// Test: read_file tool execution
// =============================================================================

mod read_file_execution {
    use super::*;

    /// Test reading an existing file
    #[tokio::test]
    #[serial]
    async fn test_read_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "Hello, World!\nLine 2\nLine 3").unwrap();
        
        let mut agent = create_test_agent(&temp_dir).await;
        
        let tool_call = make_tool_call(
            "read_file",
            serde_json::json!({ "file_path": test_file.to_string_lossy() }),
        );
        
        let result = agent.execute_tool(&tool_call).await.unwrap();
        
        assert!(result.contains("Hello, World!"), "Should contain file content: {}", result);
        assert!(result.contains("Line 2"), "Should contain all lines: {}", result);
    }

    /// Test reading a non-existent file returns error
    #[tokio::test]
    #[serial]
    async fn test_read_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let mut agent = create_test_agent(&temp_dir).await;
        
        let tool_call = make_tool_call(
            "read_file",
            serde_json::json!({ "file_path": "/nonexistent/path/file.txt" }),
        );
        
        let result = agent.execute_tool(&tool_call).await;
        
        // Should return an error or error message
        assert!(
            result.is_err() || result.as_ref().unwrap().contains("error") || result.as_ref().unwrap().contains("not found") || result.as_ref().unwrap().contains("No such file"),
            "Should indicate file not found: {:?}", result
        );
    }

    /// Test reading with character range
    #[tokio::test]
    #[serial]
    async fn test_read_file_with_range() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "0123456789ABCDEF").unwrap();
        
        let mut agent = create_test_agent(&temp_dir).await;
        
        let tool_call = make_tool_call(
            "read_file",
            serde_json::json!({
                "file_path": test_file.to_string_lossy(),
                "start": 5,
                "end": 10
            }),
        );
        
        let result = agent.execute_tool(&tool_call).await.unwrap();
        
        // Should contain the substring from position 5 to 10
        assert!(result.contains("56789"), "Should contain range content: {}", result);
    }
}

// =============================================================================
// Test: write_file tool execution
// =============================================================================

mod write_file_execution {
    use super::*;

    /// Test writing a new file
    #[tokio::test]
    #[serial]
    async fn test_write_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let new_file = temp_dir.path().join("new_file.txt");
        
        assert!(!new_file.exists(), "File should not exist initially");
        
        let mut agent = create_test_agent(&temp_dir).await;
        
        let tool_call = make_tool_call(
            "write_file",
            serde_json::json!({
                "file_path": new_file.to_string_lossy(),
                "content": "New content here"
            }),
        );
        
        let result = agent.execute_tool(&tool_call).await.unwrap();
        
        // Should report success
        assert!(result.contains("✅") || result.to_lowercase().contains("success") || result.to_lowercase().contains("wrote"),
            "Should report success: {}", result);
        
        // File should now exist with correct content
        assert!(new_file.exists(), "File should exist after write");
        let content = fs::read_to_string(&new_file).unwrap();
        assert_eq!(content, "New content here");
    }

    /// Test overwriting an existing file
    #[tokio::test]
    #[serial]
    async fn test_overwrite_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("existing.txt");
        fs::write(&test_file, "Original content").unwrap();
        
        let mut agent = create_test_agent(&temp_dir).await;
        
        let tool_call = make_tool_call(
            "write_file",
            serde_json::json!({
                "file_path": test_file.to_string_lossy(),
                "content": "Replaced content"
            }),
        );
        
        let result = agent.execute_tool(&tool_call).await.unwrap();
        
        assert!(result.contains("✅") || result.to_lowercase().contains("success") || result.to_lowercase().contains("wrote"),
            "Should report success: {}", result);
        
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "Replaced content");
    }

    /// Test writing creates parent directories
    #[tokio::test]
    #[serial]
    async fn test_write_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let nested_file = temp_dir.path().join("a/b/c/nested.txt");
        
        let mut agent = create_test_agent(&temp_dir).await;
        
        let tool_call = make_tool_call(
            "write_file",
            serde_json::json!({
                "file_path": nested_file.to_string_lossy(),
                "content": "Nested content"
            }),
        );
        
        let result = agent.execute_tool(&tool_call).await.unwrap();
        
        assert!(result.contains("✅") || result.to_lowercase().contains("success") || result.to_lowercase().contains("wrote"),
            "Should report success: {}", result);
        
        assert!(nested_file.exists(), "Nested file should exist");
        let content = fs::read_to_string(&nested_file).unwrap();
        assert_eq!(content, "Nested content");
    }
}

// =============================================================================
// Test: shell tool execution
// =============================================================================

mod shell_execution {
    use super::*;

    /// Test simple echo command
    #[tokio::test]
    #[serial]
    async fn test_shell_echo() {
        let temp_dir = TempDir::new().unwrap();
        let mut agent = create_test_agent(&temp_dir).await;
        
        let tool_call = make_tool_call(
            "shell",
            serde_json::json!({ "command": "echo 'hello world'" }),
        );
        
        let result = agent.execute_tool(&tool_call).await.unwrap();
        
        assert!(result.contains("hello world"), "Should contain echo output: {}", result);
    }

    /// Test command that produces multi-line output
    #[tokio::test]
    #[serial]
    async fn test_shell_multiline_output() {
        let temp_dir = TempDir::new().unwrap();
        let mut agent = create_test_agent(&temp_dir).await;
        
        let tool_call = make_tool_call(
            "shell",
            serde_json::json!({ "command": "echo 'line1'; echo 'line2'; echo 'line3'" }),
        );
        
        let result = agent.execute_tool(&tool_call).await.unwrap();
        
        assert!(result.contains("line1"), "Should contain line1: {}", result);
        assert!(result.contains("line2"), "Should contain line2: {}", result);
        assert!(result.contains("line3"), "Should contain line3: {}", result);
    }

    /// Test command that fails
    #[tokio::test]
    #[serial]
    async fn test_shell_failing_command() {
        let temp_dir = TempDir::new().unwrap();
        let mut agent = create_test_agent(&temp_dir).await;
        
        let tool_call = make_tool_call(
            "shell",
            serde_json::json!({ "command": "exit 1" }),
        );
        
        let result = agent.execute_tool(&tool_call).await;
        
        // Should indicate failure (either error or non-zero exit)
        assert!(
            result.is_err() || result.as_ref().unwrap().contains("exit") || result.as_ref().unwrap().contains("failed") || result.as_ref().unwrap().contains("error"),
            "Should indicate command failure: {:?}", result
        );
    }

    /// Test command with working directory context
    #[tokio::test]
    #[serial]
    async fn test_shell_pwd() {
        let temp_dir = TempDir::new().unwrap();
        let mut agent = create_test_agent(&temp_dir).await;
        
        let tool_call = make_tool_call(
            "shell",
            serde_json::json!({ "command": "pwd" }),
        );
        
        let result = agent.execute_tool(&tool_call).await.unwrap();
        
        // Should show the temp directory path
        let temp_path = temp_dir.path().to_string_lossy();
        assert!(result.contains(&*temp_path) || result.contains("private"), 
            "Should show current directory: {} (expected to contain {})", result, temp_path);
    }
}

// =============================================================================
// Test: str_replace tool execution
// =============================================================================

mod str_replace_execution {
    use super::*;

    /// Test applying a simple diff
    #[tokio::test]
    #[serial]
    async fn test_str_replace_simple() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "line 1\nold line\nline 3\n").unwrap();
        
        let mut agent = create_test_agent(&temp_dir).await;
        
        let diff = "@@ -1,3 +1,3 @@\n line 1\n-old line\n+new line\n line 3\n";
        
        let tool_call = make_tool_call(
            "str_replace",
            serde_json::json!({
                "file_path": test_file.to_string_lossy(),
                "diff": diff
            }),
        );
        
        let result = agent.execute_tool(&tool_call).await.unwrap();
        
        assert!(result.contains("✅") || result.to_lowercase().contains("applied") || result.to_lowercase().contains("success"),
            "Should report success: {}", result);
        
        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("new line"), "Should contain new content: {}", content);
        assert!(!content.contains("old line"), "Should not contain old content: {}", content);
    }

    /// Test diff that adds lines
    #[tokio::test]
    #[serial]
    async fn test_str_replace_add_lines() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "line 1\nline 3\n").unwrap();
        
        let mut agent = create_test_agent(&temp_dir).await;
        
        let diff = "@@ -1,2 +1,3 @@\n line 1\n+line 2\n line 3\n";
        
        let tool_call = make_tool_call(
            "str_replace",
            serde_json::json!({
                "file_path": test_file.to_string_lossy(),
                "diff": diff
            }),
        );
        
        let result = agent.execute_tool(&tool_call).await.unwrap();
        
        assert!(result.contains("✅") || result.to_lowercase().contains("applied"),
            "Should report success: {}", result);
        
        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("line 2"), "Should contain added line: {}", content);
    }

    /// Test diff with pattern not found
    #[tokio::test]
    #[serial]
    async fn test_str_replace_pattern_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "actual content\n").unwrap();
        
        let mut agent = create_test_agent(&temp_dir).await;
        
        let diff = "@@ -1,1 +1,1 @@\n-nonexistent pattern\n+replacement\n";
        
        let tool_call = make_tool_call(
            "str_replace",
            serde_json::json!({
                "file_path": test_file.to_string_lossy(),
                "diff": diff
            }),
        );
        
        let result = agent.execute_tool(&tool_call).await;
        
        // Should indicate pattern not found
        assert!(
            result.is_err() || result.as_ref().unwrap().to_lowercase().contains("not found") || result.as_ref().unwrap().to_lowercase().contains("pattern") || result.as_ref().unwrap().to_lowercase().contains("error"),
            "Should indicate pattern not found: {:?}", result
        );
    }
}

// =============================================================================
// Test: TODO tool execution
// =============================================================================

mod todo_execution {
    use super::*;

    /// Test writing and reading TODO
    #[tokio::test]
    #[serial]
    async fn test_todo_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let mut agent = create_test_agent(&temp_dir).await;
        
        // Write TODO
        let write_call = make_tool_call(
            "todo_write",
            serde_json::json!({
                "content": "- [ ] Task 1\n- [x] Task 2\n- [ ] Task 3"
            }),
        );
        
        let write_result = agent.execute_tool(&write_call).await.unwrap();
        assert!(write_result.contains("✅") || write_result.to_lowercase().contains("success"),
            "Write should succeed: {}", write_result);
        
        // Read TODO
        let read_call = make_tool_call("todo_read", serde_json::json!({}));
        let read_result = agent.execute_tool(&read_call).await.unwrap();
        
        assert!(read_result.contains("Task 1"), "Should contain Task 1: {}", read_result);
        assert!(read_result.contains("Task 2"), "Should contain Task 2: {}", read_result);
        assert!(read_result.contains("Task 3"), "Should contain Task 3: {}", read_result);
    }

    /// Test reading empty TODO
    #[tokio::test]
    #[serial]
    async fn test_todo_read_empty() {
        let temp_dir = TempDir::new().unwrap();
        let mut agent = create_test_agent(&temp_dir).await;
        
        let read_call = make_tool_call("todo_read", serde_json::json!({}));
        let result = agent.execute_tool(&read_call).await.unwrap();
        
        assert!(result.to_lowercase().contains("empty") || result.contains("no todo"),
            "Should indicate empty: {}", result);
    }

    /// Test TODO persists to file
    #[tokio::test]
    #[serial]
    async fn test_todo_persists_to_file() {
        let temp_dir = TempDir::new().unwrap();
        let todo_path = temp_dir.path().join("todo.g3.md");
        
        {
            let mut agent = create_test_agent(&temp_dir).await;
            
            let write_call = make_tool_call(
                "todo_write",
                serde_json::json!({
                    "content": "- [ ] Persistent task"
                }),
            );
            
            agent.execute_tool(&write_call).await.unwrap();
        }
        
        // File should exist after agent is dropped
        assert!(todo_path.exists(), "TODO file should persist");
        let content = fs::read_to_string(&todo_path).unwrap();
        assert!(content.contains("Persistent task"), "Content should persist: {}", content);
    }
}
