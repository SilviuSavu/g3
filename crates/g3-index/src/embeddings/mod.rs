//! Embedding provider trait and implementations.
//!
//! This module defines the interface for generating embeddings
//! and provides implementations for various embedding APIs.

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, warn};

/// Errors that can occur during embedding generation.
#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error("API request failed: {0}")]
    ApiError(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Rate limited, retry after {0} seconds")]
    RateLimited(u64),

    #[error("Network error: {0}")]
    NetworkError(String),
}

/// Trait for embedding providers.
///
/// Implementations should be Send + Sync to allow use in async contexts.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed multiple texts in a batch
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Get the number of dimensions
    fn dimensions(&self) -> usize;

    /// Get the model name
    fn model_name(&self) -> &str;

    /// Get the maximum batch size supported.
    fn max_batch_size(&self) -> usize {
        32
    }
}

/// Request body for embedding API
#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

/// Response from embedding API
#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

/// Individual embedding data in response
#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

/// OpenRouter embedding provider using Qwen3-Embedding-8B.
pub struct OpenRouterEmbeddings {
    api_key: String,
    model: String,
    dimensions: usize,
    client: Client,
    base_url: String,
}

impl OpenRouterEmbeddings {
    /// Create a new OpenRouter embeddings provider with default settings.
    ///
    /// Uses Qwen3-Embedding-8B with 4096 dimensions by default.
    pub fn new(api_key: String, model: Option<String>, dimensions: Option<usize>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| "qwen/qwen3-embedding-8b".to_string()),
            dimensions: dimensions.unwrap_or(4096),
            client: Client::new(),
            base_url: "https://openrouter.ai/api/v1/embeddings".to_string(),
        }
    }

    /// Set a custom base URL (useful for testing or proxies).
    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    /// Send embedding request with retry logic for rate limits.
    async fn send_request(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let request_body = EmbeddingRequest {
            model: self.model.clone(),
            input: texts.clone(),
        };

        let mut retry_count = 0;
        let max_retries = 3;
        let mut backoff_secs = 1u64;

        loop {
            debug!(
                "Sending embedding request for {} texts to {}",
                texts.len(),
                self.base_url
            );

            let response = self
                .client
                .post(&self.base_url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&request_body)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Network error: {}", e))?;

            let status = response.status();

            if status.is_success() {
                let embedding_response: EmbeddingResponse = response
                    .json()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

                // Sort by index to ensure correct order
                let mut embeddings: Vec<(usize, Vec<f32>)> = embedding_response
                    .data
                    .into_iter()
                    .map(|d| (d.index, d.embedding))
                    .collect();
                embeddings.sort_by_key(|(idx, _)| *idx);

                return Ok(embeddings.into_iter().map(|(_, emb)| emb).collect());
            }

            if status.as_u16() == 429 {
                // Rate limited
                retry_count += 1;
                if retry_count > max_retries {
                    return Err(anyhow::anyhow!(
                        "Rate limited after {} retries",
                        max_retries
                    ));
                }

                // Try to extract retry-after header
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(backoff_secs);

                warn!(
                    "Rate limited, retrying after {} seconds (attempt {}/{})",
                    retry_after, retry_count, max_retries
                );

                tokio::time::sleep(tokio::time::Duration::from_secs(retry_after)).await;
                backoff_secs *= 2; // Exponential backoff
                continue;
            }

            // Other error
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!(
                "API error ({}): {}",
                status.as_u16(),
                error_body
            ));
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenRouterEmbeddings {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed_batch(&[text.to_string()]).await?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding returned"))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        debug!("Embedding batch of {} texts", texts.len());
        self.send_request(texts.to_vec()).await
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn max_batch_size(&self) -> usize {
        32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = OpenRouterEmbeddings::new("test-key".to_string(), None, None);
        assert_eq!(provider.dimensions(), 4096);
        assert_eq!(provider.model_name(), "qwen/qwen3-embedding-8b");
        assert_eq!(provider.max_batch_size(), 32);
    }

    #[test]
    fn test_provider_custom_model() {
        let provider = OpenRouterEmbeddings::new(
            "test-key".to_string(),
            Some("custom/model".to_string()),
            Some(1024),
        );
        assert_eq!(provider.dimensions(), 1024);
        assert_eq!(provider.model_name(), "custom/model");
    }

    #[test]
    fn test_provider_with_base_url() {
        let provider = OpenRouterEmbeddings::new("test-key".to_string(), None, None)
            .with_base_url("http://localhost:8080/embeddings".to_string());
        assert_eq!(provider.base_url, "http://localhost:8080/embeddings");
    }
}
