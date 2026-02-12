//! TODO list tools: todo_read and todo_write.
//!
//! Session-scoped TODO tracking. The TODO is stored as markdown in
//! `.g3/sessions/<session_id>/todo.g3.md` and also cached in memory
//! via the `todo_content` Arc<RwLock<String>> on ToolContext.

use anyhow::Result;
use tracing::debug;

use crate::ui_writer::UiWriter;
use crate::ToolCall;

use super::executor::ToolContext;

/// Execute the `todo_read` tool.
/// Returns the current TODO content from the session file.
pub async fn execute_todo_read<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing todo_read tool call");

    let todo_path = ctx.get_todo_path();

    // Try reading from file first (source of truth)
    let content = if todo_path.exists() {
        std::fs::read_to_string(&todo_path).unwrap_or_default()
    } else {
        // Fall back to in-memory cache
        ctx.todo_content.read().await.clone()
    };

    let trimmed = content.trim();

    if trimmed.is_empty() {
        ctx.ui_writer.print_todo_compact(None, false);
        Ok("No TODO list exists yet. Use todo_write to create one.".to_string())
    } else {
        ctx.ui_writer.print_todo_compact(Some(trimmed), false);
        Ok(trimmed.to_string())
    }
}

/// Execute the `todo_write` tool.
/// Creates or replaces the TODO content for this session.
pub async fn execute_todo_write<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing todo_write tool call");

    let content = match tool_call.args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return Ok(
                "Missing required 'content' parameter. Provide the TODO list as markdown."
                    .to_string(),
            )
        }
    };

    let todo_path = ctx.get_todo_path();

    // Ensure parent directory exists
    if let Some(parent) = todo_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write to file
    std::fs::write(&todo_path, content)?;

    // Update in-memory cache
    {
        let mut guard = ctx.todo_content.write().await;
        *guard = content.to_string();
    }

    ctx.ui_writer.print_todo_compact(Some(content.trim()), true);

    let line_count = content.lines().count();
    Ok(format!(
        "TODO updated ({} lines). Path: {}",
        line_count,
        todo_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolCall;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tempfile::TempDir;

    /// Minimal UiWriter for testing
    struct TestUiWriter;
    impl UiWriter for TestUiWriter {
        fn print(&self, _: &str) {}
        fn println(&self, _: &str) {}
        fn print_inline(&self, _: &str) {}
        fn print_system_prompt(&self, _: &str) {}
        fn print_context_status(&self, _: &str) {}
        fn print_g3_progress(&self, _: &str) {}
        fn print_g3_status(&self, _: &str, _: &str) {}
        fn print_thin_result(&self, _: &crate::ThinResult) {}
        fn print_tool_header(&self, _: &str, _: Option<&serde_json::Value>) {}
        fn print_tool_arg(&self, _: &str, _: &str) {}
        fn print_tool_output_header(&self) {}
        fn update_tool_output_line(&self, _: &str) {}
        fn print_tool_output_line(&self, _: &str) {}
        fn print_tool_output_summary(&self, _: usize) {}
        fn print_tool_timing(&self, _: &str, _: u32, _: f32) {}
        fn print_agent_prompt(&self) {}
        fn print_agent_response(&self, _: &str) {}
        fn notify_sse_received(&self) {}
        fn print_tool_streaming_hint(&self, _: &str) {}
        fn print_tool_streaming_active(&self) {}
        fn flush(&self) {}
        fn prompt_user_yes_no(&self, _: &str) -> bool { true }
        fn prompt_user_choice(&self, _: &str, _: &[&str]) -> usize { 0 }
    }

    /// Create a ToolContext wired to a temp directory for testing.
    struct TestHarness {
        _tmp: TempDir,
        tmp_path_str: String,
        todo_content: Arc<RwLock<String>>,
        webdriver_session: Arc<RwLock<Option<Arc<tokio::sync::Mutex<crate::webdriver_session::WebDriverSession>>>>>,
        webdriver_process: Arc<RwLock<Option<tokio::process::Child>>>,
        background_process_manager: Arc<crate::background_process::BackgroundProcessManager>,
        pending_images: Vec<g3_providers::ImageContent>,
        config: g3_config::Config,
        pending_research_manager: crate::pending_research::PendingResearchManager,
    }

    impl TestHarness {
        fn new() -> Self {
            let tmp = TempDir::new().unwrap();
            let tmp_path_str = tmp.path().to_string_lossy().to_string();
            Self {
                _tmp: tmp,
                tmp_path_str,
                todo_content: Arc::new(RwLock::new(String::new())),
                webdriver_session: Arc::new(RwLock::new(None)),
                webdriver_process: Arc::new(RwLock::new(None)),
                background_process_manager: Arc::new(
                    crate::background_process::BackgroundProcessManager::new(
                        std::path::PathBuf::from("/tmp"),
                    ),
                ),
                pending_images: Vec::new(),
                config: g3_config::Config::default(),
                pending_research_manager: crate::pending_research::PendingResearchManager::new(),
            }
        }

        fn ctx(&mut self) -> ToolContext<'_, TestUiWriter> {
            ToolContext {
                config: &self.config,
                ui_writer: &TestUiWriter,
                session_id: None,
                working_dir: Some(&self.tmp_path_str),
                computer_controller: None,
                webdriver_session: &self.webdriver_session,
                webdriver_process: &self.webdriver_process,
                background_process_manager: &self.background_process_manager,
                todo_content: &self.todo_content,
                pending_images: &mut self.pending_images,
                is_autonomous: false,
                requirements_sha: None,
                context_total_tokens: 0,
                context_used_tokens: 0,
                pending_research_manager: &self.pending_research_manager,
                zai_tools_client: None,
                mcp_clients: None,
                index_client: None,
                lsp_manager: None,
                active_persona: None,
            }
        }
    }

    #[tokio::test]
    async fn test_todo_read_empty() {
        let mut harness = TestHarness::new();
        let tool_call = ToolCall {
            tool: "todo_read".to_string(),
            args: json!({}),
        };
        let mut ctx = harness.ctx();
        let result = execute_todo_read(&tool_call, &mut ctx).await.unwrap();
        assert!(result.contains("No TODO"));
    }

    #[tokio::test]
    async fn test_todo_write_and_read() {
        let mut harness = TestHarness::new();
        let content = "- [ ] Task 1\n- [ ] Task 2\n- [x] Task 3";

        // Write
        let write_call = ToolCall {
            tool: "todo_write".to_string(),
            args: json!({"content": content}),
        };
        let mut ctx = harness.ctx();
        let result = execute_todo_write(&write_call, &mut ctx).await.unwrap();
        assert!(result.contains("TODO updated"));
        assert!(result.contains("3 lines"));

        // Verify in-memory cache was updated
        let cached = harness.todo_content.read().await.clone();
        assert_eq!(cached, content);
    }

    #[tokio::test]
    async fn test_todo_write_missing_content() {
        let mut harness = TestHarness::new();
        let tool_call = ToolCall {
            tool: "todo_write".to_string(),
            args: json!({}),
        };
        let mut ctx = harness.ctx();
        let result = execute_todo_write(&tool_call, &mut ctx).await.unwrap();
        assert!(result.contains("Missing required"));
    }
}
