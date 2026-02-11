use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::debug;
use crate::ui_writer::UiWriter;
use crate::ToolCall;
use super::executor::ToolContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    pub team_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub members: Vec<TeamMember>,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub name: String,
    #[serde(default)]
    pub agent_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub pid: Option<u32>,
}

fn get_config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        PathBuf::from("/tmp/.config")
    }
}

fn get_team_dir(team_name: &str) -> PathBuf {
    get_config_dir().join("g3").join("teams").join(team_name)
}

fn get_team_tasks_dir(team_name: &str) -> PathBuf {
    get_config_dir().join("g3").join("tasks").join(team_name)
}

pub fn read_team_config(team_name: &str) -> Result<TeamConfig> {
    let path = get_team_dir(team_name).join("config.json");
    let content = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn write_team_config(config: &TeamConfig) -> Result<()> {
    let dir = get_team_dir(&config.team_name);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.json");
    std::fs::write(&path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}

fn get_team_name_from_env() -> Option<String> {
    std::env::var("G3_TEAM_NAME").ok()
}

pub async fn execute_team_create<W: UiWriter>(
    tool_call: &ToolCall,
    _ctx: &ToolContext<'_, W>,
) -> Result<String> {
    let team_name = tool_call
        .args
        .get("team_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: team_name"))?;

    let description = tool_call
        .args
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    debug!("Creating team: {}", team_name);

    let team_dir = get_team_dir(team_name);
    let tasks_dir = get_team_tasks_dir(team_name);

    std::fs::create_dir_all(&team_dir)?;
    std::fs::create_dir_all(&tasks_dir)?;

    let config = TeamConfig {
        team_name: team_name.to_string(),
        description: description.to_string(),
        members: Vec::new(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    write_team_config(&config)?;

    Ok(format!(
        "Team '{}' created successfully.\nTeam directory: {}\nTasks directory: {}",
        team_name,
        team_dir.display(),
        tasks_dir.display()
    ))
}

pub async fn execute_team_delete<W: UiWriter>(
    _tool_call: &ToolCall,
    _ctx: &ToolContext<'_, W>,
) -> Result<String> {
    let team_name = get_team_name_from_env()
        .ok_or_else(|| anyhow::anyhow!("G3_TEAM_NAME environment variable not set"))?;

    debug!("Deleting team: {}", team_name);

    let team_dir = get_team_dir(&team_name);
    let tasks_dir = get_team_tasks_dir(&team_name);

    if team_dir.exists() {
        std::fs::remove_dir_all(&team_dir)?;
    }

    if tasks_dir.exists() {
        std::fs::remove_dir_all(&tasks_dir)?;
    }

    Ok(format!("Team '{}' deleted successfully.", team_name))
}

pub async fn execute_team_spawn_teammate<W: UiWriter>(
    tool_call: &ToolCall,
    _ctx: &ToolContext<'_, W>,
) -> Result<String> {
    let team_name = get_team_name_from_env()
        .ok_or_else(|| anyhow::anyhow!("G3_TEAM_NAME environment variable not set"))?;

    let name = tool_call
        .args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: name"))?;

    let agent_type = tool_call
        .args
        .get("agent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    let task = tool_call
        .args
        .get("task")
        .and_then(|v| v.as_str());

    debug!("Spawning teammate: {} (type: {}) for team: {}", name, agent_type, team_name);

    let mut cmd = tokio::process::Command::new("g3");
    cmd.arg("--team")
        .arg(&team_name)
        .arg("--team-role")
        .arg(name)
        .env("G3_TEAM_NAME", &team_name)
        .env("G3_TEAM_ROLE", name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    if let Some(task_str) = task {
        cmd.arg(task_str);
    }

    let child = cmd.spawn()?;
    let pid = child.id().unwrap_or(0);

    let mut config = read_team_config(&team_name)?;
    
    config.members.push(TeamMember {
        name: name.to_string(),
        agent_type: agent_type.to_string(),
        status: "active".to_string(),
        pid: Some(pid),
    });

    write_team_config(&config)?;

    Ok(format!(
        "Teammate '{}' spawned successfully.\nAgent type: {}\nPID: {}\nStatus: active",
        name, agent_type, pid
    ))
}

pub async fn execute_team_shutdown_teammate<W: UiWriter>(
    tool_call: &ToolCall,
    _ctx: &ToolContext<'_, W>,
) -> Result<String> {
    let team_name = get_team_name_from_env()
        .ok_or_else(|| anyhow::anyhow!("G3_TEAM_NAME environment variable not set"))?;

    let name = tool_call
        .args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: name"))?;

    debug!("Requesting shutdown for teammate: {} in team: {}", name, team_name);

    let mailbox_dir = get_team_dir(&team_name).join("mailbox").join(name);
    std::fs::create_dir_all(&mailbox_dir)?;

    let shutdown_msg = serde_json::json!({
        "type": "shutdown_request",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "from": "team-lead"
    });

    let msg_path = mailbox_dir.join(format!("shutdown_{}.json", chrono::Utc::now().timestamp()));
    std::fs::write(&msg_path, serde_json::to_string_pretty(&shutdown_msg)?)?;

    let mut config = read_team_config(&team_name)?;
    
    if let Some(member) = config.members.iter_mut().find(|m| m.name == name) {
        member.status = "shutdown".to_string();
        write_team_config(&config)?;
    }

    Ok(format!(
        "Shutdown request sent to teammate '{}'.\nMailbox: {}",
        name,
        msg_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_config_serialization() {
        let config = TeamConfig {
            team_name: "test-team".to_string(),
            description: "A test team".to_string(),
            members: vec![
                TeamMember {
                    name: "agent1".to_string(),
                    agent_type: "builder".to_string(),
                    status: "active".to_string(),
                    pid: Some(1234),
                }
            ],
            created_at: "2026-02-11T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TeamConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.team_name, deserialized.team_name);
        assert_eq!(config.description, deserialized.description);
        assert_eq!(config.members.len(), deserialized.members.len());
        assert_eq!(config.members[0].name, deserialized.members[0].name);
        assert_eq!(config.members[0].pid, deserialized.members[0].pid);
    }

    #[test]
    fn test_team_config_round_trip() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let team_dir = tmp.path().join("g3").join("teams").join("round-trip-team");
        std::fs::create_dir_all(&team_dir).unwrap();

        let config = TeamConfig {
            team_name: "round-trip-team".to_string(),
            description: "Testing round trip".to_string(),
            members: vec![],
            created_at: "2026-02-11T00:00:00Z".to_string(),
        };

        // Write directly to temp dir
        let path = team_dir.join("config.json");
        std::fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

        // Read back
        let content = std::fs::read_to_string(&path).unwrap();
        let loaded: TeamConfig = serde_json::from_str(&content).unwrap();

        assert_eq!(config.team_name, loaded.team_name);
        assert_eq!(config.description, loaded.description);
        assert_eq!(config.members.len(), loaded.members.len());
    }

    #[test]
    fn test_team_directory_creation() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let team_dir = tmp.path().join("teams").join("test-team");
        let tasks_dir = tmp.path().join("tasks").join("test-team");

        std::fs::create_dir_all(&team_dir).unwrap();
        std::fs::create_dir_all(&tasks_dir).unwrap();

        assert!(team_dir.exists());
        assert!(tasks_dir.exists());
    }
}
