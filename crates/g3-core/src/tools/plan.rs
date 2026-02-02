//! Plan Mode - Structured task planning with cognitive forcing.
//!
//! This module implements Plan Mode, which replaces the TODO system with a
//! checklist-style plan that forces reasoning about:
//! - Happy path
//! - Negative case  
//! - Boundary condition
//!
//! A task is done ONLY when all plan items are satisfied with evidence.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use tracing::debug;

use crate::paths::{ensure_session_dir, get_session_logs_dir};
use crate::ui_writer::UiWriter;
use crate::ToolCall;

use super::executor::ToolContext;

// ============================================================================
// Plan Schema
// ============================================================================

/// State of a plan item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PlanState {
    #[default]
    Todo,
    Doing,
    Done,
    Blocked,
}

impl fmt::Display for PlanState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanState::Todo => write!(f, "todo"),
            PlanState::Doing => write!(f, "doing"),
            PlanState::Done => write!(f, "done"),
            PlanState::Blocked => write!(f, "blocked"),
        }
    }
}

impl std::str::FromStr for PlanState {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "todo" => Ok(PlanState::Todo),
            "doing" => Ok(PlanState::Doing),
            "done" => Ok(PlanState::Done),
            "blocked" => Ok(PlanState::Blocked),
            _ => Err(anyhow!("Invalid plan state: {}", s)),
        }
    }
}

/// A check with description and target.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Check {
    /// Description of what this check verifies
    pub desc: String,
    /// Target module/function/file this check applies to
    pub target: String,
}

impl Check {
    pub fn new(desc: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            desc: desc.into(),
            target: target.into(),
        }
    }

    /// Validate that the check has required fields.
    pub fn validate(&self) -> Result<()> {
        if self.desc.trim().is_empty() {
            return Err(anyhow!("Check description cannot be empty"));
        }
        if self.target.trim().is_empty() {
            return Err(anyhow!("Check target cannot be empty"));
        }
        Ok(())
    }
}

/// The three required checks for each plan item.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Checks {
    /// Happy path check - normal successful operation
    pub happy: Check,
    /// Negative case check - error handling, invalid input
    pub negative: Check,
    /// Boundary condition check - edge cases, limits
    pub boundary: Check,
}

impl Checks {
    /// Validate all three checks.
    pub fn validate(&self) -> Result<()> {
        self.happy.validate().map_err(|e| anyhow!("happy check: {}", e))?;
        self.negative.validate().map_err(|e| anyhow!("negative check: {}", e))?;
        self.boundary.validate().map_err(|e| anyhow!("boundary check: {}", e))?;
        Ok(())
    }
}

/// A single item in the plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    /// Stable identifier (e.g., "I1", "I2")
    pub id: String,
    /// What will be done
    pub description: String,
    /// Current state
    pub state: PlanState,
    /// Paths/modules this affects
    pub touches: Vec<String>,
    /// The three required checks
    pub checks: Checks,
    /// Evidence when done (file:line, test names, snippets)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    /// Short explanation including implementation nuances
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl PlanItem {
    /// Create a new plan item with required fields.
    pub fn new(
        id: impl Into<String>,
        description: impl Into<String>,
        touches: Vec<String>,
        checks: Checks,
    ) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            state: PlanState::Todo,
            touches,
            checks,
            evidence: Vec::new(),
            notes: None,
        }
    }

    /// Validate the plan item structure.
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(anyhow!("Item id cannot be empty"));
        }
        if self.description.trim().is_empty() {
            return Err(anyhow!("Item description cannot be empty"));
        }
        if self.touches.is_empty() {
            return Err(anyhow!("Item must specify at least one path/module in 'touches'"));
        }
        self.checks.validate().map_err(|e| anyhow!("Item '{}': {}", self.id, e))?;

        // If done, must have evidence and notes
        if self.state == PlanState::Done {
            if self.evidence.is_empty() {
                return Err(anyhow!(
                    "Item '{}' is marked done but has no evidence",
                    self.id
                ));
            }
            if self.notes.as_ref().map(|n| n.trim().is_empty()).unwrap_or(true) {
                return Err(anyhow!(
                    "Item '{}' is marked done but has no notes",
                    self.id
                ));
            }
        }

        Ok(())
    }

    /// Check if this item is terminal (done or blocked).
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, PlanState::Done | PlanState::Blocked)
    }
}

/// A complete plan with metadata and items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Unique identifier for this plan
    pub plan_id: String,
    /// Current revision number (increments on each write)
    pub revision: u32,
    /// The revision that was approved (None if not yet approved)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_revision: Option<u32>,
    /// The plan items
    pub items: Vec<PlanItem>,
}

