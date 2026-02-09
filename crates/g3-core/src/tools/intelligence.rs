//! Code intelligence tools for autonomous agents.
//!
//! These tools provide comprehensive codebase intelligence:
//! - Find symbol definitions
//! - Find all references/uses of a symbol
//! - Find callers and callees of a function
//! - Search for similar code patterns
//! - Explore the dependency graph

use anyhow::Result;
use serde_json::json;
use tracing::info;

use crate::tools::executor::ToolContext;
use crate::ui_writer::UiWriter;
use crate::ToolCall;

/// Execute the code_intelligence tool.
/// This is a multipurpose tool with subcommands for various intelligence operations.
pub async fn execute_code_intelligence<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let args = &tool_call.args;

    // Get the subcommand
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("find");

    let symbol = args
        .get("symbol")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let depth = args
        .get("depth")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(2);

    match command {
        "find" => execute_find_definition(tool_call, ctx, symbol).await,
        "refs" => execute_find_references(tool_call, ctx, symbol).await,
        "callers" => execute_find_callers(tool_call, ctx, symbol, depth).await,
        "callees" => execute_find_callees(tool_call, ctx, symbol, depth).await,
        "similar" => execute_find_similar(tool_call, ctx, symbol).await,
        "graph" => execute_explore_graph(tool_call, ctx, symbol, depth).await,
        "query" => execute_graph_query(tool_call, ctx, symbol, depth).await,
        _ => Ok(json!({
            "status": "error",
            "message": format!("Unknown command: {}", command),
            "available_commands": ["find", "refs", "callers", "callees", "similar", "graph", "query"]
        }).to_string()),
    }
}

/// Execute the find_definition subcommand.
/// Finds the definition of a symbol using indexed search.
async fn execute_find_definition<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
    symbol: &str,
) -> Result<String> {
    if symbol.is_empty() {
        return Ok(json!({
            "status": "error",
            "message": "Missing symbol parameter"
        }).to_string());
    }

    info!("Finding definition for: {}", symbol);

    match &ctx.index_client {
        Some(client) => {
            match client.find_symbols_by_name(symbol).await {
                Ok(results) => {
                    let formatted: Vec<_> = results
                        .into_iter()
                        .map(|s| json!({
                            "id": s.id,
                            "name": s.name,
                            "kind": s.kind,
                            "file_id": s.file_id,
                            "line_start": s.line_start,
                            "line_end": s.line_end
                        }))
                        .collect();
                    Ok(json!({
                        "status": "success",
                        "source": "indexed",
                        "results": formatted,
                        "count": formatted.len()
                    }).to_string())
                }
                Err(e) => Ok(json!({
                    "status": "error",
                    "message": format!("Indexed search failed: {}", e)
                }).to_string()),
            }
        }
        None => Ok(json!({
            "status": "error",
            "message": "No index client available. Run index_codebase first."
        }).to_string()),
    }
}

/// Execute the find_references subcommand.
/// Finds all references/uses of a symbol.
async fn execute_find_references<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
    symbol: &str,
) -> Result<String> {
    if symbol.is_empty() {
        return Ok(json!({
            "status": "error",
            "message": "Missing symbol parameter"
        }).to_string());
    }

    info!("Finding references for: {}", symbol);

    match &ctx.index_client {
        Some(client) => {
            match client.find_references(symbol).await {
                Ok(results) => {
                    let formatted: Vec<_> = results
                        .into_iter()
                        .map(|r| json!({
                            "source": r.source,
                            "target": r.target,
                            "file": r.file,
                            "line": r.line
                        }))
                        .collect();
                    Ok(json!({
                        "status": "success",
                        "source": "indexed",
                        "results": formatted,
                        "count": formatted.len()
                    }).to_string())
                }
                Err(e) => Ok(json!({
                    "status": "error",
                    "message": format!("Indexed search failed: {}", e)
                }).to_string()),
            }
        }
        None => Ok(json!({
            "status": "error",
            "message": "No index client available. Run index_codebase first."
        }).to_string()),
    }
}

