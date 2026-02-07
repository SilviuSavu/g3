//! Index tools for codebase semantic search.
//!
//! These tools allow the agent to index the codebase and perform
//! semantic code searches using vector embeddings.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use tracing::{debug, info, warn};

use crate::index_client::IndexClient;
use crate::tools::executor::ToolContext;
use crate::ui_writer::UiWriter;
use crate::ToolCall;

/// Execute the index_codebase tool.
pub async fn execute_index_codebase<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let path = args.get("path").and_then(|v| v.as_str()).map(String::from);

    let force = args
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Check if indexing is enabled
    if !ctx.config.index.enabled {
        return Ok(json!({
            "status": "error",
            "message": "Indexing is not enabled. Set `index.enabled = true` in your config."
        }).to_string());
    }

    let work_dir = path
        .as_deref()
        .or(ctx.working_dir)
        .unwrap_or(".");

    let work_path = Path::new(work_dir);

    info!("Indexing codebase at: {} (force={})", work_dir, force);

    // Get or create index client
    let client: Arc<IndexClient> = match &ctx.index_client {
        Some(client) => client.clone(),
        None => {
            // Initialize a new client
            info!("Initializing new IndexClient for {}", work_dir);
            match IndexClient::new(&ctx.config.index, work_path).await {
                Ok(client) => Arc::new(client),
                Err(e) => {
                    warn!("Failed to initialize IndexClient: {}", e);
                    return Ok(json!({
                        "status": "error",
                        "message": format!("Failed to initialize index client: {}", e),
                        "hint": "Check that Qdrant is running and API key is configured"
                    }).to_string());
                }
            }
        }
    };

    // Store the client in the context for future use
    ctx.index_client = Some(client.clone());

    // Perform indexing
    match client.index(force).await {
        Ok(stats) => {
            let result = json!({
                "status": "success",
                "files_processed": stats.files_processed,
                "chunks_created": stats.chunks_created,
                "chunks_updated": stats.chunks_updated,
                "chunks_deleted": stats.chunks_deleted,
                "files_skipped": stats.files_skipped,
                "duration_ms": stats.duration_ms,
                "embedding_calls": stats.embedding_calls,
                "working_dir": work_dir,
                "force": force
            });
            Ok(serde_json::to_string_pretty(&result)?)
        }
        Err(e) => {
            warn!("Indexing failed: {}", e);
            Ok(json!({
                "status": "error",
                "message": format!("Indexing failed: {}", e)
            }).to_string())
        }
    }
}

/// Execute the list_directory tool.
pub async fn execute_list_directory<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: path"))?;

    let include_hidden = args
        .get("include_hidden")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let work_dir = ctx.working_dir.unwrap_or(".");
    let work_path = Path::new(work_dir);
    let dir_path = work_path.join(path);

    if !dir_path.exists() {
        return Ok(json!({
            "status": "error",
            "message": format!("Directory not found: {}", dir_path.display())
        }).to_string());
    }

    let mut entries: Vec<serde_json::Value> = Vec::new();

    for entry in dir_path.read_dir()? {
        if let Ok(entry) = entry {
            let file_type = entry.file_type()?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Skip hidden files by default
            if !include_hidden && file_name_str.starts_with('.') {
                continue;
            }

            // Get metadata
            let metadata = entry.metadata()?;
            let size = metadata.len();

            // Count lines for text files
            let line_count = if file_type.is_file() {
                // Try to read the file to count lines
                match std::fs::read_to_string(entry.path()) {
                    Ok(content) => content.lines().count(),
                    Err(_) => 0,
                }
            } else {
                0
            };

            entries.push(json!({
                "name": file_name_str,
                "is_dir": file_type.is_dir(),
                "is_file": file_type.is_file(),
                "size": size,
                "lines": line_count
            }));
        }
    }

    Ok(json!({
        "status": "success",
        "path": path,
        "include_hidden": include_hidden,
        "entries": entries,
        "count": entries.len()
    }).to_string())
}

