//! Z.ai (Zhipu AI) provider implementation for the g3-providers crate.
//!
//! This module provides an implementation of the `LLMProvider` trait for Z.ai's GLM models,
//! supporting both completion and streaming modes through an OpenAI-compatible API with
//! Z.ai-specific extensions.
//!
//! # Features
//!
//! - Support for GLM models (glm-4.7, etc.)
//! - Both completion and streaming response modes
//! - Native tool calling support
//! - Thinking/reasoning mode with `reasoning_content` support
//! - Preserved thinking across conversation turns
//! - Regional endpoint support (International and China)
//! - Standalone API tools (web search, web reader, OCR)
//!
//! # Endpoints
//!
//! - International: `https://api.z.ai/api/paas/v4/chat/completions`
//! - China: `https://open.bigmodel.cn/api/paas/v4/chat/completions`
//!
//! # Standalone Tool API Endpoints
//!
//! - Web Search: `POST /web_search`
//! - Web Reader: `POST /reader`
//! - Layout Parsing (OCR): `POST /layout_parsing`
//!
//! # Usage
//!
//! ```rust,no_run
//! use g3_providers::{ZaiProvider, LLMProvider, CompletionRequest, Message, MessageRole};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let provider = ZaiProvider::new(
//!         "your-api-key".to_string(),
//!         Some("glm-4.7".to_string()),
//!         None, // base_url (defaults to international)
//!         Some(4096),
//!         Some(0.7),
//!         true,  // enable_thinking
//!         false, // preserve_thinking
//!     )?;
//!
//!     let request = CompletionRequest {
//!         messages: vec![
//!             Message::new(MessageRole::System, "You are a helpful assistant.".to_string()),
//!             Message::new(MessageRole::User, "Hello!".to_string()),
//!         ],
//!         max_tokens: Some(1000),
//!         temperature: Some(0.7),
//!         stream: false,
//!         tools: None,
//!         disable_thinking: false,
//!         stop_sequences: vec![],
//!     };
//!
//!     let response = provider.complete(request).await?;
//!     println!("Response: {}", response.content);
//!
//!     Ok(())
//! }
//! ```

use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures_util::stream::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error};

use crate::{
    streaming::{decode_utf8_streaming, make_final_chunk_full, make_text_chunk, make_tool_chunk},
    CompletionChunk, CompletionRequest, CompletionResponse, CompletionStream, LLMProvider, Message,
    MessageRole, Tool, ToolCall, Usage,
};

/// Default base URL for Z.ai international API
const ZAI_DEFAULT_BASE_URL: &str = "https://api.z.ai/api/paas/v4";

/// Default model for Z.ai
const ZAI_DEFAULT_MODEL: &str = "glm-4.7";

/// Z.ai provider implementation
#[derive(Clone)]
pub struct ZaiProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    name: String,
    enable_thinking: bool,
    preserve_thinking: bool,
    /// Enable web search tool injection in chat completions
    enable_web_search_in_chat: bool,
    /// Search engine to use: "bing" or "google" (default: "bing")
    web_search_engine: Option<String>,
    /// Number of search results (1-50, default: 10)
    web_search_count: Option<u32>,
    /// Recency filter: "day", "week", "month", "year"
    web_search_recency: Option<String>,
    /// Content size: "medium" or "high" (default: "medium")
    web_search_content_size: Option<String>,
}

impl ZaiProvider {
    /// Create a new ZaiProvider with the given configuration.
    pub fn new(
        api_key: String,
        model: Option<String>,
        base_url: Option<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        enable_thinking: bool,
        preserve_thinking: bool,
    ) -> Result<Self> {
        Self::new_with_name(
            "zai".to_string(),
            api_key,
            model,
            base_url,
            max_tokens,
            temperature,
            enable_thinking,
            preserve_thinking,
            false, // enable_web_search_in_chat
            None,  // web_search_engine
            None,  // web_search_count
            None,  // web_search_recency
            None,  // web_search_content_size
        )
    }

    /// Create a new ZaiProvider with a custom name (e.g., "zai.default").
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_name(
        name: String,
        api_key: String,
        model: Option<String>,
        base_url: Option<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        enable_thinking: bool,
        preserve_thinking: bool,
        enable_web_search_in_chat: bool,
        web_search_engine: Option<String>,
        web_search_count: Option<u32>,
        web_search_recency: Option<String>,
        web_search_content_size: Option<String>,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| anyhow!("Failed to create HTTP client: {}", e))?;

        let model = model.unwrap_or_else(|| ZAI_DEFAULT_MODEL.to_string());
        let base_url = base_url.unwrap_or_else(|| ZAI_DEFAULT_BASE_URL.to_string());

        debug!(
            "Initialized Z.ai provider '{}' with model: {}, base_url: {}, thinking: {}, web_search: {}",
            name, model, base_url, enable_thinking, enable_web_search_in_chat
        );

