//! Vision-based tools: vision_find_text, vision_click_text, vision_click_near_text, extract_text_with_boxes.

use anyhow::Result;
use tracing::debug;

use crate::ui_writer::UiWriter;
use crate::ToolCall;

use super::executor::ToolContext;

/// Execute the `vision_find_text` tool.
pub async fn execute_vision_find_text<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing vision_find_text tool call");

    let controller = match ctx.computer_controller {
        Some(c) => c,
        None => {
            return Ok(
                "❌ Computer control not enabled. Set computer_control.enabled = true in config."
                    .to_string(),
            )
        }
    };

    let app_name = tool_call
        .args
        .get("app_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing app_name parameter"))?;

    let text = tool_call
        .args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing text parameter"))?;

    match controller.find_text_in_app(app_name, text).await {
        Ok(Some(location)) => Ok(format!(
            "✅ Found '{}' in {} at position ({}, {}) with size {}x{} (confidence: {:.0}%)",
            location.text,
            app_name,
            location.x,
            location.y,
            location.width,
            location.height,
            location.confidence * 100.0
        )),
        Ok(None) => Ok(format!("❌ Could not find '{}' in {}", text, app_name)),
        Err(e) => Ok(format!("❌ Error finding text: {}", e)),
    }
}

/// Execute the `vision_click_text` tool.
pub async fn execute_vision_click_text<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing vision_click_text tool call");

    let controller = match ctx.computer_controller {
        Some(c) => c,
        None => {
            return Ok(
                "❌ Computer control not enabled. Set computer_control.enabled = true in config."
                    .to_string(),
            )
        }
    };

    let app_name = tool_call
        .args
        .get("app_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing app_name parameter"))?;

    let text = tool_call
        .args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing text parameter"))?;

    match controller.find_text_in_app(app_name, text).await {
        Ok(Some(location)) => {
            // Click on center of text
            // IMPORTANT: location coordinates are in NSScreen space (Y=0 at BOTTOM, increases UPWARD)
            // location.x is the LEFT edge of the bounding box
            // location.y is the TOP edge of the bounding box (highest Y value in NSScreen space)
            // location.width and location.height are already scaled to screen space
            // To get center: we need to add half the SCALED width and subtract half the SCALED height

            if location.width == 0 || location.height == 0 {
                return Ok(format!(
                    "❌ Invalid bounding box dimensions: width={}, height={}",
                    location.width, location.height
                ));
            }

            debug!(
                "[vision_click_text] Location from find_text_in_app: x={}, y={}, width={}, height={}, text='{}'",
                location.x, location.y, location.width, location.height, location.text
            );

            // Calculate center using the SCALED dimensions
            // X: Use right edge instead of center (Vision OCR bounding box seems offset)
            // This gives us: left edge + full width = right edge
            // Y: top edge - half of scaled height (subtract because Y increases upward)
            let click_x = location.x + location.width; // Right edge
            let half_height = location.height / 2;
            let click_y = location.y - half_height;

            debug!(
                "[vision_click_text] Click position calculation: x={} + {} = {} (right edge), y={} - {} = {}",
                location.x, location.width, click_x, location.y, half_height, click_y
            );

            match controller.click_at(click_x, click_y, Some(app_name)) {
                Ok(_) => Ok(format!(
                    "✅ Clicked on '{}' in {} at ({}, {})",
                    text, app_name, click_x, click_y
                )),
                Err(e) => Ok(format!("❌ Failed to click: {}", e)),
            }
        }
        Ok(None) => Ok(format!("❌ Could not find '{}' in {}", text, app_name)),
        Err(e) => Ok(format!("❌ Error finding text: {}", e)),
    }
}

/// Execute the `vision_click_near_text` tool.
pub async fn execute_vision_click_near_text<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing vision_click_near_text tool call");

    let controller = match ctx.computer_controller {
        Some(c) => c,
        None => {
            return Ok(
                "❌ Computer control not enabled. Set computer_control.enabled = true in config."
                    .to_string(),
            )
        }
    };

    let app_name = tool_call
        .args
        .get("app_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing app_name parameter"))?;

    let text = tool_call
        .args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing text parameter"))?;

    let direction = tool_call
        .args
        .get("direction")
        .and_then(|v| v.as_str())
        .unwrap_or("right");

    let distance = tool_call
        .args
        .get("distance")
        .and_then(|v| v.as_i64())
        .unwrap_or(50) as i32;

    match controller.find_text_in_app(app_name, text).await {
        Ok(Some(location)) => {
            // Calculate click position based on direction
            // location.x is LEFT edge, location.y is TOP edge (in NSScreen space)
            let (click_x, click_y) = match direction {
                "right" => (
                    location.x + location.width + distance,
                    location.y - (location.height / 2),
                ),
                "below" => (
                    location.x + (location.width / 2),
                    location.y - location.height - distance,
                ),
                "left" => (location.x - distance, location.y - (location.height / 2)),
                "above" => (location.x + (location.width / 2), location.y + distance),
                _ => (
                    location.x + location.width + distance,
                    location.y - (location.height / 2),
                ),
            };
            debug!(
                "[vision_click_near_text] Clicking {} of text at ({}, {})",
                direction, click_x, click_y
            );

            match controller.click_at(click_x, click_y, Some(app_name)) {
                Ok(_) => Ok(format!(
                    "✅ Clicked {} of '{}' in {} at ({}, {})",
                    direction, text, app_name, click_x, click_y
                )),
                Err(e) => Ok(format!("❌ Failed to click: {}", e)),
            }
        }
        Ok(None) => Ok(format!("❌ Could not find '{}' in {}", text, app_name)),
        Err(e) => Ok(format!("❌ Error finding text: {}", e)),
    }
}

/// Execute the `extract_text_with_boxes` tool.
pub async fn execute_extract_text_with_boxes<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing extract_text_with_boxes tool call");

    if !ctx.config.macax.enabled {
        return Ok(
            "❌ extract_text_with_boxes requires --macax flag to be enabled".to_string(),
        );
    }

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
        .ok_or_else(|| anyhow::anyhow!("Missing path parameter"))?;

    // Optional: take screenshot of app first
    let final_path = if let Some(app_name) = tool_call.args.get("app_name").and_then(|v| v.as_str())
    {
        let temp_path = format!("/tmp/g3_extract_boxes_{}.png", uuid::Uuid::new_v4());
        match controller
            .take_screenshot(&temp_path, None, Some(app_name))
            .await
        {
            Ok(_) => temp_path,
            Err(e) => return Ok(format!("❌ Failed to take screenshot: {}", e)),
        }
    } else {
        path.to_string()
    };

    // Extract text with locations
    match controller.extract_text_with_locations(&final_path).await {
        Ok(locations) => {
            // Clean up temp file if we created one
            if final_path != path {
                let _ = std::fs::remove_file(&final_path);
            }

            // Return as JSON
            match serde_json::to_string_pretty(&locations) {
                Ok(json) => Ok(format!(
                    "✅ Extracted {} text elements:\n{}",
                    locations.len(),
                    json
                )),
                Err(e) => Ok(format!("❌ Failed to serialize results: {}", e)),
            }
        }
        Err(e) => Ok(format!("❌ Failed to extract text: {}", e)),
    }
}