/// Execute the preview_file tool.
pub async fn execute_preview_file<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: path"))?;

    let num_lines = args
        .get("num_lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as usize;

    let work_dir = ctx.working_dir.unwrap_or(".");
    let work_path = Path::new(work_dir);
    let file_path = work_path.join(path);

    if !file_path.exists() {
        return Ok(json!({
            "status": "error",
            "message": format!("File not found: {}", file_path.display())
        }).to_string());
    }

    match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let preview_lines = lines.iter().take(num_lines).cloned().collect::<Vec<&str>>();

            Ok(json!({
                "status": "success",
                "path": path,
                "num_lines": num_lines,
                "total_lines": lines.len(),
                "preview": preview_lines.join("\n")
            }).to_string())
        }
        Err(e) => Ok(json!({
            "status": "error",
            "message": format!("Failed to read file: {}", e)
        }).to_string()),
    }
}

/// Execute the semantic_search tool.
pub async fn execute_semantic_search<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;

    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(10)
        .min(50);

    let file_filter = args
        .get("file_filter")
        .and_then(|v| v.as_str());

    // Check if indexing is enabled
    if !ctx.config.index.enabled {
        return Ok(json!({
            "status": "error",
            "message": "Semantic search requires indexing to be enabled. Set `index.enabled = true` in your config."
        }).to_string());
    }

    debug!(
        "Semantic search: query='{}', limit={}, filter={:?}",
        query, limit, file_filter
    );

    // Get index client - must be initialized first via index_codebase
    let client = match &ctx.index_client {
        Some(client) => client.clone(),
        None => {
            // Try to initialize from working directory
            let work_dir = ctx.working_dir.unwrap_or(".");
            let work_path = Path::new(work_dir);

            match IndexClient::new(&ctx.config.index, work_path).await {
                Ok(client) => Arc::new(client),
                Err(e) => {
                    return Ok(json!({
                        "status": "error",
                        "message": format!("Index not initialized. Run `index_codebase` first. Error: {}", e)
                    }).to_string());
                }
            }
        }
    };

    // Perform search
    match client.search(query, limit, file_filter).await {
        Ok(results) => {
            let formatted_results: Vec<serde_json::Value> = results
                .iter()
                .map(|r| {
                    json!({
                        "file": r.file_path,
                        "lines": format!("{}-{}", r.start_line, r.end_line),
                        "kind": r.kind,
                        "name": r.name,
                        "score": format!("{:.3}", r.score),
                        "content": truncate_content(&r.content, 500)
                    })
                })
                .collect();

            let result = json!({
                "status": "success",
                "query": query,
                "count": results.len(),
                "results": formatted_results
            });
            Ok(serde_json::to_string_pretty(&result)?)
        }
        Err(e) => {
            warn!("Search failed: {}", e);
            Ok(json!({
                "status": "error",
                "message": format!("Search failed: {}", e)
            }).to_string())
        }
    }
}

/// Execute the index_status tool.
pub async fn execute_index_status<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    // Check if indexing is enabled
    if !ctx.config.index.enabled {
        return Ok(json!({
            "enabled": false,
            "status": "disabled",
            "message": "Indexing is not enabled. Set `index.enabled = true` in your config."
        }).to_string());
    }

    // Check if client is initialized
    match &ctx.index_client {
        Some(client) => {
            let stats = client.get_stats().await;
            let result = json!({
                "enabled": true,
                "status": "connected",
                "working_dir": client.working_dir().to_string_lossy(),
                "stats": {
                    "files_indexed": stats.files_processed,
                    "total_chunks": stats.chunks_created
                },
                "config": {
                    "qdrant_url": ctx.config.index.qdrant_url,
                    "collection": ctx.config.index.collection_name,
                    "embedding_model": ctx.config.index.embeddings.model,
                    "dimensions": ctx.config.index.embeddings.dimensions,
                    "hybrid_search": ctx.config.index.search.hybrid,
                    "bm25_weight": ctx.config.index.search.bm25_weight,
                    "vector_weight": ctx.config.index.search.vector_weight,
                }
            });
            Ok(serde_json::to_string_pretty(&result)?)
        }
        None => {
            let result = json!({
                "enabled": true,
                "status": "not_initialized",
                "message": "Index client not initialized. Run `index_codebase` to initialize.",
                "config": {
                    "qdrant_url": ctx.config.index.qdrant_url,
                    "collection": ctx.config.index.collection_name,
                    "embedding_model": ctx.config.index.embeddings.model,
                    "dimensions": ctx.config.index.embeddings.dimensions,
                    "hybrid_search": ctx.config.index.search.hybrid,
                    "bm25_weight": ctx.config.index.search.bm25_weight,
                    "vector_weight": ctx.config.index.search.vector_weight,
                }
            });
            Ok(serde_json::to_string_pretty(&result)?)
        }
    }
}

