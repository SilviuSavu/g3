use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Running,
    Idle,
    Complete,
    Failed,
}

#[derive(Debug, Clone)]
pub struct SubagentEntry {
    pub agent_id: String,
    pub agent_type: String,
    pub status: AgentStatus,
    pub context_pct: f32,
    pub model: String,
    pub last_tool: Option<String>,
    pub last_activity: SystemTime,
}

pub struct SubagentMonitor {
    log_dir: PathBuf,
    agents: HashMap<String, SubagentEntry>,
    last_status_line_count: usize,
    last_subagent_stop_count: usize,
    last_pre_tool_count: usize,
}

impl SubagentMonitor {
    pub fn new(log_dir: PathBuf) -> Self {
        Self {
            log_dir,
            agents: HashMap::new(),
            last_status_line_count: 0,
            last_subagent_stop_count: 0,
            last_pre_tool_count: 0,
        }
    }

    pub async fn run(mut self, tx: mpsc::UnboundedSender<Vec<SubagentEntry>>) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

        loop {
            interval.tick().await;

            let mut changed = false;
            changed |= self.poll_status_line();
            changed |= self.poll_subagent_stop();
            changed |= self.poll_pre_tool_use();

            if changed {
                if tx.send(self.get_entries()).is_err() {
                    // Receiver dropped, exit
                    break;
                }
            }
        }
    }

    fn poll_status_line(&mut self) -> bool {
        let path = self.log_dir.join("status_line.json");
        let Ok(content) = fs::read_to_string(&path) else {
            return false;
        };

        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            return false;
        };

        let Some(array) = value.as_array() else {
            return false;
        };

        let mut changed = false;
        for entry in array.iter().skip(self.last_status_line_count) {
            let Some(input_data) = entry.get("input_data") else {
                continue;
            };

            let Some(session_id) = input_data.get("session_id").and_then(|v| v.as_str()) else {
                continue;
            };

            let agent_id = session_id.chars().take(7).collect::<String>();

            let model = input_data
                .get("model")
                .and_then(|m| m.get("display_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();

            let context_pct = input_data
                .get("context_window")
                .and_then(|cw| cw.get("used_percentage"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;

            let agent = self.agents.entry(agent_id.clone()).or_insert_with(|| {
                changed = true;
                SubagentEntry {
                    agent_id: agent_id.clone(),
                    agent_type: String::new(),
                    status: AgentStatus::Running,
                    context_pct: 0.0,
                    model: String::new(),
                    last_tool: None,
                    last_activity: SystemTime::now(),
                }
            });

            if agent.model != model || agent.context_pct != context_pct {
                changed = true;
            }

            agent.model = model;
            agent.context_pct = context_pct;
            agent.status = AgentStatus::Running;
            agent.last_activity = SystemTime::now();
        }

        self.last_status_line_count = array.len();
        changed
    }

    fn poll_subagent_stop(&mut self) -> bool {
        let path = self.log_dir.join("subagent_stop.json");
        let Ok(content) = fs::read_to_string(&path) else {
            return false;
        };

        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            return false;
        };

        let Some(array) = value.as_array() else {
            return false;
        };

        let mut changed = false;
        for entry in array.iter().skip(self.last_subagent_stop_count) {
            let Some(agent_id) = entry.get("agent_id").and_then(|v| v.as_str()) else {
                continue;
            };

            let agent_type = entry
                .get("agent_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if let Some(agent) = self.agents.get_mut(agent_id) {
                if agent.status != AgentStatus::Complete {
                    changed = true;
                }
                agent.status = AgentStatus::Complete;
                if !agent_type.is_empty() {
                    agent.agent_type = agent_type.to_string();
                }
                agent.last_activity = SystemTime::now();
            }
        }

        self.last_subagent_stop_count = array.len();
        changed
    }

    fn poll_pre_tool_use(&mut self) -> bool {
        let path = self.log_dir.join("pre_tool_use.json");
        let Ok(content) = fs::read_to_string(&path) else {
            return false;
        };

        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            return false;
        };

        let Some(array) = value.as_array() else {
            return false;
        };

        let mut changed = false;
        for entry in array.iter().skip(self.last_pre_tool_count) {
            let Some(session_id) = entry.get("session_id").and_then(|v| v.as_str()) else {
                continue;
            };

            let agent_id = session_id.chars().take(7).collect::<String>();

            let tool_name = entry
                .get("tool_name")
                .and_then(|v| v.as_str())
                .map(String::from);

            if let Some(agent) = self.agents.get_mut(&agent_id) {
                if agent.last_tool != tool_name {
                    changed = true;
                }
                agent.last_tool = tool_name;
                agent.last_activity = SystemTime::now();
            }
        }

        self.last_pre_tool_count = array.len();
        changed
    }

    fn get_entries(&self) -> Vec<SubagentEntry> {
        let mut entries: Vec<_> = self.agents.values().cloned().collect();
        entries.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_status_default() {
        let entry = SubagentEntry {
            agent_id: "abc1234".to_string(),
            agent_type: "Explore".to_string(),
            status: AgentStatus::Running,
            context_pct: 0.0,
            model: "Opus 4.6".to_string(),
            last_tool: None,
            last_activity: SystemTime::now(),
        };
        assert_eq!(entry.status, AgentStatus::Running);
    }

    #[test]
    fn test_monitor_new() {
        let monitor = SubagentMonitor::new(PathBuf::from("/tmp/nonexistent"));
        assert!(monitor.agents.is_empty());
    }
}
