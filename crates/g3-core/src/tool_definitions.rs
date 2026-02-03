//! Tool definitions for the agent's available tools.
//!
//! This module contains the JSON schema definitions for all tools that can be
//! used by the agent when interacting with LLM providers that support native
//! tool calling.

use g3_providers::Tool;
use serde_json::json;

/// Configuration for which optional tool sets to enable
#[derive(Debug, Clone, Copy, Default)]
pub struct ToolConfig {
    pub webdriver: bool,
    pub computer_control: bool,
    pub exclude_research: bool,
    pub zai_tools: bool,
    pub mcp_tools: bool,
}

impl ToolConfig {
    pub fn new(webdriver: bool, computer_control: bool, zai_tools: bool) -> Self {
        Self {
            webdriver,
            computer_control,
            exclude_research: false,
            zai_tools,
            mcp_tools: false,
        }
    }

    /// Create a config with MCP tools enabled.
    pub fn with_mcp_tools(mut self) -> Self {
        self.mcp_tools = true;
        self
    }

    /// Create a config with the research tool excluded.
    /// Used for scout agent to prevent recursion.
    pub fn with_research_excluded(mut self) -> Self {
        self.exclude_research = true;
        self
    }
}

/// Create tool definitions for native tool calling providers.
///
/// Returns a vector of Tool definitions that describe the available tools
/// and their input schemas.
pub fn create_tool_definitions(config: ToolConfig) -> Vec<Tool> {
    let mut tools = create_core_tools(config.exclude_research);

    if config.webdriver {
        tools.extend(create_webdriver_tools());
    }

    if config.zai_tools {
        tools.extend(create_zai_tools());
    }

    if config.mcp_tools {
        tools.extend(create_mcp_tools());
    }

    tools
}