/// Truncate content to a maximum length, preserving word boundaries.
fn truncate_content(content: &str, max_len: usize) -> String {
    if content.len() <= max_len {
        return content.to_string();
    }

    // Find a good break point
    let truncated = &content[..max_len];
    if let Some(pos) = truncated.rfind('\n') {
        format!("{}...", &content[..pos])
    } else if let Some(pos) = truncated.rfind(' ') {
        format!("{}...", &content[..pos])
    } else {
        format!("{}...", truncated)
    }
}

// ============================================================================
// Knowledge Graph Tools
// ============================================================================

/// Execute the graph_find_symbol tool.
pub async fn execute_graph_find_symbol<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;

    // Check if indexing is enabled
    if !ctx.config.index.enabled {
        return Ok(json!({
            "status": "error",
            "message": "Graph search requires indexing to be enabled. Set `index.enabled = true` in your config."
        }).to_string());
    }

    // Get index client
    let client = get_or_init_client(ctx).await?;

    // Check if graph is available
    if !client.has_graph().await {
        return Ok(json!({
            "status": "error",
            "message": "Knowledge graph not available. Run `index_codebase` first to build the graph."
        }).to_string());
    }

    // Find symbols
    match client.find_symbols_by_name(name).await {
        Ok(symbols) => {
            let formatted: Vec<serde_json::Value> = symbols
                .iter()
                .map(|s| {
                    json!({
                        "id": s.id,
                        "name": s.name,
                        "kind": s.kind,
                        "file": s.file_id,
                        "lines": format!("{}-{}", s.line_start, s.line_end),
                        "signature": s.signature
                    })
                })
                .collect();

            let result = json!({
                "status": "success",
                "name": name,
                "count": symbols.len(),
                "symbols": formatted
            });
            Ok(serde_json::to_string_pretty(&result)?)
        }
        Err(e) => {
            warn!("Graph find_symbol failed: {}", e);
            Ok(json!({
                "status": "error",
                "message": format!("Failed to find symbols: {}", e)
            }).to_string())
        }
    }
}

/// Execute the graph_file_symbols tool.
pub async fn execute_graph_file_symbols<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let file_path = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

    // Check if indexing is enabled
    if !ctx.config.index.enabled {
        return Ok(json!({
            "status": "error",
            "message": "Graph search requires indexing to be enabled."
        }).to_string());
    }

    // Get index client
    let client = get_or_init_client(ctx).await?;

    // Check if graph is available
    if !client.has_graph().await {
        return Ok(json!({
            "status": "error",
            "message": "Knowledge graph not available. Run `index_codebase` first."
        }).to_string());
    }

    // Get symbols in file
    match client.get_file_symbols(file_path).await {
        Ok(symbols) => {
            let formatted: Vec<serde_json::Value> = symbols
                .iter()
                .map(|s| {
                    json!({
                        "id": s.id,
                        "name": s.name,
                        "kind": s.kind,
                        "lines": format!("{}-{}", s.line_start, s.line_end),
                        "signature": s.signature
                    })
                })
                .collect();

            let result = json!({
                "status": "success",
                "file": file_path,
                "count": symbols.len(),
                "symbols": formatted
            });
            Ok(serde_json::to_string_pretty(&result)?)
        }
        Err(e) => {
            warn!("Graph file_symbols failed: {}", e);
            Ok(json!({
                "status": "error",
                "message": format!("Failed to get file symbols: {}", e)
            }).to_string())
        }
    }
}

