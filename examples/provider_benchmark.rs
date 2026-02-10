//! Provider Benchmark
//!
//! Simple benchmark for measuring LLM provider performance:
//! - Time to first token (TTFT)
//! - Total completion time
//! - Tokens per second throughput
//!
//! # Usage
//!
//! ```bash
//! # Benchmark Z.ai provider
//! ZAI_API_KEY=your-key cargo run --example provider_benchmark -- zai
//!
//! # Benchmark with custom endpoint
//! ZAI_API_KEY=your-key ZAI_BASE_URL=https://api.z.ai/api/coding/paas/v4 \
//!     cargo run --example provider_benchmark -- zai
//!
//! # Benchmark Anthropic
//! ANTHROPIC_API_KEY=your-key cargo run --example provider_benchmark -- anthropic
//!
//! # Benchmark OpenAI
//! OPENAI_API_KEY=your-key cargo run --example provider_benchmark -- openai
//! ```

use anyhow::{anyhow, Result};
use g3_providers::{
    AnthropicProvider, CompletionRequest, LLMProvider, Message, MessageRole,
    OpenAIProvider, ZaiProvider,
};
use std::time::{Duration, Instant};
use tokio_stream::StreamExt;

const WARMUP_RUNS: usize = 1;
const BENCHMARK_RUNS: usize = 3;

// Test prompts of varying complexity
const PROMPTS: &[(&str, &str)] = &[
    ("short", "What is 2+2?"),
    ("medium", "Explain the difference between TCP and UDP in 2-3 sentences."),
    ("long", "Write a short function in Rust that reverses a string. Include a brief comment explaining the approach."),
];

#[derive(Debug)]
struct BenchmarkResult {
    prompt_type: String,
    ttft_ms: f64,
    total_time_ms: f64,
    completion_tokens: u32,
    tokens_per_second: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let provider_name = args.get(1).map(|s| s.as_str()).unwrap_or("zai");

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║                    Provider Benchmark                            ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    let provider = create_provider(provider_name)?;

    println!("Provider: {} (model: {})", provider.name(), provider.model());
    println!("Warmup runs: {}", WARMUP_RUNS);
    println!("Benchmark runs: {}", BENCHMARK_RUNS);
    println!();

    // Run benchmarks
    let mut all_results: Vec<BenchmarkResult> = Vec::new();

    for (prompt_type, prompt) in PROMPTS {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Benchmarking: {} prompt", prompt_type);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        // Warmup
        for i in 0..WARMUP_RUNS {
            print!("  Warmup {}/{}... ", i + 1, WARMUP_RUNS);
            let _ = run_single_benchmark(provider.as_ref(), prompt).await;
            println!("done");
        }

        // Actual benchmark runs
        let mut results: Vec<BenchmarkResult> = Vec::new();
        for i in 0..BENCHMARK_RUNS {
            print!("  Run {}/{}... ", i + 1, BENCHMARK_RUNS);
            match run_single_benchmark(provider.as_ref(), prompt).await {
                Ok(result) => {
                    println!(
                        "TTFT: {:.0}ms, Total: {:.0}ms, {:.1} tok/s",
                        result.ttft_ms, result.total_time_ms, result.tokens_per_second
                    );
                    results.push(BenchmarkResult {
                        prompt_type: prompt_type.to_string(),
                        ..result
                    });
                }
                Err(e) => {
                    println!("ERROR: {}", e);
                }
            }
        }

        if !results.is_empty() {
            let avg_ttft = results.iter().map(|r| r.ttft_ms).sum::<f64>() / results.len() as f64;
            let avg_total =
                results.iter().map(|r| r.total_time_ms).sum::<f64>() / results.len() as f64;
            let avg_tps =
                results.iter().map(|r| r.tokens_per_second).sum::<f64>() / results.len() as f64;

            println!("\n  📊 Average (n={}):", results.len());
            println!("     TTFT: {:.0}ms", avg_ttft);
            println!("     Total: {:.0}ms", avg_total);
            println!("     Throughput: {:.1} tokens/sec", avg_tps);

            all_results.extend(results);
        }
        println!();
    }

