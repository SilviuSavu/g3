//! Team task list tools for multi-agent coordination.
//!
//! File-based task system where each task is a JSON file in a shared directory.
//! Tasks have: id, subject, description, status, owner, blocks, blockedBy, activeForm, metadata.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::ui_writer::UiWriter;
use crate::ToolCall;
use super::executor::ToolContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTask {
    pub id: String,
    pub subject: String,
    pub description: String,
    #[serde(default = "default_status")]
    pub status: String, // pending, in_progress, completed
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub blocks: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub active_form: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn default_status() -> String { "pending".to_string() }

fn get_config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        PathBuf::from("/tmp/.config")
    }
}

/// Get the tasks directory for a team
fn get_team_tasks_dir(team_name: &str) -> PathBuf {
    get_config_dir().join("g3").join("tasks").join(team_name)
}

/// Generate next task ID by scanning existing tasks
fn next_task_id(tasks_dir: &Path) -> String {
    let mut max_id: u32 = 0;
    if let Ok(entries) = std::fs::read_dir(tasks_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(id_str) = name.strip_suffix(".json") {
                    if let Ok(id) = id_str.parse::<u32>() {
                        max_id = max_id.max(id);
                    }
                }
            }
        }
    }
    (max_id + 1).to_string()
}

/// Read a single task from file
fn read_task(tasks_dir: &Path, task_id: &str) -> Result<TeamTask> {
    let path = tasks_dir.join(format!("{}.json", task_id));
    let content = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Write a single task to file
fn write_task(tasks_dir: &Path, task: &TeamTask) -> Result<()> {
    std::fs::create_dir_all(tasks_dir)?;
    let path = tasks_dir.join(format!("{}.json", task.id));
    let content = serde_json::to_string_pretty(task)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Read all tasks from directory
fn read_all_tasks(tasks_dir: &Path) -> Vec<TeamTask> {
    let mut tasks = Vec::new();
    if let Ok(entries) = std::fs::read_dir(tasks_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".json") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if let Ok(task) = serde_json::from_str::<TeamTask>(&content) {
                            tasks.push(task);
                        }
                    }
                }
            }
        }
    }
    // Sort by ID numerically
    tasks.sort_by(|a, b| {
        a.id.parse::<u32>().unwrap_or(0).cmp(&b.id.parse::<u32>().unwrap_or(0))
    });
    tasks
}

/// Get team name from ToolContext working_dir or environment
fn get_team_name(_ctx_working_dir: Option<&str>) -> Option<String> {
    std::env::var("G3_TEAM_NAME").ok()
}

// === Tool implementations ===

pub async fn execute_team_task_create<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing team_task_create tool call");
    
    let team_name = match get_team_name(ctx.working_dir) {
        Some(name) => name,
        None => return Ok("Error: No team context. Set G3_TEAM_NAME or use --team flag.".to_string()),
    };
    
    let subject = match tool_call.args.get("subject").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return Ok("Missing required 'subject' parameter.".to_string()),
    };
    
    let description = tool_call.args.get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    
    let active_form = tool_call.args.get("activeForm")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    
    let tasks_dir = get_team_tasks_dir(&team_name);
    std::fs::create_dir_all(&tasks_dir)?;
    
    let id = next_task_id(&tasks_dir);
    let now = chrono::Utc::now().to_rfc3339();
    
    let task = TeamTask {
        id: id.clone(),
        subject,
        description,
        status: "pending".to_string(),
        owner: None,
        blocks: Vec::new(),
        blocked_by: Vec::new(),
        active_form,
        metadata: HashMap::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    
    write_task(&tasks_dir, &task)?;
    
    Ok(format!("Created task #{}: {}", task.id, task.subject))
}

pub async fn execute_team_task_list<W: UiWriter>(
    _tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing team_task_list tool call");
    
    let team_name = match get_team_name(ctx.working_dir) {
        Some(name) => name,
        None => return Ok("Error: No team context.".to_string()),
    };
    
    let tasks_dir = get_team_tasks_dir(&team_name);
    let tasks = read_all_tasks(&tasks_dir);
    
    if tasks.is_empty() {
        return Ok("No tasks found.".to_string());
    }
    
    let mut output = String::new();
    for task in &tasks {
        let owner_str = task.owner.as_deref().unwrap_or("-");
        let blocked_str = if task.blocked_by.is_empty() {
            String::new()
        } else {
            format!(" [blocked by: {}]", task.blocked_by.join(", "))
        };
        output.push_str(&format!(
            "#{} [{}] {} (owner: {}){}\n",
            task.id, task.status, task.subject, owner_str, blocked_str
        ));
    }
    
    Ok(output.trim_end().to_string())
}

pub async fn execute_team_task_get<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing team_task_get tool call");
    
    let team_name = match get_team_name(ctx.working_dir) {
        Some(name) => name,
        None => return Ok("Error: No team context.".to_string()),
    };
    
    let task_id = match tool_call.args.get("taskId").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return Ok("Missing required 'taskId' parameter.".to_string()),
    };
    
    let tasks_dir = get_team_tasks_dir(&team_name);
    match read_task(&tasks_dir, task_id) {
        Ok(task) => {
            Ok(serde_json::to_string_pretty(&task)?)
        }
        Err(_) => Ok(format!("Task #{} not found.", task_id)),
    }
}