impl Plan {
    /// Create a new plan with the given ID.
    pub fn new(plan_id: impl Into<String>) -> Self {
        Self {
            plan_id: plan_id.into(),
            revision: 1,
            approved_revision: None,
            items: Vec::new(),
        }
    }

    /// Check if the plan has been approved.
    pub fn is_approved(&self) -> bool {
        self.approved_revision.is_some()
    }

    /// Approve the current revision.
    pub fn approve(&mut self) {
        self.approved_revision = Some(self.revision);
    }

    /// Check if all items are terminal (done or blocked).
    pub fn is_complete(&self) -> bool {
        !self.items.is_empty() && self.items.iter().all(|item| item.is_terminal())
    }

    /// Validate the entire plan structure.
    pub fn validate(&self) -> Result<()> {
        if self.plan_id.trim().is_empty() {
            return Err(anyhow!("Plan ID cannot be empty"));
        }

        if self.items.is_empty() {
            return Err(anyhow!("Plan must have at least one item"));
        }

        if self.items.len() > 7 {
            // Warn but don't fail - this is a guideline
            debug!("Plan has {} items (recommended max is 7)", self.items.len());
        }

        // Check for duplicate IDs
        let mut seen_ids = std::collections::HashSet::new();
        for item in &self.items {
            if !seen_ids.insert(&item.id) {
                return Err(anyhow!("Duplicate item ID: {}", item.id));
            }
            item.validate()?;
        }

        Ok(())
    }

    /// Get a summary of the plan status.
    pub fn status_summary(&self) -> String {
        let total = self.items.len();
        let done = self.items.iter().filter(|i| i.state == PlanState::Done).count();
        let doing = self.items.iter().filter(|i| i.state == PlanState::Doing).count();
        let blocked = self.items.iter().filter(|i| i.state == PlanState::Blocked).count();
        let todo = self.items.iter().filter(|i| i.state == PlanState::Todo).count();

        let approved_str = if let Some(rev) = self.approved_revision {
            format!(" (approved at rev {})", rev)
        } else {
            " (NOT APPROVED)".to_string()
        };

        format!(
            "Plan '{}' rev {}{}: {}/{} done, {} doing, {} blocked, {} todo",
            self.plan_id, self.revision, approved_str, done, total, doing, blocked, todo
        )
    }
}

// ============================================================================
// Plan Storage
// ============================================================================

/// Get the path to the plan.g3.md file for a session.
pub fn get_plan_path(session_id: &str) -> PathBuf {
    get_session_logs_dir(session_id).join("plan.g3.md")
}

/// Read a plan from the session's plan.g3.md file.
pub fn read_plan(session_id: &str) -> Result<Option<Plan>> {
    let path = get_plan_path(session_id);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)?;
    
    // Extract YAML from markdown code block
    let yaml_content = extract_yaml_from_markdown(&content)?;
    
    let plan: Plan = serde_yaml::from_str(&yaml_content)?;
    Ok(Some(plan))
}

/// Write a plan to the session's plan.g3.md file.
pub fn write_plan(session_id: &str, plan: &Plan) -> Result<()> {
    // Validate before writing
    plan.validate()?;

    let _ = ensure_session_dir(session_id)?;
    let path = get_plan_path(session_id);

    // Format as markdown with YAML code block
    let content = format_plan_as_markdown(plan);
    
    std::fs::write(&path, content)?;
    Ok(())
}

/// Extract YAML content from a markdown file with ```yaml code block.
fn extract_yaml_from_markdown(content: &str) -> Result<String> {
    // Look for ```yaml ... ``` block
    let start_marker = "```yaml";
    let end_marker = "```";

    if let Some(start_idx) = content.find(start_marker) {
        let yaml_start = start_idx + start_marker.len();
        if let Some(end_idx) = content[yaml_start..].find(end_marker) {
            let yaml = content[yaml_start..yaml_start + end_idx].trim();
            return Ok(yaml.to_string());
        }
    }

    // If no code block, try parsing the whole content as YAML
    Ok(content.to_string())
}

/// Format a plan as markdown with embedded YAML.
fn format_plan_as_markdown(plan: &Plan) -> String {
    let yaml = serde_yaml::to_string(plan).unwrap_or_else(|_| "# Error serializing plan".to_string());
    
    let mut md = String::new();
    md.push_str(&format!("# Plan: {}\n\n", plan.plan_id));
    md.push_str(&format!("**Status**: {}\n\n", plan.status_summary()));
    md.push_str("## Plan Data\n\n");
    md.push_str("```yaml\n");
    md.push_str(&yaml);
    md.push_str("```\n");
    
    md
}

