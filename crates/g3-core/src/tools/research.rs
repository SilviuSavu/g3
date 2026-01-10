//! Research tool: spawns a scout agent to perform web-based research.

use anyhow::Result;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::ui_writer::UiWriter;
use crate::ToolCall;

use super::executor::ToolContext;

/// Delimiter markers for scout report extraction
const REPORT_START_MARKER: &str = "---SCOUT_REPORT_START---";
const REPORT_END_MARKER: &str = "---SCOUT_REPORT_END---";

/// Execute the research tool by spawning a scout agent.
///
/// This tool:
/// 1. Spawns `g3 --agent scout` with the query
/// 2. Captures stdout and extracts the report between delimiter markers
/// 3. Returns the report content directly
pub async fn execute_research<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let query = tool_call
        .args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required 'query' parameter"))?;

    ctx.ui_writer.print_tool_header("research", None);
    ctx.ui_writer.print_tool_arg("query", query);
    
    // Find the g3 executable path
    let g3_path = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("g3"));

    // Spawn the scout agent
    let mut child = Command::new(&g3_path)
        .arg("--agent")
        .arg("scout")
        .arg("--webdriver")  // Scout needs webdriver for web research
        .arg("--new-session")  // Always start fresh for research
        .arg("--quiet")  // Suppress log file creation
        .arg(query)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn scout agent: {}", e))?;

    // Capture stdout to find the report content
    let stdout = child.stdout.take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture scout agent stdout"))?;
    
    let mut reader = BufReader::new(stdout).lines();
    let mut all_output = Vec::new();

    // Print a header for the scout output
    ctx.ui_writer.println("\n📡 Scout agent researching...");
    
    // Collect all lines
    while let Some(line) = reader.next_line().await? {
        ctx.ui_writer.println(&format!("  {}", line));
        all_output.push(line);
    }

    // Wait for the process to complete
    let status = child.wait().await
        .map_err(|e| anyhow::anyhow!("Failed to wait for scout agent: {}", e))?;

    if !status.success() {
        return Ok(format!("❌ Scout agent failed with exit code: {:?}", status.code()));
    }

    // Join all output and extract the report between markers
    let full_output = all_output.join("\n");
    
    extract_report(&full_output)
}

/// Extract the research report from scout output.
/// 
/// Looks for content between SCOUT_REPORT_START and SCOUT_REPORT_END markers.
/// Strips ANSI escape codes from the extracted content.
fn extract_report(output: &str) -> Result<String> {
    // Strip ANSI codes from the entire output first
    let clean_output = strip_ansi_codes(output);
    
    // Find the start marker
    let start_pos = clean_output.find(REPORT_START_MARKER)
        .ok_or_else(|| anyhow::anyhow!(
            "Scout agent did not output a properly formatted report. Expected {} marker.",
            REPORT_START_MARKER
        ))?;
    
    // Find the end marker
    let end_pos = clean_output.find(REPORT_END_MARKER)
        .ok_or_else(|| anyhow::anyhow!(
            "Scout agent report is incomplete. Expected {} marker.",
            REPORT_END_MARKER
        ))?;
    
    if end_pos <= start_pos {
        return Err(anyhow::anyhow!("Invalid report format: end marker before start marker"));
    }
    
    // Extract content between markers
    let report_start = start_pos + REPORT_START_MARKER.len();
    let report_content = clean_output[report_start..end_pos].trim();
    
    if report_content.is_empty() {
        return Ok("❌ Scout agent returned an empty report.".to_string());
    }
    
    Ok(format!("📋 Research Report:\n\n{}", report_content))
}

/// Strip ANSI escape codes from a string.
/// 
/// Handles common ANSI sequences like:
/// - CSI sequences: \x1b[...m (colors, styles)
/// - OSC sequences: \x1b]...\x07 (terminal titles, etc.)
fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Start of escape sequence
            match chars.peek() {
                Some('[') => {
                    // CSI sequence: \x1b[...X where X is a letter
                    chars.next(); // consume '['
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC sequence: \x1b]...\x07
                    chars.next(); // consume ']'
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next == '\x07' {
                            break;
                        }
                    }
                }
                _ => {
                    // Unknown escape, skip just the ESC
                }
            }
        } else {
            result.push(c);
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_codes() {
        // Simple color code
        assert_eq!(strip_ansi_codes("\x1b[31mred\x1b[0m"), "red");
        
        // RGB color code (like the bug we saw)
        assert_eq!(
            strip_ansi_codes("\x1b[38;2;216;177;114mtmp/file.md\x1b[0m"),
            "tmp/file.md"
        );
        
        // Multiple codes
        assert_eq!(
            strip_ansi_codes("\x1b[1m\x1b[32mbold green\x1b[0m normal"),
            "bold green normal"
        );
        
        // No codes
        assert_eq!(strip_ansi_codes("plain text"), "plain text");
        
        // Empty string
        assert_eq!(strip_ansi_codes(""), "");
    }

    #[test]
    fn test_extract_report_success() {
        let output = r#"Some preamble text
---SCOUT_REPORT_START---
# Research Brief

This is the report content.
---SCOUT_REPORT_END---
Some trailing text"#;
        
        let result = extract_report(output).unwrap();
        assert!(result.contains("Research Brief"));
        assert!(result.contains("This is the report content."));
        assert!(!result.contains("preamble"));
        assert!(!result.contains("trailing"));
    }

    #[test]
    fn test_extract_report_with_ansi_codes() {
        let output = "\x1b[32m---SCOUT_REPORT_START---\x1b[0m\n# Report\n\x1b[31m---SCOUT_REPORT_END---\x1b[0m";
        
        let result = extract_report(output).unwrap();
        assert!(result.contains("# Report"));
    }

    #[test]
    fn test_extract_report_missing_start() {
        let output = "No markers here";
        let result = extract_report(output);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SCOUT_REPORT_START"));
    }

    #[test]
    fn test_extract_report_missing_end() {
        let output = "---SCOUT_REPORT_START---\nContent but no end";
        let result = extract_report(output);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SCOUT_REPORT_END"));
    }

    #[test]
    fn test_extract_report_empty_content() {
        let output = "---SCOUT_REPORT_START---\n---SCOUT_REPORT_END---";
        let result = extract_report(output).unwrap();
        assert!(result.contains("empty report"));
    }
}
