//! LSP (Language Server Protocol) tools for code intelligence.
//!
//! These tools provide IDE-like code navigation capabilities:
//! - Go to definition
//! - Find references
//! - Hover information
//! - Document/workspace symbols
//! - Go to implementation
//! - Call hierarchy

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use g3_lsp::{
    CallHierarchyIncomingCall, CallHierarchyOutgoingCall, DocumentSymbol,
    HoverContents, LspClient, LspLocation, LspServerConfig, MarkedString,
    SymbolInformation, SymbolKind,
};
use serde_json::json;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::tools::executor::ToolContext;
use crate::ui_writer::UiWriter;
use crate::ToolCall;

/// Manager for LSP client connections.
/// Lazily starts language servers as needed and caches them.
pub struct LspManager {
    /// Active LSP clients keyed by language ID.
    clients: RwLock<std::collections::HashMap<String, Arc<LspClient>>>,
    /// Root path for the workspace.
    root_path: std::path::PathBuf,
}

impl LspManager {
    /// Create a new LSP manager for the given workspace root.
    pub fn new(root_path: std::path::PathBuf) -> Self {
        Self {
            clients: RwLock::new(std::collections::HashMap::new()),
            root_path,
        }
    }

    /// Get or create an LSP client for the given language.
    pub async fn get_client(&self, language: &str) -> Result<Arc<LspClient>, String> {
        // Check if we already have a client
        {
            let clients = self.clients.read().await;
            if let Some(client) = clients.get(language) {
                return Ok(client.clone());
            }
        }

        // Create a new client
        let config = match language {
            "rust" => LspServerConfig::rust_analyzer(),
            "typescript" | "javascript" => LspServerConfig::typescript(),
            "python" => LspServerConfig::python(),
            "go" => LspServerConfig::go(),
            _ => return Err(format!("Unsupported language: {}", language)),
        };

        info!(language = language, "Starting LSP server");

        match LspClient::start(config, &self.root_path).await {
            Ok(client) => {
                let client = Arc::new(client);
                let mut clients = self.clients.write().await;
                clients.insert(language.to_string(), client.clone());
                Ok(client)
            }
            Err(e) => Err(format!("Failed to start LSP server for {}: {}", language, e)),
        }
    }

    /// Detect language from file extension.
    pub fn detect_language(file_path: &str) -> Option<&'static str> {
        let path = Path::new(file_path);
        match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => Some("rust"),
            Some("ts") | Some("tsx") => Some("typescript"),
            Some("js") | Some("jsx") => Some("javascript"),
            Some("py") | Some("pyi") => Some("python"),
            Some("go") => Some("go"),
            _ => None,
        }
    }

    /// Get status of all active LSP servers.
    pub async fn get_status(&self) -> Vec<(String, bool)> {
        let clients = self.clients.read().await;
        clients
            .iter()
            .map(|(lang, _client)| (lang.clone(), true)) // All cached clients are assumed running
            .collect()
    }

    /// Shutdown all LSP clients.
    pub async fn shutdown_all(&self) {
        let mut clients = self.clients.write().await;
        for (lang, client) in clients.drain() {
            info!(language = %lang, "Shutting down LSP client");
            // We can't call shutdown since it consumes self, and we have Arc
            // The client will be dropped when the Arc count goes to 0
            drop(client);
        }
    }
}

/// Execute the lsp_goto_definition tool.
pub async fn execute_goto_definition<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let file_path = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

    let line = args
        .get("line")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: line"))? as u32;

    let character = args
        .get("character")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: character"))? as u32;

    debug!(
        file = file_path,
        line = line,
        character = character,
        "Executing lsp_goto_definition"
    );

    // Detect language
    let language = LspManager::detect_language(file_path)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine language for file: {}", file_path))?;

    // Get or create LSP manager
    let lsp_manager = get_or_create_lsp_manager(ctx).await?;

    // Get client
    let client = lsp_manager
        .get_client(language)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    // Execute goto definition
    match client.goto_definition(Path::new(file_path), line, character).await {
        Ok(locations) => {
            if locations.is_empty() {
                Ok(json!({
                    "status": "success",
                    "message": "No definition found at this location",
                    "locations": []
                })
                .to_string())
            } else {
                let formatted = format_locations(&locations);
                Ok(json!({
                    "status": "success",
                    "count": locations.len(),
                    "locations": formatted
                })
                .to_string())
            }
        }
        Err(e) => Ok(json!({
            "status": "error",
            "message": format!("Failed to find definition: {}", e)
        })
        .to_string()),
    }
}