        Ok(Self {
            client,
            api_key,
            model,
            base_url,
            max_tokens,
            temperature,
            name,
            enable_thinking,
            preserve_thinking,
            enable_web_search_in_chat,
            web_search_engine,
            web_search_count,
            web_search_recency,
            web_search_content_size,
        })
    }

    /// Create the request body for the Z.ai API.
    fn create_request_body(
        &self,
        messages: &[Message],
        tools: Option<&[Tool]>,
        stream: bool,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        disable_thinking: bool,
    ) -> serde_json::Value {
        let mut body = json!({
            "model": self.model,
            "messages": convert_messages(messages),
            "stream": stream,
        });

        if let Some(max_tokens) = max_tokens.or(self.max_tokens) {
            body["max_tokens"] = json!(max_tokens);
        }

        if let Some(temperature) = temperature.or(self.temperature) {
            body["temperature"] = json!(temperature);
        }

        // Add tools if provided (OpenAI format)
        if let Some(tools) = tools {
            if !tools.is_empty() {
                body["tools"] = json!(convert_tools(tools));
                body["parallel_tool_calls"] = json!(true);
            }
        }

        // Add thinking configuration if enabled and not disabled for this request
        if self.enable_thinking && !disable_thinking {
            body["extra_body"] = json!({
                "thinking": {
                    "type": "enabled",
                    "clear_thinking": !self.preserve_thinking
                }
            });
        }

        // Inject web_search tool if enabled (Z.ai special tool type)
        if self.enable_web_search_in_chat {
            // Build web_search config
            let mut web_search_config = json!({
                "enable": true,
                "search_engine": self.web_search_engine.as_deref().unwrap_or("bing"),
            });

            if let Some(count) = self.web_search_count {
                web_search_config["count"] = json!(count);
            }
            if let Some(recency) = &self.web_search_recency {
                web_search_config["search_recency_filter"] = json!(recency);
            }
            if let Some(content_size) = &self.web_search_content_size {
                web_search_config["content_size"] = json!(content_size);
            }

            // Z.ai's web_search is a special tool type (not "function")
            let web_search_tool = json!({
                "type": "web_search",
                "web_search": web_search_config
            });

            if let Some(tools_array) = body.get_mut("tools").and_then(|t| t.as_array_mut()) {
                tools_array.push(web_search_tool);
            } else {
                body["tools"] = json!([web_search_tool]);
            }

            debug!("Injected web_search tool into request");
        }

        // Request usage data for streaming
        if stream {
            body["stream_options"] = json!({
                "include_usage": true,
            });
        }

        body
    }

    /// Parse a streaming response from the Z.ai API.
    async fn parse_streaming_response(
        &self,
        mut stream: impl futures_util::Stream<Item = reqwest::Result<Bytes>> + Unpin,
        tx: mpsc::Sender<Result<CompletionChunk>>,
    ) -> Option<Usage> {
        let mut buffer = String::new();
        let mut byte_buffer = Vec::new();
        let mut accumulated_usage: Option<Usage> = None;
        let mut current_tool_calls: Vec<ZaiStreamingToolCall> = Vec::new();
        let mut accumulated_reasoning = String::new();
        let mut last_finish_reason: Option<String> = None;
        let mut tool_calls_started: bool = false;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    byte_buffer.extend_from_slice(&chunk);

                    let Some(chunk_str) = decode_utf8_streaming(&mut byte_buffer) else {
                        continue;
                    };

                    buffer.push_str(&chunk_str);

                    // Process complete lines
                    while let Some(line_end) = buffer.find('\n') {
                        let line = buffer[..line_end].trim().to_string();
                        buffer.drain(..line_end + 1);

                        if line.is_empty() {
                            continue;
                        }

                        // Parse Server-Sent Events format
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                debug!("Received stream completion marker");

                                // Send final chunk with any remaining un-emitted tool calls
                                let tool_calls = if current_tool_calls.is_empty() {
                                    vec![]
                                } else {
                                    current_tool_calls
                                        .iter()
                                        .filter(|tc| !tc.emitted)
                                        .filter_map(|tc| tc.to_tool_call())
                                        .collect()
                                };

                                // Include reasoning content if accumulated (for preserved thinking)
                                let reasoning = if accumulated_reasoning.is_empty() {
                                    None
                                } else {
                                    Some(accumulated_reasoning.clone())
                                };

                                let stop_reason = convert_finish_reason(last_finish_reason.as_deref());
                                let final_chunk = make_final_chunk_full(
                                    tool_calls,
                                    accumulated_usage.clone(),
                                    stop_reason,
                                    reasoning,
                                );
                                let _ = tx.send(Ok(final_chunk)).await;

                                return accumulated_usage;
                            }

                            // Parse the JSON data
                            match serde_json::from_str::<ZaiStreamChunk>(data) {
                                Ok(chunk_data) => {
                                    // Handle content from choices
                                    for choice in &chunk_data.choices {
                                        // Handle reasoning_content (Z.ai-specific thinking)
                                        if let Some(reasoning) = &choice.delta.reasoning_content {
                                            if !reasoning.is_empty() {
                                                accumulated_reasoning.push_str(reasoning);
                                                debug!(
                                                    "Received reasoning content: {} chars",
                                                    reasoning.len()
                                                );
                                                // We don't send reasoning content to the UI,
                                                // it's for internal use / preserved thinking
                                            }
                                        }

                                        // Handle regular content
                                        if let Some(content) = &choice.delta.content {
                                            if !content.is_empty() {
                                                if tool_calls_started {
                                                    // After tool calls begin, redirect content
                                                    // to reasoning (GLM-5 interleaves CoT text
                                                    // with tool call deltas)
                                                    accumulated_reasoning.push_str(content);
                                                } else {
                                                    let chunk = make_text_chunk(content.clone());
                                                    if tx.send(Ok(chunk)).await.is_err() {
                                                        debug!("Receiver dropped, stopping stream");
                                                        return accumulated_usage;
                                                    }
                                                }
                                            }
                                        }

                                        // Handle tool calls (OpenAI format)
                                        if let Some(delta_tool_calls) = &choice.delta.tool_calls {
                                            if !delta_tool_calls.is_empty() {
                                                tool_calls_started = true;
                                            }
                                            for delta_tool_call in delta_tool_calls {
                                                if let Some(index) = delta_tool_call.index {
                                                    // Ensure we have enough tool calls in our vector
                                                    while current_tool_calls.len() <= index {
                                                        current_tool_calls
                                                            .push(ZaiStreamingToolCall::default());
                                                    }

                                                    let tool_call = &mut current_tool_calls[index];

                                                    if let Some(id) = &delta_tool_call.id {
                                                        tool_call.id = Some(id.clone());
                                                    }

                                                    if let Some(function) = &delta_tool_call.function
                                                    {
                                                        if let Some(name) = &function.name {
                                                            tool_call.name = Some(name.clone());
                                                        }
                                                        if let Some(arguments) = &function.arguments
                                                        {
                                                            tool_call.arguments.push_str(arguments);
                                                        }
                                                    }

                                                    // Emit tool call early if complete
                                                    if !current_tool_calls[index].emitted
                                                        && current_tool_calls[index].is_complete()
                                                    {
                                                        if let Some(completed) =
                                                            current_tool_calls[index].to_tool_call()
                                                        {
                                                            let chunk =
                                                                make_tool_chunk(vec![completed]);
                                                            if tx.send(Ok(chunk)).await.is_err() {
                                                                debug!("Receiver dropped, stopping stream");
                                                                return accumulated_usage;
                                                            }
                                                            current_tool_calls[index].emitted =
                                                                true;
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // Capture finish_reason for the final chunk
                                        if let Some(ref reason) = choice.finish_reason {
                                            last_finish_reason = Some(reason.clone());
                                        }

                                        // Check for finish_reason to send remaining tool calls
                                        if choice.finish_reason.is_some()
                                            && !current_tool_calls.is_empty()
                                        {
                                            let tool_calls: Vec<ToolCall> = current_tool_calls
                                                .iter()
                                                .filter(|tc| !tc.emitted)
                                                .filter_map(|tc| tc.to_tool_call())
                                                .collect();

                                            if !tool_calls.is_empty() {
                                                let chunk = make_tool_chunk(tool_calls);
                                                if tx.send(Ok(chunk)).await.is_err() {
                                                    debug!("Receiver dropped, stopping stream");
                                                    return accumulated_usage;
                                                }
                                            }
                                        }
                                    }

                                    // Handle usage
                                    if let Some(usage) = chunk_data.usage {
                                        accumulated_usage = Some(Usage {
                                            prompt_tokens: usage.prompt_tokens,
                                            completion_tokens: usage.completion_tokens,
                                            total_tokens: usage.total_tokens,
                                            cache_creation_tokens: 0,
                                            cache_read_tokens: 0,
                                        });
                                    }
                                }
                                Err(e) => {
                                    debug!("Failed to parse stream chunk: {} - Data: {}", e, data);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Stream error: {}", e);
                    let _ = tx.send(Err(anyhow!("Stream error: {}", e))).await;
                    return accumulated_usage;
                }
            }
        }

        // Send final chunk if we haven't already
        let tool_calls: Vec<ToolCall> = current_tool_calls
            .iter()
            .filter_map(|tc| tc.to_tool_call())
            .collect();

        // Include reasoning content if accumulated (for preserved thinking)
        let reasoning = if accumulated_reasoning.is_empty() {
            None
        } else {
            Some(accumulated_reasoning)
        };

        let stop_reason = convert_finish_reason(last_finish_reason.as_deref());
        let final_chunk = make_final_chunk_full(tool_calls, accumulated_usage.clone(), stop_reason, reasoning);
        let _ = tx.send(Ok(final_chunk)).await;

        accumulated_usage
    }
}

#[async_trait::async_trait]
impl LLMProvider for ZaiProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        debug!(
            "Processing Z.ai completion request with {} messages",
            request.messages.len()
        );

        let body = self.create_request_body(
            &request.messages,
            request.tools.as_deref(),
            false,
            request.max_tokens,
            request.temperature,
            request.disable_thinking,
        );

        debug!("Sending request to Z.ai API: model={}", self.model);

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send request to Z.ai API: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("Z.ai API error {}: {}", status, error_text));
        }

        let zai_response: ZaiResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse Z.ai response: {}", e))?;

        // Extract content from the response
        let content = zai_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .unwrap_or_default();

        let usage = Usage {
            prompt_tokens: zai_response.usage.prompt_tokens,
            completion_tokens: zai_response.usage.completion_tokens,
            total_tokens: zai_response.usage.total_tokens,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
        };

        debug!(
            "Z.ai completion successful: {} tokens generated",
            usage.completion_tokens
        );

        Ok(CompletionResponse {
            content,
            usage,
            model: zai_response.model,
        })
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        debug!(
            "Processing Z.ai streaming request with {} messages",
            request.messages.len()
        );

        let body = self.create_request_body(
            &request.messages,
            request.tools.as_deref(),
            true,
            request.max_tokens,
            request.temperature,
            request.disable_thinking,
        );

        debug!(
            "Sending streaming request to Z.ai API: model={}",
            self.model
        );

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send streaming request to Z.ai API: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("Z.ai API error {}: {}", status, error_text));
        }

        let stream = response.bytes_stream();
        let (tx, rx) = mpsc::channel(100);

        // Spawn task to process the stream
        let provider = self.clone();
        tokio::spawn(async move {
            let usage = provider.parse_streaming_response(stream, tx).await;
            if let Some(usage) = usage {
                debug!(
                    "Stream completed with usage - prompt: {}, completion: {}, total: {}",
                    usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
                );
            }
        });

        Ok(ReceiverStream::new(rx))
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn has_native_tool_calling(&self) -> bool {
        // Z.ai GLM models support native tool calling
        true
    }

    fn supports_cache_control(&self) -> bool {
        // Z.ai doesn't support Anthropic-style cache control
        false
    }

    fn max_tokens(&self) -> u32 {
        self.max_tokens.unwrap_or(4096)
    }

    fn temperature(&self) -> f32 {
        self.temperature.unwrap_or(0.7)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Message and Tool Conversion
// ─────────────────────────────────────────────────────────────────────────────

/// Convert OpenAI-style finish_reason to internal stop_reason format.
fn convert_finish_reason(reason: Option<&str>) -> Option<String> {
    reason.map(|r| match r {
        "stop" => "end_turn".to_string(),
        "tool_calls" => "tool_use".to_string(),
        "length" => "max_tokens".to_string(),
        other => other.to_string(),
    })
}

/// Convert g3 messages to Z.ai message format (OpenAI-compatible).
fn convert_messages(messages: &[Message]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|msg| {
            json!({
                "role": match msg.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                },
                "content": msg.content,
            })
        })
        .collect()
}

