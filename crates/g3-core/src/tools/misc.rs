//! Miscellaneous tools: take_screenshot, code_coverage, code_search.

use anyhow::Result;
use tracing::debug;

use crate::ui_writer::UiWriter;
use crate::ToolCall;

use super::executor::ToolContext;

/// Execute the `take_screenshot` tool.
pub async fn execute_take_screenshot<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing take_screenshot tool call");
    
    let controller = match ctx.computer_controller {
        Some(c) => c,
        None => {
            return Ok(
                "❌ Computer control not enabled. Set computer_control.enabled = true in config."
                    .to_string(),
            )
        }
    };

    let path = tool_call
        .args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing path argument"))?;

    // Extract window_id (app name) - REQUIRED
    let window_id = tool_call
        .args
        .get("window_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Missing window_id argument. You must specify which window to capture \
                (e.g., 'Safari', 'Terminal', 'Google Chrome')."
            )
        })?;

    // Extract region if provided
    let region = tool_call
        .args
        .get("region")
        .and_then(|v| v.as_object())
        .map(|region_obj| g3_computer_control::types::Rect {
            x: region_obj.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            y: region_obj.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            width: region_obj
                .get("width")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32,
            height: region_obj
                .get("height")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32,
        });

    match controller.take_screenshot(path, region, Some(window_id)).await {
        Ok(_) => {
            // Get the actual path where the screenshot was saved
            let actual_path = if path.starts_with('/') {
                path.to_string()
            } else {
                let temp_dir = std::env::temp_dir();
                format!("{}/{}", temp_dir.display().to_string().trim_end_matches('/'), path)
            };

            Ok(format!(
                "✅ Screenshot of {} saved to: {}",
                window_id, actual_path
            ))
        }
        Err(e) => Ok(format!("❌ Failed to take screenshot: {}", e)),
    }
}

/// Execute the `code_coverage` tool.
pub async fn execute_code_coverage<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing code_coverage tool call");
    let _ = tool_call; // unused
    
    ctx.ui_writer
        .print_context_status("🔍 Generating code coverage report...");

    // Ensure coverage tools are installed
    match g3_execution::ensure_coverage_tools_installed() {
        Ok(already_installed) => {
            if !already_installed {
                ctx.ui_writer
                    .print_context_status("✅ Coverage tools installed successfully");
            }
        }
        Err(e) => {
            return Ok(format!("❌ Failed to install coverage tools: {}", e));
        }
    }

    // Run cargo llvm-cov --workspace
    let output = std::process::Command::new("cargo")
        .args(["llvm-cov", "--workspace"])
        .current_dir(std::env::current_dir()?)
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = String::from("✅ Code coverage report generated successfully\n\n");
        result.push_str("## Coverage Summary\n");
        result.push_str(&stdout);
        if !stderr.is_empty() {
            result.push_str("\n## Warnings\n");
            result.push_str(&stderr);
        }
        Ok(result)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(format!("❌ Failed to generate coverage report:\n{}", stderr))
    }
}

/// Execute the `code_search` tool.
pub async fn execute_code_search<W: UiWriter>(
    tool_call: &ToolCall,
    _ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing code_search tool call");

    // Parse the request
    let request: crate::code_search::CodeSearchRequest =
        match serde_json::from_value(tool_call.args.clone()) {
            Ok(req) => req,
            Err(e) => {
                return Ok(format!("❌ Invalid code_search arguments: {}", e));
            }
        };

    // Execute the code search
    match crate::code_search::execute_code_search(request).await {
        Ok(response) => {
            // Serialize the response to JSON
            match serde_json::to_string_pretty(&response) {
                Ok(json_output) => Ok(format!("✅ Code search completed\n{}", json_output)),
                Err(e) => Ok(format!("❌ Failed to serialize response: {}", e)),
            }
        }
        Err(e) => Ok(format!("❌ Code search failed: {}", e)),
    }
}

/// Execute the `switch_mode` tool.
/// Allows the agent to recommend switching to a different execution mode.
pub async fn execute_switch_mode<W: UiWriter>(
    tool_call: &ToolCall,
    _ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing switch_mode tool call");

    let new_mode = tool_call
        .args
        .get("mode")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing mode argument. Available modes: interactive, autonomous, accumulative, studio"))?;

    match new_mode {
        "interactive" | "autonomous" | "accumulative" | "studio" => {
            Ok(format!(
                "✅ Mode switch recommended: `{}`\n\n\
                To switch modes, please run g3 with the appropriate flag:\n\
                - interactive: `g3` or `g3 --chat`\n\
                - autonomous: `g3 --autonomous [--max-turns N]`\n\
                - accumulative: `g3 --auto`\n\
                - studio: `studio run [--agent <name>]`\n\n\
                Note: Mode switching requires restarting g3 with the new flag.",
                new_mode
            ))
        }
        _ => Ok(format!(
            "❌ Unknown mode: `{}`. Available modes: interactive, autonomous, accumulative, studio",
            new_mode
        )),
    }
}

