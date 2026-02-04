//! Language Server Protocol client for G3 AI agent.
//!
//! This crate provides LSP client functionality to enable code intelligence
//! features like go-to-definition, find references, hover information, and more.
//!
//! # Architecture
//!
//! - `LspClient`: Manages a single LSP server connection
//! - `StdioTransport`: Handles process spawning and stdio communication
//! - `LspServerConfig`: Configuration for different language servers
//!
//! # Example
//!
//! ```no_run
//! use g3_lsp::{LspClient, LspServerConfig};
//! use std::path::Path;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create configuration for rust-analyzer
//! let config = LspServerConfig::rust_analyzer();
//!
//! // Start the client
//! let mut client = LspClient::start(config, Path::new("/path/to/project")).await?;
//!
//! // Go to definition (line and character are 1-indexed)
//! let locations = client.goto_definition(
//!     Path::new("/path/to/file.rs"),
//!     10,  // line
//!     5,   // character
//! ).await?;
//!
//! // Shutdown when done
//! client.shutdown().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Line Number Convention
//!
//! All public APIs use **1-indexed** line and character numbers, matching
//! what users see in editors. Internally, these are converted to LSP's
//! 0-indexed positions.

pub mod client;
pub mod discovery;
pub mod lifecycle;
pub mod manager;
pub mod transport;
pub mod types;

// Re-exports for convenient access
pub use client::LspClient;
pub use discovery::{default_server_config, detect_language, detect_project, find_project_root, root_markers};
pub use lifecycle::{health_check, HealthStatus};
pub use manager::{LspManager, LspManagerConfig, ServerStatus};
pub use types::{LspError, LspLocation, LspPosition, LspServerConfig};

// Re-export commonly used lsp-types for consumers
pub use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, DocumentSymbol,
    Hover, HoverContents, Location, MarkedString, MarkupContent, SymbolInformation, SymbolKind,
};

// Re-export url::Url for convenience
pub use url::Url;
