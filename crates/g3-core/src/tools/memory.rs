//! Workspace memory tool: remember.
//!
//! Features:
//! - Fuzzy section deduplication (similar names detected)
//! - Content similarity detection
//! - Section size limits to prevent bloat
//! - Size warning when memory exceeds threshold
//! - Compact tool to clean up existing memory
//!
//! Memory format:
//! ```markdown
//! # Workspace Memory
//! > Updated: 2026-01-15T12:00:00Z | Size: 25.3k chars
//!
//! ### Feature Name
//! Brief description of what this feature/subsystem does.
//!
//! - `file/path.rs`
//!   - `FunctionName()` - what it does
//!   - `StructName` - purpose, key fields
//! ```

use anyhow::Result;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tracing::{info, warn};

use crate::ui_writer::UiWriter;
use crate::ToolCall;

use super::executor::ToolContext;

/// Maximum recommended memory size (in characters) before warning
const MAX_MEMORY_SIZE: usize = 50_000; // ~50KB = ~11K tokens

/// Maximum size per section (in characters) to prevent bloat
const MAX_SECTION_SIZE: usize = 2_000;

/// Minimum similarity ratio (0.0-1.0) to consider sections as duplicates
const SIMILARITY_THRESHOLD: f64 = 0.75;

/// Get the path to the memory file.
/// Memory is stored at `analysis/memory.md` in the working directory (version controlled).
fn get_memory_path(working_dir: Option<&str>) -> PathBuf {
    let base = working_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    base.join("analysis").join("memory.md")
}

/// Format the file size in a human-readable way.
fn format_size(chars: usize) -> String {
    if chars < 1000 {
        format!("{} chars", chars)
    } else {
        format!("{:.1}k chars", chars as f64 / 1000.0)
    }
}

/// Calculate similarity ratio between two strings using simple word overlap.
/// Returns a value between 0.0 (no similarity) and 1.0 (identical).
fn similarity_ratio(a: &str, b: &str) -> f64 {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    
    // Exact match
    if a_lower == b_lower {
        return 1.0;
    }
    
    // Check if one is a substring/prefix of the other (singular/plural, etc.)
    if a_lower.starts_with(&b_lower) || b_lower.starts_with(&a_lower) {
        return 0.9;
    }
    
    // Get words
    let a_words: Vec<&str> = a_lower.split_whitespace().collect();
    let b_words: Vec<&str> = b_lower.split_whitespace().collect();
    
    if a_words.is_empty() || b_words.is_empty() {
        return 0.0;
    }
    
    // Calculate word overlap
    let a_set: HashSet<&str> = a_words.iter().copied().collect();
    let b_set: HashSet<&str> = b_words.iter().copied().collect();
    let intersection = a_set.intersection(&b_set).count();
    
    // Use Dice coefficient: 2 * |intersection| / (|A| + |B|)
    // This is more generous than Jaccard for partial matches
    let dice = (2.0 * intersection as f64) / (a_words.len() + b_words.len()) as f64;
    
    dice
}

/// Check if a section name is similar to any existing section.
/// Returns the name of the similar existing section, if found.
fn find_similar_section<'a>(name: &str, existing_names: &'a [String]) -> Option<&'a str> {
    for existing in existing_names {
        let similarity = similarity_ratio(name, existing);
        if similarity >= SIMILARITY_THRESHOLD {
            return Some(existing.as_str());
        }
    }
    None
}

/// Truncate section content to MAX_SECTION_SIZE, preserving line boundaries.
fn truncate_section_content(content: &str) -> String {
    if content.len() <= MAX_SECTION_SIZE {
        return content.to_string();
    }
    
    // Find a good truncation point (end of line)
    let truncated = &content[..MAX_SECTION_SIZE];
    if let Some(last_newline) = truncated.rfind('\n') {
        format!("{}\n... (truncated)", &content[..last_newline])
    } else {
        format!("{}... (truncated)", truncated)
    }
}

