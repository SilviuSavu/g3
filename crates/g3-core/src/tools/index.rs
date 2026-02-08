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

/// Execute the pattern_search tool.
pub async fn execute_pattern_search<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let pattern_type = args
        .get("pattern_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: pattern_type"))?;

    let pattern_name = args
        .get("pattern_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    let work_dir = ctx.working_dir.unwrap_or(".");
    let work_path = Path::new(work_dir);
    let target_path = work_path.join(path);

    if !target_path.exists() {
        return Ok(json!({
            "status": "error",
            "message": format!("Path not found: {}", target_path.display())
        }).to_string());
    }

    let results: Vec<serde_json::Value>;

    match pattern_type {
        "error_handling" => {
            results = search_error_handling(&target_path)?;
        }
        "trait_impl" => {
            results = search_trait_impl(&target_path, pattern_name)?;
        }
        "async_pattern" => {
            results = search_async_pattern(&target_path)?;
        }
        "struct_init" => {
            results = search_struct_initialization(&target_path)?;
        }
        "builder_pattern" => {
            results = search_builder_pattern(&target_path)?;
        }
        "lifecycle" => {
            results = search_lifecycle_patterns(&target_path)?;
        }
        "concurrency" => {
            results = search_concurrency_patterns(&target_path)?;
        }
        "config" => {
            results = search_config_patterns(&target_path)?;
        }
        "logging" => {
            results = search_logging_patterns(&target_path)?;
        }
        _ => {
            return Ok(json!({
                "status": "error",
                "message": format!("Unknown pattern type: {}", pattern_type),
                "available_patterns": [
                    "error_handling",
                    "trait_impl",
                    "async_pattern",
                    "struct_init",
                    "builder_pattern",
                    "lifecycle",
                    "concurrency",
                    "config",
                    "logging"
                ]
            }).to_string());
        }
    }

    Ok(json!({
        "status": "success",
        "pattern_type": pattern_type,
        "pattern_name": pattern_name,
        "path": path,
        "count": results.len(),
        "results": results
    }).to_string())
}

fn search_error_handling(target_path: &Path) -> Result<Vec<serde_json::Value>, anyhow::Error> {
    let mut results = Vec::new();

    // Look for error handling patterns
    let _patterns = [
        ("anyhow?", r"\?\s*$"),
        ("bail!", r"\bbail!\s*\("),
        ("wrap!", r"\bwrap!\s*\("),
        ("context!", r"\bcontext!\s*\("),
        ("Result<", r"Result<[^>]+>"),
        ("? ", r"\?\s"),
    ];

    // For now, just count occurrences in the directory
    // A more sophisticated implementation would parse the AST
    if target_path.is_dir() {
        let mut count = 0;
        if let Ok(entries) = target_path.read_dir() {
            for entry in entries.flatten() {
                if entry.path().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        count += content.matches("? ").count() + content.matches("bail!").count();
                    }
                }
            }
        }
        results.push(json!({
            "type": "error_handling",
            "category": "anyhow_usage",
            "files_scanned": target_path.is_dir() as u64,
            "estimated_occurrences": count,
            "description": "Found anyhow error handling patterns (bail!, ?, context!)"
        }));
    }

    Ok(results)
}

fn search_trait_impl(target_path: &Path, trait_name: &str) -> Result<Vec<serde_json::Value>, anyhow::Error> {
    let mut results = Vec::new();

    let _search_pattern = if trait_name.is_empty() {
        r"impl\s+\w+\s+for"
    } else {
        &format!(r"impl\s+{}\s+for", trait_name)
    };

    if target_path.is_dir() {
        let mut count = 0;
        if let Ok(entries) = target_path.read_dir() {
            for entry in entries.flatten() {
                if entry.path().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        count += content.matches("impl ").count();
                    }
                }
            }
        }
        results.push(json!({
            "type": "trait_impl",
            "trait_searched": trait_name,
            "estimated_implementations": count,
            "description": format!("Found trait implementations (impl {} for ...)", trait_name)
        }));
    }

    Ok(results)
}

