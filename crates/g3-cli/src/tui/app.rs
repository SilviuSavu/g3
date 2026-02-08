//! TUI application state and main loop.

use crate::tui::events::has_minimum_size;
use crate::tui::subagent_monitor::SubagentEntry;
use crate::tui::tui_ui_writer::TuiEvent;
use crate::tui::ui::{self, Colors};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{prelude::CrosstermBackend, Terminal};
use std::io::Stdout;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// Which pane has focus.
#[derive(Debug, Clone, PartialEq)]
pub enum Pane {
    Main,
    Subagent,
}

/// Role of a chat message.
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
    Error,
}

/// Structured content for a chat message.
#[derive(Debug, Clone)]
pub enum ChatContent {
    Text(String),
    ToolCompact {
        name: String,
        path: String,
        summary: String,
        tokens: u32,
        duration_secs: f64,
        context_pct: f32,
    },
    ToolVerbose {
        name: String,
        path: String,
        lines: Vec<String>,
        tokens: u32,
        duration_secs: f64,
        context_pct: f32,
    },
}

impl ChatContent {
    pub fn as_text(&self) -> &str {
        match self {
            ChatContent::Text(s) => s,
            _ => "",
        }
    }

    pub fn push_str(&mut self, text: &str) {
        if let ChatContent::Text(s) = self {
            s.push_str(text);
        }
    }

    pub fn is_text(&self) -> bool {
        matches!(self, ChatContent::Text(_))
    }
}

/// A single chat message for display.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: ChatContent,
}

/// A pending prompt from the agent that needs user input.
#[derive(Debug)]
pub enum PendingPrompt {
    YesNo {
        message: String,
        responder: oneshot::Sender<bool>,
    },
    Choice {
        message: String,
        options: Vec<String>,
        responder: oneshot::Sender<usize>,
    },
}

impl PendingPrompt {
    pub fn message(&self) -> &str {
        match self {
            PendingPrompt::YesNo { message, .. } => message,
            PendingPrompt::Choice { message, .. } => message,
        }
    }
}

/// Application state.
pub struct App {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    running: bool,
    pub input_buffer: String,
    pub cursor_position: usize,
    pub messages: Vec<ChatMessage>,
    pub context_percentage: f32,
    pub current_tool: Option<String>,
    pub pending_prompt: Option<PendingPrompt>,
    pub scroll_offset: u16,
    pub colors: Colors,
    pub active_pane: Pane,
    pub split_ratio: f32,
    pub subagent_entries: Vec<SubagentEntry>,
    pub subagent_scroll: usize,
    pub model_name: String,
    pub cost_dollars: f64,
    pub is_thinking: bool,
    agent_input_tx: mpsc::UnboundedSender<String>,
    tui_event_rx: mpsc::UnboundedReceiver<TuiEvent>,
    subagent_rx: mpsc::UnboundedReceiver<Vec<SubagentEntry>>,
    /// Accumulator for verbose tool output lines
    verbose_tool_lines: Vec<String>,
    verbose_tool_name: String,
    verbose_tool_path: String,
}

impl App {
    pub fn new(
        agent_input_tx: mpsc::UnboundedSender<String>,
        tui_event_rx: mpsc::UnboundedReceiver<TuiEvent>,
        subagent_rx: mpsc::UnboundedReceiver<Vec<SubagentEntry>>,
    ) -> anyhow::Result<Self> {
        if !has_minimum_size(80, 24) {
            anyhow::bail!("Terminal too small. Minimum required: 80x24");
        }

        let backend = CrosstermBackend::new(std::io::stdout());
        let terminal = Terminal::new(backend)?;

        Ok(App {
            terminal,
            running: true,
            input_buffer: String::new(),
            cursor_position: 0,
            messages: Vec::new(),
            context_percentage: 0.0,
            current_tool: None,
            pending_prompt: None,
            scroll_offset: 0,
            colors: Colors::default(),
            active_pane: Pane::Main,
            split_ratio: 0.7,
            subagent_entries: Vec::new(),
            subagent_scroll: 0,
            model_name: String::new(),
            cost_dollars: 0.0,
            is_thinking: false,
            agent_input_tx,
            tui_event_rx,
            subagent_rx,
            verbose_tool_lines: Vec::new(),
            verbose_tool_name: String::new(),
            verbose_tool_path: String::new(),
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture,
        )?;
        self.terminal.clear()?;

        let result = self.event_loop();

        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture,
        )?;
        self.terminal.show_cursor()?;

        result
    }