/// Check if content is low-value (e.g., mostly test details, excessive line ranges).
fn is_low_value_content(content: &str) -> bool {
    let content_lower = content.to_lowercase();
    
    // Skip if it's mostly test file references
    let test_mentions = content_lower.matches("test").count();
    let total_words = content.split_whitespace().count();
    if total_words > 10 && test_mentions * 4 > total_words {
        return true;
    }
    
    // Skip if it's excessively verbose with byte ranges
    let byte_ranges = content.matches("[").count() + content.matches("..").count();
    if byte_ranges > 20 {
        return true;
    }
    
    false
}

/// Update workspace memory programmatically (no ToolCall needed).
/// Used by codebase scout to persist stats after a scan.
pub fn update_memory(notes: &str, working_dir: Option<&str>) -> Result<()> {
    let memory_path = get_memory_path(working_dir);

    if let Some(parent) = memory_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let existing = if memory_path.exists() {
        std::fs::read_to_string(&memory_path)?
    } else {
        String::new()
    };

    let updated = merge_memory(&existing, notes);
    let timestamp = Utc::now().format("%Y-%m-%d").to_string();
    let size = format_size(updated.len());

    // Warn if memory is getting large
    if updated.len() > MAX_MEMORY_SIZE {
        warn!(
            "Memory file is large ({}). Consider running memory compaction.",
            format_size(updated.len())
        );
    }

    let final_content = update_header(&updated, &timestamp, &size);

    std::fs::write(&memory_path, &final_content)?;
    Ok(())
}

/// Execute the remember tool.
/// Merges new notes with existing memory and saves to file.
pub async fn execute_remember<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let notes = tool_call
        .args
        .get("notes")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required 'notes' parameter"))?;

    // Validate notes
    let notes = notes.trim();
    if notes.is_empty() {
        return Err(anyhow::anyhow!("Notes cannot be empty"));
    }

    if notes.len() > MAX_MEMORY_SIZE / 2 {
        warn!(
            "Large notes submitted ({}). Consider breaking into smaller sections.",
            format_size(notes.len())
        );
    }

    let memory_path = get_memory_path(ctx.working_dir);

    // Ensure analysis directory exists
    if let Some(parent) = memory_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Read existing memory or create new
    let existing = if memory_path.exists() {
        std::fs::read_to_string(&memory_path)?
    } else {
        String::new()
    };

    // Merge notes with existing memory (with fuzzy deduplication)
    let (updated, skipped) = merge_memory_with_stats(&existing, notes);

    // Add/update header with timestamp and size
    let timestamp = Utc::now().format("%Y-%m-%d").to_string();
    let size = format_size(updated.len());
    let final_content = update_header(&updated, &timestamp, &size);

    // Write back
    std::fs::write(&memory_path, &final_content)?;

    if skipped > 0 {
        Ok(format!(
            "Memory updated. Size: {} ({} duplicate section(s) skipped)",
            format_size(final_content.len()),
            skipped
        ))
    } else {
        Ok(format!(
            "Memory updated. Size: {}",
            format_size(final_content.len())
        ))
    }
}

/// Execute the memory compact tool.
/// Deduplicates sections and removes redundant content.
pub async fn execute_memory_compact<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let memory_path = get_memory_path(ctx.working_dir);

    if !memory_path.exists() {
        return Ok("No memory file found. Nothing to compact.".to_string());
    }

    let existing = std::fs::read_to_string(&memory_path)?;
    let original_size = existing.len();

    // Compact the memory
    let compacted = compact_memory(&existing);
    let compacted_size = compacted.len();

    if compacted_size >= original_size {
        return Ok(format!(
            "Memory already compact. Size: {}",
            format_size(original_size)
        ));
    }

    // Write compacted version
    let timestamp = Utc::now().format("%Y-%m-%d").to_string();
    let size = format_size(compacted_size);
    let final_content = update_header(&compacted, &timestamp, &size);

    std::fs::write(&memory_path, &final_content)?;

    let saved = original_size - compacted_size;
    let saved_pct = (saved as f64 / original_size as f64) * 100.0;

    info!(
        "Memory compacted: {} → {} (saved {}, {:.1}%)",
        format_size(original_size),
        format_size(compacted_size),
        format_size(saved),
        saved_pct
    );

    Ok(format!(
        "Memory compacted: {} → {} (saved {}, {:.1}%)",
        format_size(original_size),
        format_size(compacted_size),
        format_size(saved),
        saved_pct
    ))
}

