//! Utility functions for diff parsing, shell escaping, and JSON fixing.
//!
//! This module contains helper functions used by the agent for:
//! - Applying unified diffs to strings
//! - Shell command escaping
//! - JSON quote fixing

use anyhow::Result;
use tracing::debug;

/// Apply unified diff to an input string with optional [start, end) bounds.
///
/// # Arguments
/// * `file_content` - The original file content
/// * `diff` - The unified diff to apply
/// * `start_char` - Optional start character position (0-indexed, inclusive)
/// * `end_char` - Optional end character position (0-indexed, exclusive)
///
/// # Returns
/// The modified content with the diff applied
pub fn apply_unified_diff_to_string(
    file_content: &str,
    diff: &str,
    start_char: Option<usize>,
    end_char: Option<usize>,
) -> Result<String> {
    // Parse full unified diff into hunks and apply sequentially.
    let hunks = parse_unified_diff_hunks(diff);
    if hunks.is_empty() {
        anyhow::bail!(
            "Invalid diff format. Expected unified diff with @@ hunks or +/- with context lines"
        );
    }

    // Normalize line endings to avoid CRLF/CR mismatches
    let content_norm = file_content.replace("\r\n", "\n").replace('\r', "\n");

    // Determine and validate the search range
    let search_start = start_char.unwrap_or(0);
    let search_end = end_char.unwrap_or(content_norm.len());

    if search_start > content_norm.len() {
        anyhow::bail!(
            "start position {} exceeds file length {}",
            search_start,
            content_norm.len()
        );
    }
    if search_end > content_norm.len() {
        anyhow::bail!(
            "end position {} exceeds file length {}",
            search_end,
            content_norm.len()
        );
    }
    if search_start > search_end {
        anyhow::bail!(
            "start position {} is greater than end position {}",
            search_start,
            search_end
        );
    }

    // Extract the region we're going to modify, ensuring we're at char boundaries
    // Find the nearest valid char boundaries
    let start_boundary = if search_start == 0 {
        0
    } else {
        content_norm
            .char_indices()
            .find(|(i, _)| *i >= search_start)
            .map(|(i, _)| i)
            .unwrap_or(search_start)
    };
    let end_boundary = content_norm
        .char_indices()
        .find(|(i, _)| *i >= search_end)
        .map(|(i, _)| i)
        .unwrap_or(content_norm.len());

    let mut region_content = content_norm[start_boundary..end_boundary].to_string();

    // Apply hunks in order
    for (idx, (old_block, new_block)) in hunks.iter().enumerate() {
        debug!(
            "Applying hunk {}: old_len={}, new_len={}",
            idx + 1,
            old_block.len(),
            new_block.len()
        );

        if let Some(pos) = region_content.find(old_block) {
            let endpos = pos + old_block.len();
            region_content.replace_range(pos..endpos, new_block);
        } else {
            // Not found; provide helpful diagnostics with a short preview
            let preview_len = old_block.len().min(200);
            let mut old_preview = old_block[..preview_len].to_string();
            if old_block.len() > preview_len {
                old_preview.push_str("...");
            }

            let range_note = if start_char.is_some() || end_char.is_some() {
                format!(
                    " (within character range {}:{})",
                    start_boundary, end_boundary
                )
            } else {
                String::new()
            };

            anyhow::bail!(
                "Pattern not found in file{}\nHunk {} failed. Searched for:\n{}",
                range_note,
                idx + 1,
                old_preview
            );
        }
    }

    // Reconstruct the full content with the modified region
    let mut result = String::with_capacity(content_norm.len() + region_content.len());
    result.push_str(&content_norm[..start_boundary]);
    result.push_str(&region_content);
    result.push_str(&content_norm[end_boundary..]);
    Ok(result)
}