    fn event_loop(&mut self) -> anyhow::Result<()> {
        while self.running {
            self.draw()?;
            self.process_tui_events();
            self.process_subagent_events();

            if event::poll(Duration::from_millis(16))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key_event(key);
                }
            }
        }
        Ok(())
    }

    fn draw(&mut self) -> anyhow::Result<()> {
        let colors = self.colors.clone();
        let messages = self.messages.clone();
        let input_buffer = self.input_buffer.clone();
        let cursor_position = self.cursor_position;
        let context_percentage = self.context_percentage;
        let current_tool = self.current_tool.clone();
        let scroll_offset = self.scroll_offset;
        let active_pane = self.active_pane.clone();
        let split_ratio = self.split_ratio;
        let subagent_entries = self.subagent_entries.clone();
        let subagent_scroll = self.subagent_scroll;
        let model_name = self.model_name.clone();
        let cost_dollars = self.cost_dollars;
        let is_thinking = self.is_thinking;

        self.terminal.draw(|frame| {
            let app_view = ui::AppView {
                colors: &colors,
                messages: &messages,
                input_buffer: &input_buffer,
                cursor_position,
                context_percentage,
                current_tool: &current_tool,
                scroll_offset,
                pending_prompt: &None,
                active_pane: &active_pane,
                split_ratio,
                subagent_entries: &subagent_entries,
                subagent_scroll,
                model_name: &model_name,
                cost_dollars,
                is_thinking,
            };
            ui::render(frame, &app_view);
        })?;
        Ok(())
    }

    fn process_tui_events(&mut self) {
        while let Ok(evt) = self.tui_event_rx.try_recv() {
            match evt {
                TuiEvent::ResponseChunk(text) => {
                    if let Some(last) = self.messages.last_mut() {
                        if last.role == MessageRole::Assistant && last.content.is_text() {
                            last.content.push_str(&text);
                        } else {
                            self.messages.push(ChatMessage {
                                role: MessageRole::Assistant,
                                content: ChatContent::Text(text),
                            });
                        }
                    } else {
                        self.messages.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: ChatContent::Text(text),
                        });
                    }
                    self.scroll_offset = 0;
                }
                TuiEvent::ResponseDone => {
                    self.current_tool = None;
                    self.is_thinking = false;
                }
                TuiEvent::ToolStart(name) => {
                    self.current_tool = Some(name);
                }
                TuiEvent::ToolComplete(name, summary, ctx_pct) => {
                    self.current_tool = None;
                    self.context_percentage = ctx_pct;
                    // Legacy: simple text tool display
                    let text = if summary.is_empty() {
                        format!("{} done", name)
                    } else {
                        format!("{}: {}", name, summary)
                    };
                    self.messages.push(ChatMessage {
                        role: MessageRole::Tool,
                        content: ChatContent::Text(text),
                    });
                }
                TuiEvent::ToolCompact {
                    name,
                    path,
                    summary,
                    tokens,
                    duration_secs,
                    context_pct,
                } => {
                    self.current_tool = None;
                    self.context_percentage = context_pct;
                    self.messages.push(ChatMessage {
                        role: MessageRole::Tool,
                        content: ChatContent::ToolCompact {
                            name,
                            path,
                            summary,
                            tokens,
                            duration_secs,
                            context_pct,
                        },
                    });
                }
                TuiEvent::ToolVerboseStart { name, path } => {
                    self.verbose_tool_lines.clear();
                    self.verbose_tool_name = name;
                    self.verbose_tool_path = path;
                }
                TuiEvent::ToolVerboseLine(line) => {
                    self.verbose_tool_lines.push(line);
                }
                TuiEvent::ToolVerboseEnd {
                    tokens,
                    duration_secs,
                    context_pct,
                } => {
                    self.current_tool = None;
                    self.context_percentage = context_pct;
                    let lines = std::mem::take(&mut self.verbose_tool_lines);
                    let name = std::mem::take(&mut self.verbose_tool_name);
                    let path = std::mem::take(&mut self.verbose_tool_path);
                    self.messages.push(ChatMessage {
                        role: MessageRole::Tool,
                        content: ChatContent::ToolVerbose {
                            name,
                            path,
                            lines,
                            tokens,
                            duration_secs,
                            context_pct,
                        },
                    });
                }
                TuiEvent::ContextUpdate(pct) => {
                    self.context_percentage = pct;
                }
                TuiEvent::Status(msg) => {
                    if !msg.trim().is_empty() {
                        self.messages.push(ChatMessage {
                            role: MessageRole::Tool,
                            content: ChatContent::Text(msg),
                        });
                    }
                }
                TuiEvent::SessionInfo {
                    model,
                    cost_dollars,
                } => {
                    self.model_name = model;
                    self.cost_dollars = cost_dollars;
                }
                TuiEvent::ThinkingStart => {
                    self.is_thinking = true;
                }
                TuiEvent::ThinkingStop => {
                    self.is_thinking = false;
                }
                TuiEvent::PromptYesNo(message, responder) => {
                    self.pending_prompt = Some(PendingPrompt::YesNo { message, responder });
                }
                TuiEvent::PromptChoice(message, options, responder) => {
                    self.pending_prompt =
                        Some(PendingPrompt::Choice { message, options, responder });
                }
                TuiEvent::Error(msg) => {
                    self.messages.push(ChatMessage {
                        role: MessageRole::Error,
                        content: ChatContent::Text(msg),
                    });
                }
            }
        }
    }

    fn process_subagent_events(&mut self) {
        while let Ok(entries) = self.subagent_rx.try_recv() {
            self.subagent_entries = entries;
        }
    }

    fn handle_key_event(&mut self, key: event::KeyEvent) {
        if self.pending_prompt.is_some() {
            self.handle_prompt_key(key);
            return;
        }

        // Pane-specific keys when subagent panel is focused
        if self.active_pane == Pane::Subagent {
            match key.code {
                KeyCode::Char('j') => {
                    if self.subagent_scroll < self.subagent_entries.len().saturating_sub(1) {
                        self.subagent_scroll += 1;
                    }
                    return;
                }
                KeyCode::Char('k') => {
                    self.subagent_scroll = self.subagent_scroll.saturating_sub(1);
                    return;
                }
                KeyCode::Tab => {
                    self.active_pane = Pane::Main;
                    return;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.running = false;
                    return;
                }
                KeyCode::Esc => {
                    self.active_pane = Pane::Main;
                    return;
                }
                _ => return,
            }
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
            }
            KeyCode::Tab => {
                if !self.subagent_entries.is_empty() {
                    self.active_pane = Pane::Subagent;
                }
            }
            // Ctrl+H: decrease split ratio
            KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.split_ratio = (self.split_ratio - 0.05).max(0.5);
            }
            // Ctrl+L: increase split ratio
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.split_ratio = (self.split_ratio + 0.05).min(0.85);
            }
            KeyCode::Enter => {
                self.submit_input();
            }
            KeyCode::Char(c) => {
                self.input_buffer.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input_buffer.remove(self.cursor_position);
                }
            }
            KeyCode::Delete => {
                if self.cursor_position < self.input_buffer.len() {
                    self.input_buffer.remove(self.cursor_position);
                }
            }
            KeyCode::Left => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_position < self.input_buffer.len() {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_position = 0;
            }
            KeyCode::End => {
                self.cursor_position = self.input_buffer.len();
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_add(10);
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
            }
            KeyCode::Esc => {
                if self.input_buffer.is_empty() {
                    self.running = false;
                } else {
                    self.input_buffer.clear();
                    self.cursor_position = 0;
                }
            }
            _ => {}
        }
    }

    fn handle_prompt_key(&mut self, key: event::KeyEvent) {
        let prompt = match self.pending_prompt.take() {
            Some(p) => p,
            None => return,
        };

        match prompt {
            PendingPrompt::YesNo { message, responder } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let _ = responder.send(true);
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    let _ = responder.send(false);
                }
                _ => {
                    self.pending_prompt = Some(PendingPrompt::YesNo { message, responder });
                }
            },
            PendingPrompt::Choice {
                message,
                options,
                responder,
            } => {
                if let KeyCode::Char(c) = key.code {
                    if let Some(digit) = c.to_digit(10) {
                        let idx = digit as usize;
                        if idx >= 1 && idx <= options.len() {
                            let _ = responder.send(idx - 1);
                            return;
                        }
                    }
                }
                self.pending_prompt = Some(PendingPrompt::Choice {
                    message,
                    options,
                    responder,
                });
            }
        }
    }

    fn submit_input(&mut self) {
        let text = self.input_buffer.trim().to_string();
        if text.is_empty() {
            return;
        }

        self.messages.push(ChatMessage {
            role: MessageRole::User,
            content: ChatContent::Text(text.clone()),
        });

        let _ = self.agent_input_tx.send(text);

        self.input_buffer.clear();
        self.cursor_position = 0;
        self.scroll_offset = 0;
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
        );
        let _ = self.terminal.show_cursor();
    }
}
