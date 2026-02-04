//! LSP types, error definitions, and configuration structures.

use lsp_types::{Location, Position};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use url::Url;

/// Errors that can occur during LSP operations.
#[derive(Debug, Error)]
pub enum LspError {
    /// The LSP server is not running.
    #[error("LSP server is not running")]
    ServerNotRunning,

    /// Failed to start the LSP server.
    #[error("Failed to start LSP server: {0}")]
    ServerStartFailed(String),

    /// Request timed out waiting for response.
    #[error("Request timed out after {0}ms")]
    RequestTimeout(u64),

    /// The requested capability is not supported by the server.
    #[error("Capability not supported: {0}")]
    CapabilityNotSupported(String),

    /// JSON-RPC protocol error.
    #[error("JSON-RPC error: code={code}, message={message}")]
    JsonRpcError { code: i32, message: String },

    /// I/O error during communication.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Invalid file path.
    #[error("Invalid file path: {0}")]
    InvalidPath(String),

    /// Server executable not found.
    #[error("LSP server executable not found: {0}")]
    ExecutableNotFound(String),

    /// Initialization failed.
    #[error("LSP initialization failed: {0}")]
    InitializationFailed(String),

    /// Generic error wrapper.
    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for LspError {
    fn from(err: anyhow::Error) -> Self {
        LspError::Other(err.to_string())
    }
}

/// Configuration for an LSP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerConfig {
    /// Language identifier (e.g., "rust", "python", "typescript").
    pub language_id: String,

    /// Path to the LSP server executable.
    pub command: String,

    /// Arguments to pass to the server.
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables to set.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,

    /// File extensions this server handles.
    #[serde(default)]
    pub file_extensions: Vec<String>,

    /// Request timeout in milliseconds.
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,

    /// Working directory for the server process.
    pub working_directory: Option<PathBuf>,
}

fn default_timeout() -> u64 {
    30000 // 30 seconds
}

impl LspServerConfig {
    /// Create a new LSP server configuration.
    pub fn new(language_id: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            language_id: language_id.into(),
            command: command.into(),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            file_extensions: Vec::new(),
            timeout_ms: default_timeout(),
            working_directory: None,
        }
    }

    /// Add arguments to the server command.
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// Set file extensions this server handles.
    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.file_extensions = extensions;
        self
    }

    /// Set the working directory.
    pub fn with_working_directory(mut self, dir: PathBuf) -> Self {
        self.working_directory = Some(dir);
        self
    }

    /// Set the request timeout.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Create configuration for rust-analyzer.
    pub fn rust_analyzer() -> Self {
        Self::new("rust", "rust-analyzer")
            .with_extensions(vec!["rs".to_string()])
    }

    /// Create configuration for TypeScript language server.
    pub fn typescript() -> Self {
        Self::new("typescript", "typescript-language-server")
            .with_args(vec!["--stdio".to_string()])
            .with_extensions(vec!["ts".to_string(), "tsx".to_string(), "js".to_string(), "jsx".to_string()])
    }

    /// Create configuration for Python language server (pyright).
    pub fn python() -> Self {
        Self::new("python", "pyright-langserver")
            .with_args(vec!["--stdio".to_string()])
            .with_extensions(vec!["py".to_string(), "pyi".to_string()])
    }

    /// Create configuration for Go language server (gopls).
    pub fn go() -> Self {
        Self::new("go", "gopls")
            .with_extensions(vec!["go".to_string()])
    }

    /// Create configuration for clangd (C/C++).
    pub fn clangd() -> Self {
        Self::new("cpp", "clangd")
            .with_extensions(vec![
                "c".to_string(),
                "h".to_string(),
                "cpp".to_string(),
                "cc".to_string(),
                "cxx".to_string(),
                "hpp".to_string(),
                "hxx".to_string(),
            ])
    }

    /// Create configuration for Zig language server (zls).
    pub fn zls() -> Self {
        Self::new("zig", "zls")
            .with_extensions(vec!["zig".to_string()])
    }
}

/// A position in a document with 1-indexed line and character.
/// This is the user-facing representation; internally we convert to 0-indexed for LSP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LspPosition {
    /// 1-indexed line number.
    pub line: u32,
    /// 1-indexed character offset.
    pub character: u32,
}

impl LspPosition {
    /// Create a new position (1-indexed).
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }

    /// Convert to LSP's 0-indexed Position.
    pub fn to_lsp_position(&self) -> Position {
        Position {
            line: self.line.saturating_sub(1),
            character: self.character.saturating_sub(1),
        }
    }

    /// Create from LSP's 0-indexed Position.
    pub fn from_lsp_position(pos: Position) -> Self {
        Self {
            line: pos.line + 1,
            character: pos.character + 1,
        }
    }
}

/// A location in a document with 1-indexed positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspLocation {
    /// File path.
    pub path: PathBuf,
    /// Start position (1-indexed).
    pub start: LspPosition,
    /// End position (1-indexed).
    pub end: LspPosition,
}

impl LspLocation {
    /// Create from an LSP Location.
    pub fn from_lsp_location(loc: &Location) -> Result<Self, LspError> {
        let path = loc
            .uri
            .to_file_path()
            .map_err(|_| LspError::InvalidPath(loc.uri.to_string()))?;

        Ok(Self {
            path,
            start: LspPosition::from_lsp_position(loc.range.start),
            end: LspPosition::from_lsp_position(loc.range.end),
        })
    }
}

/// Convert a file path to an LSP URI.
pub fn path_to_uri(path: &std::path::Path) -> Result<Url, LspError> {
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    Url::from_file_path(&absolute_path)
        .map_err(|_| LspError::InvalidPath(absolute_path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_conversion() {
        // 1-indexed input
        let pos = LspPosition::new(10, 5);

        // Convert to 0-indexed LSP position
        let lsp_pos = pos.to_lsp_position();
        assert_eq!(lsp_pos.line, 9);
        assert_eq!(lsp_pos.character, 4);

        // Convert back
        let pos2 = LspPosition::from_lsp_position(lsp_pos);
        assert_eq!(pos2.line, 10);
        assert_eq!(pos2.character, 5);
    }

    #[test]
    fn test_position_edge_cases() {
        // Line 1, char 1 should become 0, 0
        let pos = LspPosition::new(1, 1);
        let lsp_pos = pos.to_lsp_position();
        assert_eq!(lsp_pos.line, 0);
        assert_eq!(lsp_pos.character, 0);

        // Zero input (invalid but should not panic)
        let pos = LspPosition::new(0, 0);
        let lsp_pos = pos.to_lsp_position();
        assert_eq!(lsp_pos.line, 0);
        assert_eq!(lsp_pos.character, 0);
    }

    #[test]
    fn test_server_config_builders() {
        let config = LspServerConfig::rust_analyzer();
        assert_eq!(config.language_id, "rust");
        assert_eq!(config.command, "rust-analyzer");
        assert!(config.file_extensions.contains(&"rs".to_string()));

        let config = LspServerConfig::typescript();
        assert_eq!(config.language_id, "typescript");
        assert!(config.args.contains(&"--stdio".to_string()));
    }
}