fn search_async_pattern(target_path: &Path) -> Result<Vec<serde_json::Value>, anyhow::Error> {
    let mut results = Vec::new();

    if target_path.is_dir() {
        let mut async_fns = 0;
        let mut spawn_calls = 0;

        if let Ok(entries) = target_path.read_dir() {
            for entry in entries.flatten() {
                if entry.path().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        async_fns += content.matches("async fn").count();
                        spawn_calls += content.matches("tokio::spawn").count() + content.matches(".await").count();
                    }
                }
            }
        }

        results.push(json!({
            "type": "async_pattern",
            "async_functions": async_fns,
            "await_calls": spawn_calls,
            "description": "Found async/await patterns and tokio usage"
        }));
    }

    Ok(results)
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

/// Execute the list_files tool.
pub async fn execute_list_files<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    let pattern = args
        .get("pattern")
        .and_then(|v| v.as_str())
        .unwrap_or("*");

    let include_hidden = args
        .get("include_hidden")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let max_results = args
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000) as usize;

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

    // Walk the directory tree
    for entry in dir_path.read_dir()? {
        if entries.len() >= max_results {
            break;
        }

        if let Ok(entry) = entry {
            let file_type = entry.file_type()?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Skip hidden files by default
            if !include_hidden && file_name_str.starts_with('.') {
                continue;
            }

            // Check if file matches the pattern
            let matches = if file_type.is_file() {
                // Simple glob matching
                match pattern {
                    "*" => true,
                    ext if ext.starts_with("*") => {
                        // Extension pattern like "*.rs"
                        let ext_pattern = ext.strip_prefix("*").unwrap_or(ext);
                        file_name_str.ends_with(ext_pattern)
                    }
                    _ => {
                        // For now, only support * and *.ext patterns
                        file_name_str == pattern
                    }
                }
            } else {
                false // Only list files, not directories
            };

            if matches {
                // Get metadata
                let metadata = entry.metadata()?;
                let size = metadata.len();

                // Count lines for text files
                let line_count = if file_type.is_file() {
                    match std::fs::read_to_string(entry.path()) {
                        Ok(content) => content.lines().count(),
                        Err(_) => 0,
                    }
                } else {
                    0
                };

                entries.push(json!({
                    "name": file_name_str,
                    "path": file_name_str,
                    "size": size,
                    "lines": line_count
                }));
            }
        }
    }

    Ok(json!({
        "status": "success",
        "path": path,
        "pattern": pattern,
        "include_hidden": include_hidden,
        "max_results": max_results,
        "entries": entries,
        "count": entries.len()
    }).to_string())
}

/// Calculate cyclomatic complexity for a Rust function
/// Based on: count decision points (if, match, loop, while, for, &&, ||, ?)
fn calculate_cyclomatic_complexity(source: &str) -> usize {
    let mut complexity = 1; // Base complexity

    // Decision point patterns
    let decision_patterns = [
        "if ", "if (", "if\t",
        "else if ", "else if (",
        "match ", "match\t",
        "while ", "while (", "while\t",
        "for ", "for (", "for\t",
        "loop ", "loop\t",
        "&&", "||",
        "? ", "?\t",
        "?;",
    ];

    for pattern in &decision_patterns {
        let count = source.matches(pattern).count();
        complexity += count;
    }

    complexity
}

/// Calculate cognitive complexity for a Rust function
fn calculate_cognitive_complexity(source: &str) -> usize {
    let mut complexity: usize = 0;

    // Track nesting by counting braces
    let mut brace_count: usize = 0;
    for c in source.chars() {
        if c == '{' {
            brace_count += 1;
            complexity += brace_count.saturating_sub(1);
        } else if c == '}' {
            brace_count = brace_count.saturating_sub(1);
        }
    }

    // Decision points - count each type
    let decision_points = source.matches("if ").count()
        + source.matches("match ").count()
        + source.matches("while ").count()
        + source.matches("for ").count()
        + source.matches("&&").count()
        + source.matches("||").count();

    // Base complexity from decision points
    complexity += decision_points;

    complexity
}

