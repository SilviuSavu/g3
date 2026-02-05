//! Beads CLI wrapper tools implementation.
//!
//! This module provides handlers for the `bd` (Beads) CLI tool for git-backed issue tracking.
//! All commands shell out to the `bd` CLI with `--json` output and parse responses with serde_json.

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::debug;

use super::executor::ToolContext;
use crate::ui_writer::UiWriter;
use crate::ToolCall;

// =============================================================================
// Lifecycle Hooks
// =============================================================================

/// Check if beads is available in this project.
/// Returns true if bd CLI is installed AND we're in a beads project.
pub fn is_beads_available() -> bool {
    is_beads_installed() && detect_beads_project().is_some()
}

/// Get beads context for session start (SessionStart hook equivalent).
/// Returns formatted beads state to inject into the system context.
/// This is called when a new session is initialized.
pub async fn get_beads_session_context(working_dir: Option<&str>) -> Option<String> {
    debug!("get_beads_session_context called with working_dir: {:?}", working_dir);

    if !is_beads_installed() {
        debug!("Beads: bd CLI not installed");
        return None;
    }

    // Check if we're in a beads project
    let work_dir = if let Some(dir) = working_dir {
        debug!("Beads: checking for .beads in provided dir: {}", dir);
        detect_beads_project_from(Path::new(dir))
    } else {
        let cwd = std::env::current_dir().ok();
        debug!("Beads: checking for .beads from cwd: {:?}", cwd);
        detect_beads_project()
    };

    let work_dir = match work_dir {
        Some(dir) => {
            debug!("Beads: found project at {:?}", dir);
            dir
        }
        None => {
            debug!("Beads: no .beads directory found");
            return None;
        }
    };

    debug!("Beads SessionStart hook: injecting workflow context from {:?}", work_dir);

    // Run bd prime to get AI-optimized markdown context
    // Note: bd prime outputs markdown (not JSON) designed for AI context injection
    match run_bd_prime_markdown(&work_dir).await {
        Ok(markdown) => {
            debug!("Beads: got {} bytes of context from bd prime", markdown.len());
            let mut context = String::new();
            context.push_str("\n\n## Beads Workflow Context\n\n");
            context.push_str(&markdown);
            context.push_str("\nUse beads_* tools to interact with the issue tracker.\n");
            Some(context)
        }
        Err(e) => {
            debug!("Beads prime failed (non-fatal): {}", e);
            None
        }
    }
}