/// Create the core tools that are always available
fn create_core_tools(exclude_research: bool) -> Vec<Tool> {
    let mut tools = vec![
        Tool {
            name: "shell".to_string(),
            description: "Execute shell commands in the current working directory. Do NOT prefix commands with `cd <path> &&` - commands already run in the workspace.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    }
                },
                "required": ["command"]
            }),
        },
        Tool {
            name: "background_process".to_string(),
            description: "Launch a long-running process in the background (e.g., game servers, dev servers). The process runs independently and logs are captured to a file. Use the regular 'shell' tool to read logs (cat/tail), check status (ps), or stop the process (kill). Returns the PID and log file path.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "A unique name for this process (e.g., 'game_server', 'my_app'). Used to identify the process and its log file."
                    },
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute in the background"
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Optional working directory. Defaults to current directory if not specified."
                    }
                },
                "required": ["name", "command"]
            }),
        },
        Tool {
            name: "read_file".to_string(),
            description: "Read the contents of a file. Optionally read a specific character range.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "The path to the file to read"
                    },
                    "start": {
                        "type": "integer",
                        "description": "Starting character position (0-indexed, inclusive). If omitted, reads from beginning."
                    },
                    "end": {
                        "type": "integer",
                        "description": "Ending character position (0-indexed, EXCLUSIVE). If omitted, reads to end of file."
                    }
                },
                "required": ["file_path"]
            }),
        },
        Tool {
            name: "read_image".to_string(),
            description: "Read one or more image files and send them to the LLM for visual analysis. Supports PNG, JPEG, GIF, and WebP formats. Use this when you need to visually inspect images (e.g., find sprites, analyze UI, read diagrams). The images will be included in your next response for analysis.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Array of paths to image files to read"
                    }
                },
                "required": ["file_paths"]
            }),
        },
        Tool {
            name: "write_file".to_string(),
            description: "Write content to a file (creates or overwrites). You MUST provide all arguments".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "The path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write to the file"
                    }
                },
                "required": ["file_path", "content"]
            }),
        },
        Tool {
            name: "str_replace".to_string(),
            description: "Apply a unified diff to a file. Supports multiple hunks and context lines. Optionally constrain the search to a [start, end) character range (0-indexed; end is EXCLUSIVE). Useful to disambiguate matches or limit scope in large files.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "The path to the file to edit"
                    },
                    "diff": {
                        "type": "string",
                        "description": "A unified diff showing what to replace. Supports @@ hunk headers, context lines, and multiple hunks (---/+++ headers optional for minimal diffs)."
                    },
                    "start": {
                        "type": "integer",
                        "description": "Starting character position in the file (0-indexed, inclusive). If omitted, searches from beginning."
                    },
                    "end": {
                        "type": "integer",
                        "description": "Ending character position in the file (0-indexed, EXCLUSIVE - character at this position is NOT included). If omitted, searches to end of file."
                    }
                },
                "required": ["file_path", "diff"]
            }),
        },
        Tool {
            name: "screenshot".to_string(),
            description: "Capture a screenshot of a specific application window. You MUST specify the window_id parameter with the application name (e.g., 'Safari', 'Terminal', 'Google Chrome'). The tool will automatically use the native screencapture command with the application's window ID for a clean capture. Use list_windows first to identify available windows.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Filename for the screenshot (e.g., 'safari.png'). If a relative path is provided, the screenshot will be saved to ~/tmp or $TMPDIR. Use an absolute path to save elsewhere."
                    },
                    "window_id": {
                        "type": "string",
                        "description": "REQUIRED: Application name to capture (e.g., 'Safari', 'Terminal', 'Google Chrome'). The tool will capture the frontmost window of that application using its native window ID."
                    },
                    "region": {
                        "type": "object",
                        "properties": {
                            "x": {"type": "integer"},
                            "y": {"type": "integer"},
                            "width": {"type": "integer"},
                            "height": {"type": "integer"}
                        }
                    }
                },
                "required": ["path", "window_id"]
            }),
        },
        Tool {
            name: "coverage".to_string(),
            description: "Generate a code coverage report for the entire workspace using cargo llvm-cov. This runs all tests with coverage instrumentation and returns a summary of coverage statistics. Requires llvm-tools-preview and cargo-llvm-cov to be installed (they will be auto-installed if missing).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        Tool {
            name: "code_search".to_string(),
            description: "Syntax-aware code search that understands code structure, not just text. Finds actual functions, classes, methods, and other code constructs - ignores matches in comments and strings. Much more accurate than grep for code searches. Supports batch searches (up to 20 parallel) with structured results and context lines. Languages: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, Racket. Uses tree-sitter query syntax.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "searches": {
                        "type": "array",
                        "maxItems": 20,
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string", "description": "Label for this search." },
                                "query": { "type": "string", "description": "tree-sitter query in S-expression format (e.g., \"(function_item name: (identifier) @name)\")" },
                                "language": { "type": "string", "enum": ["rust", "python", "javascript", "typescript", "go", "java", "c", "cpp", "racket"], "description": "Programming language to search." },
                                "paths": { "type": "array", "items": { "type": "string" }, "description": "Paths/dirs to search. Defaults to current dir if empty." },
                                "context_lines": { "type": "integer", "minimum": 0, "maximum": 20, "default": 0, "description": "Lines of context to include around each match." }
                            },
                            "required": ["name", "query", "language"]
                        }
                    },
                    "max_concurrency": { "type": "integer", "minimum": 1, "default": 4 },
                    "max_matches_per_search": { "type": "integer", "minimum": 1, "default": 500 }
                },
                "required": ["searches"]
            }),
        },
    ];

    // Conditionally add the research tool (excluded for scout agent to prevent recursion)
    if !exclude_research {
        tools.push(Tool {
            name: "research".to_string(),
            description: "Initiate web-based research on a topic. This tool is ASYNCHRONOUS - it spawns a research agent in the background and returns immediately with a research_id. Results are automatically injected into the conversation when ready. Use this when you need to research APIs, SDKs, libraries, approaches, bugs, or documentation. If you need the results before continuing, say so and yield the turn to the user. Check status with research_status tool.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The research question or topic to investigate. Be specific about what you need to know."
                    }
                },
                "required": ["query"]
            }),
        });

        // research_status tool - check status of pending research
        tools.push(Tool {
            name: "research_status".to_string(),
            description: "Check the status of pending research tasks. Call without arguments to list all pending research, or with a research_id to check a specific task. Use this to see if research has completed before it's automatically injected.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "research_id": {
                        "type": "string",
                        "description": "Optional: specific research_id to check. If omitted, lists all pending research tasks."
                    }
                },
                "required": []
            }),
        });
    }

    // Plan Mode tools
    tools.push(Tool {
        name: "plan_read".to_string(),
        description: "Read the current Plan for this session. Shows the plan structure with items, their states, checks (happy/negative/boundary), evidence, and notes. Use this to review the plan before making updates.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
    });

    tools.push(Tool {
        name: "plan_write".to_string(),
        description: r#"Create or update the Plan for this session. The plan must be provided as YAML with the following structure:

- plan_id: Unique identifier for the plan
- revision: Will be auto-incremented
- items: Array of plan items, each with:
  - id: Stable identifier (e.g., "I1")
  - description: What will be done
  - state: todo | doing | done | blocked
  - touches: Array of paths/modules affected
  - checks:
      happy: {desc, target} - Normal successful operation
      negative: {desc, target} - Error handling, invalid input
      boundary: {desc, target} - Edge cases, limits
  - evidence: Array of file:line refs, test names (required when done)
  - notes: Implementation explanation (required when done)

Rules:
- Keep items ≤ 7 by default
- All three checks (happy, negative, boundary) are required
- Cannot remove items from an approved plan (mark as blocked instead)
- Evidence and notes required when marking item as done"#.to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "plan": {
                    "type": "string",
                    "description": "The plan as YAML. Must include plan_id and items array."
                }
            },
            "required": ["plan"]
        }),
    });

    tools.push(Tool {
        name: "plan_approve".to_string(),
        description: "Mark the current plan revision as approved. This is called by the user (not the agent) to approve a drafted plan before implementation begins. Once approved, plan items cannot be removed (only marked as blocked). The agent should ask for approval after drafting a plan.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
    });

    // Workspace memory tool (memory is auto-loaded at startup, only remember is needed)
    tools.push(Tool {
        name: "remember".to_string(),
        description: "Update the workspace memory with new discoveries. Call this at the END of your turn (before your summary) if you discovered something worth noting. Provide your notes in markdown format - they will be merged with existing memory.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "notes": {
                    "type": "string",
                    "description": "New discoveries to add to memory in markdown format. Use the format:\n### Feature Name\n- `file/path.rs` [start..end] - `function_name()`, `StructName`\n\nDo not include content already in memory."
                }
            },
            "required": ["notes"]
        }),
    });

    // ACD rehydration tool
    tools.push(Tool {
        name: "rehydrate".to_string(),
        description: "Restore dehydrated conversation history from a previous context segment. Use this when you see a DEHYDRATED CONTEXT stub and need to recall the full conversation details from that segment.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "fragment_id": {
                    "type": "string",
                    "description": "The fragment ID to restore (from a DEHYDRATED CONTEXT stub message)"
                }
            },
            "required": ["fragment_id"]
        }),
    });

    tools
}

