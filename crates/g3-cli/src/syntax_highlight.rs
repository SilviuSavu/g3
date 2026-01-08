//! Syntax highlighting for code blocks using syntect.
//!
//! This module provides functionality to extract code blocks from markdown,
//! apply syntax highlighting using syntect, and return the highlighted output
//! while leaving the rest of the markdown intact.

use once_cell::sync::Lazy;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

/// Lazily loaded syntax set with default syntaxes.
static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);

/// Lazily loaded theme set with default themes.
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

/// A segment of markdown content - either plain text or a code block.
#[derive(Debug)]
enum MarkdownSegment<'a> {
    /// Plain markdown text (not a code block)
    Text(&'a str),
    /// A fenced code block with optional language and content
    CodeBlock { lang: Option<&'a str>, code: &'a str },
}

/// Parse markdown into segments of text and code blocks.
fn parse_markdown_segments(markdown: &str) -> Vec<MarkdownSegment<'_>> {
    let mut segments = Vec::new();
    let mut remaining = markdown;

    while !remaining.is_empty() {
        // Look for the start of a code block (``` at start of line or after newline)
        if let Some(fence_start) = find_code_fence_start(remaining) {
            // Add any text before the fence
            if fence_start > 0 {
                segments.push(MarkdownSegment::Text(&remaining[..fence_start]));
            }

            // Parse the code block
            let after_fence = &remaining[fence_start..];
            if let Some((lang, code, end_pos)) = parse_code_block(after_fence) {
                segments.push(MarkdownSegment::CodeBlock { lang, code });
                remaining = &after_fence[end_pos..];
            } else {
                // Malformed fence - treat as text and continue
                segments.push(MarkdownSegment::Text(&remaining[..fence_start + 3]));
                remaining = &remaining[fence_start + 3..];
            }
        } else {
            // No more code blocks - rest is plain text
            segments.push(MarkdownSegment::Text(remaining));
            break;
        }
    }

    segments
}

/// Find the start position of a code fence (```) that begins a line.
fn find_code_fence_start(text: &str) -> Option<usize> {
    let mut pos = 0;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            // Return position at start of the ``` (after any leading whitespace on line)
            let whitespace_len = line.len() - trimmed.len();
            return Some(pos + whitespace_len);
        }
        pos += line.len() + 1; // +1 for newline
    }
    None
}

/// Parse a code block starting at the opening fence.
/// Returns (language, code_content, end_position_after_closing_fence).
fn parse_code_block(text: &str) -> Option<(Option<&str>, &str, usize)> {
    // text starts with ```
    let first_line_end = text.find('\n')?;
    let first_line = &text[3..first_line_end].trim();

    // Extract language (if any)
    let lang = if first_line.is_empty() {
        None
    } else {
        // Language is the first word on the line
        let lang_str = first_line.split_whitespace().next().unwrap_or(*first_line);
        Some(lang_str)
    };

    // Find the closing fence
    let code_start = first_line_end + 1;
    let after_opening = &text[code_start..];

    // Look for closing ``` at start of a line
    let mut search_pos = 0;
    for line in after_opening.lines() {
        if line.trim_start().starts_with("```") {
            // Found closing fence
            let code = &after_opening[..search_pos];
            let closing_fence_end = search_pos + line.len();
            // Include the newline after closing fence if present
            let total_end = if after_opening.len() > closing_fence_end
                && after_opening.as_bytes().get(closing_fence_end) == Some(&b'\n')
            {
                code_start + closing_fence_end + 1
            } else {
                code_start + closing_fence_end
            };
            return Some((lang, code, total_end));
        }
        search_pos += line.len() + 1; // +1 for newline
    }

    // No closing fence found - treat entire rest as code
    Some((lang, after_opening, text.len()))
}

/// Highlight a code block with the given language.
fn highlight_code(code: &str, lang: Option<&str>) -> String {
    let syntax = lang
        .and_then(|l| SYNTAX_SET.find_syntax_by_token(l))
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    // Use a dark theme suitable for terminals
    let theme = &THEME_SET.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut output = String::new();

    for line in LinesWithEndings::from(code) {
        match highlighter.highlight_line(line, &SYNTAX_SET) {
            Ok(ranges) => {
                let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
                output.push_str(&escaped);
            }
            Err(_) => {
                // Fallback: just append the line without highlighting
                output.push_str(line);
            }
        }
    }

    // Reset terminal colors at the end
    output.push_str("\x1b[0m");
    output
}

/// Render markdown with syntax-highlighted code blocks.
///
/// This function:
/// 1. Parses the markdown to find code blocks
/// 2. Applies syntect highlighting to code blocks
/// 3. Renders non-code portions with termimad
/// 4. Combines everything into the final output
pub fn render_markdown_with_highlighting(markdown: &str, skin: &termimad::MadSkin) -> String {
    let segments = parse_markdown_segments(markdown);
    let mut output = String::new();

    for segment in segments {
        match segment {
            MarkdownSegment::Text(text) => {
                if !text.is_empty() {
                    // Render with termimad
                    let rendered = skin.term_text(text);
                    output.push_str(&format!("{}", rendered));
                }
            }
            MarkdownSegment::CodeBlock { lang, code } => {
                // Add a subtle header showing the language
                if let Some(l) = lang {
                    output.push_str(&format!("\x1b[2;3m{}\x1b[0m\n", l));
                }
                // Highlight and append the code
                let highlighted = highlight_code(code, lang);
                output.push_str(&highlighted);
                // Ensure we end with a newline
                if !output.ends_with('\n') {
                    output.push('\n');
                }
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_code_block() {
        let md = "Some text\n```rust\nfn main() {}\n```\nMore text";
        let segments = parse_markdown_segments(md);

        assert_eq!(segments.len(), 3);
        assert!(matches!(segments[0], MarkdownSegment::Text("Some text\n")));
        assert!(matches!(
            segments[1],
            MarkdownSegment::CodeBlock {
                lang: Some("rust"),
                code: "fn main() {}\n"
            }
        ));
        assert!(matches!(segments[2], MarkdownSegment::Text("More text")));
    }

    #[test]
    fn test_parse_no_language() {
        let md = "```\nplain code\n```";
        let segments = parse_markdown_segments(md);

        assert_eq!(segments.len(), 1);
        assert!(matches!(
            segments[0],
            MarkdownSegment::CodeBlock {
                lang: None,
                code: "plain code\n"
            }
        ));
    }

    #[test]
    fn test_highlight_rust_code() {
        let code = "fn main() {\n    println!(\"Hello\");\n}\n";
        let highlighted = highlight_code(code, Some("rust"));

        // Should contain ANSI escape codes
        assert!(highlighted.contains("\x1b["));
        // Should end with reset
        assert!(highlighted.ends_with("\x1b[0m"));
    }

    #[test]
    fn test_no_code_blocks() {
        let md = "Just plain markdown with **bold** and *italic*.";
        let segments = parse_markdown_segments(md);

        assert_eq!(segments.len(), 1);
        assert!(matches!(segments[0], MarkdownSegment::Text(_)));
    }
}
