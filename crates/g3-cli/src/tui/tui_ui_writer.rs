//! TUI-specific UiWriter implementation.
//!
//! Bridges the async Agent engine to the synchronous ratatui event loop
//! by sending TuiEvent messages over an unbounded mpsc channel.

use g3_core::ui_writer::UiWriter;
use std::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

/// Events the agent sends to the TUI for rendering.
#[derive(Debug)]
pub enum TuiEvent {
    /// Streaming response chunk from the agent
    ResponseChunk(String),
    /// Agent finished its current response
    ResponseDone,
    /// A tool execution started
    ToolStart(String),
    /// A tool execution completed (tool_name, summary, context_percentage)
    ToolComplete(String, String, f32),
    /// Context window percentage update
    ContextUpdate(f32),
    /// Status message from agent
    Status(String),
    /// Agent is requesting a yes/no prompt from the user
    PromptYesNo(String, oneshot::Sender<bool>),
    /// Agent is requesting a choice prompt from the user
    PromptChoice(String, Vec<String>, oneshot::Sender<usize>),
    /// Agent encountered an error
    Error(String),
    /// Rich tool compact display (replaces ToolComplete for new rendering)
    ToolCompact {
        name: String,
        path: String,
        summary: String,
        tokens: u32,
        duration_secs: f64,
        context_pct: f32,
    },
    /// Start of verbose tool output block
    ToolVerboseStart {
        name: String,
        path: String,
    },
    /// A line of verbose tool output
    ToolVerboseLine(String),
    /// End of verbose tool output block
    ToolVerboseEnd {
        tokens: u32,
        duration_secs: f64,
        context_pct: f32,
    },
    /// Session info update (model, cost)
    SessionInfo {
        model: String,
        cost_dollars: f64,
    },
    /// Agent started thinking (waiting for response)
    ThinkingStart,
    /// Agent stopped thinking (response started)
    ThinkingStop,
}

/// UiWriter implementation that sends events to the TUI via a channel.
pub struct TuiUiWriter {
    tx: mpsc::UnboundedSender<TuiEvent>,
    /// Tracks the current tool name for compact display
    current_tool: Mutex<Option<String>>,
    /// Tracks the current tool path for compact display
    current_tool_path: Mutex<Option<String>>,
    /// Tracks whether agent is thinking (waiting for response)
    thinking: Mutex<bool>,
}

impl TuiUiWriter {
    pub fn new(tx: mpsc::UnboundedSender<TuiEvent>) -> Self {
        Self {
            tx,
            current_tool: Mutex::new(None),
            current_tool_path: Mutex::new(None),
            thinking: Mutex::new(false),
        }
    }

    fn send(&self, event: TuiEvent) {
        let _ = self.tx.send(event);
    }
}

impl UiWriter for TuiUiWriter {
    fn print(&self, _message: &str) {
        // Suppressed in TUI — these are CLI progress messages
    }

    fn println(&self, _message: &str) {
        // Suppressed in TUI
    }

    fn print_inline(&self, _message: &str) {
        // Suppressed in TUI
    }

    fn print_system_prompt(&self, _prompt: &str) {}

    fn print_context_status(&self, _message: &str) {
        // Context status shown in status bar, not chat
    }

    fn print_g3_progress(&self, _message: &str) {
        // Progress shown via ThinkingStart/tool status, not chat messages
    }

    fn print_g3_status(&self, _message: &str, _status: &str) {
        // Status shown in status bar
    }

    fn print_thin_result(&self, result: &g3_core::ThinResult) {
        if result.had_changes {
            self.send(TuiEvent::Status(format!(
                "Context thinned: {}% -> {}%",
                result.before_percentage, result.after_percentage
            )));
        }
    }