/// Run bd prime and return the markdown output directly.
/// Unlike other bd commands, prime outputs AI-optimized markdown, not JSON.
async fn run_bd_prime_markdown(work_dir: &Path) -> Result<String> {
    debug!("Running bd prime in {:?}", work_dir);

    let output = Command::new("bd")
        .args(["prime"])
        .current_dir(work_dir)
        .output()
        .await
        .map_err(|e| anyhow!("Failed to execute bd prime: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("bd prime failed: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get beads context for pre-compaction (PreCompact hook equivalent).
/// Returns a compact summary of beads state to preserve across compaction.
/// This is called before context compaction to ensure workflow state isn't lost.
pub async fn get_beads_precompact_context(working_dir: Option<&str>) -> Option<String> {
    if !is_beads_installed() {
        return None;
    }

    let work_dir = if let Some(dir) = working_dir {
        detect_beads_project_from(Path::new(dir))
    } else {
        detect_beads_project()
    };

    if work_dir.is_none() {
        return None;
    }

    debug!("Beads PreCompact hook: preserving workflow context");

    // Get ready issues (minimal context to preserve)
    match run_bd_command(&["ready", "--json"], working_dir).await {
        Ok(json) => {
            let mut context = String::new();
            context.push_str("\n## Beads State (preserved across compaction)\n");

            if let Some(issues) = json.as_array() {
                if issues.is_empty() {
                    context.push_str("No ready issues.\n");
                } else {
                    context.push_str("Ready issues:\n");
                    for issue in issues.iter().take(5) {  // Limit to 5 to save tokens
                        format_issue_brief(&mut context, issue);
                    }
                    if issues.len() > 5 {
                        context.push_str(&format!("... and {} more\n", issues.len() - 5));
                    }
                }
            }

            Some(context)
        }
        Err(e) => {
            debug!("Beads ready failed during precompact (non-fatal): {}", e);
            None
        }
    }
}

/// Format a brief issue line for context injection.
fn format_issue_brief(output: &mut String, issue: &Value) {
    let id = issue.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let title = issue.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
    let priority = issue.get("priority").and_then(|v| v.as_u64()).unwrap_or(2);
    let priority_label = match priority {
        0 => "P0",
        1 => "P1",
        2 => "P2",
        3 => "P3",
        4 => "P4",
        _ => "P?",
    };
    output.push_str(&format!("- [{}] {} ({})\n", id, title, priority_label));
}

// =============================================================================
// Core Utilities
// =============================================================================

/// Check if `bd` CLI is installed.
fn is_beads_installed() -> bool {
    std::process::Command::new("which")
        .arg("bd")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Find .beads/ directory in current or parent directories.
fn detect_beads_project() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        let beads_dir = current.join(".beads");
        if beads_dir.is_dir() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Find .beads/ directory starting from a specific path.
fn detect_beads_project_from(start_path: &Path) -> Option<PathBuf> {
    let mut current = start_path.to_path_buf();
    loop {
        let beads_dir = current.join(".beads");
        if beads_dir.is_dir() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Run a bd command with args and return JSON output.
async fn run_bd_command(args: &[&str], working_dir: Option<&str>) -> Result<Value> {
    if !is_beads_installed() {
        return Err(anyhow!(
            "Beads CLI (bd) is not installed. Install with: brew install steveyegge/beads/bd"
        ));
    }

    // Determine working directory
    let work_dir = if let Some(dir) = working_dir {
        let path = PathBuf::from(dir);
        detect_beads_project_from(&path)
    } else {
        detect_beads_project()
    };

    let work_dir = work_dir.ok_or_else(|| {
        anyhow!("Not in a Beads project. Run 'bd init' to initialize.")
    })?;

    debug!("Running bd command: bd {} in {:?}", args.join(" "), work_dir);

    let output = Command::new("bd")
        .args(args)
        .current_dir(&work_dir)
        .output()
        .await
        .map_err(|e| anyhow!("Failed to execute bd command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow!(
            "bd command failed (exit code: {:?}):\nstderr: {}\nstdout: {}",
            output.status.code(),
            stderr,
            stdout
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Try to parse as JSON
    serde_json::from_str(&stdout).map_err(|e| {
        anyhow!(
            "Failed to parse bd output as JSON: {}\nOutput was: {}",
            e,
            stdout
        )
    })
}

/// Format JSON output for human readability.
fn format_beads_output(json: &Value, tool_name: &str) -> String {
    let mut output = String::new();

    match tool_name {
        "beads_ready" => {
            output.push_str("## Ready Issues (Unblocked, by Priority)\n\n");
            if let Some(issues) = json.as_array() {
                if issues.is_empty() {
                    output.push_str("No ready issues found.\n");
                } else {
                    for issue in issues {
                        format_issue_summary(&mut output, issue);
                    }
                }
            } else {
                output.push_str(&format!("```json\n{}\n```\n", serde_json::to_string_pretty(json).unwrap_or_default()));
            }
        }
        "beads_list" => {
            output.push_str("## Issue List\n\n");
            if let Some(issues) = json.as_array() {
                if issues.is_empty() {
                    output.push_str("No issues found matching criteria.\n");
                } else {
                    for issue in issues {
                        format_issue_summary(&mut output, issue);
                    }
                }
            } else {
                output.push_str(&format!("```json\n{}\n```\n", serde_json::to_string_pretty(json).unwrap_or_default()));
            }
        }
        "beads_show" => {
            output.push_str("## Issue Details\n\n");
            format_issue_details(&mut output, json);
        }
        "beads_create" => {
            output.push_str("## Issue Created\n\n");
            if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
                output.push_str(&format!("**ID:** {}\n", id));
            }
            if let Some(title) = json.get("title").and_then(|v| v.as_str()) {
                output.push_str(&format!("**Title:** {}\n", title));
            }
            if let Some(priority) = json.get("priority") {
                output.push_str(&format!("**Priority:** {}\n", priority));
            }
        }
        "beads_update" | "beads_close" => {
            output.push_str("## Issue Updated\n\n");
            format_issue_summary(&mut output, json);
        }
        "beads_sync" => {
            output.push_str("## Sync Complete\n\n");
            if let Some(synced) = json.get("synced").and_then(|v| v.as_u64()) {
                output.push_str(&format!("Synced {} changes.\n", synced));
            }
            output.push_str(&format!("```json\n{}\n```\n", serde_json::to_string_pretty(json).unwrap_or_default()));
        }
        "beads_prime" => {
            output.push_str("## Prime Complete\n\n");
            output.push_str(&format!("```json\n{}\n```\n", serde_json::to_string_pretty(json).unwrap_or_default()));
        }
        "beads_dep" => {
            output.push_str("## Dependency Updated\n\n");
            output.push_str(&format!("```json\n{}\n```\n", serde_json::to_string_pretty(json).unwrap_or_default()));
        }
        "formula_list" => {
            output.push_str("## Available Formulas\n\n");
            if let Some(formulas) = json.as_array() {
                if formulas.is_empty() {
                    output.push_str("No formulas available.\n");
                } else {
                    for formula in formulas {
                        if let Some(name) = formula.get("name").and_then(|v| v.as_str()) {
                            output.push_str(&format!("- **{}**", name));
                            if let Some(desc) = formula.get("description").and_then(|v| v.as_str()) {
                                output.push_str(&format!(": {}", desc));
                            }
                            output.push('\n');
                        }
                    }
                }
            } else {
                output.push_str(&format!("```json\n{}\n```\n", serde_json::to_string_pretty(json).unwrap_or_default()));
            }
        }
        "formula_cook" | "mol_pour" | "mol_wisp" => {
            output.push_str("## Molecule Operation Result\n\n");
            output.push_str(&format!("```json\n{}\n```\n", serde_json::to_string_pretty(json).unwrap_or_default()));
        }
        "mol_current" => {
            output.push_str("## Current Molecule\n\n");
            if json.is_null() {
                output.push_str("No active molecule.\n");
            } else {
                output.push_str(&format!("```json\n{}\n```\n", serde_json::to_string_pretty(json).unwrap_or_default()));
            }
        }
        "mol_squash" => {
            output.push_str("## Molecule Squashed\n\n");
            output.push_str(&format!("```json\n{}\n```\n", serde_json::to_string_pretty(json).unwrap_or_default()));
        }
        _ => {
            output.push_str(&format!("## {}\n\n", tool_name));
            output.push_str(&format!("```json\n{}\n```\n", serde_json::to_string_pretty(json).unwrap_or_default()));
        }
    }

    output
}

/// Format a single issue summary line.
fn format_issue_summary(output: &mut String, issue: &Value) {
    let id = issue.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let title = issue.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
    let status = issue.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
    let priority = issue.get("priority").and_then(|v| v.as_u64()).unwrap_or(0);
    let issue_type = issue.get("type").and_then(|v| v.as_str()).unwrap_or("issue");

    let priority_label = match priority {
        0 => "P0-Critical",
        1 => "P1-High",
        2 => "P2-Medium",
        3 => "P3-Low",
        4 => "P4-Trivial",
        _ => "P?",
    };

    output.push_str(&format!(
        "- **[{}]** {} `[{}]` `[{}]` `[{}]`\n",
        id, title, status, priority_label, issue_type
    ));
}

/// Format detailed issue information.
fn format_issue_details(output: &mut String, issue: &Value) {
    let id = issue.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let title = issue.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
    let status = issue.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
    let priority = issue.get("priority").and_then(|v| v.as_u64()).unwrap_or(0);
    let issue_type = issue.get("type").and_then(|v| v.as_str()).unwrap_or("issue");
    let description = issue.get("description").and_then(|v| v.as_str());

    let priority_label = match priority {
        0 => "P0-Critical",
        1 => "P1-High",
        2 => "P2-Medium",
        3 => "P3-Low",
        4 => "P4-Trivial",
        _ => "P?",
    };

    output.push_str(&format!("**ID:** {}\n", id));
    output.push_str(&format!("**Title:** {}\n", title));
    output.push_str(&format!("**Status:** {}\n", status));
    output.push_str(&format!("**Priority:** {}\n", priority_label));
    output.push_str(&format!("**Type:** {}\n", issue_type));

    if let Some(desc) = description {
        output.push_str(&format!("\n**Description:**\n{}\n", desc));
    }

    // Show dependencies if present
    if let Some(deps) = issue.get("dependencies").or(issue.get("blocked_by")) {
        if let Some(deps_arr) = deps.as_array() {
            if !deps_arr.is_empty() {
                output.push_str("\n**Blocked By:**\n");
                for dep in deps_arr {
                    if let Some(dep_id) = dep.as_str() {
                        output.push_str(&format!("- {}\n", dep_id));
                    }
                }
            }
        }
    }

    if let Some(blocks) = issue.get("blocks") {
        if let Some(blocks_arr) = blocks.as_array() {
            if !blocks_arr.is_empty() {
                output.push_str("\n**Blocks:**\n");
                for block in blocks_arr {
                    if let Some(block_id) = block.as_str() {
                        output.push_str(&format!("- {}\n", block_id));
                    }
                }
            }
        }
    }

    if let Some(parent) = issue.get("parent").and_then(|v| v.as_str()) {
        output.push_str(&format!("\n**Parent:** {}\n", parent));
    }

    if let Some(children) = issue.get("children").and_then(|v| v.as_array()) {
        if !children.is_empty() {
            output.push_str("\n**Children:**\n");
            for child in children {
                if let Some(child_id) = child.as_str() {
                    output.push_str(&format!("- {}\n", child_id));
                }
            }
        }
    }
}

// =============================================================================
// Basic Issue Operations
// =============================================================================

/// Execute the beads_ready tool.
/// Returns unblocked issues sorted by priority.
pub async fn execute_beads_ready<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Executing beads_ready");

    let json = run_bd_command(&["ready", "--json"], ctx.working_dir).await?;
    Ok(format_beads_output(&json, "beads_ready"))
}

/// Execute the beads_create tool.
/// Creates a new issue with title, priority, and optional fields.
pub async fn execute_beads_create<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let title = tool_call
        .args
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: title"))?;

    let priority = tool_call
        .args
        .get("priority")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow!("Missing required parameter: priority (0-4)"))?;

    if priority > 4 {
        return Err(anyhow!("Priority must be between 0 and 4"));
    }

    let issue_type = tool_call
        .args
        .get("type")
        .and_then(|v| v.as_str());

    let description = tool_call
        .args
        .get("description")
        .and_then(|v| v.as_str());

    let parent = tool_call
        .args
        .get("parent")
        .and_then(|v| v.as_str());

    debug!("Executing beads_create: title={}, priority={}", title, priority);

    let mut args: Vec<&str> = vec!["create", title, "--priority"];
    let priority_str = priority.to_string();
    args.push(&priority_str);

    let type_flag;
    if let Some(t) = issue_type {
        type_flag = t.to_string();
        args.push("--type");
        args.push(&type_flag);
    }

    let desc_flag;
    if let Some(d) = description {
        desc_flag = d.to_string();
        args.push("--description");
        args.push(&desc_flag);
    }

    let parent_flag;
    if let Some(p) = parent {
        parent_flag = p.to_string();
        args.push("--parent");
        args.push(&parent_flag);
    }

    args.push("--json");

    let json = run_bd_command(&args, ctx.working_dir).await?;
    Ok(format_beads_output(&json, "beads_create"))
}

/// Execute the beads_update tool.
/// Updates an issue's status or priority.
pub async fn execute_beads_update<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let id = tool_call
        .args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: id"))?;

    let status = tool_call
        .args
        .get("status")
        .and_then(|v| v.as_str());

    let priority = tool_call
        .args
        .get("priority")
        .and_then(|v| v.as_u64());

    if status.is_none() && priority.is_none() {
        return Err(anyhow!("Must provide at least one of: status, priority"));
    }

    debug!("Executing beads_update: id={}", id);

    let mut args: Vec<&str> = vec!["update", id];

    let status_flag;
    if let Some(s) = status {
        status_flag = s.to_string();
        args.push("--status");
        args.push(&status_flag);
    }

    let priority_flag;
    if let Some(p) = priority {
        if p > 4 {
            return Err(anyhow!("Priority must be between 0 and 4"));
        }
        priority_flag = p.to_string();
        args.push("--priority");
        args.push(&priority_flag);
    }

    args.push("--json");

    let json = run_bd_command(&args, ctx.working_dir).await?;
    Ok(format_beads_output(&json, "beads_update"))
}

/// Execute the beads_close tool.
/// Closes an issue with optional reason.
pub async fn execute_beads_close<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let id = tool_call
        .args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: id"))?;

    let reason = tool_call
        .args
        .get("reason")
        .and_then(|v| v.as_str());

    let continue_flag = tool_call
        .args
        .get("continue")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    debug!("Executing beads_close: id={}", id);

    let mut args: Vec<&str> = vec!["close", id];

    let reason_flag;
    if let Some(r) = reason {
        reason_flag = r.to_string();
        args.push("--reason");
        args.push(&reason_flag);
    }

    if continue_flag {
        args.push("--continue");
    }

    args.push("--json");

    let json = run_bd_command(&args, ctx.working_dir).await?;
    Ok(format_beads_output(&json, "beads_close"))
}

/// Execute the beads_show tool.
/// Shows detailed information about a specific issue.
pub async fn execute_beads_show<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let id = tool_call
        .args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: id"))?;

    debug!("Executing beads_show: id={}", id);

    let json = run_bd_command(&["show", id, "--json"], ctx.working_dir).await?;
    Ok(format_beads_output(&json, "beads_show"))
}

/// Execute the beads_list tool.
/// Lists issues with optional filters.
pub async fn execute_beads_list<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let status = tool_call
        .args
        .get("status")
        .and_then(|v| v.as_str());

    let issue_type = tool_call
        .args
        .get("type")
        .and_then(|v| v.as_str());

    let limit = tool_call
        .args
        .get("limit")
        .and_then(|v| v.as_u64());

    debug!("Executing beads_list");

    let mut args: Vec<&str> = vec!["list"];

    let status_flag;
    if let Some(s) = status {
        status_flag = s.to_string();
        args.push("--status");
        args.push(&status_flag);
    }

    let type_flag;
    if let Some(t) = issue_type {
        type_flag = t.to_string();
        args.push("--type");
        args.push(&type_flag);
    }

    let limit_flag;
    if let Some(l) = limit {
        limit_flag = l.to_string();
        args.push("--limit");
        args.push(&limit_flag);
    }

    args.push("--json");

    let json = run_bd_command(&args, ctx.working_dir).await?;
    Ok(format_beads_output(&json, "beads_list"))
}

