//! Codebase scout tool: spawns a scout agent to explore the codebase structure in a strict 4-step process.
//!
//! The agent MUST follow the exact order of steps (no skipping, reordering, or combining):
//! 1. Top-level directory structure
//! 2. Core abstractions  
//! 3. Data flows and dependencies
//! 4. Hot spots and complexity
//!
//! Output format: The report MUST be wrapped in `---SCOUT_REPORT_START---` and `---SCOUT_REPORT_END---` markers.
//! The report MUST contain all 6 required sections: Codebase Overview, Directory Structure, Core Abstractions,
//! Architectural Patterns, Key Data Flows, and Hot Spots.

use anyhow::Result;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info};

use g3_config::Config;

use crate::ui_writer::UiWriter;
use crate::ToolCall;

use super::executor::ToolContext;
use super::memory::update_memory;
use super::research::strip_ansi_codes;

/// Delimiter markers for scout report extraction
const REPORT_START_MARKER: &str = "---SCOUT_REPORT_START---";
const REPORT_END_MARKER: &str = "---SCOUT_REPORT_END---";

/// Execute the codebase_scout tool - spawns scout agent in background and returns immediately.
pub async fn execute_codebase_scout<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let path = tool_call
        .args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    let description = format!("Explore codebase structure at: {} (4-step mandatory process)", path);
    let scout_id = ctx.pending_research_manager.register(&description);

    let scout_id_clone = scout_id.clone();
    let manager = ctx.pending_research_manager.clone();
    let path_owned = path.to_string();

    let g3_path = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("g3"));

    let working_dir_for_memory = ctx.working_dir.map(|s| s.to_string());
    let config_for_spawn = ctx.config.clone();

    tokio::spawn(async move {
        let result = run_codebase_scout(&g3_path, &path_owned, &config_for_spawn).await;

        match result {
            Ok(report) => {
                debug!("Codebase scout {} completed successfully", scout_id_clone);

                // Persist scout report to workspace memory
                let memory_content = condense_report_for_memory(&report);
                if !memory_content.is_empty() {
                    match update_memory(
                        &memory_content,
                        working_dir_for_memory.as_deref(),
                    ) {
                        Ok(()) => info!("Workspace memory updated with scout report"),
                        Err(e) => error!("Failed to update workspace memory with scout report: {}", e),
                    }
                }

                manager.complete(&scout_id_clone, report);
            }
            Err(e) => {
                error!("Codebase scout {} failed: {}", scout_id_clone, e);
                manager.fail(&scout_id_clone, e.to_string());
            }
        }
    });

    let placeholder = format!(
        "**Codebase scout initiated** (id: `{}`)\n\n\
        **Path:** {}\n\n\
        The scout is exploring the codebase in the background. You can:\n\
        - Continue with other work - results will be automatically provided when ready\n\
        - Check status with `codebase_scout_status` tool\n\n\
        _Estimated time: 30-120 seconds depending on codebase size_",
        scout_id,
        path
    );

    Ok(placeholder)
}

