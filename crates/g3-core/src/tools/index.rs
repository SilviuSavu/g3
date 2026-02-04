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
    let client = match &ctx.index_client {
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
