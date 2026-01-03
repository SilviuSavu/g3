//! Tool execution module for G3 agent.
//!
//! This module contains all tool implementations that the agent can execute.
//! Tools are organized by category:
//! - `shell` - Shell command execution and background processes
//! - `file_ops` - File reading, writing, and editing
//! - `todo` - TODO list management
//! - `webdriver` - Browser automation via WebDriver
//! - `macax` - macOS Accessibility API tools
//! - `vision` - Vision-based text finding and clicking
//! - `misc` - Other tools (screenshots, code search, etc.)

pub mod executor;
pub mod file_ops;
pub mod macax;
pub mod misc;
pub mod shell;
pub mod todo;
pub mod vision;
pub mod webdriver;

pub use executor::ToolExecutor;