/// Execute the beads_dep tool.
/// Adds or removes dependencies between issues.
pub async fn execute_beads_dep<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let action = tool_call
        .args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: action (add/remove)"))?;

    if action != "add" && action != "remove" {
        return Err(anyhow!("Action must be 'add' or 'remove'"));
    }

    let child_id = tool_call
        .args
        .get("child_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: child_id"))?;

    let parent_id = tool_call
        .args
        .get("parent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: parent_id"))?;

    let dep_type = tool_call
        .args
        .get("dep_type")
        .and_then(|v| v.as_str());

    debug!("Executing beads_dep: action={}, child={}, parent={}", action, child_id, parent_id);

    let mut args: Vec<&str> = vec!["dep", action, child_id, parent_id];

    let type_flag;
    if let Some(t) = dep_type {
        type_flag = t.to_string();
        args.push("--type");
        args.push(&type_flag);
    }
    args.push("--json");

    let json = run_bd_command(&args, ctx.working_dir).await?;
    Ok(format_beads_output(&json, "beads_dep"))
}

/// Execute the beads_sync tool.
/// Syncs with remote repository.
pub async fn execute_beads_sync<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Executing beads_sync");

    let json = run_bd_command(&["sync", "--json"], ctx.working_dir).await?;
    Ok(format_beads_output(&json, "beads_sync"))
}