/// Merge new notes into existing memory with fuzzy deduplication.
/// Returns (merged_content, skipped_count).
fn merge_memory_with_stats(existing: &str, new_notes: &str) -> (String, usize) {
    if existing.is_empty() {
        return (new_notes.trim().to_string(), 0);
    }

    let existing_body = remove_header(existing.trim());
    let new_trimmed = new_notes.trim();

    // Parse existing sections
    let mut sections: HashMap<String, String> = HashMap::new();
    let mut section_order: Vec<String> = Vec::new();
    
    for (name, content) in parse_sections_ordered(&existing_body) {
        if !sections.contains_key(&name) {
            sections.insert(name.clone(), content);
            section_order.push(name);
        }
    }

    // Get existing section names for fuzzy matching
    let existing_names: Vec<String> = section_order.clone();

    // Parse new sections and add only if not similar to existing
    let new_sections = parse_sections_ordered(new_trimmed);
    let mut skipped = 0;

    for (name, content) in new_sections {
        // Skip low-value content
        if is_low_value_content(&content) {
            skipped += 1;
            continue;
        }
        
        // Check for fuzzy duplicates
        if let Some(similar) = find_similar_section(&name, &existing_names) {
            // Section with similar name exists - skip it (keep first occurrence)
            info!("Skipping section '{}' - similar to existing '{}'", name, similar);
            skipped += 1;
            continue;
        }
        
        // Truncate if too large
        let truncated_content = truncate_section_content(&content);
        
        // Add new section
        sections.insert(name.clone(), truncated_content);
        section_order.push(name);
    }

    // Reconstruct memory body in order
    let body = reconstruct_sections_ordered(&sections, &section_order);

    (body, skipped)
}

/// Merge new notes into existing memory with deduplication.
/// Sections with similar names are not duplicated - first occurrence is kept.
fn merge_memory(existing: &str, new_notes: &str) -> String {
    let (result, _) = merge_memory_with_stats(existing, new_notes);
    result
}

