//! Qdrant vector database client wrapper.
//!
//! This module provides a high-level interface to Qdrant
//! for storing and searching code embeddings.

use anyhow::{Context, Result};
use qdrant_client::qdrant::{
    Condition, CreateCollectionBuilder, DeletePointsBuilder, Distance, Filter, PointStruct,
    PointsIdsList, QuantizationType, ScalarQuantizationBuilder, SearchPointsBuilder,
    UpsertPointsBuilder, VectorParamsBuilder,
};
use qdrant_client::Qdrant;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

// Re-export QdrantError for use by other modules
pub use qdrant_client::QdrantError;

/// Metadata stored with each vector point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointPayload {
    /// Full path to the source file
    pub file_path: String,

    /// Type of code chunk: "function", "struct", "impl", "trait", "class", "method", etc.
    pub chunk_type: String,

    /// Name of the function/struct/trait/etc.
    pub name: String,

    /// Full signature, e.g., "pub async fn foo(&self) -> Result<String>"
    pub signature: Option<String>,

    /// Starting line number (1-indexed)
    pub line_start: usize,

    /// Ending line number (1-indexed)
    pub line_end: usize,

    /// Module path, e.g., "crate::foo::bar"
    pub module: Option<String>,

    /// Enclosing scope, e.g., "impl Foo" or "impl Trait for Bar"
    pub scope: Option<String>,

    /// The actual source code of this chunk
    pub code: String,
}

impl Default for PointPayload {
    fn default() -> Self {
        Self {
            file_path: String::new(),
            chunk_type: String::new(),
            name: String::new(),
            signature: None,
            line_start: 0,
            line_end: 0,
            module: None,
            scope: None,
            code: String::new(),
        }
    }
}

/// Configuration for connecting to Qdrant.
#[derive(Debug, Clone)]
pub struct QdrantConfig {
    /// Qdrant server URL
    pub url: String,

    /// API key (optional)
    pub api_key: Option<String>,

    /// Collection name
    pub collection_name: String,

    /// Vector dimensions
    pub dimensions: usize,
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:6334".to_string(),
            api_key: None,
            collection_name: crate::DEFAULT_COLLECTION.to_string(),
            dimensions: crate::DEFAULT_DIMENSIONS,
        }
    }
}

/// A point to upsert into Qdrant.
#[derive(Debug, Clone)]
pub struct Point {
    /// Unique identifier (UUID string)
    pub id: String,

    /// Embedding vector
    pub vector: Vec<f32>,

    /// Payload metadata
    pub payload: PointPayload,
}

/// A search hit result from Qdrant.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Point ID
    pub id: String,

    /// Similarity score (higher is better for cosine)
    pub score: f32,

    /// Payload metadata
    pub payload: PointPayload,
}

/// Filter conditions for search.
#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    /// Filter by file path prefix
    pub file_path_prefix: Option<String>,

    /// Filter by chunk types (e.g., ["function", "struct"])
    pub chunk_types: Option<Vec<String>>,

    /// Filter by programming language
    pub language: Option<String>,
}

impl SearchFilter {
    /// Create a new empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by file path prefix.
    pub fn with_path_prefix(mut self, prefix: String) -> Self {
        self.file_path_prefix = Some(prefix);
        self
    }

    /// Filter by chunk types.
    pub fn with_chunk_types(mut self, types: Vec<String>) -> Self {
        self.chunk_types = Some(types);
        self
    }

    /// Filter by programming language.
    pub fn with_language(mut self, language: String) -> Self {
        self.language = Some(language);
        self
    }
}

/// High-level Qdrant client for code search.
pub struct QdrantClient {
    client: Qdrant,
    collection_name: String,
    dimensions: usize,
}