/// Execute the beads_prime tool.
/// Primes the beads database.
pub async fn execute_beads_prime<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Executing beads_prime");

    let json = run_bd_command(&["prime", "--json"], ctx.working_dir).await?;
    Ok(format_beads_output(&json, "beads_prime"))
}

// =============================================================================
// Molecule Operations
// =============================================================================

/// Execute the formula_list tool.
/// Lists available formulas.
pub async fn execute_formula_list<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Executing formula_list");

    let json = run_bd_command(&["formula", "list", "--json"], ctx.working_dir).await?;
    Ok(format_beads_output(&json, "formula_list"))
}

/// Execute the formula_cook tool.
/// Cooks a formula with optional variables.
pub async fn execute_formula_cook<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let formula = tool_call
        .args
        .get("formula")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: formula"))?;

    let vars = tool_call
        .args
        .get("vars")
        .and_then(|v| v.as_object());

    debug!("Executing formula_cook: formula={}", formula);

    let mut args: Vec<String> = vec!["cook".to_string(), formula.to_string()];

    // Add variables as --var key=value
    if let Some(vars_obj) = vars {
        for (key, value) in vars_obj {
            let val_str = match value {
                Value::String(s) => s.clone(),
                _ => value.to_string(),
            };
            args.push("--var".to_string());
            args.push(format!("{}={}", key, val_str));
        }
    }

    args.push("--json".to_string());

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let json = run_bd_command(&args_refs, ctx.working_dir).await?;
    Ok(format_beads_output(&json, "formula_cook"))
}