    // Summary
    if !all_results.is_empty() {
        println!("════════════════════════════════════════════════════════════════════");
        println!("SUMMARY");
        println!("════════════════════════════════════════════════════════════════════");

        let overall_avg_ttft =
            all_results.iter().map(|r| r.ttft_ms).sum::<f64>() / all_results.len() as f64;
        let overall_avg_tps = all_results.iter().map(|r| r.tokens_per_second).sum::<f64>()
            / all_results.len() as f64;

        println!("Provider: {}", provider.name());
        println!("Model: {}", provider.model());
        println!("Total runs: {}", all_results.len());
        println!("Average TTFT: {:.0}ms", overall_avg_ttft);
        println!("Average throughput: {:.1} tokens/sec", overall_avg_tps);
    }

    Ok(())
}

fn create_provider(name: &str) -> Result<Box<dyn LLMProvider>> {
    match name {
        "zai" => {
            let api_key = std::env::var("ZAI_API_KEY")
                .map_err(|_| anyhow!("ZAI_API_KEY environment variable required"))?;
            let base_url = std::env::var("ZAI_BASE_URL").ok();

            let provider = ZaiProvider::new(
                api_key,
                Some("glm-4.7".to_string()),
                base_url,
                Some(1024),
                Some(0.7),
                true,  // enable_thinking
                false, // preserve_thinking
            )?;
            Ok(Box::new(provider))
        }
        "anthropic" => {
            let api_key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| anyhow!("ANTHROPIC_API_KEY environment variable required"))?;

            let provider = AnthropicProvider::new(
                api_key,
                Some("claude-sonnet-4-5".to_string()),
                Some(1024),
                Some(0.7),
                None,  // cache_config
                None,  // enable_1m_context
                None,  // thinking_budget_tokens
            )?;
            Ok(Box::new(provider))
        }
        "openai" => {
            let api_key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| anyhow!("OPENAI_API_KEY environment variable required"))?;

            let provider = OpenAIProvider::new(
                api_key,
                Some("gpt-4-turbo".to_string()),
                None, // base_url
                Some(1024),
                Some(0.7),
            )?;
            Ok(Box::new(provider))
        }
        _ => Err(anyhow!(
            "Unknown provider: {}. Valid options: zai, anthropic, openai",
            name
        )),
    }
}

async fn run_single_benchmark(
    provider: &dyn LLMProvider,
    prompt: &str,
) -> Result<BenchmarkResult> {
    let request = CompletionRequest {
        messages: vec![Message::new(MessageRole::User, prompt.to_string())],
        max_tokens: Some(256),
        temperature: Some(0.7),
        stream: true,
        tools: None,
        disable_thinking: false,
        stop_sequences: vec![],
    };

    let start = Instant::now();
    let mut stream = provider.stream(request).await?;

    let mut ttft: Option<Duration> = None;
    let mut completion_tokens: u32 = 0;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                // Record time to first token
                if ttft.is_none() && !chunk.content.is_empty() {
                    ttft = Some(start.elapsed());
                }

                if chunk.finished {
                    if let Some(usage) = chunk.usage {
                        completion_tokens = usage.completion_tokens;
                    }
                    break;
                }
            }
            Err(e) => return Err(e),
        }
    }

    let total_time = start.elapsed();
    let ttft = ttft.unwrap_or(total_time);

    let tokens_per_second = if total_time.as_secs_f64() > 0.0 {
        completion_tokens as f64 / total_time.as_secs_f64()
    } else {
        0.0
    };

    Ok(BenchmarkResult {
        prompt_type: String::new(),
        ttft_ms: ttft.as_secs_f64() * 1000.0,
        total_time_ms: total_time.as_secs_f64() * 1000.0,
        completion_tokens,
        tokens_per_second,
    })
}