impl QdrantClient {
    /// Connect to Qdrant server.
    ///
    /// # Arguments
    /// * `url` - Qdrant server URL (e.g., "http://localhost:6334")
    /// * `collection_name` - Name of the collection to use
    /// * `dimensions` - Vector dimensions (must match embedding model output)
    pub async fn connect(url: &str, collection_name: &str, dimensions: usize) -> Result<Self> {
        info!("Connecting to Qdrant at {}", url);

        let client = Qdrant::from_url(url)
            .skip_compatibility_check()
            .build()
            .context("Failed to connect to Qdrant")?;

        Ok(Self {
            client,
            collection_name: collection_name.to_string(),
            dimensions,
        })
    }

    /// Create a new Qdrant client from configuration.
    pub async fn from_config(config: &QdrantConfig) -> Result<Self> {
        let mut builder = Qdrant::from_url(&config.url)
            .skip_compatibility_check();

        if let Some(ref api_key) = config.api_key {
            builder = builder.api_key(api_key.clone());
        }

        let client = builder.build().context("Failed to connect to Qdrant")?;

        info!("Connected to Qdrant at {}", config.url);

        Ok(Self {
            client,
            collection_name: config.collection_name.clone(),
            dimensions: config.dimensions,
        })
    }

    /// Create collection if it doesn't exist (with scalar quantization for 4x compression).
    pub async fn ensure_collection(&self) -> Result<()> {
        // Check if collection exists
        let collections = self.client.list_collections().await?;
        let exists = collections
            .collections
            .iter()
            .any(|c| c.name == self.collection_name);

        if !exists {
            info!(
                "Creating collection: {} with {} dimensions",
                self.collection_name, self.dimensions
            );

            self.client
                .create_collection(
                    CreateCollectionBuilder::new(&self.collection_name)
                        .vectors_config(VectorParamsBuilder::new(
                            self.dimensions as u64,
                            Distance::Cosine,
                        ))
                        .quantization_config(
                            ScalarQuantizationBuilder::default()
                                .r#type(QuantizationType::Int8.into())
                                .quantile(0.99)
                                .always_ram(true),
                        ),
                )
                .await
                .context("Failed to create collection")?;

            info!("Collection {} created successfully", self.collection_name);
        } else {
            debug!("Collection {} already exists", self.collection_name);
        }

        Ok(())
    }

    /// Upsert points (vectors with payloads).
    pub async fn upsert_points(&self, points: Vec<Point>) -> Result<()> {
        if points.is_empty() {
            debug!("No points to upsert");
            return Ok(());
        }

        debug!("Upserting {} points", points.len());

        let qdrant_points: Vec<PointStruct> = points
            .into_iter()
            .map(|p| {
                let payload: HashMap<String, qdrant_client::qdrant::Value> =
                    payload_to_qdrant_map(&p.payload);
                PointStruct::new(p.id, p.vector, payload)
            })
            .collect();

        self.client
            .upsert_points(UpsertPointsBuilder::new(
                &self.collection_name,
                qdrant_points,
            ))
            .await
            .context("Failed to upsert points")?;

        Ok(())
    }