/// Convert g3 tools to Z.ai tool format (OpenAI-compatible).
fn convert_tools(tools: &[Tool]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }
            })
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// API Response Structures
// ─────────────────────────────────────────────────────────────────────────────

/// Non-streaming response from Z.ai API.
#[derive(Debug, Deserialize)]
struct ZaiResponse {
    choices: Vec<ZaiChoice>,
    usage: ZaiUsage,
    model: String,
}

#[derive(Debug, Deserialize)]
struct ZaiChoice {
    message: ZaiMessage,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ZaiMessage {
    content: Option<String>,
    /// Z.ai-specific: reasoning content from thinking mode
    #[serde(default)]
    #[allow(dead_code)]
    reasoning_content: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    tool_calls: Option<Vec<ZaiToolCall>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ZaiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: ZaiFunction,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ZaiFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ZaiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Streaming Response Structures
// ─────────────────────────────────────────────────────────────────────────────

/// Streaming chunk from Z.ai API.
#[derive(Debug, Deserialize)]
struct ZaiStreamChunk {
    choices: Vec<ZaiStreamChoice>,
    usage: Option<ZaiUsage>,
}

#[derive(Debug, Deserialize)]
struct ZaiStreamChoice {
    delta: ZaiDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ZaiDelta {
    content: Option<String>,
    /// Z.ai-specific: reasoning content from thinking mode
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ZaiDeltaToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ZaiDeltaToolCall {
    index: Option<usize>,
    id: Option<String>,
    function: Option<ZaiDeltaFunction>,
}

#[derive(Debug, Deserialize)]
struct ZaiDeltaFunction {
    name: Option<String>,
    arguments: Option<String>,
}

/// Streaming tool call accumulator.
#[derive(Debug, Default)]
struct ZaiStreamingToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
    emitted: bool,
}

impl ZaiStreamingToolCall {
    fn to_tool_call(&self) -> Option<ToolCall> {
        let id = self.id.as_ref()?;
        let name = self.name.as_ref()?;

        let args = serde_json::from_str(&self.arguments).unwrap_or(serde_json::Value::Null);

        Some(ToolCall {
            id: id.clone(),
            tool: name.clone(),
            args,
        })
    }

    /// Returns true when id, name, and arguments are all present and arguments
    /// parses as valid JSON.
    fn is_complete(&self) -> bool {
        self.id.is_some()
            && self.name.is_some()
            && !self.arguments.is_empty()
            && serde_json::from_str::<serde_json::Value>(&self.arguments).is_ok()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = ZaiProvider::new(
            "test-api-key".to_string(),
            Some("glm-4.7".to_string()),
            None,
            Some(4096),
            Some(0.7),
            true,
            false,
        )
        .unwrap();

        assert_eq!(provider.name(), "zai");
        assert_eq!(provider.model(), "glm-4.7");
        assert_eq!(provider.max_tokens(), 4096);
        assert_eq!(provider.temperature(), 0.7);
        assert!(provider.has_native_tool_calling());
        assert!(!provider.supports_cache_control());
    }

    #[test]
    fn test_provider_with_custom_name() {
        let provider = ZaiProvider::new_with_name(
            "zai.default".to_string(),
            "test-api-key".to_string(),
            None,
            None,
            None,
            None,
            false,
            false,
            false, // enable_web_search_in_chat
            None,  // web_search_engine
            None,  // web_search_count
            None,  // web_search_recency
            None,  // web_search_content_size
        )
        .unwrap();

        assert_eq!(provider.name(), "zai.default");
        assert_eq!(provider.model(), ZAI_DEFAULT_MODEL);
    }

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::new(
                MessageRole::System,
                "You are a helpful assistant.".to_string(),
            ),
            Message::new(MessageRole::User, "Hello!".to_string()),
            Message::new(MessageRole::Assistant, "Hi there!".to_string()),
        ];

        let converted = convert_messages(&messages);

        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0]["role"], "system");
        assert_eq!(converted[0]["content"], "You are a helpful assistant.");
        assert_eq!(converted[1]["role"], "user");
        assert_eq!(converted[1]["content"], "Hello!");
        assert_eq!(converted[2]["role"], "assistant");
        assert_eq!(converted[2]["content"], "Hi there!");
    }

    #[test]
    fn test_tool_conversion() {
        let tools = vec![Tool {
            name: "get_weather".to_string(),
            description: "Get the current weather".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city and state"
                    }
                },
                "required": ["location"]
            }),
        }];

        let converted = convert_tools(&tools);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["type"], "function");
        assert_eq!(converted[0]["function"]["name"], "get_weather");
        assert_eq!(
            converted[0]["function"]["description"],
            "Get the current weather"
        );
    }

    #[test]
    fn test_request_body_with_thinking() {
        let provider = ZaiProvider::new(
            "test-api-key".to_string(),
            Some("glm-4.7".to_string()),
            None,
            Some(4096),
            Some(0.7),
            true,  // enable_thinking
            false, // preserve_thinking (clear_thinking = true)
        )
        .unwrap();

        let messages = vec![Message::new(MessageRole::User, "Test message".to_string())];

        let body = provider.create_request_body(&messages, None, false, None, None, false);

        assert_eq!(body["model"], "glm-4.7");
        assert!(body["extra_body"]["thinking"]["type"] == "enabled");
        assert!(body["extra_body"]["thinking"]["clear_thinking"] == true);
    }

    #[test]
    fn test_request_body_with_preserved_thinking() {
        let provider = ZaiProvider::new(
            "test-api-key".to_string(),
            Some("glm-4.7".to_string()),
            None,
            Some(4096),
            Some(0.7),
            true, // enable_thinking
            true, // preserve_thinking (clear_thinking = false)
        )
        .unwrap();

        let messages = vec![Message::new(MessageRole::User, "Test message".to_string())];

        let body = provider.create_request_body(&messages, None, false, None, None, false);

        assert!(body["extra_body"]["thinking"]["clear_thinking"] == false);
    }

    #[test]
    fn test_request_body_without_thinking() {
        let provider = ZaiProvider::new(
            "test-api-key".to_string(),
            Some("glm-4.7".to_string()),
            None,
            Some(4096),
            Some(0.7),
            false, // enable_thinking
            false,
        )
        .unwrap();

        let messages = vec![Message::new(MessageRole::User, "Test message".to_string())];

        let body = provider.create_request_body(&messages, None, false, None, None, false);

        assert!(body.get("extra_body").is_none());
    }

    #[test]
    fn test_request_body_thinking_disabled_per_request() {
        let provider = ZaiProvider::new(
            "test-api-key".to_string(),
            Some("glm-4.7".to_string()),
            None,
            Some(4096),
            Some(0.7),
            true, // enable_thinking globally
            false,
        )
        .unwrap();

        let messages = vec![Message::new(MessageRole::User, "Test message".to_string())];

        // disable_thinking = true should override the global setting
        let body = provider.create_request_body(&messages, None, false, None, None, true);

        assert!(body.get("extra_body").is_none());
    }

    #[test]
    fn test_default_base_url() {
        let provider = ZaiProvider::new(
            "test-api-key".to_string(),
            None,
            None, // No base_url specified
            None,
            None,
            false,
            false,
        )
        .unwrap();

        assert_eq!(provider.base_url, ZAI_DEFAULT_BASE_URL);
    }

    #[test]
    fn test_custom_base_url() {
        let provider = ZaiProvider::new(
            "test-api-key".to_string(),
            None,
            Some("https://open.bigmodel.cn/api/paas/v4".to_string()),
            None,
            None,
            false,
            false,
        )
        .unwrap();

        assert_eq!(provider.base_url, "https://open.bigmodel.cn/api/paas/v4");
    }

    #[test]
    fn test_request_body_with_web_search() {
        let provider = ZaiProvider::new_with_name(
            "zai.test".to_string(),
            "test-api-key".to_string(),
            Some("glm-4.7".to_string()),
            None,
            Some(4096),
            Some(0.7),
            false, // enable_thinking
            false, // preserve_thinking
            true,  // enable_web_search_in_chat
            Some("google".to_string()), // web_search_engine
            Some(5), // web_search_count
            Some("week".to_string()), // web_search_recency
            Some("high".to_string()), // web_search_content_size
        )
        .unwrap();

        let messages = vec![Message::new(MessageRole::User, "Test message".to_string())];
        let body = provider.create_request_body(&messages, None, false, None, None, false);

        // Check that web_search tool was injected
        let tools = body["tools"].as_array().expect("tools should be an array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "web_search");
        assert_eq!(tools[0]["web_search"]["enable"], true);
        assert_eq!(tools[0]["web_search"]["search_engine"], "google");
        assert_eq!(tools[0]["web_search"]["count"], 5);
        assert_eq!(tools[0]["web_search"]["search_recency_filter"], "week");
        assert_eq!(tools[0]["web_search"]["content_size"], "high");
    }

    #[test]
    fn test_request_body_with_web_search_and_function_tools() {
        let provider = ZaiProvider::new_with_name(
            "zai.test".to_string(),
            "test-api-key".to_string(),
            Some("glm-4.7".to_string()),
            None,
            Some(4096),
            Some(0.7),
            false, // enable_thinking
            false, // preserve_thinking
            true,  // enable_web_search_in_chat
            None,  // web_search_engine (default: bing)
            None,  // web_search_count
            None,  // web_search_recency
            None,  // web_search_content_size
        )
        .unwrap();

        let messages = vec![Message::new(MessageRole::User, "Test message".to_string())];
        let function_tools = vec![Tool {
            name: "get_weather".to_string(),
            description: "Get weather".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        }];

        let body = provider.create_request_body(&messages, Some(&function_tools), false, None, None, false);

        // Check that both function tools and web_search tool were added
        let tools = body["tools"].as_array().expect("tools should be an array");
        assert_eq!(tools.len(), 2);
        // First tool should be the function tool
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "get_weather");
        // Second tool should be web_search
        assert_eq!(tools[1]["type"], "web_search");
        assert_eq!(tools[1]["web_search"]["enable"], true);
        assert_eq!(tools[1]["web_search"]["search_engine"], "bing"); // default
    }

    #[test]
    fn test_streaming_tool_call_conversion() {
        let mut tool_call = ZaiStreamingToolCall::default();
        assert!(tool_call.to_tool_call().is_none());

        tool_call.id = Some("call_123".to_string());
        assert!(tool_call.to_tool_call().is_none());

        tool_call.name = Some("get_weather".to_string());
        tool_call.arguments = r#"{"location": "San Francisco"}"#.to_string();

        let converted = tool_call.to_tool_call().unwrap();
        assert_eq!(converted.id, "call_123");
        assert_eq!(converted.tool, "get_weather");
        assert_eq!(converted.args["location"], "San Francisco");
    }

    #[test]
    fn test_streaming_tool_call_is_complete() {
        let mut tc = ZaiStreamingToolCall::default();
        assert!(!tc.is_complete(), "empty tool call should not be complete");

        tc.id = Some("call_1".to_string());
        assert!(!tc.is_complete(), "missing name and args");

        tc.name = Some("bash".to_string());
        assert!(!tc.is_complete(), "missing args");

        tc.arguments = r#"{"command": "ls"}"#.to_string();
        assert!(tc.is_complete(), "all fields present with valid JSON");
    }

    #[test]
    fn test_streaming_tool_call_emitted_flag() {
        let mut tc = ZaiStreamingToolCall::default();
        assert!(!tc.emitted, "emitted should default to false");

        tc.id = Some("call_1".to_string());
        tc.name = Some("bash".to_string());
        tc.arguments = r#"{"command": "ls"}"#.to_string();
        assert!(tc.is_complete());

        // Simulate emission
        tc.emitted = true;

        // to_tool_call still works (emitted is just a flag, not a gate)
        assert!(tc.to_tool_call().is_some());
        assert!(tc.emitted);
    }

    #[test]
    fn test_streaming_tool_call_partial_json_not_complete() {
        let mut tc = ZaiStreamingToolCall {
            id: Some("call_1".to_string()),
            name: Some("bash".to_string()),
            arguments: String::new(),
            emitted: false,
        };

        // Partial JSON - not complete
        tc.arguments = r#"{"command": "#.to_string();
        assert!(!tc.is_complete(), "partial JSON should not be complete");

        // Still partial
        tc.arguments.push_str(r#""ls -la""#);
        assert!(!tc.is_complete(), "missing closing brace");

        // Now complete
        tc.arguments.push('}');
        assert!(tc.is_complete(), "valid JSON should be complete");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Z.ai Standalone Tools API Client
// ─────────────────────────────────────────────────────────────────────────────

/// Client for Z.ai standalone tool APIs (Web Search, Web Reader, Layout Parsing).
///
/// This client provides access to Z.ai's specialized tool endpoints that can be used
/// independently of the chat completion API.
///
/// # Example
///
/// ```rust,no_run
/// use g3_providers::ZaiToolsClient;
/// use g3_providers::zai::{ZaiWebSearchRequest, ZaiWebReaderRequest, ZaiLayoutParsingRequest};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let client = ZaiToolsClient::new("your-api-key".to_string(), None);
///
///     // Web Search
///     let search_req = ZaiWebSearchRequest {
///         query: "Rust programming language".to_string(),
///         count: Some(5),
///         search_domain_filter: None,
///         search_recency_filter: None,
///     };
///     let results = client.web_search(search_req).await?;
///
///     // Web Reader
///     let reader_req = ZaiWebReaderRequest {
///         url: "https://example.com".to_string(),
///         format: Some("markdown".to_string()),
///         retain_images: Some(true),
///         timeout: Some(30),
///     };
///     let content = client.web_reader(reader_req).await?;
///
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct ZaiToolsClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl ZaiToolsClient {
    /// Create a new ZaiToolsClient with the given API key and optional base URL.
    ///
    /// # Arguments
    ///
    /// * `api_key` - Your Z.ai API key
    /// * `base_url` - Optional custom base URL. Defaults to `https://api.z.ai/api/paas/v4`
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        let base_url = base_url.unwrap_or_else(|| ZAI_DEFAULT_BASE_URL.to_string());

        debug!("Initialized Z.ai Tools client with base_url: {}", base_url);

        Self {
            client,
            api_key,
            base_url,
        }
    }

    /// Perform a web search using Z.ai's Web Search API.
    ///
    /// # Arguments
    ///
    /// * `req` - The web search request parameters
    ///
    /// # Returns
    ///
    /// A `ZaiWebSearchResponse` containing the search results.
    pub async fn web_search(&self, req: ZaiWebSearchRequest) -> Result<ZaiWebSearchResponse> {
        debug!("Performing web search for query: {}", req.query);

        let response = self
            .client
            .post(format!("{}/web_search", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send web search request: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("Z.ai Web Search API error {}: {}", status, error_text));
        }

        let result: ZaiWebSearchResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse web search response: {}", e))?;

        debug!(
            "Web search returned {} results",
            result.search_result.len()
        );

        Ok(result)
    }

    /// Fetch and convert a webpage to markdown or text using Z.ai's Web Reader API.
    ///
    /// # Arguments
    ///
    /// * `req` - The web reader request parameters
    ///
    /// # Returns
    ///
    /// A `ZaiWebReaderResponse` containing the converted content.
    pub async fn web_reader(&self, req: ZaiWebReaderRequest) -> Result<ZaiWebReaderResponse> {
        debug!("Reading webpage: {}", req.url);

        let response = self
            .client
            .post(format!("{}/reader", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send web reader request: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("Z.ai Web Reader API error {}: {}", status, error_text));
        }

        let result: ZaiWebReaderResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse web reader response: {}", e))?;

        debug!(
            "Web reader returned {} chars of content",
            result.content.len()
        );

        Ok(result)
    }

    /// Parse layout/perform OCR on an image or PDF using Z.ai's Layout Parsing API.
    ///
    /// # Arguments
    ///
    /// * `req` - The layout parsing request parameters
    ///
    /// # Returns
    ///
    /// A `ZaiLayoutParsingResponse` containing the extracted text content.
    pub async fn layout_parsing(&self, req: ZaiLayoutParsingRequest) -> Result<ZaiLayoutParsingResponse> {
        debug!("Performing layout parsing with model: {}", req.model);

        let response = self
            .client
            .post(format!("{}/layout_parsing", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send layout parsing request: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("Z.ai Layout Parsing API error {}: {}", status, error_text));
        }

        let result: ZaiLayoutParsingResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse layout parsing response: {}", e))?;

        debug!(
            "Layout parsing returned {} chars of content",
            result.content.len()
        );

        Ok(result)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Z.ai Tools API Request/Response Types
// ─────────────────────────────────────────────────────────────────────────────

/// Request parameters for Z.ai Web Search API.
#[derive(Debug, Clone, Serialize)]
pub struct ZaiWebSearchRequest {
    /// The search query string.
    pub query: String,

    /// Number of results to return (1-50, default 10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,

    /// Whitelist of domains to search within.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_domain_filter: Option<Vec<String>>,

    /// Filter results by recency: "day", "week", "month", "year".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_recency_filter: Option<String>,
}

/// Response from Z.ai Web Search API.
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiWebSearchResponse {
    /// List of search results.
    pub search_result: Vec<ZaiSearchResult>,
}

/// Individual search result from Z.ai Web Search API.
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiSearchResult {
    /// Title of the search result.
    pub title: String,

    /// URL of the search result.
    pub link: String,

    /// Content snippet/summary of the search result.
    pub content: String,

    /// Optional media URL associated with the result.
    #[serde(default)]
    pub media: Option<String>,

    /// Optional favicon/icon URL for the source website.
    #[serde(default)]
    pub icon: Option<String>,
}

/// Request parameters for Z.ai Web Reader API.
#[derive(Debug, Clone, Serialize)]
pub struct ZaiWebReaderRequest {
    /// The URL to fetch and convert.
    pub url: String,

    /// Output format: "markdown" or "text" (default "markdown").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// Whether to retain images in the output (default true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retain_images: Option<bool>,

    /// Request timeout in seconds (default 20).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

/// Response from Z.ai Web Reader API.
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiWebReaderResponse {
    /// The converted content (markdown or text).
    pub content: String,

    /// Optional title extracted from the webpage.
    #[serde(default)]
    pub title: Option<String>,
}

/// Request parameters for Z.ai Layout Parsing (OCR) API.
#[derive(Debug, Clone, Serialize)]
pub struct ZaiLayoutParsingRequest {
    /// The model to use (always "glm-ocr").
    pub model: String,

    /// The file to parse: URL or base64 data URI.
    pub file: String,

    /// Optional page range for PDFs (e.g., "1-5").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_range: Option<String>,
}

/// Response from Z.ai Layout Parsing (OCR) API.
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiLayoutParsingResponse {
    /// The extracted text content.
    pub content: String,

    /// Optional usage information.
    #[serde(default)]
    pub usage: Option<ZaiToolsUsage>,
}

/// Usage information for Z.ai Tools API responses.
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiToolsUsage {
    /// Number of prompt tokens used.
    pub prompt_tokens: u32,

    /// Number of completion tokens used.
    pub completion_tokens: u32,

    /// Total number of tokens used.
    pub total_tokens: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Z.ai Tools API Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tools_tests {
    use super::*;

    #[test]
    fn test_tools_client_creation() {
        let client = ZaiToolsClient::new("test-api-key".to_string(), None);
        assert_eq!(client.base_url, ZAI_DEFAULT_BASE_URL);
    }

    #[test]
    fn test_tools_client_custom_base_url() {
        let client = ZaiToolsClient::new(
            "test-api-key".to_string(),
            Some("https://custom.api.example.com".to_string()),
        );
        assert_eq!(client.base_url, "https://custom.api.example.com");
    }

    #[test]
    fn test_web_search_request_serialization() {
        let req = ZaiWebSearchRequest {
            query: "test query".to_string(),
            count: Some(5),
            search_domain_filter: Some(vec!["example.com".to_string()]),
            search_recency_filter: Some("week".to_string()),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["query"], "test query");
        assert_eq!(json["count"], 5);
        assert_eq!(json["search_domain_filter"], serde_json::json!(["example.com"]));
        assert_eq!(json["search_recency_filter"], "week");
    }

    #[test]
    fn test_web_search_request_minimal_serialization() {
        let req = ZaiWebSearchRequest {
            query: "test query".to_string(),
            count: None,
            search_domain_filter: None,
            search_recency_filter: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["query"], "test query");
        assert!(json.get("count").is_none());
        assert!(json.get("search_domain_filter").is_none());
        assert!(json.get("search_recency_filter").is_none());
    }

    #[test]
    fn test_web_reader_request_serialization() {
        let req = ZaiWebReaderRequest {
            url: "https://example.com".to_string(),
            format: Some("markdown".to_string()),
            retain_images: Some(false),
            timeout: Some(30),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["url"], "https://example.com");
        assert_eq!(json["format"], "markdown");
        assert_eq!(json["retain_images"], false);
        assert_eq!(json["timeout"], 30);
    }

    #[test]
    fn test_layout_parsing_request_serialization() {
        let req = ZaiLayoutParsingRequest {
            model: "glm-ocr".to_string(),
            file: "https://example.com/image.png".to_string(),
            page_range: Some("1-5".to_string()),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "glm-ocr");
        assert_eq!(json["file"], "https://example.com/image.png");
        assert_eq!(json["page_range"], "1-5");
    }

    #[test]
    fn test_web_search_response_deserialization() {
        let json = r#"{
            "search_result": [
                {
                    "title": "Test Result",
                    "link": "https://example.com",
                    "content": "Test content",
                    "media": "https://example.com/media.jpg",
                    "icon": "https://example.com/favicon.ico"
                }
            ]
        }"#;

        let response: ZaiWebSearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.search_result.len(), 1);
        assert_eq!(response.search_result[0].title, "Test Result");
        assert_eq!(response.search_result[0].link, "https://example.com");
        assert_eq!(response.search_result[0].content, "Test content");
        assert_eq!(response.search_result[0].media, Some("https://example.com/media.jpg".to_string()));
        assert_eq!(response.search_result[0].icon, Some("https://example.com/favicon.ico".to_string()));
    }

    #[test]
    fn test_web_reader_response_deserialization() {
        let json = r##"{
            "content": "Example Page - This is the content.",
            "title": "Example Page"
        }"##;

        let response: ZaiWebReaderResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.content, "Example Page - This is the content.");
        assert_eq!(response.title, Some("Example Page".to_string()));
    }

    #[test]
    fn test_layout_parsing_response_deserialization() {
        let json = r#"{
            "content": "Extracted text from image",
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "total_tokens": 150
            }
        }"#;

        let response: ZaiLayoutParsingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.content, "Extracted text from image");
        assert!(response.usage.is_some());
        let usage = response.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }
}