/// Execute the lsp_find_references tool.
pub async fn execute_find_references<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let file_path = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

    let line = args
        .get("line")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: line"))? as u32;

    let character = args
        .get("character")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: character"))? as u32;

    let include_declaration = args
        .get("include_declaration")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    debug!(
        file = file_path,
        line = line,
        character = character,
        include_declaration = include_declaration,
        "Executing lsp_find_references"
    );

    let language = LspManager::detect_language(file_path)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine language for file: {}", file_path))?;

    let lsp_manager = get_or_create_lsp_manager(ctx).await?;
    let client = lsp_manager
        .get_client(language)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    match client
        .find_references(Path::new(file_path), line, character, include_declaration)
        .await
    {
        Ok(locations) => {
            if locations.is_empty() {
                Ok(json!({
                    "status": "success",
                    "message": "No references found",
                    "locations": []
                })
                .to_string())
            } else {
                let formatted = format_locations(&locations);
                Ok(json!({
                    "status": "success",
                    "count": locations.len(),
                    "locations": formatted
                })
                .to_string())
            }
        }
        Err(e) => Ok(json!({
            "status": "error",
            "message": format!("Failed to find references: {}", e)
        })
        .to_string()),
    }
}

/// Execute the lsp_hover tool.
pub async fn execute_hover<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let file_path = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

    let line = args
        .get("line")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: line"))? as u32;

    let character = args
        .get("character")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: character"))? as u32;

    debug!(
        file = file_path,
        line = line,
        character = character,
        "Executing lsp_hover"
    );

    let language = LspManager::detect_language(file_path)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine language for file: {}", file_path))?;

    let lsp_manager = get_or_create_lsp_manager(ctx).await?;
    let client = lsp_manager
        .get_client(language)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    match client.hover(Path::new(file_path), line, character).await {
        Ok(Some(hover)) => {
            let content = format_hover_contents(&hover.contents);
            Ok(json!({
                "status": "success",
                "content": content
            })
            .to_string())
        }
        Ok(None) => Ok(json!({
            "status": "success",
            "message": "No hover information available at this location"
        })
        .to_string()),
        Err(e) => Ok(json!({
            "status": "error",
            "message": format!("Failed to get hover info: {}", e)
        })
        .to_string()),
    }
}

/// Execute the lsp_document_symbols tool.
pub async fn execute_document_symbols<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let file_path = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

    debug!(file = file_path, "Executing lsp_document_symbols");

    let language = LspManager::detect_language(file_path)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine language for file: {}", file_path))?;

    let lsp_manager = get_or_create_lsp_manager(ctx).await?;
    let client = lsp_manager
        .get_client(language)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    match client.document_symbols(Path::new(file_path)).await {
        Ok(symbols) => {
            if symbols.is_empty() {
                Ok(json!({
                    "status": "success",
                    "message": "No symbols found in document",
                    "symbols": []
                })
                .to_string())
            } else {
                let formatted = format_document_symbols(&symbols, 0);
                Ok(json!({
                    "status": "success",
                    "count": count_symbols(&symbols),
                    "symbols": formatted
                })
                .to_string())
            }
        }
        Err(e) => Ok(json!({
            "status": "error",
            "message": format!("Failed to get document symbols: {}", e)
        })
        .to_string()),
    }
}

