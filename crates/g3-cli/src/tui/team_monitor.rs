use std::fs;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
pub enum MemberStatus {
    Active,
    Idle,
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct TeamMemberEntry {
    pub name: String,
    pub agent_type: String,
    pub status: MemberStatus,
}

#[derive(Debug, Clone)]
pub struct TeamTaskEntry {
    pub id: String,
    pub subject: String,
    pub status: String, // pending, in_progress, completed
    pub owner: Option<String>,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TeamState {
    pub team_name: String,
    pub members: Vec<TeamMemberEntry>,
    pub tasks: Vec<TeamTaskEntry>,
}

pub struct TeamMonitor {
    team_name: String,
    config_dir: PathBuf, // e.g. ~/.config/g3
    config_size: u64,
    tasks_dir_mtime: u64, // track directory modification
}

impl TeamMonitor {
    pub fn new(team_name: String) -> Self {
        let config_dir = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(xdg)
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".config")
        } else {
            PathBuf::from("/tmp/.config")
        };
        let config_dir = config_dir.join("g3");

        Self {
            team_name,
            config_dir,
            config_size: 0,
            tasks_dir_mtime: 0,
        }
    }

    fn team_config_path(&self) -> PathBuf {
        self.config_dir.join("teams").join(&self.team_name).join("config.json")
    }

    fn tasks_dir(&self) -> PathBuf {
        self.config_dir.join("tasks").join(&self.team_name)
    }

    fn file_size(path: &PathBuf) -> u64 {
        fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }

    fn dir_total_size(path: &PathBuf) -> u64 {
        // Sum of all file sizes in directory as a cheap change detection
        let Ok(entries) = fs::read_dir(path) else { return 0 };
        entries.flatten()
            .filter(|e| e.file_name().to_str().map_or(false, |n| n.ends_with(".json")))
            .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
            .sum()
    }

    pub async fn run(mut self, tx: mpsc::UnboundedSender<TeamState>) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

        loop {
            interval.tick().await;

            let config_size = Self::file_size(&self.team_config_path());
            let tasks_size = Self::dir_total_size(&self.tasks_dir());

            if config_size == self.config_size && tasks_size == self.tasks_dir_mtime {
                continue;
            }
            self.config_size = config_size;
            self.tasks_dir_mtime = tasks_size;

            let state = self.read_state();
            if tx.send(state).is_err() {
                break;
            }
        }
    }

    fn read_state(&self) -> TeamState {
        let members = self.read_members();
        let tasks = self.read_tasks();
        TeamState {
            team_name: self.team_name.clone(),
            members,
            tasks,
        }
    }

    fn read_members(&self) -> Vec<TeamMemberEntry> {
        let path = self.team_config_path();
        let Ok(content) = fs::read_to_string(&path) else { return Vec::new() };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else { return Vec::new() };

        let Some(members) = value.get("members").and_then(|v| v.as_array()) else { return Vec::new() };

        members.iter().filter_map(|m| {
            let name = m.get("name")?.as_str()?.to_string();
            let agent_type = m.get("agent_type").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let status_str = m.get("status").and_then(|v| v.as_str()).unwrap_or("active");
            let status = match status_str {
                "shutdown" => MemberStatus::Shutdown,
                "idle" => MemberStatus::Idle,
                _ => MemberStatus::Active,
            };
            Some(TeamMemberEntry { name, agent_type, status })
        }).collect()
    }

    fn read_tasks(&self) -> Vec<TeamTaskEntry> {
        let tasks_dir = self.tasks_dir();
        let Ok(entries) = fs::read_dir(&tasks_dir) else { return Vec::new() };

        let mut tasks: Vec<TeamTaskEntry> = entries.flatten()
            .filter(|e| e.file_name().to_str().map_or(false, |n| n.ends_with(".json")))
            .filter_map(|e| {
                let content = fs::read_to_string(e.path()).ok()?;
                let v: serde_json::Value = serde_json::from_str(&content).ok()?;
                let id = v.get("id")?.as_str()?.to_string();
                let subject = v.get("subject").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let status = v.get("status").and_then(|v| v.as_str()).unwrap_or("pending").to_string();
                let owner = v.get("owner").and_then(|v| v.as_str()).map(String::from);
                let blocked_by = v.get("blocked_by")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                Some(TeamTaskEntry { id, subject, status, owner, blocked_by })
            })
            .collect();

        // Sort by ID numerically
        tasks.sort_by(|a, b| {
            a.id.parse::<u32>().unwrap_or(0).cmp(&b.id.parse::<u32>().unwrap_or(0))
        });
        tasks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_monitor_new() {
        let monitor = TeamMonitor::new("test-team".to_string());
        assert_eq!(monitor.team_name, "test-team");
        assert_eq!(monitor.config_size, 0);
    }

    #[test]
    fn test_member_status_variants() {
        assert_eq!(MemberStatus::Active, MemberStatus::Active);
        assert_eq!(MemberStatus::Idle, MemberStatus::Idle);
        assert_eq!(MemberStatus::Shutdown, MemberStatus::Shutdown);
    }

    #[test]
    fn test_team_state_clone() {
        let state = TeamState {
            team_name: "test".to_string(),
            members: vec![],
            tasks: vec![],
        };
        let cloned = state.clone();
        assert_eq!(cloned.team_name, "test");
    }

    #[test]
    fn test_read_members_no_file() {
        let monitor = TeamMonitor::new("nonexistent-team".to_string());
        assert!(monitor.read_members().is_empty());
    }

    #[test]
    fn test_read_tasks_no_dir() {
        let monitor = TeamMonitor::new("nonexistent-team".to_string());
        assert!(monitor.read_tasks().is_empty());
    }
}