/// Execute the graph_find_callers tool.
pub async fn execute_graph_find_callers<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let symbol_id = args
        .get("symbol_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: symbol_id"))?;

    // Check if indexing is enabled
    if !ctx.config.index.enabled {
        return Ok(json!({
            "status": "error",
            "message": "Graph search requires indexing to be enabled."
        }).to_string());
    }

    // Get index client
    let client = get_or_init_client(ctx).await?;

    // Check if graph is available
    if !client.has_graph().await {
        return Ok(json!({
            "status": "error",
            "message": "Knowledge graph not available. Run `index_codebase` first."
        }).to_string());
    }

    // Find callers
    match client.find_callers(symbol_id).await {
        Ok(callers) => {
            let result = json!({
                "status": "success",
                "symbol_id": symbol_id,
                "count": callers.len(),
                "callers": callers
            });
            Ok(serde_json::to_string_pretty(&result)?)
        }
        Err(e) => {
            warn!("Graph find_callers failed: {}", e);
            Ok(json!({
                "status": "error",
                "message": format!("Failed to find callers: {}", e)
            }).to_string())
        }
    }
}

/// Execute the graph_find_references tool.
pub async fn execute_graph_find_references<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let symbol_id = args
        .get("symbol_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: symbol_id"))?;

    // Check if indexing is enabled
    if !ctx.config.index.enabled {
        return Ok(json!({
            "status": "error",
            "message": "Graph search requires indexing to be enabled."
        }).to_string());
    }

    // Get index client
    let client = get_or_init_client(ctx).await?;

    // Check if graph is available
    if !client.has_graph().await {
        return Ok(json!({
            "status": "error",
            "message": "Knowledge graph not available. Run `index_codebase` first."
        }).to_string());
    }

    // Find references
    match client.find_references(symbol_id).await {
        Ok(refs) => {
            let formatted: Vec<serde_json::Value> = refs
                .iter()
                .map(|r| {
                    json!({
                        "source": r.source,
                        "target": r.target,
                        "file": r.file,
                        "line": r.line
                    })
                })
                .collect();

            let result = json!({
                "status": "success",
                "symbol_id": symbol_id,
                "count": refs.len(),
                "references": formatted
            });
            Ok(serde_json::to_string_pretty(&result)?)
        }
        Err(e) => {
            warn!("Graph find_references failed: {}", e);
            Ok(json!({
                "status": "error",
                "message": format!("Failed to find references: {}", e)
            }).to_string())
        }
    }
}

/// Execute the graph_stats tool.
pub async fn execute_graph_stats<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    // Check if indexing is enabled
    if !ctx.config.index.enabled {
        return Ok(json!({
            "status": "error",
            "message": "Graph requires indexing to be enabled."
        }).to_string());
    }

    // Get index client
    let client = get_or_init_client(ctx).await?;

    // Check if graph is available
    if !client.has_graph().await {
        return Ok(json!({
            "status": "success",
            "graph_enabled": false,
            "message": "Knowledge graph not initialized. Run `index_codebase` to build it."
        }).to_string());
    }

    // Get graph stats
    match client.get_graph_stats().await {
        Ok(stats) => {
            let result = json!({
                "status": "success",
                "graph_enabled": true,
                "symbol_count": stats.symbol_count,
                "file_count": stats.file_count
            });
            Ok(serde_json::to_string_pretty(&result)?)
        }
        Err(e) => {
            warn!("Graph stats failed: {}", e);
            Ok(json!({
                "status": "error",
                "message": format!("Failed to get graph stats: {}", e)
            }).to_string())
        }
    }
}

/// Helper to get or initialize the index client.
async fn get_or_init_client<W: UiWriter>(
    ctx: &mut ToolContext<'_, W>,
) -> Result<Arc<IndexClient>> {
    match &ctx.index_client {
        Some(client) => Ok(client.clone()),
        None => {
            let work_dir = ctx.working_dir.unwrap_or(".");
            let work_path = Path::new(work_dir);

            match IndexClient::new(&ctx.config.index, work_path).await {
                Ok(client) => Ok(Arc::new(client)),
                Err(e) => anyhow::bail!(
                    "Index not initialized. Run `index_codebase` first. Error: {}",
                    e
                ),
            }
        }
    }
}