/// Execute the lsp_workspace_symbols tool.
pub async fn execute_workspace_symbols<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;

    // Optional language parameter - defaults to rust for now
    let language = args
        .get("language")
        .and_then(|v| v.as_str())
        .unwrap_or("rust");

    debug!(query = query, language = language, "Executing lsp_workspace_symbols");

    let lsp_manager = get_or_create_lsp_manager(ctx).await?;
    let client = lsp_manager
        .get_client(language)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    match client.workspace_symbols(query).await {
        Ok(symbols) => {
            if symbols.is_empty() {
                Ok(json!({
                    "status": "success",
                    "message": format!("No symbols matching '{}' found", query),
                    "symbols": []
                })
                .to_string())
            } else {
                let formatted = format_symbol_information(&symbols);
                Ok(json!({
                    "status": "success",
                    "query": query,
                    "count": symbols.len(),
                    "symbols": formatted
                })
                .to_string())
            }
        }
        Err(e) => Ok(json!({
            "status": "error",
            "message": format!("Failed to search workspace symbols: {}", e)
        })
        .to_string()),
    }
}

/// Execute the lsp_goto_implementation tool.
pub async fn execute_goto_implementation<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let file_path = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

    let line = args
        .get("line")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: line"))? as u32;

    let character = args
        .get("character")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: character"))? as u32;

    debug!(
        file = file_path,
        line = line,
        character = character,
        "Executing lsp_goto_implementation"
    );

    let language = LspManager::detect_language(file_path)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine language for file: {}", file_path))?;

    let lsp_manager = get_or_create_lsp_manager(ctx).await?;
    let client = lsp_manager
        .get_client(language)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    match client
        .goto_implementation(Path::new(file_path), line, character)
        .await
    {
        Ok(locations) => {
            if locations.is_empty() {
                Ok(json!({
                    "status": "success",
                    "message": "No implementations found",
                    "locations": []
                })
                .to_string())
            } else {
                let formatted = format_locations(&locations);
                Ok(json!({
                    "status": "success",
                    "count": locations.len(),
                    "locations": formatted
                })
                .to_string())
            }
        }
        Err(e) => Ok(json!({
            "status": "error",
            "message": format!("Failed to find implementations: {}", e)
        })
        .to_string()),
    }
}

/// Execute the lsp_call_hierarchy tool.
pub async fn execute_call_hierarchy<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let file_path = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

    let line = args
        .get("line")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: line"))? as u32;

    let character = args
        .get("character")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: character"))? as u32;

    let direction = args
        .get("direction")
        .and_then(|v| v.as_str())
        .unwrap_or("both");

    debug!(
        file = file_path,
        line = line,
        character = character,
        direction = direction,
        "Executing lsp_call_hierarchy"
    );

    let language = LspManager::detect_language(file_path)
        .ok_or_else(|| anyhow::anyhow!("Cannot determine language for file: {}", file_path))?;

    let lsp_manager = get_or_create_lsp_manager(ctx).await?;
    let client = lsp_manager
        .get_client(language)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    // First, prepare the call hierarchy
    let items = match client
        .prepare_call_hierarchy(Path::new(file_path), line, character)
        .await
    {
        Ok(items) => items,
        Err(e) => {
            return Ok(json!({
                "status": "error",
                "message": format!("Failed to prepare call hierarchy: {}", e)
            })
            .to_string());
        }
    };

    if items.is_empty() {
        return Ok(json!({
            "status": "success",
            "message": "No call hierarchy item found at this location"
        })
        .to_string());
    }

    let item = items.into_iter().next().unwrap();
    let mut result = json!({
        "status": "success",
        "item": {
            "name": item.name,
            "kind": format_symbol_kind(item.kind),
            "file": item.uri.to_file_path().ok().map(|p| p.display().to_string()),
            "line": item.selection_range.start.line + 1,
        }
    });

    // Get incoming calls if requested
    if direction == "incoming" || direction == "both" {
        match client.incoming_calls(item.clone()).await {
            Ok(calls) => {
                let formatted = format_incoming_calls(&calls);
                result["incoming_calls"] = json!(formatted);
                result["incoming_count"] = json!(calls.len());
            }
            Err(e) => {
                warn!(error = %e, "Failed to get incoming calls");
                result["incoming_error"] = json!(e.to_string());
            }
        }
    }

    // Get outgoing calls if requested
    if direction == "outgoing" || direction == "both" {
        match client.outgoing_calls(item).await {
            Ok(calls) => {
                let formatted = format_outgoing_calls(&calls);
                result["outgoing_calls"] = json!(formatted);
                result["outgoing_count"] = json!(calls.len());
            }
            Err(e) => {
                warn!(error = %e, "Failed to get outgoing calls");
                result["outgoing_error"] = json!(e.to_string());
            }
        }
    }

    Ok(result.to_string())
}

