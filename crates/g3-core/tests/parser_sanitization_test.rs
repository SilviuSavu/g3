//! Parser Sanitization Edge Case Tests
//!
//! CHARACTERIZATION: These tests verify edge cases for the inline tool pattern
//! sanitization that prevents parser poisoning.
//!
//! What these tests protect:
//! - Tool call patterns in various contexts (code blocks, quotes, etc.)
//! - Edge cases at line boundaries
//! - Unicode handling in sanitization
//!
//! What these tests intentionally do NOT assert:
//! - Internal parser state
//! - Exact sanitization implementation
//!
//! Related commits:
//! - 4c36cc0: fix: prevent parser poisoning from inline tool-call JSON patterns

use g3_core::streaming_parser::sanitize_inline_tool_patterns;

// =============================================================================
// Test: Code block contexts
// =============================================================================

mod code_block_contexts {
    use super::*;

    /// Test tool pattern in markdown inline code
    #[test]
    fn test_inline_code_backticks() {
        let input = "Use `{\"tool\": \"shell\"}` to run commands";
        let result = sanitize_inline_tool_patterns(input);
        
        // Should be sanitized since it's inline
        assert!(!result.contains("{\"tool\":"), "Inline code should be sanitized");
    }

    /// Test tool pattern after code fence (should NOT be sanitized)
    #[test]
    fn test_after_code_fence_standalone() {
        // Tool call on its own line after a code fence marker
        let input = "```\n{\"tool\": \"shell\", \"args\": {}}";
        let result = sanitize_inline_tool_patterns(input);
        
        // The tool call is on its own line, should NOT be sanitized
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines[1].starts_with("{\"tool\":"), "Standalone after fence should not be sanitized");
    }

    /// Test tool pattern in prose explanation
    #[test]
    fn test_prose_explanation() {
        let input = "The format is {\"tool\": \"name\", \"args\": {...}} where name is the tool";
        let result = sanitize_inline_tool_patterns(input);
        
        assert!(!result.contains("{\"tool\":"), "Prose should be sanitized");
    }
}

// =============================================================================
// Test: Line boundary edge cases
// =============================================================================

mod line_boundary_cases {
    use super::*;

    /// Test empty lines don't affect detection
    #[test]
    fn test_empty_lines_before_tool_call() {
        let input = "\n\n{\"tool\": \"shell\", \"args\": {}}";
        let result = sanitize_inline_tool_patterns(input);
        
        // Tool call is on its own line (after empty lines), should NOT be sanitized
        assert!(result.contains("{\"tool\":"), "Standalone after empty lines should not be sanitized");
    }

    /// Test whitespace-only lines
    #[test]
    fn test_whitespace_only_lines() {
        let input = "   \n  \n{\"tool\": \"shell\", \"args\": {}}";
        let result = sanitize_inline_tool_patterns(input);
        
        // Tool call is on its own line, should NOT be sanitized
        assert!(result.contains("{\"tool\":"), "Standalone after whitespace lines should not be sanitized");
    }

    /// Test tool call with leading whitespace (indented)
    #[test]
    fn test_indented_tool_call() {
        let input = "    {\"tool\": \"shell\", \"args\": {}}";
        let result = sanitize_inline_tool_patterns(input);
        
        // Indented but on its own line, should NOT be sanitized
        assert!(result.contains("{\"tool\":"), "Indented standalone should not be sanitized");
    }

    /// Test tool call with tabs
    #[test]
    fn test_tab_indented_tool_call() {
        let input = "\t{\"tool\": \"shell\", \"args\": {}}";
        let result = sanitize_inline_tool_patterns(input);
        
        // Tab-indented but on its own line, should NOT be sanitized
        assert!(result.contains("{\"tool\":"), "Tab-indented standalone should not be sanitized");
    }
}

// =============================================================================
// Test: Special characters and Unicode
// =============================================================================

mod unicode_handling {
    use super::*;

    /// Test tool pattern after emoji
    #[test]
    fn test_after_emoji() {
        let input = "🔧 {\"tool\": \"shell\"}";
        let result = sanitize_inline_tool_patterns(input);
        
        // Emoji before means it's inline, should be sanitized
        assert!(!result.contains("{\"tool\":"), "After emoji should be sanitized");
    }

    /// Test tool pattern after bullet point
    #[test]
    fn test_after_bullet() {
        let input = "• {\"tool\": \"shell\"}";
        let result = sanitize_inline_tool_patterns(input);
        
        // Bullet before means it's inline, should be sanitized
        assert!(!result.contains("{\"tool\":"), "After bullet should be sanitized");
    }