    fn print_tool_header(&self, tool_name: &str, tool_args: Option<&serde_json::Value>) {
        *self.current_tool.lock().unwrap() = Some(tool_name.to_string());
        // Extract path from tool args (common field names: "path", "file_path", "command")
        let path = tool_args.and_then(|args| {
            args.get("file_path")
                .or_else(|| args.get("path"))
                .or_else(|| args.get("command"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });
        *self.current_tool_path.lock().unwrap() = path;
        self.send(TuiEvent::ToolStart(tool_name.to_string()));
    }

    fn print_tool_arg(&self, _key: &str, _value: &str) {}

    fn print_tool_output_header(&self) {
        let name = self
            .current_tool
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_default();
        let path = self
            .current_tool_path
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_default();
        self.send(TuiEvent::ToolVerboseStart { name, path });
    }

    fn update_tool_output_line(&self, _line: &str) {}

    fn print_tool_output_line(&self, line: &str) {
        self.send(TuiEvent::ToolVerboseLine(line.to_string()));
    }

    fn print_tool_output_summary(&self, _hidden_count: usize) {}

    fn print_tool_compact(
        &self,
        tool_name: &str,
        summary: &str,
        duration_str: &str,
        tokens_delta: u32,
        context_percentage: f32,
    ) -> bool {
        let path = self
            .current_tool_path
            .lock()
            .unwrap()
            .take()
            .unwrap_or_default();
        // Parse duration from string like "1.2s" or "1m 23s"
        let duration_secs = parse_duration_str(duration_str);
        self.send(TuiEvent::ToolCompact {
            name: tool_name.to_string(),
            path,
            summary: summary.to_string(),
            tokens: tokens_delta,
            duration_secs,
            context_pct: context_percentage,
        });
        self.send(TuiEvent::ContextUpdate(context_percentage));
        true
    }

    fn print_todo_compact(&self, _content: Option<&str>, _is_write: bool) -> bool {
        false
    }

    fn print_plan_compact(
        &self,
        _plan_yaml: Option<&str>,
        _plan_file_path: Option<&str>,
        _is_write: bool,
    ) -> bool {
        false
    }

    fn print_tool_timing(
        &self,
        duration_str: &str,
        tokens_delta: u32,
        context_percentage: f32,
    ) {
        let duration_secs = parse_duration_str(duration_str);
        self.send(TuiEvent::ToolVerboseEnd {
            tokens: tokens_delta,
            duration_secs,
            context_pct: context_percentage,
        });
        self.send(TuiEvent::ContextUpdate(context_percentage));
    }

    fn print_agent_prompt(&self) {
        *self.thinking.lock().unwrap() = true;
        self.send(TuiEvent::ThinkingStart);
    }

    fn print_agent_response(&self, content: &str) {
        let mut thinking = self.thinking.lock().unwrap();
        if *thinking {
            self.send(TuiEvent::ThinkingStop);
            *thinking = false;
        }
        self.send(TuiEvent::ResponseChunk(content.to_string()));
    }

    fn notify_sse_received(&self) {}

    fn print_tool_streaming_hint(&self, _tool_name: &str) {
        // Tool start already sent by print_tool_header
    }

    fn print_tool_streaming_active(&self) {}

    fn flush(&self) {}

    fn prompt_user_yes_no(&self, message: &str) -> bool {
        let (tx, rx) = oneshot::channel();
        self.send(TuiEvent::PromptYesNo(message.to_string(), tx));
        rx.blocking_recv().unwrap_or(false)
    }

    fn prompt_user_choice(&self, message: &str, options: &[&str]) -> usize {
        let (tx, rx) = oneshot::channel();
        let options_owned: Vec<String> = options.iter().map(|s| s.to_string()).collect();
        self.send(TuiEvent::PromptChoice(
            message.to_string(),
            options_owned,
            tx,
        ));
        rx.blocking_recv().unwrap_or(0)
    }

    fn finish_streaming_markdown(&self) {
        self.send(TuiEvent::ResponseDone);
    }
}

/// Parse a duration string like "1.2s", "1m 23s", "1h 5m" into seconds
fn parse_duration_str(s: &str) -> f64 {
    let s = s.trim();
    // Try "Xh Ym"
    if let Some(h_pos) = s.find('h') {
        let hours: f64 = s[..h_pos].trim().parse().unwrap_or(0.0);
        let rest = &s[h_pos + 1..];
        let minutes: f64 = rest
            .trim()
            .trim_end_matches('m')
            .trim()
            .parse()
            .unwrap_or(0.0);
        return hours * 3600.0 + minutes * 60.0;
    }
    // Try "Xm Ys"
    if let Some(m_pos) = s.find('m') {
        let minutes: f64 = s[..m_pos].trim().parse().unwrap_or(0.0);
        let rest = &s[m_pos + 1..];
        let seconds: f64 = rest
            .trim()
            .trim_end_matches('s')
            .trim()
            .parse()
            .unwrap_or(0.0);
        return minutes * 60.0 + seconds;
    }
    // Try "Xs"
    s.trim_end_matches('s').trim().parse().unwrap_or(0.0)
}
