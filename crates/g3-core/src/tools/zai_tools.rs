//! Z.ai standalone tools implementation.
//!
//! This module provides handlers for Z.ai's standalone tool APIs:
//! - Web Search
//! - Web Reader
//! - OCR (Layout Parsing)

use anyhow::{anyhow, Result};
use g3_providers::{ZaiLayoutParsingRequest, ZaiWebReaderRequest, ZaiWebSearchRequest};
use tracing::debug;

use super::executor::ToolContext;
use crate::ui_writer::UiWriter;
use crate::ToolCall;

/// Execute the zai_web_search tool.
pub async fn execute_web_search<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let client = ctx
        .zai_tools_client
        .as_ref()
        .ok_or_else(|| anyhow!("Z.ai tools client not configured. Enable zai_tools in config."))?;

    let query = tool_call
        .args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: query"))?;

    let count = tool_call
        .args
        .get("count")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    let search_domain_filter = tool_call
        .args
        .get("search_domain_filter")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

    let search_recency_filter = tool_call
        .args
        .get("search_recency_filter")
        .and_then(|v| v.as_str())
        .map(String::from);

    debug!("Executing Z.ai web search for: {}", query);

    let request = ZaiWebSearchRequest {
        query: query.to_string(),
        count,
        search_domain_filter,
        search_recency_filter,
    };

    let response = client.web_search(request).await?;

    // Format results for display
    let mut output = format!("## Web Search Results for: {}\n\n", query);
    for (i, result) in response.search_result.iter().enumerate() {
        output.push_str(&format!(
            "### {}. {}\n**URL:** {}\n\n{}\n\n",
            i + 1,
            result.title,
            result.link,
            result.content
        ));
    }

    if response.search_result.is_empty() {
        output.push_str("No results found.\n");
    }

    Ok(output)
}

/// Execute the zai_web_reader tool.
pub async fn execute_web_reader<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let client = ctx
        .zai_tools_client
        .as_ref()
        .ok_or_else(|| anyhow!("Z.ai tools client not configured. Enable zai_tools in config."))?;

    let url = tool_call
        .args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: url"))?;

    let format = tool_call
        .args
        .get("format")
        .and_then(|v| v.as_str())
        .map(String::from);

    let retain_images = tool_call
        .args
        .get("retain_images")
        .and_then(|v| v.as_bool());

    let timeout = tool_call
        .args
        .get("timeout")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    debug!("Executing Z.ai web reader for: {}", url);

    let request = ZaiWebReaderRequest {
        url: url.to_string(),
        format,
        retain_images,
        timeout,
    };

    let response = client.web_reader(request).await?;

    // Format output with optional title
    let mut output = String::new();
    if let Some(title) = &response.title {
        output.push_str(&format!("# {}\n\n", title));
    }
    output.push_str(&response.content);

    Ok(output)
}

/// Execute the zai_ocr tool (layout parsing).
pub async fn execute_ocr<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let client = ctx
        .zai_tools_client
        .as_ref()
        .ok_or_else(|| anyhow!("Z.ai tools client not configured. Enable zai_tools in config."))?;

    let file = tool_call
        .args
        .get("file")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: file"))?;

    let page_range = tool_call
        .args
        .get("page_range")
        .and_then(|v| v.as_str())
        .map(String::from);

    debug!("Executing Z.ai OCR for file: {}", file);

    let request = ZaiLayoutParsingRequest {
        model: "glm-ocr".to_string(),
        file: file.to_string(),
        page_range,
    };

    let response = client.layout_parsing(request).await?;

    // Format output
    let mut output = String::from("## OCR Results\n\n");
    output.push_str(&response.content);

    if let Some(usage) = &response.usage {
        output.push_str(&format!(
            "\n\n---\n*Tokens used: {} prompt, {} completion, {} total*",
            usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
        ));
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_module_compiles() {
        // Basic compilation test
        // The actual functionality requires a Z.ai API key and network access
    }
}
