//! Structured persona system for g3 agents.
//!
//! Provides typed metadata for agent personas including role, keywords (for flock routing),
//! scope boundaries (read-only enforcement), and tool overrides (exclusions).
//!
//! Supports two front matter formats:
//! - New: TOML between `+++` delimiters
//! - Legacy: HTML comment `<!-- tools: -toolname -->`

use anyhow::Result;
use serde::Deserialize;

/// Parsed agent file: persona metadata + prompt text.
#[derive(Debug, Clone)]
pub struct AgentFile {
    pub id: String,
    pub persona: PersonaData,
    pub prompt: String,
    pub from_disk: bool,
}

/// Structured persona metadata extracted from agent front matter.
#[derive(Debug, Clone, Default)]
pub struct PersonaData {
    pub display_name: String,
    pub role: String,
    pub keywords: Vec<String>,
    pub scope: ScopeBoundaries,
    pub tool_overrides: ToolOverrides,
    /// Stop sequences that halt LLM generation when encountered in output
    pub stop_sequences: Vec<String>,
}

/// Scope boundaries for agent enforcement.
#[derive(Debug, Clone, Default)]
pub struct ScopeBoundaries {
    pub read_only: bool,
    pub forbidden_tools: Vec<String>,
    pub constraints: Vec<String>,
}

/// Tool overrides (exclusions).
#[derive(Debug, Clone, Default)]
pub struct ToolOverrides {
    pub exclude_tools: Vec<String>,
}

// --- TOML deserialization structs ---

#[derive(Deserialize)]
struct TomlFrontMatter {
    display_name: Option<String>,
    role: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    scope: Option<TomlScope>,
    tools: Option<TomlTools>,
    #[serde(default)]
    stop_sequences: Vec<String>,
}

#[derive(Deserialize)]
struct TomlScope {
    #[serde(default)]
    read_only: bool,
    #[serde(default)]
    forbidden_tools: Vec<String>,
    #[serde(default)]
    constraints: Vec<String>,
}

#[derive(Deserialize)]
struct TomlTools {
    #[serde(default)]
    exclude: Vec<String>,
}

/// Parse an agent file's content into an `AgentFile`.
///
/// Supports two front matter formats:
/// 1. TOML between `+++` delimiters (new format)
/// 2. Legacy HTML comment `<!-- tools: -toolname -->` (backward compat)
/// 3. No front matter at all (bare prompt)
pub fn parse_agent_file(id: &str, content: &str, from_disk: bool) -> Result<AgentFile> {
    // Try TOML front matter first
    if let Some(result) = try_parse_toml_front_matter(id, content, from_disk) {
        return result;
    }

    // Try legacy HTML comment format
    if let Some(result) = try_parse_legacy_front_matter(id, content, from_disk) {
        return Ok(result);
    }

    // No front matter - bare prompt
    Ok(AgentFile {
        id: id.to_string(),
        persona: PersonaData {
            display_name: id.to_string(),
            ..Default::default()
        },
        prompt: content.to_string(),
        from_disk,
    })
}

/// Try to parse TOML front matter between `+++` delimiters.
fn try_parse_toml_front_matter(id: &str, content: &str, from_disk: bool) -> Option<Result<AgentFile>> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("+++") {
        return None;
    }

    // Find the closing +++
    let after_open = &trimmed[3..];
    let close_pos = after_open.find("+++")?;
    let toml_str = &after_open[..close_pos];
    let prompt = after_open[close_pos + 3..].trim_start().to_string();

    let parsed: TomlFrontMatter = match toml::from_str(toml_str) {
        Ok(v) => v,
        Err(e) => return Some(Err(anyhow::anyhow!("Failed to parse TOML front matter for agent '{}': {}", id, e))),
    };

    let persona = PersonaData {
        display_name: parsed.display_name.unwrap_or_else(|| id.to_string()),
        role: parsed.role.unwrap_or_default(),
        keywords: parsed.keywords,
        scope: parsed.scope.map(|s| ScopeBoundaries {
            read_only: s.read_only,
            forbidden_tools: s.forbidden_tools,
            constraints: s.constraints,
        }).unwrap_or_default(),
        tool_overrides: parsed.tools.map(|t| ToolOverrides {
            exclude_tools: t.exclude,
        }).unwrap_or_default(),
        stop_sequences: parsed.stop_sequences,
    };

    Some(Ok(AgentFile {
        id: id.to_string(),
        persona,
        prompt,
        from_disk,
    }))
}

/// Try to parse legacy `<!-- tools: -toolname -->` format.
fn try_parse_legacy_front_matter(id: &str, content: &str, from_disk: bool) -> Option<AgentFile> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("<!--") {
        return None;
    }

    // Find closing -->
    let close_pos = trimmed.find("-->")?;
    let comment = &trimmed[4..close_pos].trim();
    let prompt = trimmed[close_pos + 3..].trim_start().to_string();

    let mut exclude_tools = Vec::new();
    for line in comment.lines() {
        let line = line.trim();
        if line.starts_with("tools:") {
            let tools_str = line.strip_prefix("tools:").unwrap().trim();
            for tool in tools_str.split_whitespace() {
                if let Some(name) = tool.strip_prefix('-') {
                    exclude_tools.push(name.to_string());
                }
            }
        }
    }

    Some(AgentFile {
        id: id.to_string(),
        persona: PersonaData {
            display_name: id.to_string(),
            tool_overrides: ToolOverrides { exclude_tools },
            ..Default::default()
        },
        prompt,
        from_disk,
    })
}