/// Execute the lsp_diagnostics tool.
pub async fn execute_diagnostics<W: UiWriter>(
    tool_call: &ToolCall,
    _ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let file_path = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

    debug!(file = file_path, "Executing lsp_diagnostics");

    // Note: LSP diagnostics are typically pushed from server to client via notifications.
    // For now, we return a message explaining this limitation.
    // A full implementation would require tracking diagnostics from didOpen/didChange notifications.

    Ok(json!({
        "status": "info",
        "message": "LSP diagnostics are delivered via server notifications. For immediate diagnostics, use the 'shell' tool to run language-specific linters (e.g., 'cargo check' for Rust, 'tsc --noEmit' for TypeScript).",
        "file": file_path,
        "suggestion": "Try running the appropriate compiler/linter for this file type to get diagnostics."
    })
    .to_string())
}

/// Execute the lsp_status tool.
pub async fn execute_status<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Executing lsp_status");

    // Check if LSP is enabled in config
    if !ctx.config.lsp.enabled {
        return Ok(json!({
            "status": "disabled",
            "message": "LSP integration is disabled. Set `lsp.enabled = true` in your config."
        })
        .to_string());
    }

    // Get LSP manager status if available
    if let Some(ref lsp_manager) = ctx.lsp_manager {
        let servers = lsp_manager.get_status().await;

        if servers.is_empty() {
            return Ok(json!({
                "status": "enabled",
                "message": "LSP integration is enabled but no servers are currently running",
                "active_servers": [],
                "supported_languages": ["rust", "typescript", "javascript", "python", "go"]
            })
            .to_string());
        }

        let server_info: Vec<serde_json::Value> = servers
            .iter()
            .map(|(lang, running)| {
                json!({
                    "language": lang,
                    "running": running
                })
            })
            .collect();

        Ok(json!({
            "status": "enabled",
            "active_servers": server_info,
            "supported_languages": ["rust", "typescript", "javascript", "python", "go"]
        })
        .to_string())
    } else {
        Ok(json!({
            "status": "enabled",
            "message": "LSP integration is enabled but manager not initialized. Will start on first use.",
            "supported_languages": ["rust", "typescript", "javascript", "python", "go"]
        })
        .to_string())
    }
}

// Helper function to get or create LSP manager from context
async fn get_or_create_lsp_manager<W: UiWriter>(
    ctx: &mut ToolContext<'_, W>,
) -> Result<Arc<LspManager>> {
    // Check if LSP is enabled
    if !ctx.config.lsp.enabled {
        return Err(anyhow::anyhow!(
            "LSP integration is disabled. Set `lsp.enabled = true` in your config."
        ));
    }

    // Return existing manager if available
    if let Some(ref manager) = ctx.lsp_manager {
        return Ok(manager.clone());
    }

    // Create new manager
    let work_dir = ctx.working_dir.unwrap_or(".");
    let manager = Arc::new(LspManager::new(std::path::PathBuf::from(work_dir)));

    // Note: We can't store it back in ctx since we only have &mut and would need interior mutability
    // The caller should ideally store this in a persistent location

    Ok(manager)
}

// Formatting helpers

fn format_locations(locations: &[LspLocation]) -> Vec<serde_json::Value> {
    locations
        .iter()
        .map(|loc| {
            json!({
                "file": loc.path.display().to_string(),
                "start_line": loc.start.line,
                "start_character": loc.start.character,
                "end_line": loc.end.line,
                "end_character": loc.end.character
            })
        })
        .collect()
}

