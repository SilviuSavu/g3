//! Team messaging tool: inter-agent communication via file-based mailbox.
//!
//! Provides a file-based mailbox system for agents in a team to communicate.
//! Each agent has an inbox directory where messages are delivered as JSON files.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::debug;

use crate::ui_writer::UiWriter;
use crate::ToolCall;

use super::executor::ToolContext;

/// Message structure for team communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub msg_type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approve: Option<bool>,
}

/// Get the config directory (~/.config or $XDG_CONFIG_HOME).
fn get_config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        PathBuf::from("/tmp/.config")
    }
}

/// Get the mailbox directory for a specific agent.
fn get_mailbox_dir(team_name: &str, agent_name: &str) -> PathBuf {
    get_config_dir()
        .join("g3")
        .join("teams")
        .join(team_name)
        .join("mailbox")
        .join(agent_name)
}

/// Get the current team name from environment.
fn get_team_name() -> Option<String> {
    std::env::var("G3_TEAM_NAME").ok()
}

/// Get the current agent name from environment.
fn get_agent_name() -> Option<String> {
    std::env::var("G3_TEAM_ROLE").ok()
}

/// Team configuration structure.
#[derive(Debug, Deserialize)]
struct TeamConfig {
    #[allow(dead_code)]
    team_name: String,
    members: Vec<TeamMember>,
}

#[derive(Debug, Deserialize)]
struct TeamMember {
    name: String,
}

/// Read team members from config file.
fn read_team_members(team_name: &str) -> Result<Vec<String>> {
    let config_path = get_config_dir()
        .join("g3")
        .join("teams")
        .join(team_name)
        .join("config.json");

    let content = std::fs::read_to_string(&config_path)?;
    let config: TeamConfig = serde_json::from_str(&content)?;

    Ok(config.members.into_iter().map(|m| m.name).collect())
}

/// Write a message to an agent's inbox.
fn write_message(team_name: &str, recipient: &str, message: &TeamMessage) -> Result<()> {
    let inbox_dir = get_mailbox_dir(team_name, recipient);
    std::fs::create_dir_all(&inbox_dir)?;

    let timestamp = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let filename = format!("{}-{}.json", timestamp, message.id);
    let filepath = inbox_dir.join(filename);

    let json = serde_json::to_string_pretty(message)?;
    std::fs::write(&filepath, json)?;

    debug!("Wrote message to {}", filepath.display());
    Ok(())
}

/// Execute the team_send_message tool.
/// Handles message, broadcast, shutdown_request, and shutdown_response types.
pub async fn execute_team_send_message<W: UiWriter>(
    tool_call: &ToolCall,
    _ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let team_name = get_team_name()
        .ok_or_else(|| anyhow::anyhow!("G3_TEAM_NAME not set - not running in team mode"))?;

    let agent_name = get_agent_name()
        .ok_or_else(|| anyhow::anyhow!("G3_TEAM_ROLE not set - not running in team mode"))?;

    let msg_type = tool_call
        .args
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required 'type' parameter"))?;

    let content = tool_call
        .args
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let summary = tool_call
        .args
        .get("summary")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match msg_type {
        "message" => {
            let recipient = tool_call
                .args
                .get("recipient")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing required 'recipient' parameter"))?;

            if content.is_empty() {
                return Err(anyhow::anyhow!("Missing required 'content' parameter"));
            }

            if summary.is_none() {
                return Err(anyhow::anyhow!("Missing required 'summary' parameter"));
            }

            let message = TeamMessage {
                id: uuid::Uuid::new_v4().to_string(),
                from: agent_name.clone(),
                to: recipient.to_string(),
                msg_type: "message".to_string(),
                content,
                summary,
                timestamp: Utc::now().to_rfc3339(),
                request_id: None,
                approve: None,
            };

            write_message(&team_name, recipient, &message)?;

            Ok(format!("Message sent to {}", recipient))
        }

        "broadcast" => {
            if content.is_empty() {
                return Err(anyhow::anyhow!("Missing required 'content' parameter"));
            }

            if summary.is_none() {
                return Err(anyhow::anyhow!("Missing required 'summary' parameter"));
            }

            let members = read_team_members(&team_name)?;
            let recipients: Vec<_> = members
                .into_iter()
                .filter(|name| name != &agent_name)
                .collect();

            if recipients.is_empty() {
                return Ok("No other team members to broadcast to".to_string());
            }

            for recipient in &recipients {
                let message = TeamMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    from: agent_name.clone(),
                    to: recipient.clone(),
                    msg_type: "broadcast".to_string(),
                    content: content.clone(),
                    summary: summary.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    request_id: None,
                    approve: None,
                };

                write_message(&team_name, recipient, &message)?;
            }

            Ok(format!("Broadcast sent to {} team members", recipients.len()))
        }

        "shutdown_request" => {
            let recipient = tool_call
                .args
                .get("recipient")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing required 'recipient' parameter"))?;

            let request_id = uuid::Uuid::new_v4().to_string();

            let message = TeamMessage {
                id: uuid::Uuid::new_v4().to_string(),
                from: agent_name.clone(),
                to: recipient.to_string(),
                msg_type: "shutdown_request".to_string(),
                content,
                summary: None,
                timestamp: Utc::now().to_rfc3339(),
                request_id: Some(request_id.clone()),
                approve: None,
            };

            write_message(&team_name, recipient, &message)?;

            Ok(format!(
                "Shutdown request sent to {} (request_id: {})",
                recipient, request_id
            ))
        }

        "shutdown_response" => {
            let request_id = tool_call
                .args
                .get("request_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing required 'request_id' parameter"))?;

            let approve = tool_call
                .args
                .get("approve")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| anyhow::anyhow!("Missing required 'approve' parameter"))?;

            // For shutdown response, we need to find the original request to know who to respond to
            // For now, assume recipient is provided or default to team-lead
            let recipient = tool_call
                .args
                .get("recipient")
                .and_then(|v| v.as_str())
                .unwrap_or("team-lead");

            let message = TeamMessage {
                id: uuid::Uuid::new_v4().to_string(),
                from: agent_name.clone(),
                to: recipient.to_string(),
                msg_type: "shutdown_response".to_string(),
                content,
                summary: None,
                timestamp: Utc::now().to_rfc3339(),
                request_id: Some(request_id.to_string()),
                approve: Some(approve),
            };

            write_message(&team_name, recipient, &message)?;

            let status = if approve { "approved" } else { "rejected" };
            Ok(format!("Shutdown response ({}) sent to {}", status, recipient))
        }

        _ => Err(anyhow::anyhow!("Unknown message type: {}", msg_type)),
    }
}