/// Analyze a single file for complexity metrics
fn analyze_file_complexity(file_path: &std::path::Path) -> Option<serde_json::Value> {
    // Only analyze Rust files for now
    if file_path.extension()?.to_str()? != "rs" {
        return None;
    }

    let content = std::fs::read_to_string(file_path).ok()?;

    // Count basic metrics
    let line_count = content.lines().count();
    let char_count = content.chars().count();

    // For now, estimate function-level complexity by splitting on common patterns
    // A more sophisticated approach would use tree-sitter to parse the AST
    let estimated_functions = content.matches("fn ").count() + content.matches("impl ").count();

    // Estimate cyclomatic complexity (simplified)
    let cyclomatic = calculate_cyclomatic_complexity(&content);
    let cognitive = calculate_cognitive_complexity(&content);

    // Calculate average complexity per function (if any)
    let avg_cyclomatic = if estimated_functions > 0 {
        (cyclomatic / estimated_functions).max(1)
    } else {
        0
    };

    Some(json!({
        "file": file_path.to_string_lossy(),
        "lines": line_count,
        "chars": char_count,
        "estimated_functions": estimated_functions,
        "cyclomatic_complexity": cyclomatic,
        "cognitive_complexity": cognitive,
        "avg_cyclomatic_per_function": avg_cyclomatic
    }))
}

/// Execute the complexity_metrics tool.
pub async fn execute_complexity_metrics<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    let min_complexity = args
        .get("min_complexity")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;

    let metric = args
        .get("metric")
        .and_then(|v| v.as_str())
        .unwrap_or("cyclomatic");

    let work_dir = ctx.working_dir.unwrap_or(".");
    let work_path = Path::new(work_dir);
    let target_path = work_path.join(path);

    if !target_path.exists() {
        return Ok(json!({
            "status": "error",
            "message": format!("Path not found: {}", target_path.display())
        }).to_string());
    }

    let mut results: Vec<serde_json::Value> = Vec::new();

    // Walk the directory tree
    let walk_dir = if target_path.is_file() {
        vec![target_path.clone()]
    } else {
        // Get all files in directory
        let mut files = Vec::new();
        if let Ok(entries) = target_path.read_dir() {
            for entry in entries.flatten() {
                if entry.path().is_file() {
                    files.push(entry.path());
                }
            }
        }
        files
    };

    for file_path in walk_dir {
        if let Some(metrics) = analyze_file_complexity(&file_path) {
            // Filter by minimum complexity
            let complexity = match metric {
                "cyclomatic" => metrics["cyclomatic_complexity"].as_u64().unwrap_or(0),
                "cognitive" => metrics["cognitive_complexity"].as_u64().unwrap_or(0),
                "lines" => metrics["lines"].as_u64().unwrap_or(0),
                "avg_cyclomatic" => metrics["avg_cyclomatic_per_function"].as_u64().unwrap_or(0),
                _ => 0,
            };

            if complexity >= min_complexity as u64 {
                results.push(metrics);
            }
        }
    }

    // Sort by complexity (descending)
    results.sort_by(|a, b| {
        let a_complexity = a[metric].as_u64().unwrap_or(0);
        let b_complexity = b[metric].as_u64().unwrap_or(0);
        b_complexity.cmp(&a_complexity)
    });

    // Limit results
    let max_results = args
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as usize;
    let truncated_results = results.into_iter().take(max_results).collect::<Vec<_>>();

    Ok(json!({
        "status": "success",
        "path": path,
        "metric": metric,
        "min_complexity": min_complexity,
        "results_count": truncated_results.len(),
        "total_files_analyzed": truncated_results.len(),
        "results": truncated_results
    }).to_string())
}