    /// Search for similar vectors.
    ///
    /// # Arguments
    /// * `query_vector` - The embedding vector to search for
    /// * `limit` - Maximum number of results to return
    /// * `filter` - Optional filter conditions
    pub async fn search(
        &self,
        query_vector: Vec<f32>,
        limit: usize,
        filter: Option<SearchFilter>,
    ) -> Result<Vec<SearchHit>> {
        debug!("Searching for {} similar vectors", limit);

        let mut search_builder =
            SearchPointsBuilder::new(&self.collection_name, query_vector, limit as u64)
                .with_payload(true);

        // Build filter conditions
        if let Some(f) = filter {
            let mut conditions: Vec<Condition> = Vec::new();

            if let Some(path_prefix) = f.file_path_prefix {
                // Use match_text for prefix matching on file_path
                conditions.push(Condition::matches("file_path", path_prefix));
            }

            if let Some(chunk_types) = f.chunk_types {
                // For multiple chunk types, use nested filter condition
                if !chunk_types.is_empty() {
                    let type_conditions: Vec<Condition> = chunk_types
                        .into_iter()
                        .map(|ct| Condition::matches("chunk_type", ct))
                        .collect();
                    // Wrap in a nested filter for "should" (OR) logic
                    let nested = Condition {
                        condition_one_of: Some(
                            qdrant_client::qdrant::condition::ConditionOneOf::Filter(
                                Filter::should(type_conditions),
                            ),
                        ),
                    };
                    conditions.push(nested);
                }
            }

            if !conditions.is_empty() {
                search_builder = search_builder.filter(Filter::must(conditions));
            }
        }

        let results = self
            .client
            .search_points(search_builder)
            .await
            .context("Failed to search points")?;

        let hits: Vec<SearchHit> = results
            .result
            .into_iter()
            .map(|p| {
                let payload = qdrant_map_to_payload(&p.payload);

                let id = p
                    .id
                    .map(|id| match id.point_id_options {
                        Some(qdrant_client::qdrant::point_id::PointIdOptions::Uuid(u)) => u,
                        Some(qdrant_client::qdrant::point_id::PointIdOptions::Num(n)) => {
                            n.to_string()
                        }
                        None => String::new(),
                    })
                    .unwrap_or_default();

                SearchHit {
                    id,
                    score: p.score,
                    payload,
                }
            })
            .collect();

        debug!("Found {} search hits", hits.len());
        Ok(hits)
    }

    /// Delete points by IDs.
    pub async fn delete_points(&self, ids: Vec<String>) -> Result<()> {
        if ids.is_empty() {
            debug!("No points to delete");
            return Ok(());
        }

        debug!("Deleting {} points", ids.len());

        // Convert string IDs to point IDs
        let point_ids: Vec<_> = ids
            .into_iter()
            .map(qdrant_client::qdrant::PointId::from)
            .collect();

        self.client
            .delete_points(
                DeletePointsBuilder::new(&self.collection_name)
                    .points(PointsIdsList { ids: point_ids }),
            )
            .await
            .context("Failed to delete points")?;

        Ok(())
    }

    /// Count points in the collection.
    pub async fn count(&self) -> Result<usize> {
        let info = self
            .client
            .collection_info(&self.collection_name)
            .await
            .context("Failed to get collection info")?;

        let count = info
            .result
            .map(|r| r.points_count.unwrap_or(0) as usize)
            .unwrap_or(0);

        Ok(count)
    }

    /// Delete the entire collection.
    pub async fn delete_collection(&self) -> Result<()> {
        info!("Deleting collection: {}", self.collection_name);

        self.client
            .delete_collection(&self.collection_name)
            .await
            .context("Failed to delete collection")?;

        Ok(())
    }

    /// Get collection name.
    pub fn collection_name(&self) -> &str {
        &self.collection_name
    }