// ============================================================================
// Plan Verification
// ============================================================================

/// Verify a completed plan. Called by the agent loop when all items are done/blocked.
/// 
/// This is a placeholder that prints the plan contents.
/// In the future, this could perform additional validation.
pub fn plan_verify(plan: &Plan) {
    println!("\n{}", "=".repeat(60));
    println!("PLAN VERIFY CALLED");
    println!("{}", "=".repeat(60));
    println!("Plan ID: {}", plan.plan_id);
    println!("Revision: {}", plan.revision);
    println!("Approved: {:?}", plan.approved_revision);
    println!("Status: {}", plan.status_summary());
    println!();
    
    for item in &plan.items {
        println!("[{}] {} - {}", item.id, item.state, item.description);
        println!("  Touches: {:?}", item.touches);
        println!("  Checks:");
        println!("    Happy: {} -> {}", item.checks.happy.desc, item.checks.happy.target);
        println!("    Negative: {} -> {}", item.checks.negative.desc, item.checks.negative.target);
        println!("    Boundary: {} -> {}", item.checks.boundary.desc, item.checks.boundary.target);
        if !item.evidence.is_empty() {
            println!("  Evidence:");
            for e in &item.evidence {
                println!("    - {}", e);
            }
        }
        if let Some(notes) = &item.notes {
            println!("  Notes: {}", notes);
        }
        println!();
    }
    println!("{}\n", "=".repeat(60));
}

// ============================================================================
// Tool Implementations
// ============================================================================

/// Execute the `plan_read` tool.
pub async fn execute_plan_read<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing plan_read tool call");

    let session_id = match ctx.session_id {
        Some(id) => id,
        None => return Ok("❌ No active session - plans are session-scoped.".to_string()),
    };

    match read_plan(session_id)? {
        Some(plan) => {
            let yaml = serde_yaml::to_string(&plan)?;
            Ok(format!(
                "📋 {}\n\n```yaml\n{}```",
                plan.status_summary(),
                yaml
            ))
        }
        None => Ok("📋 No plan exists for this session. Use plan_write to create one.".to_string()),
    }
}

/// Execute the `plan_write` tool.
pub async fn execute_plan_write<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing plan_write tool call");

    let session_id = match ctx.session_id {
        Some(id) => id,
        None => return Ok("❌ No active session - plans are session-scoped.".to_string()),
    };

    // Get the plan content from args
    let plan_yaml = match tool_call.args.get("plan").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return Ok("❌ Missing 'plan' argument. Provide the plan as YAML.".to_string()),
    };

    // Parse the YAML
    let mut plan: Plan = match serde_yaml::from_str(plan_yaml) {
        Ok(p) => p,
        Err(e) => return Ok(format!("❌ Invalid plan YAML: {}", e)),
    };

    // Load existing plan to preserve approved_revision and increment revision
    if let Some(existing) = read_plan(session_id)? {
        // Preserve approved_revision from existing plan
        plan.approved_revision = existing.approved_revision;
        // Increment revision
        plan.revision = existing.revision + 1;

        // If plan was approved, ensure checks are not removed
        if existing.is_approved() {
            // Verify all existing item IDs still exist
            for existing_item in &existing.items {
                if !plan.items.iter().any(|i| i.id == existing_item.id) {
                    return Ok(format!(
                        "❌ Cannot remove item '{}' from approved plan. Items can only be marked blocked, not removed.",
                        existing_item.id
                    ));
                }
            }
        }
    }

    // Validate the plan
    if let Err(e) = plan.validate() {
        return Ok(format!("❌ Plan validation failed: {}", e));
    }

    // Write the plan
    if let Err(e) = write_plan(session_id, &plan) {
        return Ok(format!("❌ Failed to write plan: {}", e));
    }

    // Check if plan is now complete and trigger verification
    if plan.is_complete() && plan.is_approved() {
        plan_verify(&plan);
    }

    Ok(format!("✅ Plan updated: {}", plan.status_summary()))
}

