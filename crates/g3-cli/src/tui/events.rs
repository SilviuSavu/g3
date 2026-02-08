//! TUI event system using crossterm for keyboard input and terminal resize handling.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Represents different types of events that can be received in the TUI.
#[derive(Debug, Clone, PartialEq)]
pub enum EventType {
    /// A key was pressed
    Key(KeyEvent),
    /// The terminal was resized
    Resize(u16, u16),
    /// Mouse event (not yet implemented)
    Mouse(event::MouseEvent),
    /// Timeout occurred
    Timeout,
}

/// Event handler for TUI input.
pub struct EventHandler {
    /// Channel to send events to the UI
    tx: mpsc::Sender<EventType>,
    /// Channel to receive events from the input thread
    rx: mpsc::Receiver<EventType>,
    /// Whether events are enabled
    enabled: bool,
}

impl EventHandler {
    /// Create a new event handler.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        EventHandler {
            tx,
            rx,
            enabled: true,
        }
    }

    /// Start the event listener thread.
    pub fn start(&mut self) {
        if self.enabled {
            let tx = self.tx.clone();
            thread::spawn(move || {
                let poll_timeout = Duration::from_millis(100);
                loop {
                    if event::poll(poll_timeout).unwrap_or(false) {
                        match event::read().unwrap_or(Event::Key(KeyEvent::new(
                            KeyCode::Null,
                            KeyModifiers::empty(),
                        ))) {
                            Event::Key(key) => {
                                let _ = tx.send(EventType::Key(key));
                            }
                            Event::Resize(width, height) => {
                                let _ = tx.send(EventType::Resize(width, height));
                            }
                            Event::Mouse(mouse) => {
                                let _ = tx.send(EventType::Mouse(mouse));
                            }
                            _ => {}
                        }
                    } else {
                        let _ = tx.send(EventType::Timeout);
                    }
                }
            });
        }
    }

    /// Try to receive an event without blocking.
    pub fn try_receive(&self) -> Option<EventType> {
        self.rx.try_recv().ok()
    }

    /// Blocking receive of an event.
    pub fn receive(&self) -> EventType {
        self.rx.recv().unwrap_or(EventType::Timeout)
    }

    /// Enable or disable event handling.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if events are enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Event loop that polls for events and calls a callback.
pub struct EventLoop {
    /// Maximum iterations before returning control
    max_iterations: usize,
    /// Current iteration count
    iteration: usize,
    /// Start time for timeout
    start_time: Instant,
    /// Timeout duration
    timeout: Duration,
}

impl EventLoop {
    /// Create a new event loop.
    pub fn new() -> Self {
        EventLoop {
            max_iterations: 1000,
            iteration: 0,
            start_time: Instant::now(),
            timeout: Duration::from_secs(60),
        }
    }

    /// Set the maximum iterations before returning.
    pub fn with_max_iterations(mut self, iterations: usize) -> Self {
        self.max_iterations = iterations;
        self
    }

    /// Run the event loop with a callback.
    /// Returns false if the loop should exit, true to continue.
    pub fn run<F>(&mut self, event_handler: &EventHandler, mut callback: F) -> bool
    where
        F: FnMut(&EventType) -> bool,
    {
        self.iteration = 0;

        while self.iteration < self.max_iterations
            && self.start_time.elapsed() < self.timeout
        {
            if let Some(event) = event_handler.try_receive() {
                if !callback(&event) {
                    return false;
                }
            }
            self.iteration += 1;
        }

        true
    }
}

impl Default for EventLoop {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if the terminal has a minimum required size.
pub fn has_minimum_size(min_width: u16, min_height: u16) -> bool {
    if let Ok((width, height)) = crossterm::terminal::size() {
        width >= min_width && height >= min_height
    } else {
        false
    }
}

/// Get the current terminal size.
pub fn get_terminal_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((80, 24))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_handler_creation() {
        let handler = EventHandler::new();
        assert!(handler.is_enabled());
    }

    #[test]
    fn test_event_handler_enable_disable() {
        let mut handler = EventHandler::new();
        handler.set_enabled(false);
        assert!(!handler.is_enabled());
        handler.set_enabled(true);
        assert!(handler.is_enabled());
    }

    #[test]
    fn test_event_loop_creation() {
        let loop_ = EventLoop::new();
        assert_eq!(loop_.max_iterations, 1000);
    }
}
