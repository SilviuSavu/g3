//! Index tools for codebase semantic search.
//!
//! These tools allow the agent to index the codebase and perform
//! semantic code searches using vector embeddings.

use anyhow::Result;
use serde_json::json;
use tracing::{debug, info};

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
        return Ok(
            "Indexing is not enabled. Set `index.enabled = true` in your config.".to_string(),
        );
    }

    let work_dir = path
        .as_deref()
        .or(ctx.working_dir)
        .unwrap_or(".");

    info!("Indexing codebase at: {} (force={})", work_dir, force);

    // For now, return a placeholder since the full integration requires
    // setting up the Qdrant client and embeddings provider
    let result = json!({
        "status": "pending",
        "message": format!(
            "Indexing {} {}. This feature requires a running Qdrant instance and configured embedding provider.",
            work_dir,
            if force { "(force rebuild)" } else { "" }
        ),
        "config": {
            "qdrant_url": ctx.config.index.qdrant_url,
            "collection": ctx.config.index.collection_name,
            "embedding_provider": ctx.config.index.embeddings.provider,
            "embedding_model": ctx.config.index.embeddings.model,
        }
    });

    Ok(serde_json::to_string_pretty(&result)?)
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
        .and_then(|v| v.as_str())
        .map(String::from);

    // Check if indexing is enabled
    if !ctx.config.index.enabled {
        return Ok("Semantic search requires indexing to be enabled. Set `index.enabled = true` in your config and run `index_codebase` first.".to_string());
    }

    debug!(
        "Semantic search: query='{}', limit={}, filter={:?}",
        query, limit, file_filter
    );

    // For now, return a placeholder since the full integration requires
    // setting up the Qdrant client and embeddings provider
    let result = json!({
        "status": "pending",
        "message": format!(
            "Searching for: '{}' (limit={}, filter={:?}). This feature requires a running Qdrant instance with an indexed codebase.",
            query, limit, file_filter
        ),
        "query": query,
        "limit": limit,
        "file_filter": file_filter,
    });

    Ok(serde_json::to_string_pretty(&result)?)
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
            "message": "Indexing is not enabled. Set `index.enabled = true` in your config."
        })
        .to_string());
    }

    let result = json!({
        "enabled": true,
        "config": {
            "qdrant_url": ctx.config.index.qdrant_url,
            "collection": ctx.config.index.collection_name,
            "embedding_provider": ctx.config.index.embeddings.provider,
            "embedding_model": ctx.config.index.embeddings.model,
            "dimensions": ctx.config.index.embeddings.dimensions,
            "hybrid_search": ctx.config.index.search.hybrid,
            "bm25_weight": ctx.config.index.search.bm25_weight,
            "vector_weight": ctx.config.index.search.vector_weight,
        },
        "status": "not_connected",
        "message": "Index status requires connecting to Qdrant. Use `index_codebase` to initialize the index."
    });

    Ok(serde_json::to_string_pretty(&result)?)
}
