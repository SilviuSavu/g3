//! BM25 keyword search implementation for hybrid search

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// BM25 parameters
const K1: f64 = 1.2; // Term frequency saturation
const B: f64 = 0.75; // Length normalization

/// A document in the BM25 index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub text: String,
    pub tokens: Vec<String>,
}

/// BM25 search index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BM25Index {
    /// All indexed documents
    documents: HashMap<String, Document>,
    /// Inverse document frequency for each term
    idf: HashMap<String, f64>,
    /// Document lengths (in tokens)
    doc_lengths: HashMap<String, usize>,
    /// Average document length
    avg_doc_length: f64,
    /// Total document count
    doc_count: usize,
}

impl BM25Index {
    /// Create a new empty index
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            idf: HashMap::new(),
            doc_lengths: HashMap::new(),
            avg_doc_length: 0.0,
            doc_count: 0,
        }
    }

    /// Tokenize text into terms
    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty() && s.len() > 1)
            .map(String::from)
            .collect()
    }

    /// Add a document to the index
    pub fn add_document(&mut self, id: String, text: String) {
        let tokens = Self::tokenize(&text);
        let doc_length = tokens.len();

        self.documents.insert(
            id.clone(),
            Document {
                id: id.clone(),
                text,
                tokens: tokens.clone(),
            },
        );

        self.doc_lengths.insert(id, doc_length);
        self.doc_count += 1;

        // Recalculate average document length
        let total_length: usize = self.doc_lengths.values().sum();
        self.avg_doc_length = total_length as f64 / self.doc_count as f64;

        // Update document frequency for terms
        let unique_terms: std::collections::HashSet<_> = tokens.iter().collect();
        for term in unique_terms {
            *self.idf.entry(term.clone()).or_insert(0.0) += 1.0;
        }
    }

    /// Remove a document from the index
    pub fn remove_document(&mut self, id: &str) -> bool {
        if let Some(doc) = self.documents.remove(id) {
            self.doc_lengths.remove(id);
            self.doc_count -= 1;

            // Update IDF counts
            let unique_terms: std::collections::HashSet<_> = doc.tokens.iter().collect();
            for term in unique_terms {
                if let Some(count) = self.idf.get_mut(term) {
                    *count -= 1.0;
                    if *count <= 0.0 {
                        self.idf.remove(term);
                    }
                }
            }

            // Recalculate average
            if self.doc_count > 0 {
                let total_length: usize = self.doc_lengths.values().sum();
                self.avg_doc_length = total_length as f64 / self.doc_count as f64;
            } else {
                self.avg_doc_length = 0.0;
            }

            true
        } else {
            false
        }
    }

    /// Calculate IDF for a term
    fn calculate_idf(&self, term: &str) -> f64 {
        let doc_freq = self.idf.get(term).copied().unwrap_or(0.0);
        if doc_freq == 0.0 {
            return 0.0;
        }

        let n = self.doc_count as f64;
        ((n - doc_freq + 0.5) / (doc_freq + 0.5) + 1.0).ln()
    }

    /// Calculate BM25 score for a document given a query
    fn score_document(&self, doc_id: &str, query_terms: &[String]) -> f64 {
        let doc = match self.documents.get(doc_id) {
            Some(d) => d,
            None => return 0.0,
        };

        let doc_length = self.doc_lengths.get(doc_id).copied().unwrap_or(0) as f64;

        // Count term frequencies in document
        let mut term_freqs: HashMap<&str, usize> = HashMap::new();
        for token in &doc.tokens {
            *term_freqs.entry(token.as_str()).or_insert(0) += 1;
        }

        let mut score = 0.0;

        for term in query_terms {
            let idf = self.calculate_idf(term);
            let tf = term_freqs.get(term.as_str()).copied().unwrap_or(0) as f64;

            if tf > 0.0 {
                let numerator = tf * (K1 + 1.0);
                let denominator = tf + K1 * (1.0 - B + B * (doc_length / self.avg_doc_length));
                score += idf * (numerator / denominator);
            }
        }

        score
    }

    /// Search the index and return ranked results
    pub fn search(&self, query: &str, limit: usize) -> Vec<(String, f64)> {
        let query_terms = Self::tokenize(query);

        if query_terms.is_empty() {
            return Vec::new();
        }

        let mut scores: Vec<(String, f64)> = self
            .documents
            .keys()
            .map(|id| {
                let score = self.score_document(id, &query_terms);
                (id.clone(), score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        // Sort by score descending
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scores.truncate(limit);
        scores
    }

    /// Get the number of documents in the index
    pub fn len(&self) -> usize {
        self.doc_count
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.doc_count == 0
    }

    /// Clear the entire index
    pub fn clear(&mut self) {
        self.documents.clear();
        self.idf.clear();
        self.doc_lengths.clear();
        self.avg_doc_length = 0.0;
        self.doc_count = 0;
    }

    /// Save the index to a file
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load the index from a file
    pub fn load(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let index: Self = serde_json::from_str(&json)?;
        Ok(index)
    }
}

impl Default for BM25Index {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let tokens = BM25Index::tokenize("Hello, World! This is a test_function.");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"test_function".to_string()));
    }

    #[test]
    fn test_add_and_search() {
        let mut index = BM25Index::new();
        index.add_document("1".to_string(), "The quick brown fox".to_string());
        index.add_document("2".to_string(), "The lazy dog".to_string());
        index.add_document("3".to_string(), "The quick rabbit".to_string());

        let results = index.search("quick fox", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "1"); // Doc 1 should be top result
    }

    #[test]
    fn test_remove_document() {
        let mut index = BM25Index::new();
        index.add_document("1".to_string(), "Test document".to_string());
        assert_eq!(index.len(), 1);

        index.remove_document("1");
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_is_empty() {
        let index = BM25Index::new();
        assert!(index.is_empty());

        let mut index2 = BM25Index::new();
        index2.add_document("1".to_string(), "doc".to_string());
        assert!(!index2.is_empty());
    }

    #[test]
    fn test_default() {
        let index = BM25Index::default();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_tokenize_single_char_filtered() {
        // Single character tokens should be filtered out
        let tokens = BM25Index::tokenize("a b c def");
        assert!(!tokens.contains(&"a".to_string()));
        assert!(!tokens.contains(&"b".to_string()));
        assert!(!tokens.contains(&"c".to_string()));
        assert!(tokens.contains(&"def".to_string()));
    }

    #[test]
    fn test_tokenize_case_insensitive() {
        let tokens = BM25Index::tokenize("HELLO World HeLLo");
        // All should be lowercase
        assert!(tokens.iter().all(|t| *t == t.to_lowercase()));
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
    }

    #[test]
    fn test_tokenize_special_characters() {
        let tokens = BM25Index::tokenize("fn main() { let x = 42; }");
        assert!(tokens.contains(&"fn".to_string()));
        assert!(tokens.contains(&"main".to_string()));
        assert!(tokens.contains(&"let".to_string()));
        assert!(tokens.contains(&"42".to_string()));
    }

    #[test]
    fn test_tokenize_underscore_preserved() {
        let tokens = BM25Index::tokenize("my_function_name");
        assert!(tokens.contains(&"my_function_name".to_string()));
    }

    #[test]
    fn test_search_empty_query() {
        let mut index = BM25Index::new();
        index.add_document("1".to_string(), "some content".to_string());

        let results = index.search("", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_empty_index() {
        let index = BM25Index::new();
        let results = index.search("anything", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_no_matches() {
        let mut index = BM25Index::new();
        index.add_document("1".to_string(), "apple banana cherry".to_string());

        let results = index.search("xyz123", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_limit() {
        let mut index = BM25Index::new();
        for i in 0..20 {
            index.add_document(
                i.to_string(),
                format!("document number {} with common words", i),
            );
        }

        let results = index.search("document common", 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_search_ranking() {
        let mut index = BM25Index::new();
        index.add_document("1".to_string(), "rust programming language".to_string());
        index.add_document("2".to_string(), "rust rust rust".to_string());
        index.add_document("3".to_string(), "python programming".to_string());

        let results = index.search("rust", 10);
        assert!(!results.is_empty());
        // Doc with more "rust" occurrences should rank higher
        assert_eq!(results[0].0, "2");
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut index = BM25Index::new();
        index.add_document("1".to_string(), "test".to_string());

        let removed = index.remove_document("nonexistent");
        assert!(!removed);
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn test_remove_updates_idf() {
        let mut index = BM25Index::new();
        index.add_document("1".to_string(), "unique term here".to_string());
        index.add_document("2".to_string(), "different content".to_string());

        // Search for unique term
        let results_before = index.search("unique", 10);
        assert!(!results_before.is_empty());

        // Remove the document with unique term
        index.remove_document("1");

        // Now unique term should not find anything
        let results_after = index.search("unique", 10);
        assert!(results_after.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("bm25_index.json");

        let mut index = BM25Index::new();
        index.add_document("doc1".to_string(), "hello world rust".to_string());
        index.add_document("doc2".to_string(), "rust programming language".to_string());

        index.save(&path).unwrap();
        assert!(path.exists());

        let loaded = BM25Index::load(&path).unwrap();
        assert_eq!(loaded.len(), 2);

        // Search should work on loaded index
        let results = loaded.search("rust", 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_load_nonexistent() {
        let result = BM25Index::load(Path::new("/nonexistent/path/index.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_avg_doc_length_calculation() {
        let mut index = BM25Index::new();

        // Add documents of different lengths
        index.add_document("1".to_string(), "one two".to_string()); // 2 tokens
        index.add_document("2".to_string(), "three four five six".to_string()); // 4 tokens

        // Average should be 3.0
        assert!((index.avg_doc_length - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_avg_doc_length_after_remove() {
        let mut index = BM25Index::new();

        index.add_document("1".to_string(), "one two".to_string()); // 2 tokens
        index.add_document("2".to_string(), "three four five six".to_string()); // 4 tokens

        index.remove_document("2");

        // Now only doc 1 with 2 tokens
        assert!((index.avg_doc_length - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_multiple_term_query() {
        let mut index = BM25Index::new();
        index.add_document("1".to_string(), "rust async await programming".to_string());
        index.add_document("2".to_string(), "async javascript callbacks".to_string());
        index.add_document("3".to_string(), "rust systems programming".to_string());

        // Query with multiple terms
        let results = index.search("rust async", 10);
        assert!(!results.is_empty());
        // Doc 1 has both terms, should rank highest
        assert_eq!(results[0].0, "1");
    }
}
