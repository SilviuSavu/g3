//! Codebase scout tool: spawns a scout agent to explore the codebase structure.
//!
//! Follows the same async pattern as the research tool:
//! 1. Register task with PendingResearchManager
//! 2. Spawn `g3 --agent codebase-scout` in background
//! 3. Return immediately with scout_id
//! 4. Background task captures output, extracts report, calls manager.complete()

use anyhow::Result;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error};

use crate::ui_writer::UiWriter;
use crate::ToolCall;

use super::executor::ToolContext;
use super::research::extract_report_from_output;

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

    let description = format!("Explore codebase structure at: {}", path);
    let scout_id = ctx.pending_research_manager.register(&description);

    let scout_id_clone = scout_id.clone();
    let manager = ctx.pending_research_manager.clone();
    let path_owned = path.to_string();

    let g3_path = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("g3"));

    tokio::spawn(async move {
        let result = run_codebase_scout(&g3_path, &path_owned).await;

        match result {
            Ok(report) => {
                debug!("Codebase scout {} completed successfully", scout_id_clone);
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

/// Run the codebase scout agent and return the report.
async fn run_codebase_scout(
    g3_path: &std::path::Path,
    path: &str,
) -> Result<String> {
    let prompt = format!(
        "Explore the codebase at '{}' and produce a structural overview. \
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
    extract_report_from_output(&full_output)
}

/// Execute the codebase_scout_status tool - reuses research_status logic.
pub async fn execute_codebase_scout_status<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    // Reuse research_status - same manager, same format
    super::research::execute_research_status(tool_call, ctx).await
}
