//! Workspace memory tool: remember.
//!
//! Features:
//! - Automatic section deduplication (keeps first occurrence)
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
//!   - `FunctionName()` [1200..1450] - what it does, key params/return
//!   - `StructName` [500..650] - purpose, key fields
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
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
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

    // Merge notes with existing memory (with deduplication)
    let updated = merge_memory(&existing, notes);

    // Add/update header with timestamp and size
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let size = format_size(updated.len());
    let final_content = update_header(&updated, &timestamp, &size);

    // Write back
    std::fs::write(&memory_path, &final_content)?;

    Ok(format!(
        "Memory updated. Size: {}",
        format_size(final_content.len())
    ))
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
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
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

/// Merge new notes into existing memory with deduplication.
/// Sections with the same name are not duplicated - first occurrence is kept.
fn merge_memory(existing: &str, new_notes: &str) -> String {
    if existing.is_empty() {
        return new_notes.trim().to_string();
    }

    let existing_body = remove_header(existing.trim());
    let new_trimmed = new_notes.trim();

    // Parse existing sections
    let mut sections = parse_sections(&existing_body);

    // Parse new sections and add only if not already present
    let new_sections = parse_sections(new_trimmed);

    for (name, content) in new_sections {
        if !sections.contains_key(&name) {
            sections.insert(name, content);
        }
    }

    // Reconstruct memory body in order
    let body = reconstruct_sections(&sections, &existing_body, new_trimmed);

    body
}

/// Parse memory content into a map of section name -> content.
fn parse_sections(content: &str) -> HashMap<String, String> {
    let mut sections = HashMap::new();
    let mut current_name: Option<String> = None;
    let mut current_content = String::new();

    for line in content.lines() {
        if line.starts_with("### ") {
            // Save previous section
            if let Some(name) = current_name.take() {
                // Only insert if not already present (keep first occurrence)
                sections.entry(name).or_insert_with(|| current_content.trim().to_string());
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
        sections.entry(name).or_insert_with(|| current_content.trim().to_string());
    }

    sections
}

/// Reconstruct sections preserving original order + new sections at end.
fn reconstruct_sections(
    sections: &HashMap<String, String>,
    existing_body: &str,
    new_notes: &str,
) -> String {
    let mut result = Vec::new();
    let mut seen = HashSet::new();

    // First, add existing sections in their original order
    for line in existing_body.lines() {
        if line.starts_with("### ") {
            let name = line[4..].trim().to_string();
            if seen.insert(name.clone()) {
                if let Some(content) = sections.get(&name) {
                    result.push(format!("### {}", name));
                    if !content.is_empty() {
                        result.push(content.to_string());
                    }
                }
            }
        }
    }

    // Then add new sections that weren't in existing
    for line in new_notes.lines() {
        if line.starts_with("### ") {
            let name = line[4..].trim().to_string();
            if seen.insert(name.clone()) {
                if let Some(content) = sections.get(&name) {
                    result.push(format!("### {}", name));
                    if !content.is_empty() {
                        result.push(content.to_string());
                    }
                }
            }
        }
    }

    result.join("\n\n")
}

/// Compact memory by removing duplicate sections.
/// Keeps the first occurrence of each section.
pub fn compact_memory(content: &str) -> String {
    let body = remove_header(content.trim());
    let sections = parse_sections(&body);

    // Reconstruct preserving first occurrence order
    let mut result = Vec::new();
    let mut seen = HashSet::new();

    for line in body.lines() {
        if line.starts_with("### ") {
            let name = line[4..].trim().to_string();
            if seen.insert(name.clone()) {
                if let Some(section_content) = sections.get(&name) {
                    result.push(format!("### {}", name));
                    if !section_content.is_empty() {
                        result.push(section_content.clone());
                    }
                }
            }
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
    fn test_merge_memory_empty() {
        let result = merge_memory("", "### New Feature\n- `file.rs` [0..100] - `func()`");
        assert_eq!(result, "### New Feature\n- `file.rs` [0..100] - `func()`");
    }

    #[test]
    fn test_merge_memory_append() {
        let existing =
            "# Workspace Memory\n> Updated: 2025-01-10 | Size: 1k\n\n### Feature A\n- `a.rs` [0..50]";
        let new_notes = "### Feature B\n- `b.rs` [0..100]";
        let result = merge_memory(existing, new_notes);

        assert!(result.contains("### Feature A"));
        assert!(result.contains("### Feature B"));
    }

    #[test]
    fn test_merge_memory_dedup() {
        let existing =
            "# Workspace Memory\n> Updated: 2025-01-10\n\n### Feature A\n- `a.rs` [0..50]";
        let new_notes = "### Feature A\n- `a.rs` [0..999] - UPDATED";
        let result = merge_memory(existing, new_notes);

        // Should keep original (first occurrence), not the new one
        assert!(result.contains("### Feature A"));
        assert!(result.contains("[0..50]"));
        assert!(!result.contains("[0..999]"));
        // Should only appear once
        assert_eq!(result.matches("### Feature A").count(), 1);
    }

    #[test]
    fn test_parse_sections() {
        let content = "### Feature A\n- `a.rs` [0..50]\n\n### Feature B\n- `b.rs` [0..100]";
        let sections = parse_sections(content);

        assert_eq!(sections.len(), 2);
        assert!(sections.contains_key("Feature A"));
        assert!(sections.contains_key("Feature B"));
    }

    #[test]
    fn test_compact_memory() {
        let content = "# Workspace Memory\n> Updated: 2025-01-10\n\n### Feature A\n- `a.rs` [0..50]\n\n### Feature A\n- `a.rs` [999..1000]\n\n### Feature B\n- `b.rs` [100..200]";
        let compacted = compact_memory(content);

        // Should only have one Feature A (first occurrence)
        assert_eq!(compacted.matches("### Feature A").count(), 1);
        assert!(compacted.contains("[0..50]"));
        assert!(!compacted.contains("[999..1000]"));
        assert!(compacted.contains("[100..200]"));
        assert!(compacted.contains("### Feature B"));
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
        let result = update_header(content, "2025-01-10T12:00:00Z", "500 chars");

        assert!(result.starts_with("# Workspace Memory"));
        assert!(result.contains("> Updated: 2025-01-10T12:00:00Z | Size: 500 chars"));
        assert!(result.contains("### Feature"));
    }
}