/// Create WebDriver browser automation tools
fn create_webdriver_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "webdriver_start".to_string(),
            description: "Start a Safari WebDriver session for browser automation. Must be called before any other webdriver tools. Requires Safari's 'Allow Remote Automation' to be enabled in Develop menu.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        Tool {
            name: "webdriver_navigate".to_string(),
            description: "Navigate to a URL in the browser".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to navigate to (must include protocol, e.g., https://)"
                    }
                },
                "required": ["url"]
            }),
        },
        Tool {
            name: "webdriver_get_url".to_string(),
            description: "Get the current URL of the browser".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        Tool {
            name: "webdriver_get_title".to_string(),
            description: "Get the title of the current page".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        Tool {
            name: "webdriver_find_element".to_string(),
            description: "Find an element on the page by CSS selector and return its text content".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector to find the element (e.g., 'h1', '.class-name', '#id')"
                    }
                },
                "required": ["selector"]
            }),
        },
        Tool {
            name: "webdriver_find_elements".to_string(),
            description: "Find all elements matching a CSS selector and return their text content".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector to find elements"
                    }
                },
                "required": ["selector"]
            }),
        },
        Tool {
            name: "webdriver_click".to_string(),
            description: "Click an element on the page".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the element to click"
                    }
                },
                "required": ["selector"]
            }),
        },
        Tool {
            name: "webdriver_send_keys".to_string(),
            description: "Type text into an input element".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the input element"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to type into the element"
                    },
                    "clear_first": {
                        "type": "boolean",
                        "description": "Whether to clear the element before typing (default: true)"
                    }
                },
                "required": ["selector", "text"]
            }),
        },
        Tool {
            name: "webdriver_execute_script".to_string(),
            description: "Execute JavaScript code in the browser and return the result".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "script": {
                        "type": "string",
                        "description": "JavaScript code to execute (use 'return' to return a value)"
                    }
                },
                "required": ["script"]
            }),
        },
        Tool {
            name: "webdriver_get_page_source".to_string(),
            description: "Get the rendered HTML source of the current page. Returns the current DOM state after JavaScript execution.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "max_length": {
                        "type": "integer",
                        "description": "Maximum length of HTML to return (default: 10000, use 0 for no truncation)"
                    },
                    "save_to_file": {
                        "type": "string",
                        "description": "Optional file path to save the HTML instead of returning it inline"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "webdriver_screenshot".to_string(),
            description: "Take a screenshot of the browser window".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path where to save the screenshot (e.g., '/tmp/screenshot.png')"
                    }
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "webdriver_back".to_string(),
            description: "Navigate back in browser history".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        Tool {
            name: "webdriver_forward".to_string(),
            description: "Navigate forward in browser history".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        Tool {
            name: "webdriver_refresh".to_string(),
            description: "Refresh the current page".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        Tool {
            name: "webdriver_quit".to_string(),
            description: "Close the browser and end the WebDriver session".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
    ]
}

/// Create Z.ai special tools for web search, web reading, and OCR
pub fn create_zai_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "zai_web_search".to_string(),
            description: "Search the web using Z.ai's Web Search API. Returns structured results with title, link, and content snippet. Useful for finding current information, documentation, or researching topics.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                    "count": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "Number of results to return (1-50, default 10)"
                    },
                    "search_domain_filter": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of domains to limit search to"
                    },
                    "search_recency_filter": {
                        "type": "string",
                        "enum": ["day", "week", "month", "year"],
                        "description": "Filter results by recency"
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "zai_web_reader".to_string(),
            description: "Fetch a webpage and convert it to markdown or plain text using Z.ai's Web Reader API. Useful for reading web content, documentation pages, or articles.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch and convert"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["markdown", "text"],
                        "description": "Output format (default: markdown)"
                    },
                    "retain_images": {
                        "type": "boolean",
                        "description": "Whether to retain image references (default: true)"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Request timeout in seconds (default: 20)"
                    }
                },
                "required": ["url"]
            }),
        },
        Tool {
            name: "zai_ocr".to_string(),
            description: "Extract text from images or PDFs using Z.ai's GLM-OCR model. Performs layout-aware text extraction.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "URL or base64 data URI of the image/PDF to process"
                    },
                    "page_range": {
                        "type": "string",
                        "description": "For PDFs, specify page range (e.g., '1-5')"
                    }
                },
                "required": ["file"]
            }),
        },
    ]
}

