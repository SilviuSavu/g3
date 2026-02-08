//! Tool execution module for G3 agent.
//!
//! This module contains all tool implementations that the agent can execute.
//! Tools are organized by category:
//! - `shell` - Shell command execution and background processes
//! - `file_ops` - File reading, writing, and editing
//! - `plan` - Plan Mode for structured task planning
//! - `webdriver` - Browser automation via WebDriver
//! - `misc` - Other tools (screenshots, code search, etc.)
//! - `research` - Web research via scout agent
//! - `memory` - Workspace memory (remember)
//! - `acd` - Aggressive Context Dehydration (rehydrate)
//! - `beads` - Beads distributed issue tracking and molecule workflows
//! - `mcp_tools` - MCP (Model Context Protocol) tools for Z.ai servers
//! - `zai_tools` - Z.ai standalone tools (web search, web reader, OCR)
//! - `index` - Codebase indexing and semantic search

pub mod executor;
pub mod acd;
pub mod beads;
pub mod file_ops;
pub mod index;
pub mod intelligence;
pub mod lsp;
pub mod mcp_tools;
pub mod memory;
pub mod misc;
pub mod plan;
pub mod research;
pub mod shell;
pub mod todo;
pub mod webdriver;
pub mod zai_tools;

pub use executor::ToolExecutor;
pub use intelligence::execute_code_intelligence;
pub use lsp::LspManager;
pub use mcp_tools::McpClients;
