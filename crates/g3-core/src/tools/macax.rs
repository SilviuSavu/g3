//! macOS Accessibility API tools.

use anyhow::Result;
use tracing::debug;

use crate::ui_writer::UiWriter;
use crate::ToolCall;

use super::executor::ToolContext;

/// Execute the `macax_list_apps` tool.
pub async fn execute_macax_list_apps<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing macax_list_apps tool call");
    let _ = tool_call; // unused

    if !ctx.config.macax.enabled {
        return Ok(
            "❌ macOS Accessibility is not enabled. Use --macax flag to enable.".to_string(),
        );
    }

    let controller_guard = ctx.macax_controller.read().await;
    let controller = match controller_guard.as_ref() {
        Some(c) => c,
        None => return Ok("❌ macOS Accessibility controller not initialized.".to_string()),
    };

    match controller.list_applications() {
        Ok(apps) => {
            let app_list: Vec<String> = apps.iter().map(|a| a.name.clone()).collect();
            Ok(format!("Running applications:\n{}", app_list.join("\n")))
        }
        Err(e) => Ok(format!("❌ Failed to list applications: {}", e)),
    }
}

/// Execute the `macax_get_frontmost_app` tool.
pub async fn execute_macax_get_frontmost_app<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing macax_get_frontmost_app tool call");
    let _ = tool_call; // unused

    if !ctx.config.macax.enabled {
        return Ok(
            "❌ macOS Accessibility is not enabled. Use --macax flag to enable.".to_string(),
        );
    }

    let controller_guard = ctx.macax_controller.read().await;
    let controller = match controller_guard.as_ref() {
        Some(c) => c,
        None => return Ok("❌ macOS Accessibility controller not initialized.".to_string()),
    };

    match controller.get_frontmost_app() {
        Ok(app) => Ok(format!("Frontmost application: {}", app.name)),
        Err(e) => Ok(format!("❌ Failed to get frontmost app: {}", e)),
    }
}

/// Execute the `macax_activate_app` tool.
pub async fn execute_macax_activate_app<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing macax_activate_app tool call");

    if !ctx.config.macax.enabled {
        return Ok(
            "❌ macOS Accessibility is not enabled. Use --macax flag to enable.".to_string(),
        );
    }

    let app_name = match tool_call.args.get("app_name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return Ok("❌ Missing app_name argument".to_string()),
    };

    let controller_guard = ctx.macax_controller.read().await;
    let controller = match controller_guard.as_ref() {
        Some(c) => c,
        None => return Ok("❌ macOS Accessibility controller not initialized.".to_string()),
    };

    match controller.activate_app(app_name) {
        Ok(_) => Ok(format!("✅ Activated application: {}", app_name)),
        Err(e) => Ok(format!("❌ Failed to activate app: {}", e)),
    }
}

/// Execute the `macax_press_key` tool.
pub async fn execute_macax_press_key<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing macax_press_key tool call");

    if !ctx.config.macax.enabled {
        return Ok(
            "❌ macOS Accessibility is not enabled. Use --macax flag to enable.".to_string(),
        );
    }

    let app_name = match tool_call.args.get("app_name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return Ok("❌ Missing app_name argument".to_string()),
    };

    let key = match tool_call.args.get("key").and_then(|v| v.as_str()) {
        Some(k) => k,
        None => return Ok("❌ Missing key argument".to_string()),
    };

    let modifiers_vec: Vec<&str> = tool_call
        .args
        .get("modifiers")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let controller_guard = ctx.macax_controller.read().await;
    let controller = match controller_guard.as_ref() {
        Some(c) => c,
        None => return Ok("❌ macOS Accessibility controller not initialized.".to_string()),
    };

    match controller.press_key(app_name, key, modifiers_vec.clone()) {
        Ok(_) => {
            let modifier_str = if modifiers_vec.is_empty() {
                String::new()
            } else {
                format!(" with modifiers: {}", modifiers_vec.join("+"))
            };
            Ok(format!("✅ Pressed key: {}{}", key, modifier_str))
        }
        Err(e) => Ok(format!("❌ Failed to press key: {}", e)),
    }
}

/// Execute the `macax_type_text` tool.
pub async fn execute_macax_type_text<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing macax_type_text tool call");

    if !ctx.config.macax.enabled {
        return Ok(
            "❌ macOS Accessibility is not enabled. Use --macax flag to enable.".to_string(),
        );
    }

    let app_name = match tool_call.args.get("app_name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return Ok("❌ Missing app_name argument".to_string()),
    };

    let text = match tool_call.args.get("text").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return Ok("❌ Missing text argument".to_string()),
    };

    let controller_guard = ctx.macax_controller.read().await;
    let controller = match controller_guard.as_ref() {
        Some(c) => c,
        None => return Ok("❌ macOS Accessibility controller not initialized.".to_string()),
    };

    match controller.type_text(app_name, text) {
        Ok(_) => Ok(format!("✅ Typed text into {}", app_name)),
        Err(e) => Ok(format!("❌ Failed to type text: {}", e)),
    }
}