/// Parse a unified diff into a list of hunks as (old_block, new_block).
/// Each hunk contains the exact text to search for and the replacement text including context lines.
pub fn parse_unified_diff_hunks(diff: &str) -> Vec<(String, String)> {
    let mut hunks: Vec<(String, String)> = Vec::new();

    let mut old_lines: Vec<String> = Vec::new();
    let mut new_lines: Vec<String> = Vec::new();
    let mut in_hunk = false;

    for raw_line in diff.lines() {
        let line = raw_line;

        // Skip common diff headers
        if line.starts_with("diff ")
            || line.starts_with("index ")
            || line.starts_with("new file mode")
            || line.starts_with("deleted file mode")
        {
            continue;
        }

        if line.starts_with("--- ") || line.starts_with("+++ ") {
            // File header lines — ignore
            continue;
        }

        if line.starts_with("@@") {
            // Starting a new hunk — flush previous if present
            if in_hunk && (!old_lines.is_empty() || !new_lines.is_empty()) {
                hunks.push((old_lines.join("\n"), new_lines.join("\n")));
                old_lines.clear();
                new_lines.clear();
            }
            in_hunk = true;
            continue;
        }

        if !in_hunk {
            // Some minimal diffs may omit @@; start collecting once we see diff markers
            if line.starts_with(' ')
                || (line.starts_with('-') && !line.starts_with("---"))
                || (line.starts_with('+') && !line.starts_with("+++"))
            {
                in_hunk = true;
            } else {
                continue;
            }
        }

        if let Some(content) = line.strip_prefix(' ') {
            old_lines.push(content.to_string());
            new_lines.push(content.to_string());
        } else if line.starts_with('+') && !line.starts_with("+++") {
            new_lines.push(line[1..].to_string());
        } else if line.starts_with('-') && !line.starts_with("---") {
            old_lines.push(line[1..].to_string());
        } else if line.starts_with('\\') {
            // Example: "\\ No newline at end of file" — ignore
            continue;
        } else {
            // Unknown line type — ignore
        }
    }

    if in_hunk && (!old_lines.is_empty() || !new_lines.is_empty()) {
        hunks.push((old_lines.join("\n"), new_lines.join("\n")));
    }

    hunks
}

/// Helper function to properly escape shell commands.
/// Handles file paths with spaces and other special characters.
#[allow(dead_code)]
pub fn shell_escape_command(command: &str) -> String {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return command.to_string();
    }

    let cmd = parts[0];

    // Commands that typically take file paths as arguments
    let file_commands = [
        "cat", "ls", "cp", "mv", "rm", "chmod", "chown", "file", "head", "tail", "wc", "grep",
    ];

    if file_commands.contains(&cmd) {
        // For file commands, we need to be smarter about escaping
        // Check if the command already has proper quoting
        if command.contains('"') || command.contains('\'') {
            // Already has some quoting, use as-is
            return command.to_string();
        }

        // Look for file paths that need escaping (contain spaces but aren't quoted)
        let mut escaped_command = String::new();
        let mut in_quotes = false;
        let mut current_word = String::new();
        let mut words = Vec::new();

        for ch in command.chars() {
            match ch {
                ' ' if !in_quotes => {
                    if !current_word.is_empty() {
                        words.push(current_word.clone());
                        current_word.clear();
                    }
                }
                '"' => {
                    in_quotes = !in_quotes;
                    current_word.push(ch);
                }
                _ => {
                    current_word.push(ch);
                }
            }
        }

        if !current_word.is_empty() {
            words.push(current_word);
        }

        // Reconstruct the command with proper escaping
        for (i, word) in words.iter().enumerate() {
            if i > 0 {
                escaped_command.push(' ');
            }

            // If this word looks like a file path (contains / or ~) and has spaces, quote it
            if word.contains('/') || word.starts_with('~') {
                if word.contains(' ') && !word.starts_with('"') && !word.starts_with('\'') {
                    escaped_command.push_str(&format!("\"{}\"", word));
                } else {
                    escaped_command.push_str(word);
                }
            } else {
                escaped_command.push_str(word);
            }
        }

        escaped_command
    } else {
        // For non-file commands, use the original command
        command.to_string()
    }
}