/// Execute the mol_pour tool.
/// Pours a molecule from a proto.
pub async fn execute_mol_pour<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let proto = tool_call
        .args
        .get("proto")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: proto"))?;

    let vars = tool_call
        .args
        .get("vars")
        .and_then(|v| v.as_object());

    debug!("Executing mol_pour: proto={}", proto);

    let mut args: Vec<String> = vec!["mol".to_string(), "pour".to_string(), proto.to_string()];

    if let Some(vars_obj) = vars {
        for (key, value) in vars_obj {
            let val_str = match value {
                Value::String(s) => s.clone(),
                _ => value.to_string(),
            };
            args.push("--var".to_string());
            args.push(format!("{}={}", key, val_str));
        }
    }

    args.push("--json".to_string());

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let json = run_bd_command(&args_refs, ctx.working_dir).await?;
    Ok(format_beads_output(&json, "mol_pour"))
}

/// Execute the mol_wisp tool.
/// Creates a wisp from a proto.
pub async fn execute_mol_wisp<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let proto = tool_call
        .args
        .get("proto")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: proto"))?;

    let vars = tool_call
        .args
        .get("vars")
        .and_then(|v| v.as_object());

    debug!("Executing mol_wisp: proto={}", proto);

    let mut args: Vec<String> = vec!["mol".to_string(), "wisp".to_string(), proto.to_string()];

    if let Some(vars_obj) = vars {
        for (key, value) in vars_obj {
            let val_str = match value {
                Value::String(s) => s.clone(),
                _ => value.to_string(),
            };
            args.push("--var".to_string());
            args.push(format!("{}={}", key, val_str));
        }
    }

    args.push("--json".to_string());

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let json = run_bd_command(&args_refs, ctx.working_dir).await?;
    Ok(format_beads_output(&json, "mol_wisp"))
}