    /// Get configured dimensions.
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Convert PointPayload to Qdrant's HashMap<String, Value>.
fn payload_to_qdrant_map(payload: &PointPayload) -> HashMap<String, qdrant_client::qdrant::Value> {
    let mut map = HashMap::new();

    map.insert(
        "file_path".to_string(),
        qdrant_client::qdrant::Value::from(payload.file_path.clone()),
    );
    map.insert(
        "chunk_type".to_string(),
        qdrant_client::qdrant::Value::from(payload.chunk_type.clone()),
    );
    map.insert(
        "name".to_string(),
        qdrant_client::qdrant::Value::from(payload.name.clone()),
    );
    map.insert(
        "line_start".to_string(),
        qdrant_client::qdrant::Value::from(payload.line_start as i64),
    );
    map.insert(
        "line_end".to_string(),
        qdrant_client::qdrant::Value::from(payload.line_end as i64),
    );
    map.insert(
        "code".to_string(),
        qdrant_client::qdrant::Value::from(payload.code.clone()),
    );

    if let Some(ref sig) = payload.signature {
        map.insert(
            "signature".to_string(),
            qdrant_client::qdrant::Value::from(sig.clone()),
        );
    }

    if let Some(ref module) = payload.module {
        map.insert(
            "module".to_string(),
            qdrant_client::qdrant::Value::from(module.clone()),
        );
    }

    if let Some(ref scope) = payload.scope {
        map.insert(
            "scope".to_string(),
            qdrant_client::qdrant::Value::from(scope.clone()),
        );
    }

    map
}

/// Convert Qdrant's HashMap<String, Value> back to PointPayload.
fn qdrant_map_to_payload(map: &HashMap<String, qdrant_client::qdrant::Value>) -> PointPayload {
    PointPayload {
        file_path: extract_string(map.get("file_path")),
        chunk_type: extract_string(map.get("chunk_type")),
        name: extract_string(map.get("name")),
        signature: map.get("signature").and_then(|v| extract_string_opt(v)),
        line_start: extract_integer(map.get("line_start")) as usize,
        line_end: extract_integer(map.get("line_end")) as usize,
        module: map.get("module").and_then(|v| extract_string_opt(v)),
        scope: map.get("scope").and_then(|v| extract_string_opt(v)),
        code: extract_string(map.get("code")),
    }
}

fn extract_string(value: Option<&qdrant_client::qdrant::Value>) -> String {
    value
        .and_then(|v| {
            if let Some(qdrant_client::qdrant::value::Kind::StringValue(s)) = &v.kind {
                Some(s.clone())
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn extract_string_opt(value: &qdrant_client::qdrant::Value) -> Option<String> {
    if let Some(qdrant_client::qdrant::value::Kind::StringValue(s)) = &value.kind {
        Some(s.clone())
    } else {
        None
    }
}

fn extract_integer(value: Option<&qdrant_client::qdrant::Value>) -> i64 {
    value
        .and_then(|v| {
            if let Some(qdrant_client::qdrant::value::Kind::IntegerValue(i)) = &v.kind {
                Some(*i)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_payload_default() {
        let payload = PointPayload::default();
        assert!(payload.file_path.is_empty());
        assert!(payload.chunk_type.is_empty());
        assert!(payload.name.is_empty());
        assert!(payload.signature.is_none());
        assert_eq!(payload.line_start, 0);
        assert_eq!(payload.line_end, 0);
        assert!(payload.module.is_none());
        assert!(payload.scope.is_none());
        assert!(payload.code.is_empty());
    }

    #[test]
    fn test_search_filter_builder() {
        let filter = SearchFilter::new()
            .with_path_prefix("src/".to_string())
            .with_chunk_types(vec!["function".to_string(), "struct".to_string()])
            .with_language("rust".to_string());

        assert_eq!(filter.file_path_prefix, Some("src/".to_string()));
        assert_eq!(
            filter.chunk_types,
            Some(vec!["function".to_string(), "struct".to_string()])
        );
        assert_eq!(filter.language, Some("rust".to_string()));
    }

    #[test]
    fn test_payload_to_qdrant_map() {
        let payload = PointPayload {
            file_path: "src/main.rs".to_string(),
            chunk_type: "function".to_string(),
            name: "main".to_string(),
            signature: Some("fn main()".to_string()),
            line_start: 1,
            line_end: 10,
            module: Some("crate".to_string()),
            scope: None,
            code: "fn main() { }".to_string(),
        };

        let map = payload_to_qdrant_map(&payload);

        assert!(map.contains_key("file_path"));
        assert!(map.contains_key("chunk_type"));
        assert!(map.contains_key("name"));
        assert!(map.contains_key("signature"));
        assert!(map.contains_key("line_start"));
        assert!(map.contains_key("line_end"));
        assert!(map.contains_key("module"));
        assert!(!map.contains_key("scope")); // None values are not inserted
        assert!(map.contains_key("code"));
    }

    #[test]
    fn test_qdrant_config_default() {
        let config = QdrantConfig::default();
        assert_eq!(config.url, "http://localhost:6334");
        assert!(config.api_key.is_none());
        assert_eq!(config.collection_name, crate::DEFAULT_COLLECTION);
        assert_eq!(config.dimensions, crate::DEFAULT_DIMENSIONS);
    }

    #[test]
    fn test_payload_roundtrip() {
        // Test that converting to qdrant map and back preserves data
        let original = PointPayload {
            file_path: "src/lib.rs".to_string(),
            chunk_type: "struct".to_string(),
            name: "MyStruct".to_string(),
            signature: Some("pub struct MyStruct".to_string()),
            line_start: 10,
            line_end: 25,
            module: Some("crate::module".to_string()),
            scope: Some("impl Foo".to_string()),
            code: "pub struct MyStruct { field: i32 }".to_string(),
        };

        let map = payload_to_qdrant_map(&original);
        let restored = qdrant_map_to_payload(&map);

        assert_eq!(restored.file_path, original.file_path);
        assert_eq!(restored.chunk_type, original.chunk_type);
        assert_eq!(restored.name, original.name);
        assert_eq!(restored.signature, original.signature);
        assert_eq!(restored.line_start, original.line_start);
        assert_eq!(restored.line_end, original.line_end);
        assert_eq!(restored.module, original.module);
        assert_eq!(restored.scope, original.scope);
        assert_eq!(restored.code, original.code);
    }

    #[test]
    fn test_payload_roundtrip_with_none_values() {
        let original = PointPayload {
            file_path: "test.py".to_string(),
            chunk_type: "function".to_string(),
            name: "test_fn".to_string(),
            signature: None,
            line_start: 1,
            line_end: 5,
            module: None,
            scope: None,
            code: "def test_fn(): pass".to_string(),
        };

        let map = payload_to_qdrant_map(&original);
        let restored = qdrant_map_to_payload(&map);

        assert_eq!(restored.file_path, original.file_path);
        assert_eq!(restored.signature, None);
        assert_eq!(restored.module, None);
        assert_eq!(restored.scope, None);
    }

    #[test]
    fn test_search_filter_empty() {
        let filter = SearchFilter::new();
        assert!(filter.file_path_prefix.is_none());
        assert!(filter.chunk_types.is_none());
        assert!(filter.language.is_none());
    }

    #[test]
    fn test_search_filter_partial() {
        let filter = SearchFilter::new()
            .with_path_prefix("crates/".to_string());

        assert_eq!(filter.file_path_prefix, Some("crates/".to_string()));
        assert!(filter.chunk_types.is_none());
        assert!(filter.language.is_none());
    }

    #[test]
    fn test_point_struct() {
        let point = Point {
            id: "uuid-123".to_string(),
            vector: vec![0.1, 0.2, 0.3],
            payload: PointPayload::default(),
        };

        assert_eq!(point.id, "uuid-123");
        assert_eq!(point.vector.len(), 3);
    }

    #[test]
    fn test_search_hit_struct() {
        let hit = SearchHit {
            id: "hit-456".to_string(),
            score: 0.95,
            payload: PointPayload {
                file_path: "src/main.rs".to_string(),
                name: "main".to_string(),
                ..Default::default()
            },
        };

        assert_eq!(hit.id, "hit-456");
        assert!((hit.score - 0.95).abs() < f32::EPSILON);
        assert_eq!(hit.payload.name, "main");
    }

    #[test]
    fn test_qdrant_config_with_api_key() {
        let config = QdrantConfig {
            url: "https://qdrant.example.com:6333".to_string(),
            api_key: Some("secret-key".to_string()),
            collection_name: "my-collection".to_string(),
            dimensions: 1536,
        };

        assert_eq!(config.url, "https://qdrant.example.com:6333");
        assert_eq!(config.api_key, Some("secret-key".to_string()));
        assert_eq!(config.collection_name, "my-collection");
        assert_eq!(config.dimensions, 1536);
    }

    #[test]
    fn test_extract_functions_with_empty_map() {
        let map: HashMap<String, qdrant_client::qdrant::Value> = HashMap::new();
        let payload = qdrant_map_to_payload(&map);

        // Should return defaults for missing values
        assert!(payload.file_path.is_empty());
        assert!(payload.name.is_empty());
        assert_eq!(payload.line_start, 0);
        assert_eq!(payload.line_end, 0);
    }
}
