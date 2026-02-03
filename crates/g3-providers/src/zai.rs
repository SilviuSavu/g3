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
//!
//! # Endpoints
//!
//! - International: `https://api.z.ai/api/paas/v4/chat/completions`
//! - China: `https://open.bigmodel.cn/api/paas/v4/chat/completions`
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
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error};

use crate::{
    streaming::{decode_utf8_streaming, make_final_chunk_with_reasoning, make_text_chunk, make_tool_chunk},
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
        )
    }

    /// Create a new ZaiProvider with a custom name (e.g., "zai.default").
    pub fn new_with_name(
        name: String,
        api_key: String,
        model: Option<String>,
        base_url: Option<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        enable_thinking: bool,
        preserve_thinking: bool,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| anyhow!("Failed to create HTTP client: {}", e))?;

        let model = model.unwrap_or_else(|| ZAI_DEFAULT_MODEL.to_string());
        let base_url = base_url.unwrap_or_else(|| ZAI_DEFAULT_BASE_URL.to_string());

        debug!(
            "Initialized Z.ai provider '{}' with model: {}, base_url: {}, thinking: {}",
            name, model, base_url, enable_thinking
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

                                // Send final chunk with tool calls if any
                                let tool_calls = if current_tool_calls.is_empty() {
                                    vec![]
                                } else {
                                    current_tool_calls
                                        .iter()
                                        .filter_map(|tc| tc.to_tool_call())
                                        .collect()
                                };

                                // Include reasoning content if accumulated (for preserved thinking)
                                let reasoning = if accumulated_reasoning.is_empty() {
                                    None
                                } else {
                                    Some(accumulated_reasoning.clone())
                                };

                                let final_chunk = make_final_chunk_with_reasoning(
                                    tool_calls,
                                    accumulated_usage.clone(),
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
                                                let chunk = make_text_chunk(content.clone());
                                                if tx.send(Ok(chunk)).await.is_err() {
                                                    debug!("Receiver dropped, stopping stream");
                                                    return accumulated_usage;
                                                }
                                            }
                                        }

                                        // Handle tool calls (OpenAI format)
                                        if let Some(delta_tool_calls) = &choice.delta.tool_calls {
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
                                                }
                                            }
                                        }

                                        // Check for finish_reason to send tool calls
                                        if choice.finish_reason.is_some()
                                            && !current_tool_calls.is_empty()
                                        {
                                            let tool_calls: Vec<ToolCall> = current_tool_calls
                                                .iter()
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

        let final_chunk = make_final_chunk_with_reasoning(tool_calls, accumulated_usage.clone(), reasoning);
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
}
