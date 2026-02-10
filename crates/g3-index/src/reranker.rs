//! Reranker module for filtering search results using LLM-based relevance judgment.
//!
//! Supports chat completions-based reranking (LM Studio, Ollama, any OpenAI-compatible API)
//! using the Qwen3-Reranker prompt template: binary yes/no relevance filtering.

use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// A document to be reranked.
#[derive(Debug, Clone)]
pub struct RerankerDoc {
    pub id: String,
    pub content: String,
}

/// Result of reranking a single document.
#[derive(Debug, Clone)]
pub struct RerankResult {
    pub id: String,
    pub relevant: bool,
}

/// Trait for reranking search results.
#[async_trait]
pub trait Reranker: Send + Sync {
    /// Judge relevance of documents against a query.
    /// Returns results indicating which documents are relevant.
    async fn rerank(&self, query: &str, documents: &[RerankerDoc]) -> Result<Vec<RerankResult>>;
}

/// Chat completions-based reranker compatible with LM Studio, Ollama, and OpenAI-compatible APIs.
///
/// Uses the Qwen3-Reranker prompt template:
/// - System: "Judge whether the Document meets the requirements based on the Query."
/// - User: "<Instruct>: {instruction}\n<Query>: {query}\n<Document>: {content}"
/// - Response: "yes" or "no"
pub struct ChatReranker {
    client: Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    instruction: String,
}

impl ChatReranker {
    pub fn new(base_url: String, model: String, api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base_url,
            model,
            api_key,
            instruction: "Given a code search query, retrieve relevant code snippets that answer the query".to_string(),
        }
    }

    /// Parse the model response to extract yes/no judgment.
    /// Handles responses that may be wrapped in <think> tags.
    fn parse_relevance(response: &str) -> bool {
        let cleaned = if let Some(after_think) = response.split("</think>").last() {
            after_think.trim()
        } else {
            response.trim()
        };
        cleaned.to_lowercase().starts_with("yes")
    }
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: String,
    /// Qwen3-Reranker returns the yes/no judgment in reasoning_content
    /// when served through LM Studio / vLLM with thinking enabled
    reasoning_content: Option<String>,
}

#[async_trait]
impl Reranker for ChatReranker {
    async fn rerank(&self, query: &str, documents: &[RerankerDoc]) -> Result<Vec<RerankResult>> {
        let url = format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'));

        // Process documents sequentially - local LLM servers (LM Studio, Ollama)
        // can only run one inference at a time, parallel requests cause channel errors
        let mut results = Vec::with_capacity(documents.len());

        for doc in documents {
            let doc_content: String = doc.content.chars().take(2000).collect();

            let request = ChatCompletionRequest {
                model: self.model.clone(),
                messages: vec![
                    ChatMessage {
                        role: "system".to_string(),
                        content: "Judge whether the Document meets the requirements based on the Query. Answer only \"yes\" or \"no\".".to_string(),
                    },
                    ChatMessage {
                        role: "user".to_string(),
                        content: format!(
                            "<Instruct>: {}\n<Query>: {}\n<Document>: {}",
                            self.instruction, query, doc_content
                        ),
                    },
                ],
                max_tokens: 10,
                temperature: 0.0,
            };

            let mut req_builder = self.client.post(&url).json(&request);
            if let Some(key) = &self.api_key {
                req_builder = req_builder.bearer_auth(key);
            }

            let result = match req_builder.send().await {
                Ok(resp) => {
                    match resp.json::<ChatCompletionResponse>().await {
                        Ok(completion) => {
                            let relevant = completion
                                .choices
                                .first()
                                .map(|c| {
                                    // Qwen3-Reranker puts yes/no in reasoning_content
                                    // when served with thinking mode enabled
                                    let text = c.message.reasoning_content
                                        .as_deref()
                                        .unwrap_or(&c.message.content);
                                    ChatReranker::parse_relevance(text)
                                })
                                .unwrap_or(false);

                            debug!(doc_id = %doc.id, relevant, "Reranker judgment");
                            RerankResult {
                                id: doc.id.clone(),
                                relevant,
                            }
                        }
                        Err(e) => {
                            warn!(doc_id = %doc.id, error = %e, "Failed to parse reranker response");
                            RerankResult {
                                id: doc.id.clone(),
                                relevant: true, // Keep on error (don't filter)
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(doc_id = %doc.id, error = %e, "Reranker request failed");
                    RerankResult {
                        id: doc.id.clone(),
                        relevant: true, // Keep on error (don't filter)
                    }
                }
            };

            results.push(result);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_relevance_yes() {
        assert!(ChatReranker::parse_relevance("yes"));
        assert!(ChatReranker::parse_relevance("Yes"));
        assert!(ChatReranker::parse_relevance("YES"));
        assert!(ChatReranker::parse_relevance("yes, this is relevant"));
    }

    #[test]
    fn test_parse_relevance_no() {
        assert!(!ChatReranker::parse_relevance("no"));
        assert!(!ChatReranker::parse_relevance("No"));
        assert!(!ChatReranker::parse_relevance("NO"));
    }

    #[test]
    fn test_parse_relevance_with_think_tags() {
        assert!(ChatReranker::parse_relevance("<think>reasoning here</think>yes"));
        assert!(!ChatReranker::parse_relevance("<think>reasoning here</think>no"));
        assert!(ChatReranker::parse_relevance("<think>let me think...</think>\nYes"));
    }

    #[test]
    fn test_parse_relevance_empty() {
        assert!(!ChatReranker::parse_relevance(""));
        assert!(!ChatReranker::parse_relevance("   "));
    }

    #[test]
    fn test_reranker_doc_creation() {
        let doc = RerankerDoc {
            id: "test-1".to_string(),
            content: "fn hello() {}".to_string(),
        };
        assert_eq!(doc.id, "test-1");
        assert_eq!(doc.content, "fn hello() {}");
    }

    #[test]
    fn test_chat_reranker_new() {
        let reranker = ChatReranker::new(
            "http://localhost:1234".to_string(),
            "qwen3-reranker-8b-mlx".to_string(),
            None,
        );
        assert_eq!(reranker.base_url, "http://localhost:1234");
        assert_eq!(reranker.model, "qwen3-reranker-8b-mlx");
        assert!(reranker.api_key.is_none());
    }
}
