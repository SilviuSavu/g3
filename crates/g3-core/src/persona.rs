//! Structured persona system for g3 agents.
//!
//! Provides typed metadata for agent personas including role, keywords (for flock routing),
//! scope boundaries (read-only enforcement), and tool overrides (exclusions).
//!
//! Supports two front matter formats:
//! - New: TOML between `+++` delimiters
//! - Legacy: HTML comment `<!-- tools: -toolname -->`
//!
//! ## Role-Based Tool Filtering
//!
//! Roles can inherit tool presets from base roles:
//! ```toml
//! +++
//! role = "coder"
//! inherits = "researcher"
//! ```
//!
//! The `coder` role gets researcher tools + coding tools.

use anyhow::Result;
use serde::Deserialize;
use std::collections::HashSet;

/// Tool presets for different agent roles.
/// Each role has a set of allowed tools and can inherit from other roles.
pub mod roles {
    use std::collections::HashSet;
    
    /// Get the tool set for a role, including inherited tools.
    pub fn get_tools_for_role(role: &str) -> HashSet<&'static str> {
        let mut tools = HashSet::new();
        collect_tools_for_role(role, &mut tools);
        tools
    }
    
    fn collect_tools_for_role<'a>(role: &str, tools: &mut HashSet<&'a str>) {
        match role {
            "researcher" | "research" => {
                // Researcher: read-only tools
                tools.extend([
                    "read_file", "rg", "list_directory", "list_files", "scan_folder",
                    "preview_file", "semantic_search", "code_search", "pattern_search",
                    "graph_find_symbol", "graph_file_symbols", "graph_find_callers",
                    "graph_find_references", "code_intelligence", "index_codebase",
                    "index_status", "complexity_metrics", "lsp_hover", "lsp_goto_definition",
                    "lsp_find_references", "lsp_document_symbols", "lsp_workspace_symbols",
                    "beads_list", "beads_show", "beads_ready",
                ]);
            }
            "planner" | "plan" => {
                // Planner: researcher + planning tools
                collect_tools_for_role("researcher", tools);
                tools.extend([
                    "plan_read", "plan_write", "plan_approve",
                    "write_file", "str_replace",
                ]);
            }
            "coder" | "developer" => {
                // Coder: planner + code execution tools
                collect_tools_for_role("planner", tools);
                tools.extend([
                    "shell", "background_process",
                    "codebase_scout", "codebase_scout_status",
                    "research", "research_status",
                ]);
            }
            "tester" | "qa" => {
                // Tester: coder + test tools
                collect_tools_for_role("coder", tools);
                tools.extend([
                    "coverage",
                ]);
            }
            "reviewer" => {
                // Reviewer: researcher + review tools
                collect_tools_for_role("researcher", tools);
                tools.extend([
                    "str_replace", "write_file",  // Can suggest fixes
                ]);
            }
            "deployer" | "ops" => {
                // Deployer: coder + deployment tools
                collect_tools_for_role("coder", tools);
                tools.extend([
                    "webdriver_start", "webdriver_navigate", "webdriver_click",
                    "webdriver_send_keys", "webdriver_find_element", "webdriver_screenshot",
                    "screenshot",
                ]);
            }
            "orchestrator" | "lead" => {
                // Orchestrator: all tools
                collect_tools_for_role("tester", tools);
                tools.extend([
                    "beads_create", "beads_update", "beads_close", "beads_dep", "beads_sync",
                    "todo_read", "todo_write", "remember", "memory_compact",
                ]);
            }
            _ => {
                // Unknown role - give minimal safe tools
                tools.extend([
                    "read_file", "rg", "list_directory",
                ]);
            }
        }
    }
    
    /// Get the inheritance chain for a role.
    pub fn get_inheritance_chain(role: &str) -> Vec<&'static str> {
        match role {
            "tester" | "qa" => vec!["coder", "planner", "researcher"],
            "coder" | "developer" => vec!["planner", "researcher"],
            "planner" | "plan" => vec!["researcher"],
            "deployer" | "ops" => vec!["coder", "planner", "researcher"],
            "orchestrator" | "lead" => vec!["tester", "coder", "planner", "researcher"],
            _ => vec![],
        }
    }
    
    /// Check if a role inherits from another role.
    pub fn inherits_from(role: &str, base: &str) -> bool {
        get_inheritance_chain(role).contains(&base)
    }
}

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
    /// Base role to inherit tools from
    pub inherits: Option<String>,
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
    /// Base role to inherit tool permissions from
    #[serde(default)]
    inherits: Option<String>,
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
        inherits: parsed.inherits,
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
    
    /// Check if a tool is allowed based on role and inheritance.
    /// Returns true if the tool is permitted for this persona's role.
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        // First check explicit exclusions
        if self.should_exclude_tool(tool_name) {
            return false;
        }
        
        // Get tools from role (including inheritance)
        let role = if self.role.is_empty() { "researcher" } else { &self.role };
        let allowed_tools = roles::get_tools_for_role(role);
        
        allowed_tools.contains(tool_name)
    }
    
    /// Get all allowed tools for this persona's role.
    pub fn get_allowed_tools(&self) -> HashSet<&'static str> {
        let role = if self.role.is_empty() { "researcher" } else { &self.role };
        roles::get_tools_for_role(role)
    }
}

/// Validate tool configuration for a persona.
/// Returns a list of invalid tool references found in exclude_tools.
pub fn validate_tool_config(persona: &PersonaData) -> Vec<String> {
    // For now, just return the exclude_tools list
    // In a full implementation, you'd check against known tool names
    persona.tool_overrides.exclude_tools.clone()
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
    
    #[test]
    fn test_role_tool_filtering() {
        let researcher = PersonaData {
            role: "researcher".to_string(),
            ..Default::default()
        };
        // Researcher can read but not write
        assert!(researcher.is_tool_allowed("read_file"));
        assert!(researcher.is_tool_allowed("rg"));
        assert!(!researcher.is_tool_allowed("shell"));
        assert!(!researcher.is_tool_allowed("write_file"));
        
        let coder = PersonaData {
            role: "coder".to_string(),
            ..Default::default()
        };
        // Coder inherits researcher + gets shell/write
        assert!(coder.is_tool_allowed("read_file"));
        assert!(coder.is_tool_allowed("shell"));
        assert!(coder.is_tool_allowed("write_file"));
        assert!(coder.is_tool_allowed("plan_write"));
    }
    
    #[test]
    fn test_role_inheritance() {
        // Tester inherits coder, which inherits planner, which inherits researcher
        let tester = PersonaData {
            role: "tester".to_string(),
            ..Default::default()
        };
        assert!(tester.is_tool_allowed("read_file"));  // from researcher
        assert!(tester.is_tool_allowed("write_file")); // from planner
        assert!(tester.is_tool_allowed("shell"));      // from coder
        assert!(tester.is_tool_allowed("coverage"));   // from tester
    }
    
    #[test]
    fn test_tool_exclusion_overrides_role() {
        let restricted_coder = PersonaData {
            role: "coder".to_string(),
            tool_overrides: ToolOverrides {
                exclude_tools: vec!["shell".to_string()],
            },
            ..Default::default()
        };
        // Shell normally allowed for coder, but excluded here
        assert!(!restricted_coder.is_tool_allowed("shell"));
        assert!(restricted_coder.is_tool_allowed("write_file"));
    }
}