/// Parse memory content into ordered list of (section name, content).
fn parse_sections_ordered(content: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_content = String::new();

    for line in content.lines() {
        if line.starts_with("### ") {
            // Save previous section
            if let Some(name) = current_name.take() {
                sections.push((name, current_content.trim().to_string()));
            }
            current_content = String::new();
            current_name = Some(line[4..].trim().to_string());
        } else if current_name.is_some() {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    // Save last section
    if let Some(name) = current_name {
        sections.push((name, current_content.trim().to_string()));
    }

    sections
}

/// Reconstruct sections in the given order.
fn reconstruct_sections_ordered(sections: &HashMap<String, String>, order: &[String]) -> String {
    let mut result = Vec::new();

    for name in order {
        if let Some(content) = sections.get(name) {
            result.push(format!("### {}", name));
            if !content.is_empty() {
                result.push(content.to_string());
            }
        }
    }

    result.join("\n\n")
}

/// Compact memory by removing duplicate sections.
/// Keeps the first occurrence of each section.
pub fn compact_memory(content: &str) -> String {
    let body = remove_header(content.trim());
    
    // Use fuzzy matching for compaction
    let mut seen_names: Vec<String> = Vec::new();
    let mut result = Vec::new();

    for (name, section_content) in parse_sections_ordered(&body) {
        // Check if we've seen a similar name
        if let Some(similar) = find_similar_section(&name, &seen_names) {
            info!("Compacting: '{}' is similar to '{}'", name, similar);
            continue;
        }
        
        // Truncate oversized sections
        let truncated = truncate_section_content(&section_content);
        
        seen_names.push(name.clone());
        result.push(format!("### {}", name));
        if !truncated.is_empty() {
            result.push(truncated);
        }
    }

    result.join("\n\n")
}

/// Remove the header line (# Workspace Memory and > Updated: ...) from content.
fn remove_header(content: &str) -> String {
    let mut lines: Vec<&str> = content.lines().collect();

    // Remove "# Workspace Memory" if first line
    if !lines.is_empty() && lines[0].starts_with("# Workspace Memory") {
        lines.remove(0);
    }

    // Remove "> Updated: ..." line if present at start
    if !lines.is_empty() && lines[0].starts_with("> Updated:") {
        lines.remove(0);
    }

    // Remove leading empty lines
    while !lines.is_empty() && lines[0].trim().is_empty() {
        lines.remove(0);
    }

    lines.join("\n")
}

/// Update or add the header with timestamp and size.
fn update_header(content: &str, timestamp: &str, size: &str) -> String {
    let body = remove_header(content);
    format!(
        "# Workspace Memory\n> Updated: {} | Size: {}\n\n{}",
        timestamp,
        size,
        body.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 chars");
        assert_eq!(format_size(999), "999 chars");
        assert_eq!(format_size(1000), "1.0k chars");
        assert_eq!(format_size(2500), "2.5k chars");
        assert_eq!(format_size(10000), "10.0k chars");
    }

    #[test]
    fn test_similarity_ratio() {
        // Exact match
        assert_eq!(similarity_ratio("Core Abstractions", "Core Abstractions"), 1.0);
        
        // Similar names
        let sim = similarity_ratio("Core Abstractions", "Core Abstraction");
        assert!(sim > 0.75, "Similar names should have high similarity: {}", sim);
        
        // Different names
        let sim = similarity_ratio("Error Handling", "Session Storage");
        assert!(sim < 0.5, "Different names should have low similarity: {}", sim);
        
        // Case insensitive
        let sim = similarity_ratio("CORE ABSTRACTIONS", "core abstractions");
        assert_eq!(sim, 1.0, "Should be case insensitive");
    }

    #[test]
    fn test_find_similar_section() {
        let existing = vec![
            "Core Abstractions".to_string(),
            "Session Storage".to_string(),
        ];
        
        // Exact match
        assert_eq!(find_similar_section("Core Abstractions", &existing), Some("Core Abstractions"));
        
        // Similar name
        assert_eq!(find_similar_section("Core Abstraction", &existing), Some("Core Abstractions"));
        
        // Different name
        assert_eq!(find_similar_section("Error Handling", &existing), None);
    }

    #[test]
    fn test_truncate_section_content() {
        // Short content unchanged
        let short = "Short content";
        assert_eq!(truncate_section_content(short), short);
        
        // Long content truncated
        let long = "x".repeat(3000);
        let truncated = truncate_section_content(&long);
        assert!(truncated.len() <= MAX_SECTION_SIZE + 20); // +20 for "... (truncated)"
        assert!(truncated.ends_with("(truncated)"));
    }

    #[test]
    fn test_is_low_value_content() {
        // Test-heavy content (>10 words, >25% "test")
        let test_content = "test_foo test_bar test_baz test helper test runner test mock test stub test fixture test suite test case test util";
        assert!(is_low_value_content(test_content));

        // Excessive byte ranges (>20 occurrences of [ or ..)
        let range_content = "func [1..10] handler [20..30] parser [40..50] scanner [60..70] lexer [80..90] reader [100..110] writer [120..130] builder [140..150] mapper [160..170] filter [180..190] sorter [200..210]";
        assert!(is_low_value_content(range_content));

        // Good content
        let good_content = "Key function for handling errors in the system";
        assert!(!is_low_value_content(good_content));
    }

    #[test]
    fn test_merge_memory_empty() {
        let result = merge_memory("", "### New Feature\n- `file.rs` - `func()`");
        assert_eq!(result, "### New Feature\n- `file.rs` - `func()`");
    }

    #[test]
    fn test_merge_memory_append() {
        let existing =
            "# Workspace Memory\n> Updated: 2025-01-10 | Size: 1k\n\n### Feature A\n- `a.rs` - func";
        let new_notes = "### Feature B\n- `b.rs` - func";
        let result = merge_memory(existing, new_notes);

        assert!(result.contains("### Feature A"));
        assert!(result.contains("### Feature B"));
    }

    #[test]
    fn test_merge_memory_exact_dedup() {
        let existing =
            "# Workspace Memory\n> Updated: 2025-01-10\n\n### Feature A\n- `a.rs` - func";
        let new_notes = "### Feature A\n- `a.rs` - UPDATED";
        let result = merge_memory(existing, new_notes);

        // Should keep original (first occurrence), not the new one
        assert!(result.contains("### Feature A"));
        assert!(result.contains("- `a.rs` - func"));
        assert!(!result.contains("UPDATED"));
        // Should only appear once
        assert_eq!(result.matches("### Feature A").count(), 1);
    }

    #[test]
    fn test_merge_memory_fuzzy_dedup() {
        let existing =
            "# Workspace Memory\n> Updated: 2025-01-10\n\n### Core Abstractions\n- `a.rs` - func";
        let new_notes = "### Core Abstraction\n- `b.rs` - func";
        let result = merge_memory(existing, new_notes);

        // Should keep original (first occurrence) due to fuzzy match
        assert_eq!(result.matches("### Core").count(), 1);
        assert!(result.contains("- `a.rs`"));
        assert!(!result.contains("- `b.rs`"));
    }

    #[test]
    fn test_merge_memory_skips_low_value() {
        let existing = "# Workspace Memory\n> Updated: 2025-01-10\n\n### Feature A\n- func";
        // This should be skipped - test-heavy (>10 words, >25% "test")
        let new_notes = "### Test Details\n- test_foo test_bar test_baz test helper test runner test mock test stub test fixture test suite test case test util";
        let (result, skipped) = merge_memory_with_stats(existing, new_notes);

        assert!(!result.contains("Test Details"));
        assert_eq!(skipped, 1);
    }

    #[test]
    fn test_parse_sections_ordered() {
        let content = "### Feature A\n- `a.rs`\n\n### Feature B\n- `b.rs`";
        let sections = parse_sections_ordered(content);

        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].0, "Feature A");
        assert_eq!(sections[1].0, "Feature B");
    }

    #[test]
    fn test_compact_memory() {
        let content = "# Workspace Memory\n> Updated: 2025-01-10\n\n### Feature A\n- `a.rs`\n\n### Feature A\n- `a.rs` duplicate\n\n### Feature B\n- `b.rs`";
        let compacted = compact_memory(content);

        // Should only have one Feature A (first occurrence)
        assert_eq!(compacted.matches("### Feature A").count(), 1);
        assert!(compacted.contains("### Feature B"));
    }

    #[test]
    fn test_compact_memory_fuzzy() {
        let content = "# Workspace Memory\n> Updated: 2025-01-10\n\n### Core Abstractions\n- content a\n\n### Core Abstraction\n- content b\n\n### Session\n- content c";
        let compacted = compact_memory(content);

        // Should only have one "Core" section due to fuzzy matching
        assert_eq!(compacted.matches("### Core").count(), 1);
        assert!(compacted.contains("### Session"));
    }

    #[test]
    fn test_remove_header() {
        let content = "# Workspace Memory\n> Updated: 2025-01-10 | Size: 1k\n\n### Feature\n- details";
        let result = remove_header(content);
        assert!(!result.contains("# Workspace Memory"));
        assert!(!result.contains("> Updated:"));
        assert!(result.contains("### Feature"));
    }

    #[test]
    fn test_update_header() {
        let content = "### Feature\n- details";
        let result = update_header(content, "2025-01-10", "500 chars");

        assert!(result.starts_with("# Workspace Memory"));
        assert!(result.contains("> Updated: 2025-01-10 | Size: 500 chars"));
        assert!(result.contains("### Feature"));
    }
}
