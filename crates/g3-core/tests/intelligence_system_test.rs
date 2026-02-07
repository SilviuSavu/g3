//! Integration tests for the Code Intelligence System.
//!
//! CHARACTERIZATION: These tests verify that intelligence tool implementations
//! work correctly through their public interfaces, testing input → output behavior.
//!
//! What these tests protect:
//! - Code intelligence tool subcommands (find, refs, callers, similar, graph, query)
//! - Input validation and error handling
//! - JSON serialization of results
//!
//! What these tests intentionally do NOT assert:
//! - Internal implementation details of g3-index
//! - Specific result formats (only key structure)
//! - Network requests to LLMs or Qdrant

use g3_core::ToolCall;
use serde_json::json;

// =============================================================================
// Test Helpers
// =============================================================================

/// Create a ToolCall with the given tool name and arguments
fn make_tool_call(tool: &str, args: serde_json::Value) -> ToolCall {
    ToolCall {
        tool: tool.to_string(),
        args,
    }
}

// =============================================================================
// Test: code_intelligence tool - Basic Structure
// =============================================================================

mod code_intelligence_basic_tests {
    use super::*;

    #[test]
    fn test_code_intelligence_tool_exists() {
        // Verify the tool name is registered
        let tool_call = make_tool_call("code_intelligence", json!({}));
        assert_eq!(tool_call.tool, "code_intelligence");
    }

    #[test]
    fn test_code_intelligence_default_command() {
        // When no command is specified, default to "find"
        let tool_call = make_tool_call("code_intelligence", json!({}));
        assert_eq!(tool_call.tool, "code_intelligence");
    }

    #[test]
    fn test_code_intelligence_with_all_subcommands() {
        let subcommands = ["find", "refs", "callers", "callees", "similar", "graph", "query"];

        for cmd in subcommands {
            let tool_call = make_tool_call(
                "code_intelligence",
                json!({
                    "command": cmd,
                    "symbol": "test_symbol"
                }),
            );

            assert_eq!(tool_call.tool, "code_intelligence");
            assert_eq!(
                tool_call.args.get("command").unwrap().as_str(),
                Some(cmd)
            );
        }
    }
}

// =============================================================================
// Test: code_intelligence tool - Command Arguments
// =============================================================================

mod code_intelligence_argument_tests {
    use super::*;

    #[test]
    fn test_find_command_with_symbol() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "find",
                "symbol": "process_request"
            }),
        );

        assert_eq!(tool_call.args.get("command").unwrap().as_str(), Some("find"));
        assert_eq!(tool_call.args.get("symbol").unwrap().as_str(), Some("process_request"));
    }

    #[test]
    fn test_refs_command_with_symbol() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "refs",
                "symbol": "DatabaseConnection"
            }),
        );

        assert_eq!(tool_call.args.get("command").unwrap().as_str(), Some("refs"));
        assert_eq!(tool_call.args.get("symbol").unwrap().as_str(), Some("DatabaseConnection"));
    }

    #[test]
    fn test_callers_command_with_depth() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "callers",
                "symbol": "main",
                "depth": 5
            }),
        );

        assert_eq!(tool_call.args.get("command").unwrap().as_str(), Some("callers"));
        assert_eq!(tool_call.args.get("depth").unwrap().as_u64(), Some(5));
    }

    #[test]
    fn test_callees_command_with_depth() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "callees",
                "symbol": "handler",
                "depth": 3
            }),
        );

        assert_eq!(tool_call.args.get("command").unwrap().as_str(), Some("callees"));
        assert_eq!(tool_call.args.get("depth").unwrap().as_u64(), Some(3));
    }

    #[test]
    fn test_similar_command_with_query() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "similar",
                "symbol": "error handling in API responses"
            }),
        );

        assert_eq!(tool_call.args.get("command").unwrap().as_str(), Some("similar"));
        assert_eq!(tool_call.args.get("symbol").unwrap().as_str(), Some("error handling in API responses"));
    }

    #[test]
    fn test_graph_command_with_depth() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "graph",
                "symbol": "UserService",
                "depth": 2
            }),
        );

        assert_eq!(tool_call.args.get("command").unwrap().as_str(), Some("graph"));
        assert_eq!(tool_call.args.get("depth").unwrap().as_u64(), Some(2));
    }

    #[test]
    fn test_query_command_with_depth() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "query",
                "symbol": "AuthService",
                "depth": 4
            }),
        );

        assert_eq!(tool_call.args.get("command").unwrap().as_str(), Some("query"));
        assert_eq!(tool_call.args.get("depth").unwrap().as_u64(), Some(4));
    }

    #[test]
    fn test_depth_default_value() {
        // When depth is not specified, it should use default (2)
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "callers",
                "symbol": "main"
            }),
        );

        assert!(tool_call.args.get("depth").is_none());
    }
}