/// Execute the team_read_messages tool.
/// Reads all messages from the current agent's inbox, deletes them after reading.
pub async fn execute_team_read_messages<W: UiWriter>(
    _tool_call: &ToolCall,
    _ctx: &mut ToolContext<'_, W>,
) -> Result<String> {
    let team_name = get_team_name()
        .ok_or_else(|| anyhow::anyhow!("G3_TEAM_NAME not set - not running in team mode"))?;

    let agent_name = get_agent_name()
        .ok_or_else(|| anyhow::anyhow!("G3_TEAM_ROLE not set - not running in team mode"))?;

    let inbox_dir = get_mailbox_dir(&team_name, &agent_name);

    if !inbox_dir.exists() {
        return Ok("No messages (inbox does not exist)".to_string());
    }

    let mut messages: Vec<(PathBuf, TeamMessage)> = Vec::new();

    for entry in std::fs::read_dir(&inbox_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let content = std::fs::read_to_string(&path)?;
            match serde_json::from_str::<TeamMessage>(&content) {
                Ok(msg) => messages.push((path, msg)),
                Err(e) => {
                    debug!("Failed to parse message {}: {}", path.display(), e);
                }
            }
        }
    }

    if messages.is_empty() {
        return Ok("No messages in inbox".to_string());
    }

    // Sort by timestamp
    messages.sort_by(|a, b| a.1.timestamp.cmp(&b.1.timestamp));

    let mut output = format!("Received {} message(s):\n\n", messages.len());

    for (path, msg) in &messages {
        output.push_str(&format!("---\n"));
        output.push_str(&format!("From: {}\n", msg.from));
        output.push_str(&format!("Type: {}\n", msg.msg_type));
        output.push_str(&format!("Time: {}\n", msg.timestamp));

        if let Some(summary) = &msg.summary {
            output.push_str(&format!("Summary: {}\n", summary));
        }

        if let Some(request_id) = &msg.request_id {
            output.push_str(&format!("Request ID: {}\n", request_id));
        }

        if let Some(approve) = msg.approve {
            output.push_str(&format!("Approve: {}\n", approve));
        }

        if !msg.content.is_empty() {
            output.push_str(&format!("Content:\n{}\n", msg.content));
        }

        // Delete the message after reading
        if let Err(e) = std::fs::remove_file(path) {
            debug!("Failed to delete message {}: {}", path.display(), e);
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = TeamMessage {
            id: "test-id".to_string(),
            from: "agent1".to_string(),
            to: "agent2".to_string(),
            msg_type: "message".to_string(),
            content: "Hello".to_string(),
            summary: Some("Greeting".to_string()),
            timestamp: "2026-02-11T12:00:00Z".to_string(),
            request_id: None,
            approve: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: TeamMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "test-id");
        assert_eq!(parsed.from, "agent1");
        assert_eq!(parsed.to, "agent2");
        assert_eq!(parsed.msg_type, "message");
        assert_eq!(parsed.content, "Hello");
        assert_eq!(parsed.summary, Some("Greeting".to_string()));
    }

    #[test]
    fn test_shutdown_request_serialization() {
        let msg = TeamMessage {
            id: "test-id".to_string(),
            from: "team-lead".to_string(),
            to: "worker".to_string(),
            msg_type: "shutdown_request".to_string(),
            content: "Task complete".to_string(),
            summary: None,
            timestamp: "2026-02-11T12:00:00Z".to_string(),
            request_id: Some("req-123".to_string()),
            approve: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: TeamMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.msg_type, "shutdown_request");
        assert_eq!(parsed.request_id, Some("req-123".to_string()));
        assert_eq!(parsed.approve, None);
    }

    #[test]
    fn test_shutdown_response_serialization() {
        let msg = TeamMessage {
            id: "test-id".to_string(),
            from: "worker".to_string(),
            to: "team-lead".to_string(),
            msg_type: "shutdown_response".to_string(),
            content: "All done".to_string(),
            summary: None,
            timestamp: "2026-02-11T12:00:00Z".to_string(),
            request_id: Some("req-123".to_string()),
            approve: Some(true),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: TeamMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.msg_type, "shutdown_response");
        assert_eq!(parsed.request_id, Some("req-123".to_string()));
        assert_eq!(parsed.approve, Some(true));
    }

    #[test]
    fn test_write_and_read_message() {
        use std::env;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let config_dir = temp.path().to_path_buf();

        // Override config dir for this test
        unsafe { env::set_var("XDG_CONFIG_HOME", config_dir.to_str().unwrap()); }

        let team_name = &format!("test-team-{}", uuid::Uuid::new_v4());
        let recipient = "agent1";

        let msg = TeamMessage {
            id: uuid::Uuid::new_v4().to_string(),
            from: "sender".to_string(),
            to: recipient.to_string(),
            msg_type: "message".to_string(),
            content: "Test content".to_string(),
            summary: Some("Test summary".to_string()),
            timestamp: Utc::now().to_rfc3339(),
            request_id: None,
            approve: None,
        };

        // Write message
        write_message(&team_name, recipient, &msg).unwrap();

        // Verify it exists
        let inbox_dir = get_mailbox_dir(&team_name, recipient);
        assert!(inbox_dir.exists());

        let entries: Vec<_> = std::fs::read_dir(&inbox_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        assert_eq!(entries.len(), 1);

        // Read and verify
        let path = entries[0].path();
        let content = std::fs::read_to_string(&path).unwrap();
        let read_msg: TeamMessage = serde_json::from_str(&content).unwrap();

        assert_eq!(read_msg.id, msg.id);
        assert_eq!(read_msg.from, "sender");
        assert_eq!(read_msg.content, "Test content");

        // Clean up
        unsafe { env::remove_var("XDG_CONFIG_HOME"); }
    }

    #[test]
    fn test_read_messages_clears_inbox() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let inbox_dir = temp.path().join("inbox");
        std::fs::create_dir_all(&inbox_dir).unwrap();

        // Write two message files directly
        for i in 0..2 {
            let msg = TeamMessage {
                id: uuid::Uuid::new_v4().to_string(),
                from: "sender".to_string(),
                to: "agent1".to_string(),
                msg_type: "message".to_string(),
                content: format!("Message {}", i),
                summary: Some(format!("Summary {}", i)),
                timestamp: Utc::now().to_rfc3339(),
                request_id: None,
                approve: None,
            };
            let filename = format!("{}-{}.json", Utc::now().timestamp_millis(), uuid::Uuid::new_v4());
            let path = inbox_dir.join(&filename);
            std::fs::write(&path, serde_json::to_string(&msg).unwrap()).unwrap();
        }

        let count_before = std::fs::read_dir(&inbox_dir).unwrap().count();
        assert_eq!(count_before, 2);

        // Simulate consume: read then delete
        let messages: Vec<PathBuf> = std::fs::read_dir(&inbox_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();

        for path in messages {
            std::fs::remove_file(path).unwrap();
        }

        let count_after = std::fs::read_dir(&inbox_dir).unwrap().count();
        assert_eq!(count_after, 0);
    }
}
