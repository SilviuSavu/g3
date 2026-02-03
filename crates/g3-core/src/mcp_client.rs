//! MCP (Model Context Protocol) HTTP client for connecting to remote MCP servers.
//!
//! This module provides a client for Z.ai's remote HTTP-based MCP servers
//! which provide access to tools like web search, web reader, and vision capabilities.

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tracing::debug;

/// MCP HTTP client for connecting to remote MCP servers.
#[derive(Clone)]
pub struct McpHttpClient {
    endpoint: String,
    api_key: String,
    client: Client,
}

impl McpHttpClient {
    /// Create a new MCP HTTP client.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The MCP server endpoint URL
    /// * `api_key` - API key for authentication
    pub fn new(endpoint: &str, api_key: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        debug!("Initialized MCP client for endpoint: {}", endpoint);

        Self {
            endpoint: endpoint.to_string(),
            api_key: api_key.to_string(),
            client,
        }
    }

    /// List available tools from the MCP server.
    pub async fn list_tools(&self) -> Result<Vec<McpToolDefinition>> {
        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "tools/list".to_string(),
            params: None,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to list MCP tools: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("MCP list_tools error {}: {}", status, error_text));
        }

        let result: McpResponse<McpToolsListResult> = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse MCP tools list: {}", e))?;

        Ok(result.result.tools)
    }

    /// Call a tool on the MCP server.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the tool to call
    /// * `args` - The arguments to pass to the tool
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<McpToolResult> {
        debug!("Calling MCP tool: {} with args: {:?}", name, args);

        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": name,
                "arguments": args
            })),
        };

        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to call MCP tool '{}': {}", name, e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("MCP tool call error {}: {}", status, error_text));
        }

        let result: McpResponse<McpToolResult> = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse MCP tool result: {}", e))?;

        Ok(result.result)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MCP Protocol Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct McpRequest {
    jsonrpc: String,
    id: u32,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct McpResponse<T> {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u32,
    result: T,
}

#[derive(Debug, Clone, Deserialize)]
struct McpToolsListResult {
    tools: Vec<McpToolDefinition>,
}

/// MCP tool definition returned by tools/list.
#[derive(Debug, Clone, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub input_schema: Option<Value>,
}

/// Result from calling an MCP tool.
#[derive(Debug, Clone, Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    #[serde(default)]
    pub is_error: bool,
}

/// Content item in an MCP tool result.
#[derive(Debug, Clone, Deserialize)]
pub struct McpContent {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
}

impl McpToolResult {
    /// Get the text content from the result.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| c.text.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_client_creation() {
        let client = McpHttpClient::new(
            "https://api.z.ai/api/mcp/web_search_prime/mcp",
            "test-api-key",
        );
        assert_eq!(
            client.endpoint,
            "https://api.z.ai/api/mcp/web_search_prime/mcp"
        );
    }

    #[test]
    fn test_mcp_tool_result_text() {
        let result = McpToolResult {
            content: vec![
                McpContent {
                    content_type: "text".to_string(),
                    text: Some("Hello".to_string()),
                    data: None,
                    mime_type: None,
                },
                McpContent {
                    content_type: "text".to_string(),
                    text: Some("World".to_string()),
                    data: None,
                    mime_type: None,
                },
            ],
            is_error: false,
        };
        assert_eq!(result.text(), "Hello\nWorld");
    }
}