/// Execute the find_callers subcommand.
/// Finds functions that call the given symbol.
async fn execute_find_callers<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
    symbol: &str,
    depth: usize,
) -> Result<String> {
    if symbol.is_empty() {
        return Ok(json!({
            "status": "error",
            "message": "Missing symbol parameter"
        }).to_string());
    }

    info!("Finding callers for: {} (depth={})", symbol, depth);

    match &ctx.index_client {
        Some(client) => {
            match client.find_symbols_by_name(symbol).await {
                Ok(symbols) => {
                    let mut all_callers = Vec::new();

                    for sym in symbols.iter().take(5) {
                        match client.find_callers(&sym.id).await {
                            Ok(callers) => {
                                for caller_id in callers {
                                    all_callers.push(json!({
                                        "caller_id": caller_id,
                                        "depth": 1
                                    }));
                                }
                            }
                            Err(e) => {
                                return Ok(json!({
                                    "status": "error",
                                    "message": format!("Failed to find callers: {}", e)
                                }).to_string());
                            }
                        }
                    }

                    Ok(json!({
                        "status": "success",
                        "source": "graph",
                        "symbol": symbol,
                        "callers": all_callers,
                        "count": all_callers.len()
                    }).to_string())
                }
                Err(e) => Ok(json!({
                    "status": "error",
                    "message": format!("Failed to find symbol: {}", e)
                }).to_string()),
            }
        }
        None => Ok(json!({
            "status": "error",
            "message": "No index client available. Run index_codebase first."
        }).to_string()),
    }
}

/// Execute the find_callees subcommand.
/// Finds functions called by the given symbol.
async fn execute_find_callees<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
    symbol: &str,
    depth: usize,
) -> Result<String> {
    if symbol.is_empty() {
        return Ok(json!({
            "status": "error",
            "message": "Missing symbol parameter"
        }).to_string());
    }

    info!("Finding callees for: {} (depth={})", symbol, depth);

    match &ctx.index_client {
        Some(client) => {
            match client.find_symbols_by_name(symbol).await {
                Ok(symbols) => {
                    let mut all_callees = Vec::new();

                    for sym in symbols.iter().take(5) {
                        match client.find_callees(&sym.id).await {
                            Ok(callees) => {
                                for callee_id in callees {
                                    all_callees.push(json!({
                                        "callee_id": callee_id,
                                        "depth": 1
                                    }));
                                }
                            }
                            Err(e) => {
                                return Ok(json!({
                                    "status": "error",
                                    "message": format!("Failed to find callees: {}", e)
                                }).to_string());
                            }
                        }
                    }

                    Ok(json!({
                        "status": "success",
                        "source": "graph",
                        "symbol": symbol,
                        "callees": all_callees,
                        "count": all_callees.len()
                    }).to_string())
                }
                Err(e) => Ok(json!({
                    "status": "error",
                    "message": format!("Failed to find symbol: {}", e)
                }).to_string()),
            }
        }
        None => Ok(json!({
            "status": "error",
            "message": "No index client available. Run index_codebase first."
        }).to_string()),
    }
}

/// Execute the find_similar subcommand.
/// Finds similar code patterns using semantic search.
async fn execute_find_similar<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
    query: &str,
) -> Result<String> {
    if query.is_empty() {
        return Ok(json!({
            "status": "error",
            "message": "Missing query parameter"
        }).to_string());
    }

    info!("Finding similar code for: {}", query);

    match &ctx.index_client {
        Some(client) => {
            // Use semantic search via the search method (limit 10, no file filter)
            match client.search(query, 10, None).await {
                Ok(results) => {
                    let formatted: Vec<_> = results
                        .into_iter()
                        .map(|r| json!({
                            "id": r.id,
                            "file_path": r.file_path,
                            "start_line": r.start_line,
                            "end_line": r.end_line,
                            "content": r.content,
                            "kind": r.kind,
                            "name": r.name,
                            "signature": r.signature,
                            "score": r.score
                        }))
                        .collect();

                    Ok(json!({
                        "status": "success",
                        "source": "semantic",
                        "query": query,
                        "results": formatted,
                        "count": formatted.len()
                    }).to_string())
                }
                Err(e) => Ok(json!({
                    "status": "error",
                    "message": format!("Semantic search failed: {}", e)
                }).to_string()),
            }
        }
        None => Ok(json!({
            "status": "error",
            "message": "No index client available. Run index_codebase first."
        }).to_string()),
    }
}

