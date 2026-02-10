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
    // File size cache to avoid re-reading unchanged files
    status_line_size: u64,
    subagent_stop_size: u64,
    pre_tool_size: u64,
}

impl SubagentMonitor {
    pub fn new(log_dir: PathBuf) -> Self {
        // Skip ALL existing log entries — only track activity that starts after the TUI launches.
        // This prevents showing phantom agents from previous/unrelated sessions.
        let last_status_line_count = Self::count_entries(&log_dir.join("status_line.json"));
        let last_subagent_stop_count = Self::count_entries(&log_dir.join("subagent_stop.json"));
        let last_pre_tool_count = Self::count_entries(&log_dir.join("pre_tool_use.json"));

        let status_line_size = Self::file_size(&log_dir.join("status_line.json"));
        let subagent_stop_size = Self::file_size(&log_dir.join("subagent_stop.json"));
        let pre_tool_size = Self::file_size(&log_dir.join("pre_tool_use.json"));

        Self {
            log_dir,
            agents: HashMap::new(),
            last_status_line_count,
            last_subagent_stop_count,
            last_pre_tool_count,
            status_line_size,
            subagent_stop_size,
            pre_tool_size,
        }
    }

    fn count_entries(path: &PathBuf) -> usize {
        let Ok(content) = fs::read_to_string(path) else {
            return 0;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            return 0;
        };
        value.as_array().map_or(0, |a| a.len())
    }

    fn file_size(path: &PathBuf) -> u64 {
        fs::metadata(path).map(|m| m.len()).unwrap_or(0)
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
                    break;
                }
            }
        }
    }

    fn poll_status_line(&mut self) -> bool {
        let path = self.log_dir.join("status_line.json");

        // Check file size first — skip if unchanged
        let current_size = Self::file_size(&path);
        if current_size == self.status_line_size {
            return false;
        }
        self.status_line_size = current_size;

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

        let current_size = Self::file_size(&path);
        if current_size == self.subagent_stop_size {
            return false;
        }
        self.subagent_stop_size = current_size;

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

        let current_size = Self::file_size(&path);
        if current_size == self.pre_tool_size {
            return false;
        }
        self.pre_tool_size = current_size;

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

    #[test]
    fn test_count_entries_nonexistent() {
        assert_eq!(SubagentMonitor::count_entries(&PathBuf::from("/tmp/nonexistent.json")), 0);
    }

    #[test]
    fn test_file_size_nonexistent() {
        assert_eq!(SubagentMonitor::file_size(&PathBuf::from("/tmp/nonexistent.json")), 0);
    }
}
