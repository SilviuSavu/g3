//! Channel-based UiWriter for DyTopo worker agents.

use g3_core::ui_writer::UiWriter;
use std::sync::Mutex;
use tokio::sync::mpsc;

pub struct ChannelUiWriter {
    sender: mpsc::UnboundedSender<String>,
    buffer: Mutex<String>,
}

impl ChannelUiWriter {
    pub fn new(sender: mpsc::UnboundedSender<String>) -> Self {
        Self { sender, buffer: Mutex::new(String::new()) }
    }
    pub fn get_buffer(&self) -> String {
        self.buffer.lock().unwrap().clone()
    }
    pub fn take_buffer(&self) -> String {
        let mut buf = self.buffer.lock().unwrap();
        std::mem::take(&mut *buf)
    }
}

impl UiWriter for ChannelUiWriter {
    fn print(&self, _message: &str) {}
    fn println(&self, _message: &str) {}
    fn print_inline(&self, _message: &str) {}
    fn print_system_prompt(&self, _prompt: &str) {}
    fn print_context_status(&self, _message: &str) {}
    fn print_g3_progress(&self, _message: &str) {}
    fn print_g3_status(&self, _message: &str, _status: &str) {}
    fn print_thin_result(&self, _result: &g3_core::ThinResult) {}
    fn print_tool_header(&self, _tool_name: &str, _tool_args: Option<&serde_json::Value>) {}
    fn print_tool_arg(&self, _key: &str, _value: &str) {}
    fn print_tool_output_header(&self) {}
    fn update_tool_output_line(&self, _line: &str) {}
    fn print_tool_output_line(&self, _line: &str) {}
    fn print_tool_output_summary(&self, _hidden_count: usize) {}
    fn print_tool_timing(&self, _duration_str: &str, _tokens_delta: u32, _context_percentage: f32) {}
    fn print_agent_prompt(&self) {}
    fn notify_sse_received(&self) {}
    fn print_tool_streaming_hint(&self, _tool_name: &str) {}
    fn print_tool_streaming_active(&self) {}
    fn flush(&self) {}

    fn print_agent_response(&self, content: &str) {
        let mut buf = self.buffer.lock().unwrap();
        buf.push_str(content);
        let _ = self.sender.send(content.to_string());
    }
    fn wants_full_output(&self) -> bool { true }
    fn prompt_user_yes_no(&self, _question: &str) -> bool { true }
    fn prompt_user_choice(&self, _question: &str, _choices: &[&str]) -> usize { 0 }
}