fn format_hover_contents(contents: &HoverContents) -> String {
    match contents {
        HoverContents::Scalar(marked) => format_marked_string(marked),
        HoverContents::Array(marked_strings) => marked_strings
            .iter()
            .map(format_marked_string)
            .collect::<Vec<_>>()
            .join("\n\n"),
        HoverContents::Markup(markup) => markup.value.clone(),
    }
}

fn format_marked_string(marked: &MarkedString) -> String {
    match marked {
        MarkedString::String(s) => s.clone(),
        MarkedString::LanguageString(ls) => format!("```{}\n{}\n```", ls.language, ls.value),
    }
}

fn format_document_symbols(symbols: &[DocumentSymbol], indent: usize) -> Vec<serde_json::Value> {
    let indent_str = "  ".repeat(indent);
    symbols
        .iter()
        .flat_map(|sym| {
            let mut result = vec![json!({
                "name": format!("{}{}", indent_str, sym.name),
                "kind": format_symbol_kind(sym.kind),
                "line": sym.range.start.line + 1,
                "detail": sym.detail
            })];

            if let Some(ref children) = sym.children {
                result.extend(format_document_symbols(children, indent + 1));
            }

            result
        })
        .collect()
}

fn count_symbols(symbols: &[DocumentSymbol]) -> usize {
    symbols.iter().fold(0, |acc, sym| {
        acc + 1
            + sym
                .children
                .as_ref()
                .map(|c| count_symbols(c))
                .unwrap_or(0)
    })
}

fn format_symbol_information(symbols: &[SymbolInformation]) -> Vec<serde_json::Value> {
    symbols
        .iter()
        .map(|sym| {
            json!({
                "name": sym.name,
                "kind": format_symbol_kind(sym.kind),
                "file": sym.location.uri.to_file_path().ok().map(|p| p.display().to_string()),
                "line": sym.location.range.start.line + 1,
                "container": sym.container_name
            })
        })
        .collect()
}

fn format_symbol_kind(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::FILE => "file",
        SymbolKind::MODULE => "module",
        SymbolKind::NAMESPACE => "namespace",
        SymbolKind::PACKAGE => "package",
        SymbolKind::CLASS => "class",
        SymbolKind::METHOD => "method",
        SymbolKind::PROPERTY => "property",
        SymbolKind::FIELD => "field",
        SymbolKind::CONSTRUCTOR => "constructor",
        SymbolKind::ENUM => "enum",
        SymbolKind::INTERFACE => "interface",
        SymbolKind::FUNCTION => "function",
        SymbolKind::VARIABLE => "variable",
        SymbolKind::CONSTANT => "constant",
        SymbolKind::STRING => "string",
        SymbolKind::NUMBER => "number",
        SymbolKind::BOOLEAN => "boolean",
        SymbolKind::ARRAY => "array",
        SymbolKind::OBJECT => "object",
        SymbolKind::KEY => "key",
        SymbolKind::NULL => "null",
        SymbolKind::ENUM_MEMBER => "enum_member",
        SymbolKind::STRUCT => "struct",
        SymbolKind::EVENT => "event",
        SymbolKind::OPERATOR => "operator",
        SymbolKind::TYPE_PARAMETER => "type_parameter",
        _ => "unknown",
    }
}

fn format_incoming_calls(calls: &[CallHierarchyIncomingCall]) -> Vec<serde_json::Value> {
    calls
        .iter()
        .map(|call| {
            json!({
                "from": call.from.name,
                "kind": format_symbol_kind(call.from.kind),
                "file": call.from.uri.to_file_path().ok().map(|p| p.display().to_string()),
                "line": call.from.selection_range.start.line + 1,
                "call_sites": call.from_ranges.len()
            })
        })
        .collect()
}

fn format_outgoing_calls(calls: &[CallHierarchyOutgoingCall]) -> Vec<serde_json::Value> {
    calls
        .iter()
        .map(|call| {
            json!({
                "to": call.to.name,
                "kind": format_symbol_kind(call.to.kind),
                "file": call.to.uri.to_file_path().ok().map(|p| p.display().to_string()),
                "line": call.to.selection_range.start.line + 1,
                "call_sites": call.from_ranges.len()
            })
        })
        .collect()
}