/// Execute the `plan_approve` tool.
pub async fn execute_plan_approve<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing plan_approve tool call");

    let session_id = match ctx.session_id {
        Some(id) => id,
        None => return Ok("❌ No active session - plans are session-scoped.".to_string()),
    };

    // Load existing plan
    let mut plan = match read_plan(session_id)? {
        Some(p) => p,
        None => return Ok("❌ No plan exists to approve. Use plan_write first.".to_string()),
    };

    if plan.is_approved() {
        return Ok(format!(
            "ℹ️ Plan already approved at revision {}. Current revision: {}",
            plan.approved_revision.unwrap(),
            plan.revision
        ));
    }

    // Approve the plan
    plan.approve();

    // Write back
    if let Err(e) = write_plan(session_id, &plan) {
        return Ok(format!("❌ Failed to save approved plan: {}", e));
    }

    Ok(format!(
        "✅ Plan approved at revision {}. You may now begin implementation.",
        plan.revision
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_check() -> Check {
        Check::new("Test description", "test::target")
    }

    fn make_test_checks() -> Checks {
        Checks {
            happy: make_test_check(),
            negative: make_test_check(),
            boundary: make_test_check(),
        }
    }

    fn make_test_item(id: &str) -> PlanItem {
        PlanItem::new(
            id,
            "Test item description",
            vec!["src/test.rs".to_string()],
            make_test_checks(),
        )
    }

    #[test]
    fn test_plan_state_display() {
        assert_eq!(PlanState::Todo.to_string(), "todo");
        assert_eq!(PlanState::Doing.to_string(), "doing");
        assert_eq!(PlanState::Done.to_string(), "done");
        assert_eq!(PlanState::Blocked.to_string(), "blocked");
    }

    #[test]
    fn test_plan_state_from_str() {
        assert_eq!("todo".parse::<PlanState>().unwrap(), PlanState::Todo);
        assert_eq!("DOING".parse::<PlanState>().unwrap(), PlanState::Doing);
        assert_eq!("Done".parse::<PlanState>().unwrap(), PlanState::Done);
        assert!("invalid".parse::<PlanState>().is_err());
    }

    #[test]
    fn test_check_validation() {
        let valid = Check::new("desc", "target");
        assert!(valid.validate().is_ok());

        let empty_desc = Check::new("", "target");
        assert!(empty_desc.validate().is_err());

        let empty_target = Check::new("desc", "");
        assert!(empty_target.validate().is_err());
    }

    #[test]
    fn test_plan_item_validation() {
        let item = make_test_item("I1");
        assert!(item.validate().is_ok());

        // Done item without evidence should fail
        let mut done_item = make_test_item("I2");
        done_item.state = PlanState::Done;
        assert!(done_item.validate().is_err());

        // Done item with evidence but no notes should fail
        done_item.evidence = vec!["src/test.rs:42".to_string()];
        assert!(done_item.validate().is_err());

        // Done item with evidence and notes should pass
        done_item.notes = Some("Implementation notes".to_string());
        assert!(done_item.validate().is_ok());
    }

    #[test]
    fn test_plan_validation() {
        let mut plan = Plan::new("test-plan");
        
        // Empty plan should fail
        assert!(plan.validate().is_err());

        // Plan with item should pass
        plan.items.push(make_test_item("I1"));
        assert!(plan.validate().is_ok());

        // Duplicate IDs should fail
        plan.items.push(make_test_item("I1"));
        assert!(plan.validate().is_err());
    }

    #[test]
    fn test_plan_is_complete() {
        let mut plan = Plan::new("test");
        plan.items.push(make_test_item("I1"));
        plan.items.push(make_test_item("I2"));

        assert!(!plan.is_complete());

        plan.items[0].state = PlanState::Done;
        plan.items[0].evidence = vec!["test".to_string()];
        plan.items[0].notes = Some("notes".to_string());
        assert!(!plan.is_complete());

        plan.items[1].state = PlanState::Blocked;
        assert!(plan.is_complete());
    }

    #[test]
    fn test_plan_approval() {
        let mut plan = Plan::new("test");
        plan.items.push(make_test_item("I1"));

        assert!(!plan.is_approved());
        assert_eq!(plan.approved_revision, None);

        plan.approve();
        assert!(plan.is_approved());
        assert_eq!(plan.approved_revision, Some(1));
    }

    #[test]
    fn test_yaml_extraction() {
        let md = r#"# Plan: test

**Status**: ...

## Plan Data

```yaml
plan_id: test
revision: 1
items: []
```
"#;

        let yaml = extract_yaml_from_markdown(md).unwrap();
        assert!(yaml.contains("plan_id: test"));
    }

    #[test]
    fn test_plan_serialization_roundtrip() {
        let mut plan = Plan::new("test-plan");
        plan.items.push(make_test_item("I1"));
        plan.approve();

        let yaml = serde_yaml::to_string(&plan).unwrap();
        let parsed: Plan = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(parsed.plan_id, plan.plan_id);
        assert_eq!(parsed.revision, plan.revision);
        assert_eq!(parsed.approved_revision, plan.approved_revision);
        assert_eq!(parsed.items.len(), plan.items.len());
    }
}
