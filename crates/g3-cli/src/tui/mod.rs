//! TUI (Text User Interface) module for g3 using ratatui.
//!
//! This module provides a terminal-based interface for the g3 AI coding assistant.
//! It uses the ratatui library for UI rendering and crossterm for input handling.
//!
//! # Features
//!
//! - Interactive chat interface
//! - Multiple modes (Interactive, Settings, Help, Logs)
//! - Responsive terminal layout
//! - Keyboard navigation
//! - Error and success message display
//!
//! # Usage
//!
//! ```ignore
//! use g3_cli::tui::run_tui;
//!
//! fn main() -> anyhow::Result<()> {
//!     run_tui()
//! }
//! ```

pub mod app;
pub mod events;
pub mod ui;

pub use app::App;
pub use events::EventHandler;
pub use ui::Colors;

/// Run the TUI application.
pub fn run_tui() -> anyhow::Result<()> {
    let mut app = App::new()?;
    app.run()?;
    Ok(())
}

/// Check if the TUI can run in the current environment.
pub fn can_run_tui() -> bool {
    // Check if terminal size is sufficient
    if let Ok((width, height)) = crossterm::terminal::size() {
        width >= 80 && height >= 24
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_run_tui() {
        // This will depend on the environment
        let _ = can_run_tui();
    }
}