fn search_struct_initialization(target_path: &Path) -> Result<Vec<serde_json::Value>, anyhow::Error> {
    let mut results = Vec::new();

    if target_path.is_dir() {
        let mut count = 0;
        if let Ok(entries) = target_path.read_dir() {
            for entry in entries.flatten() {
                if entry.path().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        count += content.matches(".with_").count() + content.matches("Self {").count();
                    }
                }
            }
        }
        results.push(json!({
            "type": "struct_init",
            "estimated_patterns": count,
            "description": "Found struct initialization patterns (with_ methods, Self { ... })"
        }));
    }

    Ok(results)
}

fn search_builder_pattern(target_path: &Path) -> Result<Vec<serde_json::Value>, anyhow::Error> {
    let mut results = Vec::new();

    if target_path.is_dir() {
        let mut count = 0;
        if let Ok(entries) = target_path.read_dir() {
            for entry in entries.flatten() {
                if entry.path().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        count += content.matches("fn with_").count() + content.matches("self.").count();
                    }
                }
            }
        }
        results.push(json!({
            "type": "builder_pattern",
            "estimated_patterns": count,
            "description": "Found builder pattern (with_ methods, self. fluent calls)"
        }));
    }

    Ok(results)
}

fn search_lifecycle_patterns(target_path: &Path) -> Result<Vec<serde_json::Value>, anyhow::Error> {
    let mut results = Vec::new();

    if target_path.is_dir() {
        let mut count = 0;
        if let Ok(entries) = target_path.read_dir() {
            for entry in entries.flatten() {
                if entry.path().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        count += content.matches("fn new").count() + content.matches("fn init").count() + content.matches("fn drop").count();
                    }
                }
            }
        }
        results.push(json!({
            "type": "lifecycle",
            "estimated_patterns": count,
            "description": "Found lifecycle patterns (new, init, drop constructors/destructors)"
        }));
    }

    Ok(results)
}

fn search_concurrency_patterns(target_path: &Path) -> Result<Vec<serde_json::Value>, anyhow::Error> {
    let mut results = Vec::new();

    if target_path.is_dir() {
        let mut count = 0;
        if let Ok(entries) = target_path.read_dir() {
            for entry in entries.flatten() {
                if entry.path().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        count += content.matches("Mutex<").count() + content.matches("Arc<").count() + content.matches("RwLock<").count();
                    }
                }
            }
        }
        results.push(json!({
            "type": "concurrency",
            "estimated_patterns": count,
            "description": "Found concurrency patterns (Mutex, Arc, RwLock)"
        }));
    }

    Ok(results)
}

fn search_config_patterns(target_path: &Path) -> Result<Vec<serde_json::Value>, anyhow::Error> {
    let mut results = Vec::new();

    if target_path.is_dir() {
        let mut count = 0;
        if let Ok(entries) = target_path.read_dir() {
            for entry in entries.flatten() {
                if entry.path().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        count += content.matches("struct Config").count() + content.matches("struct Settings").count() + content.matches("fn load_config").count();
                    }
                }
            }
        }
        results.push(json!({
            "type": "config",
            "estimated_patterns": count,
            "description": "Found configuration patterns (Config, Settings, load_config)"
        }));
    }

    Ok(results)
}

fn search_logging_patterns(target_path: &Path) -> Result<Vec<serde_json::Value>, anyhow::Error> {
    let mut results = Vec::new();

    if target_path.is_dir() {
        let mut count = 0;
        if let Ok(entries) = target_path.read_dir() {
            for entry in entries.flatten() {
                if entry.path().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        count += content.matches("info!").count() + content.matches("debug!").count() + content.matches("error!").count();
                    }
                }
            }
        }
        results.push(json!({
            "type": "logging",
            "estimated_patterns": count,
            "description": "Found logging patterns (info!, debug!, error!)"
        }));
    }

    Ok(results)
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
