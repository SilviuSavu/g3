//! TUI application state and main loop.

use crate::tui::events::has_minimum_size;
use crate::tui::tui_ui_writer::TuiEvent;
use crate::tui::ui::{self, Colors};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{prelude::CrosstermBackend, Terminal};
use std::io::Stdout;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// Role of a chat message.
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
    Error,
}

/// A single chat message for display.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
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
    agent_input_tx: mpsc::UnboundedSender<String>,
    tui_event_rx: mpsc::UnboundedReceiver<TuiEvent>,
}

impl App {
    pub fn new(
        agent_input_tx: mpsc::UnboundedSender<String>,
        tui_event_rx: mpsc::UnboundedReceiver<TuiEvent>,
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
            agent_input_tx,
            tui_event_rx,
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
                        if last.role == MessageRole::Assistant {
                            last.content.push_str(&text);
                        } else {
                            self.messages.push(ChatMessage {
                                role: MessageRole::Assistant,
                                content: text,
                            });
                        }
                    } else {
                        self.messages.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: text,
                        });
                    }
                    self.scroll_offset = 0;
                }
                TuiEvent::ResponseDone => {
                    self.current_tool = None;
                }
                TuiEvent::ToolStart(name) => {
                    self.current_tool = Some(name.clone());
                    self.messages.push(ChatMessage {
                        role: MessageRole::Tool,
                        content: format!("{} ...", name),
                    });
                }
                TuiEvent::ToolComplete(name, summary, ctx_pct) => {
                    self.current_tool = None;
                    self.context_percentage = ctx_pct;
                    if let Some(last) = self.messages.last_mut() {
                        if last.role == MessageRole::Tool && last.content.starts_with(&name) {
                            if summary.is_empty() {
                                last.content = format!("{} done", name);
                            } else {
                                last.content = format!("{}: {}", name, summary);
                            }
                        }
                    }
                }
                TuiEvent::ContextUpdate(pct) => {
                    self.context_percentage = pct;
                }
                TuiEvent::Status(msg) => {
                    if !msg.trim().is_empty() {
                        self.messages.push(ChatMessage {
                            role: MessageRole::Tool,
                            content: msg,
                        });
                    }
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
                        content: msg,
                    });
                }
            }
        }
    }

    fn handle_key_event(&mut self, key: event::KeyEvent) {
        if self.pending_prompt.is_some() {
            self.handle_prompt_key(key);
            return;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
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
            content: text.clone(),
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