pub async fn execute_team_task_update<W: UiWriter>(
    tool_call: &ToolCall,
    ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    debug!("Processing team_task_update tool call");
    
    let team_name = match get_team_name(ctx.working_dir) {
        Some(name) => name,
        None => return Ok("Error: No team context.".to_string()),
    };
    
    let task_id = match tool_call.args.get("taskId").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return Ok("Missing required 'taskId' parameter.".to_string()),
    };
    
    let tasks_dir = get_team_tasks_dir(&team_name);
    let mut task = match read_task(&tasks_dir, task_id) {
        Ok(t) => t,
        Err(_) => return Ok(format!("Task #{} not found.", task_id)),
    };
    
    let mut changes = Vec::new();
    
    if let Some(status) = tool_call.args.get("status").and_then(|v| v.as_str()) {
        if status == "deleted" {
            let path = tasks_dir.join(format!("{}.json", task_id));
            std::fs::remove_file(&path)?;
            return Ok(format!("Task #{} deleted.", task_id));
        }
        task.status = status.to_string();
        changes.push(format!("status={}", status));
    }
    
    if let Some(subject) = tool_call.args.get("subject").and_then(|v| v.as_str()) {
        task.subject = subject.to_string();
        changes.push("subject updated".to_string());
    }
    
    if let Some(desc) = tool_call.args.get("description").and_then(|v| v.as_str()) {
        task.description = desc.to_string();
        changes.push("description updated".to_string());
    }
    
    if let Some(owner) = tool_call.args.get("owner").and_then(|v| v.as_str()) {
        task.owner = Some(owner.to_string());
        changes.push(format!("owner={}", owner));
    }
    
    if let Some(active_form) = tool_call.args.get("activeForm").and_then(|v| v.as_str()) {
        task.active_form = Some(active_form.to_string());
        changes.push("activeForm updated".to_string());
    }
    
    // Handle addBlocks
    if let Some(blocks) = tool_call.args.get("addBlocks").and_then(|v| v.as_array()) {
        for b in blocks {
            if let Some(id) = b.as_str() {
                if !task.blocks.contains(&id.to_string()) {
                    task.blocks.push(id.to_string());
                    // Also add blockedBy to the target task
                    if let Ok(mut target) = read_task(&tasks_dir, id) {
                        if !target.blocked_by.contains(&task.id) {
                            target.blocked_by.push(task.id.clone());
                            let _ = write_task(&tasks_dir, &target);
                        }
                    }
                }
            }
        }
        changes.push("blocks updated".to_string());
    }
    
    // Handle addBlockedBy
    if let Some(blocked_by) = tool_call.args.get("addBlockedBy").and_then(|v| v.as_array()) {
        for b in blocked_by {
            if let Some(id) = b.as_str() {
                if !task.blocked_by.contains(&id.to_string()) {
                    task.blocked_by.push(id.to_string());
                    // Also add blocks to the source task
                    if let Ok(mut source) = read_task(&tasks_dir, id) {
                        if !source.blocks.contains(&task.id) {
                            source.blocks.push(task.id.clone());
                            let _ = write_task(&tasks_dir, &source);
                        }
                    }
                }
            }
        }
        changes.push("blockedBy updated".to_string());
    }
    
    // Handle metadata merge
    if let Some(metadata) = tool_call.args.get("metadata").and_then(|v| v.as_object()) {
        for (key, value) in metadata {
            if value.is_null() {
                task.metadata.remove(key);
            } else {
                task.metadata.insert(key.clone(), value.clone());
            }
        }
        changes.push("metadata updated".to_string());
    }
    
    task.updated_at = chrono::Utc::now().to_rfc3339();
    write_task(&tasks_dir, &task)?;
    
    Ok(format!("Task #{} updated: {}", task_id, changes.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn test_next_task_id_empty_dir() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(next_task_id(tmp.path()), "1");
    }

    #[test]
    fn test_next_task_id_with_existing() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("1.json"), "{}").unwrap();
        std::fs::write(tmp.path().join("3.json"), "{}").unwrap();
        assert_eq!(next_task_id(tmp.path()), "4");
    }

    #[test]
    fn test_write_and_read_task() {
        let tmp = TempDir::new().unwrap();
        let task = TeamTask {
            id: "1".to_string(),
            subject: "Test task".to_string(),
            description: "A test".to_string(),
            status: "pending".to_string(),
            owner: None,
            blocks: Vec::new(),
            blocked_by: Vec::new(),
            active_form: None,
            metadata: HashMap::new(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        write_task(tmp.path(), &task).unwrap();
        let read_back = read_task(tmp.path(), "1").unwrap();
        assert_eq!(read_back.subject, "Test task");
        assert_eq!(read_back.status, "pending");
    }

    #[test]
    fn test_read_all_tasks_sorted() {
        let tmp = TempDir::new().unwrap();
        for i in [3, 1, 2] {
            let task = TeamTask {
                id: i.to_string(),
                subject: format!("Task {}", i),
                description: String::new(),
                status: "pending".to_string(),
                owner: None,
                blocks: Vec::new(),
                blocked_by: Vec::new(),
                active_form: None,
                metadata: HashMap::new(),
                created_at: String::new(),
                updated_at: String::new(),
            };
            write_task(tmp.path(), &task).unwrap();
        }
        let tasks = read_all_tasks(tmp.path());
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].id, "1");
        assert_eq!(tasks[1].id, "2");
        assert_eq!(tasks[2].id, "3");
    }

    #[test]
    fn test_task_serialization() {
        let task = TeamTask {
            id: "1".to_string(),
            subject: "Test".to_string(),
            description: "Desc".to_string(),
            status: "in_progress".to_string(),
            owner: Some("agent-1".to_string()),
            blocks: vec!["2".to_string()],
            blocked_by: vec![],
            active_form: Some("Working on test".to_string()),
            metadata: HashMap::from([("key".to_string(), json!("value"))]),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&task).unwrap();
        let deserialized: TeamTask = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.owner, Some("agent-1".to_string()));
        assert_eq!(deserialized.blocks, vec!["2".to_string()]);
    }
}
