//! MCP (Model Context Protocol) tool handlers.
//!
//! These tools provide access to Z.ai MCP servers for web search, web reading,
//! and GitHub repository access.

use anyhow::{anyhow, Result};
use serde_json::json;
use tracing::debug;

use crate::mcp_client::McpHttpClient;
use crate::tools::executor::ToolContext;
use crate::ui_writer::UiWriter;
use crate::ToolCall;

/// Collection of MCP clients for different Z.ai MCP servers.
pub struct McpClients {
    /// Web search MCP client (webSearchPrime)
    pub web_search: Option<McpHttpClient>,
    /// Web reader MCP client (webReader)
    pub web_reader: Option<McpHttpClient>,
    /// GitHub repository access MCP client (zread)
    pub zread: Option<McpHttpClient>,
}

impl McpClients {
    /// Check if any MCP clients are configured.
    pub fn has_any(&self) -> bool {
        self.web_search.is_some() || self.web_reader.is_some() || self.zread.is_some()
    }
}

/// Execute MCP web search using webSearchPrime.
pub async fn execute_mcp_web_search<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let clients = ctx
        .mcp_clients
        .as_ref()
        .ok_or_else(|| anyhow!("MCP clients not configured"))?;
    let client = clients
        .web_search
        .as_ref()
        .ok_or_else(|| anyhow!("MCP web search not enabled. Set zai_mcp.web_search.enabled = true"))?;

    let search_query = tool_call
        .args
        .get("search_query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: search_query"))?;

    debug!("Executing MCP web search for: {}", search_query);

    // Build arguments, including optional parameters
    let mut args = json!({
        "search_query": search_query,
    });

    if let Some(content_size) = tool_call.args.get("content_size").and_then(|v| v.as_str()) {
        args["content_size"] = json!(content_size);
    }
    if let Some(location) = tool_call.args.get("location").and_then(|v| v.as_str()) {
        args["location"] = json!(location);
    }
    if let Some(filter) = tool_call
        .args
        .get("search_recency_filter")
        .and_then(|v| v.as_str())
    {
        args["search_recency_filter"] = json!(filter);
    }
    if let Some(domain_filter) = tool_call
        .args
        .get("search_domain_filter")
        .and_then(|v| v.as_str())
    {
        args["search_domain_filter"] = json!(domain_filter);
    }

    let result = client.call_tool("webSearchPrime", args).await?;

    if result.is_error {
        return Err(anyhow!("MCP web search error: {}", result.text()));
    }

    Ok(result.text())
}

/// Execute MCP web reader using webReader.
pub async fn execute_mcp_web_reader<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let clients = ctx
        .mcp_clients
        .as_ref()
        .ok_or_else(|| anyhow!("MCP clients not configured"))?;
    let client = clients
        .web_reader
        .as_ref()
        .ok_or_else(|| anyhow!("MCP web reader not enabled. Set zai_mcp.web_reader.enabled = true"))?;

    let url = tool_call
        .args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: url"))?;

    debug!("Executing MCP web reader for: {}", url);

    // Build arguments, including optional parameters
    let mut args = json!({
        "url": url,
    });

    if let Some(return_format) = tool_call.args.get("return_format").and_then(|v| v.as_str()) {
        args["return_format"] = json!(return_format);
    }
    if let Some(retain_images) = tool_call.args.get("retain_images").and_then(|v| v.as_bool()) {
        args["retain_images"] = json!(retain_images);
    }
    if let Some(timeout) = tool_call.args.get("timeout").and_then(|v| v.as_i64()) {
        args["timeout"] = json!(timeout);
    }
    if let Some(no_cache) = tool_call.args.get("no_cache").and_then(|v| v.as_bool()) {
        args["no_cache"] = json!(no_cache);
    }
    if let Some(with_links_summary) = tool_call
        .args
        .get("with_links_summary")
        .and_then(|v| v.as_bool())
    {
        args["with_links_summary"] = json!(with_links_summary);
    }
    if let Some(with_images_summary) = tool_call
        .args
        .get("with_images_summary")
        .and_then(|v| v.as_bool())
    {
        args["with_images_summary"] = json!(with_images_summary);
    }

    let result = client.call_tool("webReader", args).await?;

    if result.is_error {
        return Err(anyhow!("MCP web reader error: {}", result.text()));
    }

    Ok(result.text())
}

/// Execute MCP search_doc for GitHub repository documentation search.
pub async fn execute_mcp_search_doc<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let clients = ctx
        .mcp_clients
        .as_ref()
        .ok_or_else(|| anyhow!("MCP clients not configured"))?;
    let client = clients
        .zread
        .as_ref()
        .ok_or_else(|| anyhow!("MCP zread not enabled. Set zai_mcp.zread.enabled = true"))?;

    let repo_name = tool_call
        .args
        .get("repo_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: repo_name"))?;

    let query = tool_call
        .args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: query"))?;

    debug!(
        "Executing MCP search_doc for repo: {}, query: {}",
        repo_name, query
    );

    // Build arguments, including optional parameters
    let mut args = json!({
        "repo_name": repo_name,
        "query": query,
    });

    if let Some(language) = tool_call.args.get("language").and_then(|v| v.as_str()) {
        args["language"] = json!(language);
    }

    let result = client.call_tool("search_doc", args).await?;

    if result.is_error {
        return Err(anyhow!("MCP search_doc error: {}", result.text()));
    }

    Ok(result.text())
}

/// Execute MCP get_repo_structure for GitHub repository structure.
pub async fn execute_mcp_get_repo_structure<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let clients = ctx
        .mcp_clients
        .as_ref()
        .ok_or_else(|| anyhow!("MCP clients not configured"))?;
    let client = clients
        .zread
        .as_ref()
        .ok_or_else(|| anyhow!("MCP zread not enabled. Set zai_mcp.zread.enabled = true"))?;

    let repo_name = tool_call
        .args
        .get("repo_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: repo_name"))?;

    debug!("Executing MCP get_repo_structure for repo: {}", repo_name);

    // Build arguments, including optional parameters
    let mut args = json!({
        "repo_name": repo_name,
    });

    if let Some(dir_path) = tool_call.args.get("dir_path").and_then(|v| v.as_str()) {
        args["dir_path"] = json!(dir_path);
    }

    let result = client.call_tool("get_repo_structure", args).await?;

    if result.is_error {
        return Err(anyhow!("MCP get_repo_structure error: {}", result.text()));
    }

    Ok(result.text())
}

/// Execute MCP read_file for reading a file from a GitHub repository.
pub async fn execute_mcp_read_file<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let clients = ctx
        .mcp_clients
        .as_ref()
        .ok_or_else(|| anyhow!("MCP clients not configured"))?;
    let client = clients
        .zread
        .as_ref()
        .ok_or_else(|| anyhow!("MCP zread not enabled. Set zai_mcp.zread.enabled = true"))?;

    let repo_name = tool_call
        .args
        .get("repo_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: repo_name"))?;

    let file_path = tool_call
        .args
        .get("file_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: file_path"))?;

    debug!(
        "Executing MCP read_file for repo: {}, path: {}",
        repo_name, file_path
    );

    let args = json!({
        "repo_name": repo_name,
        "file_path": file_path,
    });

    let result = client.call_tool("read_file", args).await?;

    if result.is_error {
        return Err(anyhow!("MCP read_file error: {}", result.text()));
    }

    Ok(result.text())
}