/// Create MCP (Model Context Protocol) tools for Z.ai MCP servers
pub fn create_mcp_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "mcp_web_search".to_string(),
            description: "Search the web using Z.ai's MCP web search server (webSearchPrime). Returns structured search results with title, URL, and content summary.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "search_query": {
                        "type": "string",
                        "description": "The search query (recommended: not exceed 70 characters)"
                    },
                    "content_size": {
                        "type": "string",
                        "enum": ["medium", "high"],
                        "description": "Control web page summary size: medium (400-600 words), high (2500 words, higher cost)"
                    },
                    "location": {
                        "type": "string",
                        "enum": ["cn", "us"],
                        "description": "Region hint: cn (Chinese), us (non-Chinese). Default: cn"
                    },
                    "search_recency_filter": {
                        "type": "string",
                        "enum": ["oneDay", "oneWeek", "oneMonth", "oneYear", "noLimit"],
                        "description": "Filter results by recency. Default: noLimit"
                    },
                    "search_domain_filter": {
                        "type": "string",
                        "description": "Limit search to specific domain (e.g., www.example.com)"
                    }
                },
                "required": ["search_query"]
            }),
        },
        Tool {
            name: "mcp_web_reader".to_string(),
            description: "Fetch and convert a URL to markdown or text using Z.ai's MCP web reader server. Ideal for reading documentation, articles, or web content.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch and convert"
                    },
                    "return_format": {
                        "type": "string",
                        "enum": ["markdown", "text"],
                        "description": "Output format. Default: markdown"
                    },
                    "retain_images": {
                        "type": "boolean",
                        "description": "Whether to retain images. Default: true"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Request timeout in seconds. Default: 20"
                    },
                    "no_cache": {
                        "type": "boolean",
                        "description": "Disable caching. Default: false"
                    },
                    "with_links_summary": {
                        "type": "boolean",
                        "description": "Include links summary. Default: false"
                    },
                    "with_images_summary": {
                        "type": "boolean",
                        "description": "Include images summary. Default: false"
                    }
                },
                "required": ["url"]
            }),
        },
        Tool {
            name: "mcp_search_doc".to_string(),
            description: "Search documentation, issues, and commits of a GitHub repository using Z.ai's MCP zread server.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo_name": {
                        "type": "string",
                        "description": "GitHub repository in owner/repo format (e.g., 'vitejs/vite')"
                    },
                    "query": {
                        "type": "string",
                        "description": "Search keywords or question about the repository"
                    },
                    "language": {
                        "type": "string",
                        "enum": ["zh", "en"],
                        "description": "Response language: 'zh' (Chinese) or 'en' (English)"
                    }
                },
                "required": ["repo_name", "query"]
            }),
        },
        Tool {
            name: "mcp_get_repo_structure".to_string(),
            description: "Get the directory structure and file list of a GitHub repository using Z.ai's MCP zread server.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo_name": {
                        "type": "string",
                        "description": "GitHub repository in owner/repo format (e.g., 'vitejs/vite')"
                    },
                    "dir_path": {
                        "type": "string",
                        "description": "Directory path to inspect. Default: root '/'"
                    }
                },
                "required": ["repo_name"]
            }),
        },
        Tool {
            name: "mcp_read_file".to_string(),
            description: "Read the full code content of a specific file in a GitHub repository using Z.ai's MCP zread server.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo_name": {
                        "type": "string",
                        "description": "GitHub repository in owner/repo format (e.g., 'vitejs/vite')"
                    },
                    "file_path": {
                        "type": "string",
                        "description": "Relative path to the file (e.g., 'src/index.ts')"
                    }
                },
                "required": ["repo_name", "file_path"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_tools_count() {
        let tools = create_core_tools(false);
        // Core tools: shell, background_process, read_file, read_image,
        // write_file, str_replace, screenshot, coverage, code_search,
        // research, research_status, remember, plan_read, plan_write, plan_approve, rehydrate
        // (16 total - memory is auto-loaded, only remember tool needed)
        assert_eq!(tools.len(), 16);
    }

    #[test]
    fn test_webdriver_tools_count() {
        let tools = create_webdriver_tools();
        // 15 webdriver tools
        assert_eq!(tools.len(), 15);
    }

    #[test]
    fn test_zai_tools_count() {
        let tools = create_zai_tools();
        // 3 Z.ai tools: zai_web_search, zai_web_reader, zai_ocr
        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn test_create_tool_definitions_core_only() {
        let config = ToolConfig::default();
        let tools = create_tool_definitions(config);
        assert_eq!(tools.len(), 16);
    }

    #[test]
    fn test_create_tool_definitions_all_enabled() {
        let config = ToolConfig::new(true, true, true);
        let tools = create_tool_definitions(config);
        // 16 core + 15 webdriver + 3 zai = 34
        assert_eq!(tools.len(), 34);
    }

    #[test]
    fn test_create_tool_definitions_with_zai_tools() {
        let config = ToolConfig::new(false, false, true);
        let tools = create_tool_definitions(config);
        // 16 core + 3 zai = 19
        assert_eq!(tools.len(), 19);

        // Verify Z.ai tools are present
        assert!(tools.iter().any(|t| t.name == "zai_web_search"));
        assert!(tools.iter().any(|t| t.name == "zai_web_reader"));
        assert!(tools.iter().any(|t| t.name == "zai_ocr"));
    }

    #[test]
    fn test_tool_has_required_fields() {
        let tools = create_core_tools(false);
        for tool in tools {
            assert!(!tool.name.is_empty(), "Tool name should not be empty");
            assert!(!tool.description.is_empty(), "Tool description should not be empty");
            assert!(tool.input_schema.is_object(), "Tool input_schema should be an object");
        }
    }

    #[test]
    fn test_zai_tools_have_required_fields() {
        let tools = create_zai_tools();
        for tool in tools {
            assert!(!tool.name.is_empty(), "Tool name should not be empty");
            assert!(!tool.description.is_empty(), "Tool description should not be empty");
            assert!(tool.input_schema.is_object(), "Tool input_schema should be an object");
        }
    }

    #[test]
    fn test_research_tool_excluded() {
        let tools_with_research = create_core_tools(false);
        let tools_without_research = create_core_tools(true);

        assert_eq!(tools_with_research.len(), 16);
        assert_eq!(tools_without_research.len(), 14);  // research + research_status both excluded

        assert!(tools_with_research.iter().any(|t| t.name == "research"));
        assert!(!tools_without_research.iter().any(|t| t.name == "research"));
    }

    #[test]
    fn test_mcp_tools_count() {
        let tools = create_mcp_tools();
        // 5 MCP tools: mcp_web_search, mcp_web_reader, mcp_search_doc, mcp_get_repo_structure, mcp_read_file
        assert_eq!(tools.len(), 5);
    }

    #[test]
    fn test_create_tool_definitions_with_mcp_tools() {
        let config = ToolConfig::default().with_mcp_tools();
        let tools = create_tool_definitions(config);
        // 16 core + 5 mcp = 21
        assert_eq!(tools.len(), 21);

        // Verify MCP tools are present
        assert!(tools.iter().any(|t| t.name == "mcp_web_search"));
        assert!(tools.iter().any(|t| t.name == "mcp_web_reader"));
        assert!(tools.iter().any(|t| t.name == "mcp_search_doc"));
        assert!(tools.iter().any(|t| t.name == "mcp_get_repo_structure"));
        assert!(tools.iter().any(|t| t.name == "mcp_read_file"));
    }

    #[test]
    fn test_mcp_tools_have_required_fields() {
        let tools = create_mcp_tools();
        for tool in tools {
            assert!(!tool.name.is_empty(), "Tool name should not be empty");
            assert!(!tool.description.is_empty(), "Tool description should not be empty");
            assert!(tool.input_schema.is_object(), "Tool input_schema should be an object");
        }
    }

    #[test]
    fn test_create_tool_definitions_all_enabled_with_mcp() {
        let config = ToolConfig::new(true, true, true).with_mcp_tools();
        let tools = create_tool_definitions(config);
        // 16 core + 15 webdriver + 3 zai + 5 mcp = 39
        assert_eq!(tools.len(), 39);
    }
}