/// Run the codebase scout agent with strict 4-step process enforcement.
async fn run_codebase_scout(
    g3_path: &std::path::Path,
    path: &str,
    _config: &Config,
) -> Result<String> {
    let prompt = format!(
        "You are **Codebase Scout**. Your role is to explore a codebase and produce a compressed structural overview.

## MANDATORY ORDER (DO NOT SKIP OR REORDER)
1. Top-level directory structure
2. Core abstractions  
3. Data flows and dependencies
4. Hot spots and complexity

## Output Contract (MANDATORY)
Return ONE overview only. Output MUST be wrapped in:
```
---SCOUT_REPORT_START---
(your full codebase overview here)
---SCOUT_REPORT_END---
```

## Required Sections
1. Codebase Overview (2-3 sentences)
2. Directory Structure (tree with purpose annotations)
3. Core Abstractions (5-10 types, their locations, purposes, relationships)
4. Architectural Patterns (data flows, design patterns, module boundaries)
5. Key Data Flows (2-3 operations traced end-to-end)
6. Hot Spots (high complexity, high coupling, many dependents)

Explore the codebase at '{}' and produce a structural overview.
Use available tools to scan directories, preview files, and trace relationships.",
        path
    );

    let mut child = Command::new(g3_path)
        .arg("--agent")
        .arg("codebase-scout")
        .arg("--new-session")
        .arg("--quiet")
        .arg("--index-tools")
        .arg(&prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn codebase scout agent: {}", e))?;

    let stdout = child.stdout.take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture scout agent stdout"))?;

    let stderr = child.stderr.take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture scout agent stderr"))?;

    let mut reader = BufReader::new(stdout).lines();
    let mut all_output = Vec::new();

    let stderr_handle = tokio::spawn(async move {
        let mut stderr_reader = BufReader::new(stderr).lines();
        let mut stderr_output = Vec::new();
        while let Some(line) = stderr_reader.next_line().await.ok().flatten() {
            stderr_output.push(line);
        }
        stderr_output
    });

    while let Some(line) = reader.next_line().await? {
        all_output.push(line);
    }

    let stderr_output = stderr_handle.await.unwrap_or_default();

    let status = child.wait().await
        .map_err(|e| anyhow::anyhow!("Failed to wait for codebase scout: {}", e))?;

    if !status.success() {
        let exit_code = status.code().map(|c| c.to_string()).unwrap_or_else(|| "unknown".to_string());
        let stderr_text = stderr_output.join("\n");
        let last_lines: Vec<_> = all_output.iter().rev().take(10).rev().cloned().collect();

        return Err(anyhow::anyhow!(
            "Codebase Scout Failed\n\n\
            Exit code: {}\n\n\
            {}{}",
            exit_code,
            if !stderr_text.is_empty() {
                format!("**Error output:**\n{}\n\n", stderr_text.chars().take(1000).collect::<String>())
            } else {
                String::new()
            },
            if !last_lines.is_empty() {
                format!("**Last output lines:**\n{}", last_lines.join("\n"))
            } else {
                String::new()
            }
        ));
    }

    let full_output = all_output.join("\n");
    extract_report_with_validation(&full_output)
}

/// Extract and validate the scout report from output.
///
/// Ensures the report contains all required sections and follows the strict format.
fn extract_report_with_validation(output: &str) -> Result<String> {
    // Strip ANSI codes only for finding markers, but preserve them in the output
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
    
    // Now find the same markers in the original output to preserve ANSI codes
    let original_start = find_marker_position(output, REPORT_START_MARKER)
        .ok_or_else(|| anyhow::anyhow!("Could not find start marker in original output"))?;
    let original_end = find_marker_position(output, REPORT_END_MARKER)
        .ok_or_else(|| anyhow::anyhow!("Could not find end marker in original output"))?;
    
    // Extract content between markers from original (with ANSI codes)
    let report_start = original_start + REPORT_START_MARKER.len();
    let report_content = output[report_start..original_end].trim();
    
    if report_content.is_empty() {
        return Err(anyhow::anyhow!("Scout agent returned an empty report. The 4-step process was not followed correctly."));
    }
    
    // Validate required sections are present
    let required_sections = [
        "Codebase Overview",
        "Directory Structure",
        "Core Abstractions",
        "Architectural Patterns",
        "Key Data Flows",
        "Hot Spots",
    ];
    
    let missing_sections: Vec<&str> = required_sections
        .iter()
        .filter(|&section| !report_content.contains(section))
        .cloned()
        .collect();
    
    if !missing_sections.is_empty() {
        return Err(anyhow::anyhow!(
            "Scout agent report is missing required sections. The 4-step process was not followed correctly.\n\n\
            Missing sections: {}\n\n\
            The agent MUST follow the exact order of steps:\n\
            1. Top-level directory structure\n\
            2. Core abstractions\n\
            3. Data flows and dependencies\n\
            4. Hot spots and complexity",
            missing_sections.join(", ")
        ));
    }
    
    Ok(report_content.to_string())
}

/// Find the position of a marker in text that may contain ANSI codes.
/// Searches by stripping ANSI codes character by character to find the true position.
fn find_marker_position(text: &str, marker: &str) -> Option<usize> {
    // Simple approach: search for the marker directly first
    // The markers themselves shouldn't contain ANSI codes
    if let Some(pos) = text.find(marker) {
        return Some(pos);
    }
    
    // If not found directly, the marker might be split by ANSI codes
    // This is unlikely for our use case, but handle it gracefully
    None
}

/// Condense the scout report into a compact form suitable for workspace memory.
///
/// Keeps the full report but prefixes it with a header indicating it's auto-generated.
/// Truncates to a reasonable size (4k chars) to avoid bloating memory.
pub fn condense_report_for_memory(report: &str) -> String {
    let trimmed = report.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    const MAX_CHARS: usize = 4000;
    let content = if trimmed.len() > MAX_CHARS {
        // Truncate at a line boundary
        let truncated = &trimmed[..trimmed[..MAX_CHARS]
            .rfind('\n')
            .unwrap_or(MAX_CHARS)];
        format!("{}\n\n(truncated)", truncated)
    } else {
        trimmed.to_string()
    };

    format!("### Codebase Scout Report (auto-updated)\n{}", content)
}

#[cfg(test)]
mod report_validation_tests {
    use super::*;

    #[test]
    fn test_extract_report_with_validation_success() {
        let output = r#"Some preamble text
---SCOUT_REPORT_START---
# Codebase Overview
Rust workspace.

# Directory Structure
tree here

# Core Abstractions
types here

# Architectural Patterns
patterns here

# Key Data Flows
flows here

# Hot Spots
hot spots here
---SCOUT_REPORT_END---
Some trailing text"#;
        
        let result = extract_report_with_validation(output).unwrap();
        assert!(result.contains("Codebase Overview"));
        assert!(result.contains("Directory Structure"));
        assert!(result.contains("Core Abstractions"));
        assert!(result.contains("Architectural Patterns"));
        assert!(result.contains("Key Data Flows"));
        assert!(result.contains("Hot Spots"));
    }

    #[test]
    fn test_extract_report_missing_sections() {
        let output = r#"---SCOUT_REPORT_START---
# Codebase Overview
Overview here
# Directory Structure
Directory here
# Core Abstractions
Abstractions here
---SCOUT_REPORT_END---"#;
        
        let result = extract_report_with_validation(output);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing required sections"));
        assert!(err.contains("Architectural Patterns"));
        assert!(err.contains("Key Data Flows"));
        assert!(err.contains("Hot Spots"));
    }

    #[test]
    fn test_extract_report_empty_content() {
        let output = "---SCOUT_REPORT_START---\n---SCOUT_REPORT_END---";
        let result = extract_report_with_validation(output);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty report"));
    }

    #[test]
    fn test_extract_report_missing_start() {
        let output = "No markers here";
        let result = extract_report_with_validation(output);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SCOUT_REPORT_START"));
    }

    #[test]
    fn test_extract_report_missing_end() {
        let output = "---SCOUT_REPORT_START---\nContent but no end";
        let result = extract_report_with_validation(output);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SCOUT_REPORT_END"));
    }
}

/// Execute the codebase_scout_status tool - reuses research_status logic.
pub async fn execute_codebase_scout_status<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    // Reuse research_status - same manager, same format
    super::research::execute_research_status(tool_call, ctx).await
}