/// Execute the explore_graph subcommand.
/// Explores the dependency graph starting from a symbol.
async fn execute_explore_graph<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
    symbol: &str,
    depth: usize,
) -> Result<String> {
    if symbol.is_empty() {
        return Ok(json!({
            "status": "error",
            "message": "Missing symbol parameter"
        }).to_string());
    }

    info!("Exploring graph for: {} (depth={})", symbol, depth);

    match &ctx.index_client {
        Some(client) => {
            match client.find_symbols_by_name(symbol).await {
                Ok(symbols) => {
                    let mut traversal_results = Vec::new();

                    for sym in symbols.iter().take(3) {
                        // Get callers (parents in call graph)
                        match client.find_callers(&sym.id).await {
                            Ok(callers) => {
                                for caller_id in callers.iter().take(10) {
                                    traversal_results.push(json!({
                                        "node_id": caller_id,
                                        "type": "caller",
                                        "relation": "calls"
                                    }));
                                }
                            }
                            Err(_) => {}
                        }

                        // Get references to this symbol
                        match client.find_references(&sym.id).await {
                            Ok(refs) => {
                                for r in refs.iter().take(10) {
                                    traversal_results.push(json!({
                                        "node_id": r.source,
                                        "type": "reference",
                                        "line": r.line
                                    }));
                                }
                            }
                            Err(_) => {}
                        }
                    }

                    Ok(json!({
                        "status": "success",
                        "source": "graph",
                        "symbol": symbol,
                        "depth": depth,
                        "traversal": traversal_results,
                        "count": traversal_results.len()
                    }).to_string())
                }
                Err(e) => Ok(json!({
                    "status": "error",
                    "message": format!("Failed to find symbol: {}", e)
                }).to_string()),
            }
        }
        None => Ok(json!({
            "status": "error",
            "message": "No index client available. Run index_codebase first."
        }).to_string()),
    }
}

/// Execute the graph_query subcommand.
/// Queries the knowledge graph with various query types.
async fn execute_graph_query<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
    symbol: &str,
    depth: usize,
) -> Result<String> {
    if symbol.is_empty() {
        return Ok(json!({
            "status": "error",
            "message": "Missing symbol parameter"
        }).to_string());
    }

    info!("Graph query for: {} (depth={})", symbol, depth);

    match &ctx.index_client {
        Some(client) => {
            match client.find_symbols_by_name(symbol).await {
                Ok(symbols) => {
                    let mut callers_list = Vec::new();
                    let mut references_list = Vec::new();

                    for sym in symbols.iter().take(3) {
                        // Get callers
                        match client.find_callers(&sym.id).await {
                            Ok(callers) => {
                                for caller_id in callers.iter().take(10) {
                                    callers_list.push(json!({
                                        "id": caller_id
                                    }));
                                }
                            }
                            Err(_) => {}
                        }

                        // Get references
                        match client.find_references(&sym.id).await {
                            Ok(refs) => {
                                for r in refs.iter().take(10) {
                                    references_list.push(json!({
                                        "file": r.file,
                                        "line": r.line
                                    }));
                                }
                            }
                            Err(_) => {}
                        }
                    }

                    Ok(json!({
                        "status": "success",
                        "source": "graph",
                        "symbol": symbol,
                        "callers": callers_list,
                        "references": references_list
                    }).to_string())
                }
                Err(e) => Ok(json!({
                    "status": "error",
                    "message": format!("Failed to find symbol: {}", e)
                }).to_string()),
            }
        }
        None => Ok(json!({
            "status": "error",
            "message": "No index client available. Run index_codebase first."
        }).to_string()),
    }
}
