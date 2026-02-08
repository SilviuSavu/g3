//! TUI application state and main loop.

use crate::tui::events::{EventType, get_terminal_size, has_minimum_size};
use crate::tui::ui::{Colors, LayoutConfig, draw_main_content, draw_status_bar, draw_footer, split_with_header_footer};
use ratatui::{prelude::CrosstermBackend, Terminal};
use std::io::Stdout;

/// Current mode of the TUI.
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum AppMode {
    /// Interactive chat mode
    Interactive,
    /// View settings
    Settings,
    /// View help
    Help,
    /// View logs
    Logs,
}

/// Application state.
pub struct App {
    /// Terminal backend
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// Current application mode
    mode: AppMode,
    /// Current input text
    input: String,
    /// Recent messages/outputs
    messages: Vec<String>,
    /// Color palette
    colors: Colors,
    /// Layout configuration
    layout_config: LayoutConfig,
    /// Whether the app should continue running
    running: bool,
    /// Error message to display
    error: Option<String>,
    /// Success message to display
    success: Option<String>,
}

impl App {
    /// Create a new TUI application.
    pub fn new() -> anyhow::Result<Self> {
        // Check terminal size
        if !has_minimum_size(80, 24) {
            anyhow::bail!("Terminal too small. Minimum required: 80x24");
        }

        // Initialize terminal
        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;

        // Hide cursor
        terminal.hide_cursor()?;

        Ok(App {
            terminal,
            mode: AppMode::Interactive,
            input: String::new(),
            messages: Vec::new(),
            colors: Colors::default(),
            layout_config: LayoutConfig::default(),
            running: true,
            error: None,
            success: None,
        })
    }

    /// Run the main application loop.
    pub fn run(&mut self) -> anyhow::Result<()> {
        // Simple event loop - poll for events
        while self.running {
            // Draw the UI
            self.draw()?;
            
            // Small delay to avoid busy-waiting
            std::thread::sleep(std::time::Duration::from_millis(16));
            
            // Check for escape key to exit
            if crossterm::event::poll(std::time::Duration::from_millis(16))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    if key.code == crossterm::event::KeyCode::Esc {
                        self.running = false;
                    }
                }
            }
        }

        // Restore cursor on exit
        self.terminal.show_cursor()?;
        Ok(())
    }

    /// Draw the current UI state.
    fn draw(&mut self) -> anyhow::Result<()> {
        // Clone state before entering the closure to avoid borrow issues
        let mode = self.mode.clone();
        let messages = self.messages.clone();
        let error = self.error.clone();
        let success = self.success.clone();
        let colors = self.colors.clone();
        let layout_config = self.layout_config.clone();
        
        self.terminal.draw(|frame| {
            let size = frame.area();

            // Split the screen
            let (header, main, footer) = split_with_header_footer(size, 3, 2);

            // Draw header
            if layout_config.show_header {
                crate::tui::ui::render_header(frame, header, "g3 - AI Coding Assistant", &colors);
            }

            // Draw main content based on mode
            draw_main_content(
                frame,
                main,
                mode,
                &messages,
                &error,
                &success,
                &colors,
            );

            // Draw footer
            if layout_config.show_footer {
                draw_footer(frame, footer, mode);
            }

            // Draw status bar
            if layout_config.show_status_bar {
                draw_status_bar(frame, size, mode);
            }
        })?;

        Ok(())
    }

    /// Add a message to the output.
    pub fn add_message(&mut self, message: &str) {
        self.messages.push(message.to_string());
        // Keep only last 100 messages
        if self.messages.len() > 100 {
            self.messages.drain(0..(self.messages.len() - 100));
        }
    }

    /// Set an error message.
    pub fn set_error(&mut self, error: &str) {
        self.error = Some(error.to_string());
    }

    /// Set a success message.
    pub fn set_success(&mut self, success: &str) {
        self.success = Some(success.to_string());
    }

    /// Get the current input.
    pub fn get_input(&self) -> &str {
        &self.input
    }

    /// Clear the input.
    pub fn clear_input(&mut self) {
        self.input.clear();
    }

    /// Get the current mode.
    pub fn get_mode(&self) -> &AppMode {
        &self.mode
    }

    /// Set the current mode.
    pub fn set_mode(&mut self, mode: AppMode) {
        self.mode = mode;
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Ensure cursor is shown when app is dropped
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_creation() {
        // This test would require a proper terminal setup
        // For now, we just test that the struct has the right fields
        let _app = App::new();
        // Note: This will fail in tests without proper terminal setup
    }

    #[test]
    fn test_add_message() {
        let mut app = App::new().unwrap_or_else(|_| panic!("Terminal not available"));
        app.add_message("Test message");
        assert!(app.messages.len() > 0);
    }

    #[test]
    fn test_input_handling() {
        let mut app = App::new().unwrap_or_else(|_| panic!("Terminal not available"));
        app.input.push('H');
        assert!(app.get_input().contains('H'));
    }
}
