//! Z.ai (Zhipu AI) Provider Demo
//!
//! This example demonstrates the Z.ai provider features:
//! - Provider creation from environment variables
//! - Endpoint selection (standard vs coding API)
//! - Non-streaming completion
//! - Streaming completion with live output
//! - Tool calling demonstration
//! - Thinking mode demonstration
//!
//! # Environment Variables
//!
//! - `ZAI_API_KEY`: Your Z.ai API key (required)
//! - `ZAI_BASE_URL`: API endpoint (optional, defaults to international standard)
//!
//! # Usage
//!
//! ```bash
//! # Basic usage with international endpoint
//! ZAI_API_KEY=your-key cargo run --example zai_demo
//!
//! # With China endpoint
//! ZAI_API_KEY=your-key ZAI_BASE_URL=https://open.bigmodel.cn/api/paas/v4 cargo run --example zai_demo
//!
//! # With coding plan endpoint (recommended for g3)
//! ZAI_API_KEY=your-key ZAI_BASE_URL=https://api.z.ai/api/coding/paas/v4 cargo run --example zai_demo
//! ```

use anyhow::Result;
use g3_providers::{
    CompletionRequest, LLMProvider, Message, MessageRole, Tool, ZaiProvider,
};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt()
        .with_env_filter("g3_providers=debug")
        .init();

    // Get API key from environment
    let api_key = std::env::var("ZAI_API_KEY").expect(
        "ZAI_API_KEY environment variable required.\n\
         Get your API key from https://z.ai/ or https://open.bigmodel.cn/",
    );

    // Optional: custom base URL (defaults to international standard API)
    let base_url = std::env::var("ZAI_BASE_URL").ok();

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║              Z.ai (Zhipu AI) Provider Demo                       ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!(
        "║ Endpoint: {:55} ║",
        base_url.as_deref().unwrap_or("https://api.z.ai/api/paas/v4 (default)")
    );
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    // Create provider with thinking mode enabled
    let provider = ZaiProvider::new(
        api_key,
        Some("glm-4.7".to_string()), // Use GLM-4.7 flagship model
        base_url,
        Some(4096),  // max_tokens
        Some(0.7),   // temperature
        true,        // enable_thinking
        false,       // preserve_thinking (set to true for multi-turn reasoning)
    )?;

    println!("Provider: {} (model: {})", provider.name(), provider.model());
    println!("Native tool calling: {}", provider.has_native_tool_calling());
    println!();

    // Demo 1: Non-streaming completion
    demo_non_streaming(&provider).await?;

    // Demo 2: Streaming completion
    demo_streaming(&provider).await?;

    // Demo 3: Tool calling
    demo_tool_calling(&provider).await?;

    println!("\n✅ All demos completed successfully!");
    Ok(())
}

async fn demo_non_streaming(provider: &ZaiProvider) -> Result<()> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Demo 1: Non-streaming Completion");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let request = CompletionRequest {
        messages: vec![
            Message::new(
                MessageRole::System,
                "You are a helpful coding assistant. Be concise.".to_string(),
            ),
            Message::new(
                MessageRole::User,
                "What is the difference between `&str` and `String` in Rust? Answer in 2-3 sentences.".to_string(),
            ),
        ],
        max_tokens: Some(256),
        temperature: Some(0.7),
        stream: false,
        tools: None,
        disable_thinking: false,
    };

    let start = std::time::Instant::now();
    let response = provider.complete(request).await?;
    let elapsed = start.elapsed();

    println!("\nResponse:\n{}", response.content);
    println!("\n📊 Stats:");
    println!("  - Time: {:.2}s", elapsed.as_secs_f64());
    println!("  - Prompt tokens: {}", response.usage.prompt_tokens);
    println!("  - Completion tokens: {}", response.usage.completion_tokens);
    println!("  - Total tokens: {}", response.usage.total_tokens);
    println!();

    Ok(())
}

async fn demo_streaming(provider: &ZaiProvider) -> Result<()> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Demo 2: Streaming Completion");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let request = CompletionRequest {
        messages: vec![
            Message::new(
                MessageRole::User,
                "Write a haiku about programming in Rust.".to_string(),
            ),
        ],
        max_tokens: Some(128),
        temperature: Some(0.9),
        stream: true,
        tools: None,
        disable_thinking: false,
    };

    let start = std::time::Instant::now();
    let mut stream = provider.stream(request).await?;
    let mut first_token_time: Option<std::time::Duration> = None;
    let mut total_content = String::new();
    let mut final_reasoning: Option<String> = None;

    print!("\nResponse: ");
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                if first_token_time.is_none() && !chunk.content.is_empty() {
                    first_token_time = Some(start.elapsed());
                }

                if !chunk.content.is_empty() {
                    print!("{}", chunk.content);
                    total_content.push_str(&chunk.content);
                }

                // Capture reasoning content if available (for preserved thinking)
                if let Some(reasoning) = chunk.reasoning_content {
                    final_reasoning = Some(reasoning);
                }

                if chunk.finished {
                    println!();
                    if let Some(usage) = chunk.usage {
                        println!("\n📊 Stats:");
                        if let Some(ttft) = first_token_time {
                            println!("  - Time to first token: {:.2}ms", ttft.as_millis());
                        }
                        println!("  - Total time: {:.2}s", start.elapsed().as_secs_f64());
                        println!("  - Completion tokens: {}", usage.completion_tokens);

                        let elapsed_secs = start.elapsed().as_secs_f64();
                        if elapsed_secs > 0.0 {
                            let tps = usage.completion_tokens as f64 / elapsed_secs;
                            println!("  - Tokens/sec: {:.1}", tps);
                        }
                    }

                    if let Some(reasoning) = &final_reasoning {
                        println!("\n🧠 Reasoning content captured ({} chars)", reasoning.len());
                    }
                }
            }
            Err(e) => {
                eprintln!("\nStream error: {}", e);
                break;
            }
        }
    }
    println!();

    Ok(())
}

async fn demo_tool_calling(provider: &ZaiProvider) -> Result<()> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Demo 3: Tool Calling");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Define a simple tool
    let tools = vec![Tool {
        name: "get_weather".to_string(),
        description: "Get the current weather for a location".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city and country, e.g. 'Tokyo, Japan'"
                },
                "unit": {
                    "type": "string",
                    "enum": ["celsius", "fahrenheit"],
                    "description": "Temperature unit"
                }
            },
            "required": ["location"]
        }),
    }];

    let request = CompletionRequest {
        messages: vec![
            Message::new(
                MessageRole::User,
                "What's the weather like in San Francisco?".to_string(),
            ),
        ],
        max_tokens: Some(256),
        temperature: Some(0.7),
        stream: true,
        tools: Some(tools),
        disable_thinking: false,
    };

    let mut stream = provider.stream(request).await?;
    let mut tool_calls_received = Vec::new();

    print!("\nResponse: ");
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                if !chunk.content.is_empty() {
                    print!("{}", chunk.content);
                }

                if let Some(tool_calls) = chunk.tool_calls {
                    tool_calls_received.extend(tool_calls);
                }
            }
            Err(e) => {
                eprintln!("\nStream error: {}", e);
                break;
            }
        }
    }
    println!();

    if !tool_calls_received.is_empty() {
        println!("\n🔧 Tool calls received:");
        for tc in &tool_calls_received {
            println!("  - ID: {}", tc.id);
            println!("    Tool: {}", tc.tool);
            println!("    Args: {}", serde_json::to_string_pretty(&tc.args)?);
        }
    } else {
        println!("\n(No tool calls in this response)");
    }
    println!();

    Ok(())
}