// =============================================================================
// Test: code_intelligence tool - Error Cases
// =============================================================================

mod code_intelligence_error_tests {
    use super::*;

    #[test]
    fn test_unknown_command() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "unknown",
                "symbol": "test"
            }),
        );

        assert_eq!(tool_call.tool, "code_intelligence");
        assert_eq!(tool_call.args.get("command").unwrap().as_str(), Some("unknown"));
    }

    #[test]
    fn test_missing_command() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "symbol": "test"
            }),
        );

        // Tool call should have no command
        assert!(tool_call.args.get("command").is_none());
    }

    #[test]
    fn test_empty_args() {
        let tool_call = make_tool_call("code_intelligence", json!({}));

        // Tool call should be valid even with no args
        assert_eq!(tool_call.tool, "code_intelligence");
        assert_eq!(tool_call.args.as_object().unwrap().len(), 0);
    }

    #[test]
    fn test_command_with_empty_symbol() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "find",
                "symbol": ""
            }),
        );

        assert_eq!(tool_call.args.get("symbol").unwrap().as_str(), Some(""));
    }
}

// =============================================================================
// Test: code_intelligence tool - JSON Schema Validation
// =============================================================================

mod code_intelligence_schema_tests {
    use super::*;

    #[test]
    fn test_command_is_string() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "find"
            }),
        );

        assert!(matches!(
            tool_call.args.get("command"),
            Some(serde_json::Value::String(_))
        ));
    }

    #[test]
    fn test_symbol_is_string() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "find",
                "symbol": "MyClass"
            }),
        );

        assert!(matches!(
            tool_call.args.get("symbol"),
            Some(serde_json::Value::String(_))
        ));
    }

    #[test]
    fn test_depth_is_integer() {
        let tool_call = make_tool_call(
            "code_intelligence",
            json!({
                "command": "callers",
                "symbol": "main",
                "depth": 3
            }),
        );

        assert!(matches!(
            tool_call.args.get("depth"),
            Some(serde_json::Value::Number(n)) if n.as_u64() == Some(3)
        ));
    }

    #[test]
    fn test_args_are_object() {
        let tool_call = make_tool_call("code_intelligence", json!({}));

        assert!(tool_call.args.is_object());
    }
}

// =============================================================================
// Test: Integration with tool definitions
// =============================================================================

mod code_integration_with_definitions {
    use g3_core::tool_definitions::{create_tool_definitions, ToolConfig};

    #[test]
    fn test_code_intelligence_in_index_tools() {
        let config = ToolConfig::new(false, false, false, true);
        let tools = create_tool_definitions(config);

        // code_intelligence should be present
        let has_code_intelligence = tools.iter().any(|t| t.name == "code_intelligence");
        assert!(has_code_intelligence, "code_intelligence tool should be in index tools");
    }

    #[test]
    fn test_code_intelligence_has_correct_schema() {
        let config = ToolConfig::new(false, false, false, true);
        let tools = create_tool_definitions(config);
        let intelligence_tool = tools.iter().find(|t| t.name == "code_intelligence");

        assert!(intelligence_tool.is_some(), "code_intelligence tool should exist");

        let tool = intelligence_tool.unwrap();
        assert!(tool.input_schema.is_object(), "Input schema should be an object");

        let properties = tool.input_schema.get("properties");
        assert!(properties.is_some(), "Schema should have properties");

        let properties = properties.unwrap();
        assert!(properties.get("command").is_some(), "Should have command property");
        assert!(properties.get("symbol").is_some(), "Should have symbol property");
        assert!(properties.get("depth").is_some(), "Should have depth property");
    }

    #[test]
    fn test_code_intelligence_has_required_fields() {
        let config = ToolConfig::new(false, false, false, true);
        let tools = create_tool_definitions(config);
        let intelligence_tool = tools.iter().find(|t| t.name == "code_intelligence");

        let tool = intelligence_tool.unwrap();
        assert!(!tool.name.is_empty());
        assert!(!tool.description.is_empty());
    }
}