/// Execute the mol_current tool.
/// Shows the current active molecule.
pub async fn execute_mol_current<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let id = tool_call
        .args
        .get("id")
        .and_then(|v| v.as_str());

    debug!("Executing mol_current");

    let mut args: Vec<&str> = vec!["mol", "current"];

    if let Some(i) = id {
        args.push(i);
    }

    args.push("--json");

    let json = run_bd_command(&args, ctx.working_dir).await?;
    Ok(format_beads_output(&json, "mol_current"))
}

/// Execute the mol_squash tool.
/// Squashes a molecule.
pub async fn execute_mol_squash<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let id = tool_call
        .args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing required parameter: id"))?;

    let summary = tool_call
        .args
        .get("summary")
        .and_then(|v| v.as_str());

    debug!("Executing mol_squash: id={}", id);

    let mut args: Vec<&str> = vec!["mol", "squash", id];

    let summary_flag;
    if let Some(s) = summary {
        summary_flag = s.to_string();
        args.push("--summary");
        args.push(&summary_flag);
    }

    args.push("--json");

    let json = run_bd_command(&args, ctx.working_dir).await?;
    Ok(format_beads_output(&json, "mol_squash"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_beads_installed() {
        // This test just checks that the function runs without panicking
        let _installed = is_beads_installed();
    }

    #[test]
    fn test_detect_beads_project() {
        // This test just checks that the function runs without panicking
        let _project = detect_beads_project();
    }

    #[test]
    fn test_format_beads_output_ready() {
        let json = serde_json::json!([
            {
                "id": "abc123",
                "title": "Test Issue",
                "status": "open",
                "priority": 1,
                "type": "bug"
            }
        ]);
        let output = format_beads_output(&json, "beads_ready");
        assert!(output.contains("Ready Issues"));
        assert!(output.contains("abc123"));
        assert!(output.contains("Test Issue"));
        assert!(output.contains("P1-High"));
    }

    #[test]
    fn test_format_beads_output_empty_list() {
        let json = serde_json::json!([]);
        let output = format_beads_output(&json, "beads_list");
        assert!(output.contains("No issues found"));
    }

    #[test]
    fn test_format_beads_output_show() {
        let json = serde_json::json!({
            "id": "def456",
            "title": "Detailed Issue",
            "status": "in_progress",
            "priority": 2,
            "type": "feature",
            "description": "A detailed description"
        });
        let output = format_beads_output(&json, "beads_show");
        assert!(output.contains("Issue Details"));
        assert!(output.contains("def456"));
        assert!(output.contains("Detailed Issue"));
        assert!(output.contains("in_progress"));
        assert!(output.contains("P2-Medium"));
        assert!(output.contains("A detailed description"));
    }

    #[test]
    fn test_priority_labels() {
        for (priority, expected) in [
            (0, "P0-Critical"),
            (1, "P1-High"),
            (2, "P2-Medium"),
            (3, "P3-Low"),
            (4, "P4-Trivial"),
        ] {
            let json = serde_json::json!({
                "id": "test",
                "title": "Test",
                "status": "open",
                "priority": priority,
                "type": "issue"
            });
            let mut output = String::new();
            format_issue_summary(&mut output, &json);
            assert!(output.contains(expected), "Priority {} should be {}", priority, expected);
        }
    }
}