impl PersonaData {
    /// Count how many keywords match in the given task text (case-insensitive).
    /// Used for flock routing to select the best agent for a task.
    pub fn matches_keywords(&self, task: &str) -> usize {
        let task_lower = task.to_lowercase();
        self.keywords.iter()
            .filter(|kw| task_lower.contains(&kw.to_lowercase()))
            .count()
    }

    /// Check if a tool should be excluded based on tool overrides.
    pub fn should_exclude_tool(&self, tool_name: &str) -> bool {
        self.tool_overrides.exclude_tools.iter().any(|t| t == tool_name)
    }
}

/// Load all agent files from a workspace directory's `agents/` subdirectory.
///
/// Scans for `.md` files in `<dir>/agents/` and parses their front matter.
/// Used by flock routing to discover available agents without embedded fallbacks.
pub fn load_all_from_dir(workspace_dir: &std::path::Path) -> Vec<AgentFile> {
    let agents_dir = workspace_dir.join("agents");
    if !agents_dir.is_dir() {
        return Vec::new();
    }

    let mut agents = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&agents_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(agent_file) = parse_agent_file(stem, &content, true) {
                            agents.push(agent_file);
                        }
                    }
                }
            }
        }
    }
    agents
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_toml_front_matter() {
        let content = r#"+++
display_name = "Scout"
role = "Research specialist"
keywords = ["research", "documentation", "library"]

[scope]
read_only = true

[tools]
exclude = ["research"]
+++

You are **Scout**. Your role is to perform research.
"#;
        let agent = parse_agent_file("scout", content, false).unwrap();
        assert_eq!(agent.persona.display_name, "Scout");
        assert_eq!(agent.persona.role, "Research specialist");
        assert!(agent.persona.scope.read_only);
        assert_eq!(agent.persona.tool_overrides.exclude_tools, vec!["research"]);
        assert_eq!(agent.persona.keywords, vec!["research", "documentation", "library"]);
        assert!(agent.prompt.contains("You are **Scout**"));
    }

    #[test]
    fn test_parse_legacy_front_matter() {
        let content = r#"<!--
tools: -research
-->

You are **Scout**."#;
        let agent = parse_agent_file("scout", content, false).unwrap();
        assert_eq!(agent.persona.display_name, "scout");
        assert_eq!(agent.persona.tool_overrides.exclude_tools, vec!["research"]);
        assert!(agent.prompt.contains("You are **Scout**"));
    }

    #[test]
    fn test_parse_bare_prompt() {
        let content = "You are **Carmack**. Write fast code.";
        let agent = parse_agent_file("carmack", content, false).unwrap();
        assert_eq!(agent.persona.display_name, "carmack");
        assert!(agent.persona.tool_overrides.exclude_tools.is_empty());
        assert!(agent.prompt.contains("Carmack"));
    }

    #[test]
    fn test_keyword_matching() {
        let persona = PersonaData {
            keywords: vec!["security".into(), "vulnerability".into(), "auth".into()],
            ..Default::default()
        };
        assert_eq!(persona.matches_keywords("Fix security vulnerability in login"), 2);
        assert_eq!(persona.matches_keywords("Add new button to UI"), 0);
        assert_eq!(persona.matches_keywords("Fix AUTH token expiry"), 1);
    }

    #[test]
    fn test_should_exclude_tool() {
        let persona = PersonaData {
            tool_overrides: ToolOverrides {
                exclude_tools: vec!["research".into(), "write_file".into()],
            },
            ..Default::default()
        };
        assert!(persona.should_exclude_tool("research"));
        assert!(persona.should_exclude_tool("write_file"));
        assert!(!persona.should_exclude_tool("read_file"));
    }

    #[test]
    fn test_stop_sequences_parsing() {
        let content = r#"+++
display_name = "Scout"
stop_sequences = ["---END---", "DONE"]

[scope]
read_only = true
+++

Prompt here.
"#;
        let agent = parse_agent_file("scout", content, false).unwrap();
        assert_eq!(agent.persona.stop_sequences, vec!["---END---", "DONE"]);
    }

    #[test]
    fn test_stop_sequences_under_section_not_parsed() {
        // stop_sequences under [tools] should NOT be read as root-level
        let content = r#"+++
display_name = "Scout"

[tools]
exclude = ["research"]
stop_sequences = ["---END---"]
+++

Prompt here.
"#;
        let agent = parse_agent_file("scout", content, false).unwrap();
        // stop_sequences is under [tools], not root - should be empty
        assert!(agent.persona.stop_sequences.is_empty());
    }
}