    /// Test tool pattern after CJK text
    #[test]
    fn test_after_cjk() {
        let input = "使用 {\"tool\": \"shell\"} 命令";
        let result = sanitize_inline_tool_patterns(input);
        
        // CJK text before means it's inline, should be sanitized
        assert!(!result.contains("{\"tool\":"), "After CJK should be sanitized");
    }

    /// Test tool pattern with Unicode in args (should still detect pattern)
    #[test]
    fn test_unicode_in_args() {
        let input = "Example: {\"tool\": \"shell\", \"args\": {\"command\": \"echo 你好\"}}";
        let result = sanitize_inline_tool_patterns(input);
        
        // Should be sanitized (inline)
        assert!(!result.contains("{\"tool\":"), "Unicode in args should still be detected");
    }
}

// =============================================================================
// Test: Multiple patterns on same line
// =============================================================================

mod multiple_patterns {
    use super::*;

    /// Test three tool patterns on one line
    #[test]
    fn test_three_patterns() {
        let input = "Compare {\"tool\": \"a\"} vs {\"tool\": \"b\"} vs {\"tool\": \"c\"}";
        let result = sanitize_inline_tool_patterns(input);
        
        // All should be sanitized
        assert!(!result.contains("{\"tool\":"), "All three should be sanitized");
    }

    /// Test mixed: one standalone, one inline
    #[test]
    fn test_mixed_standalone_and_inline() {
        let input = "Text with {\"tool\": \"inline\"} here\n{\"tool\": \"standalone\", \"args\": {}}";
        let result = sanitize_inline_tool_patterns(input);
        
        let lines: Vec<&str> = result.lines().collect();
        
        // First line should have sanitized pattern
        assert!(!lines[0].contains("{\"tool\":"), "Inline should be sanitized");
        
        // Second line should NOT be sanitized (standalone)
        assert!(lines[1].starts_with("{\"tool\":"), "Standalone should not be sanitized");
    }
}

// =============================================================================
// Test: Edge cases that should NOT trigger sanitization
// =============================================================================

mod no_sanitization_cases {
    use super::*;

    /// Test similar but not matching patterns
    #[test]
    fn test_similar_but_different() {
        let inputs = [
            "{\"tools\": \"value\"}",  // "tools" not "tool"
            "{\"Tool\": \"value\"}",  // Capital T
            "{\"TOOL\": \"value\"}",  // All caps
            "{'tool': 'value'}",       // Single quotes
        ];
        
        for input in inputs {
            let result = sanitize_inline_tool_patterns(input);
            assert_eq!(result, input, "'{}' should not be modified", input);
        }
    }

    /// Test partial patterns
    #[test]
    fn test_partial_patterns() {
        let inputs = [
            "{\"tool",           // No colon
            "\"tool\":",         // No opening brace
            "tool",              // Just the word
        ];
        
        for input in inputs {
            let result = sanitize_inline_tool_patterns(input);
            assert_eq!(result, input, "'{}' should not be modified", input);
        }
    }

    /// Test JSON that happens to have "tool" as a value
    #[test]
    fn test_tool_as_value() {
        let input = "{\"name\": \"tool\"}";
        let result = sanitize_inline_tool_patterns(input);
        assert_eq!(result, input, "'tool' as value should not trigger sanitization");
    }
}

// =============================================================================
// Test: Real-world scenarios from the bug report
// =============================================================================

mod real_world_scenarios {
    use super::*;

    /// Test documentation example that caused the original bug
    #[test]
    fn test_documentation_example() {
        let input = r#"To call a tool, use this format: {"tool": "name", "args": {...}}

For example:
{"tool": "shell", "args": {"command": "ls"}}

This will execute the command."#;
        
        let result = sanitize_inline_tool_patterns(input);
        let lines: Vec<&str> = result.lines().collect();
        
        // First line has inline pattern - should be sanitized
        assert!(!lines[0].contains("{\"tool\":"), "Inline in docs should be sanitized");
        
        // The standalone example should NOT be sanitized
        assert!(lines[3].starts_with("{\"tool\":"), "Standalone example should not be sanitized");
    }

    /// Test code example in prose
    #[test]
    fn test_code_in_prose() {
        let input = "The agent responds with {\"tool\": \"read_file\"} when it needs to read files.";
        let result = sanitize_inline_tool_patterns(input);
        
        assert!(!result.contains("{\"tool\":"), "Code in prose should be sanitized");
    }
}