/// Helper function to fix nested quotes in shell commands within JSON.
#[allow(dead_code)]
pub fn fix_nested_quotes_in_shell_command(json_str: &str) -> String {
    // Look for the pattern: "command": "
    if let Some(command_start) = json_str.find(r#""command": ""#) {
        let command_value_start = command_start + r#""command": ""#.len();

        // Find the end of the command string by looking for the pattern "}
        if let Some(end_marker) = json_str[command_value_start..].find(r#"" }"#) {
            let command_end = command_value_start + end_marker;

            let before = &json_str[..command_value_start];
            let command_content = &json_str[command_value_start..command_end];
            let after = &json_str[command_end..];

            // Fix the command content by properly escaping quotes
            let mut fixed_command = String::new();
            let mut chars = command_content.chars().peekable();

            while let Some(ch) = chars.next() {
                match ch {
                    '"' => {
                        // Check if this quote is already escaped
                        if fixed_command.ends_with('\\') {
                            fixed_command.push(ch); // Already escaped, keep as-is
                        } else {
                            fixed_command.push_str(r#"\""#); // Escape the quote
                        }
                    }
                    '\\' => {
                        // Check what follows the backslash
                        if let Some(&next_ch) = chars.peek() {
                            if next_ch == '"' {
                                // This is an escaped quote, keep the backslash
                                fixed_command.push(ch);
                            } else {
                                // Regular backslash, escape it
                                fixed_command.push_str(r#"\\"#);
                            }
                        } else {
                            // Backslash at end, escape it
                            fixed_command.push_str(r#"\\"#);
                        }
                    }
                    _ => fixed_command.push(ch),
                }
            }

            return format!("{}{}{}", before, fixed_command, after);
        }
    }

    // Fallback: if we can't parse the structure, return as-is
    json_str.to_string()
}

/// Helper function to fix mixed quotes in JSON (single quotes where double quotes should be).
#[allow(dead_code)]
pub fn fix_mixed_quotes_in_json(json_str: &str) -> String {
    let mut result = String::new();
    let mut chars = json_str.chars().peekable();
    let mut in_string = false;
    let mut string_delimiter = '"';

    while let Some(ch) = chars.next() {
        match ch {
            '"' if !in_string => {
                // Start of a double-quoted string
                in_string = true;
                string_delimiter = '"';
                result.push(ch);
            }
            '\'' if !in_string => {
                // Start of a single-quoted string - convert to double quotes
                in_string = true;
                string_delimiter = '\'';
                result.push('"'); // Convert single quote to double quote
            }
            c if in_string && c == string_delimiter => {
                // End of current string
                if string_delimiter == '\'' {
                    result.push('"'); // Convert single quote to double quote
                } else {
                    result.push(c);
                }
                in_string = false;
            }
            '"' if in_string && string_delimiter == '\'' => {
                // Double quote inside single-quoted string - escape it
                result.push_str(r#"\""#);
            }
            '\\' if in_string => {
                // Escape sequence - preserve it
                result.push(ch);
                if chars.peek().is_some() {
                    result.push(chars.next().unwrap());
                }
            }
            _ => {
                result.push(ch);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_unified_diff_without_hunk_header() {
        let diff = "--- old\n-old text\n+++ new\n+new text\n";
        let hunks = parse_unified_diff_hunks(diff);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].0, "old text");
        assert_eq!(hunks[0].1, "new text");
    }

    #[test]
    fn parses_diff_with_context_and_hunk_headers() {
        let diff = "@@ -1,3 +1,3 @@\n common\n-old\n+new\n common2\n";
        let hunks = parse_unified_diff_hunks(diff);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].0, "common\nold\ncommon2");
        assert_eq!(hunks[0].1, "common\nnew\ncommon2");
    }

    #[test]
    fn apply_multi_hunk_unified_diff_to_string() {
        let original = "line 1\nkeep\nold A\nkeep 2\nold B\nkeep 3\n";
        let diff =
            "@@ -1,6 +1,6 @@\n line 1\n keep\n-old A\n+new A\n keep 2\n-old B\n+new B\n keep 3\n";
        let result = apply_unified_diff_to_string(original, diff, None, None).unwrap();
        let expected = "line 1\nkeep\nnew A\nkeep 2\nnew B\nkeep 3\n";
        assert_eq!(result, expected);
    }

    #[test]
    fn apply_diff_within_range_only() {
        let original = "A\nold\nB\nold\nC\n";
        // Only the first 'old' should be replaced due to range
        let diff = "@@ -1,3 +1,3 @@\n A\n-old\n+NEW\n B\n";
        let start = 0usize; // Start of file
        let end = original.find("B\n").unwrap() + 2; // up to end of line 'B\n'
        let result = apply_unified_diff_to_string(original, diff, Some(start), Some(end)).unwrap();
        let expected = "A\nNEW\nB\nold\nC\n";
        assert_eq!(result, expected);
    }

    #[test]
    fn shell_escape_preserves_simple_commands() {
        assert_eq!(shell_escape_command("ls -la"), "ls -la");
        assert_eq!(shell_escape_command("echo hello"), "echo hello");
    }

    #[test]
    fn fix_mixed_quotes_converts_single_to_double() {
        let input = "{'key': 'value'}";
        let result = fix_mixed_quotes_in_json(input);
        assert_eq!(result, "{\"key\": \"value\"}");
    }
}
