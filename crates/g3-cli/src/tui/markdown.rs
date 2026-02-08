//! Markdown parser for TUI rendering
//!
//! Converts markdown text into styled ratatui Lines with support for:
//! - Headers (H1-H4)
//! - Bold, italic, inline code
//! - Code blocks
//! - Ordered and unordered lists

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Parse markdown text into ratatui Lines with styling
pub fn parse_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    
    for line in text.lines() {
        // Handle code block markers
        if line.trim().starts_with("```") {
            in_code_block = !in_code_block;
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(40, 40, 40)),
            )));
            continue;
        }
        
        // Inside code block
        if in_code_block {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(40, 40, 40)),
            )));
            continue;
        }
        
        // Parse headers
        if let Some(stripped) = line.strip_prefix("#### ") {
            lines.push(Line::from(Span::styled(
                stripped.to_string(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(stripped) = line.strip_prefix("### ") {
            lines.push(Line::from(Span::styled(
                stripped.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(stripped) = line.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(
                stripped.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(stripped) = line.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(
                stripped.to_string(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        
        // Parse unordered lists
        if let Some(stripped) = line.trim_start().strip_prefix("- ") {
            let indent = line.len() - line.trim_start().len();
            let prefix = " ".repeat(indent) + "  • ";
            let content = prefix + stripped;
            lines.push(Line::from(parse_inline_styles(&content)));
            continue;
        }
        if let Some(stripped) = line.trim_start().strip_prefix("* ") {
            let indent = line.len() - line.trim_start().len();
            let prefix = " ".repeat(indent) + "  • ";
            let content = prefix + stripped;
            lines.push(Line::from(parse_inline_styles(&content)));
            continue;
        }
        
        // Parse ordered lists (1., 2., etc.)
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
            if let Some(stripped) = rest.strip_prefix(". ") {
                let indent = line.len() - trimmed.len();
                let number = &trimmed[..trimmed.len() - rest.len()];
                let prefix = " ".repeat(indent) + "  " + number + ". ";
                let content = prefix + stripped;
                lines.push(Line::from(parse_inline_styles(&content)));
                continue;
            }
        }
        
        // Regular text with inline styles
        lines.push(Line::from(parse_inline_styles(line)));
    }
    
    lines
}

/// Parse inline markdown styles within a single line
fn parse_inline_styles(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut current = String::new();
    
    while i < chars.len() {
        // Check for bold (**text**)
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            // Flush current plain text
            if !current.is_empty() {
                spans.push(Span::styled(
                    current.clone(),
                    Style::default().fg(Color::White),
                ));
                current.clear();
            }
            
            // Find closing **
            i += 2;
            let mut bold_text = String::new();
            let mut found_close = false;
            
            while i + 1 < chars.len() {
                if chars[i] == '*' && chars[i + 1] == '*' {
                    found_close = true;
                    i += 2;
                    break;
                }
                bold_text.push(chars[i]);
                i += 1;
            }
            
            if found_close {
                spans.push(Span::styled(
                    bold_text,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                // No closing **, treat as literal
                current.push_str("**");
                current.push_str(&bold_text);
            }
            continue;
        }
        
        // Check for italic (*text*)
        if chars[i] == '*' {
            // Flush current plain text
            if !current.is_empty() {
                spans.push(Span::styled(
                    current.clone(),
                    Style::default().fg(Color::White),
                ));
                current.clear();
            }
            
            // Find closing *
            i += 1;
            let mut italic_text = String::new();
            let mut found_close = false;
            
            while i < chars.len() {
                if chars[i] == '*' {
                    found_close = true;
                    i += 1;
                    break;
                }
                italic_text.push(chars[i]);
                i += 1;
            }
            
            if found_close {
                spans.push(Span::styled(
                    italic_text,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::ITALIC),
                ));
            } else {
                // No closing *, treat as literal
                current.push('*');
                current.push_str(&italic_text);
            }
            continue;
        }
        
        // Check for inline code (`code`)
        if chars[i] == '`' {
            // Flush current plain text
            if !current.is_empty() {
                spans.push(Span::styled(
                    current.clone(),
                    Style::default().fg(Color::White),
                ));
                current.clear();
            }
            
            // Find closing `
            i += 1;
            let mut code_text = String::new();
            let mut found_close = false;
            
            while i < chars.len() {
                if chars[i] == '`' {
                    found_close = true;
                    i += 1;
                    break;
                }
                code_text.push(chars[i]);
                i += 1;
            }
            
            if found_close {
                spans.push(Span::styled(
                    code_text,
                    Style::default().fg(Color::Rgb(216, 177, 114)),
                ));
            } else {
                // No closing `, treat as literal
                current.push('`');
                current.push_str(&code_text);
            }
            continue;
        }
        
        // Regular character
        current.push(chars[i]);
        i += 1;
    }
    
    // Flush remaining plain text
    if !current.is_empty() {
        spans.push(Span::styled(current, Style::default().fg(Color::White)));
    }
    
    // Return at least one span for empty lines
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), Style::default().fg(Color::White)));
    }
    
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let lines = parse_markdown("Hello world");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_headers() {
        let lines = parse_markdown("# Title\n## Subtitle\n### Section");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_bold_text() {
        let lines = parse_markdown("This is **bold** text");
        assert_eq!(lines.len(), 1);
        // Should have 3 spans: "This is ", "bold", " text"
        assert_eq!(lines[0].spans.len(), 3);
    }

    #[test]
    fn test_code_block() {
        let lines = parse_markdown("```\nfn main() {}\n```");
        assert_eq!(lines.len(), 3); // opening, code, closing
    }

    #[test]
    fn test_unordered_list() {
        let lines = parse_markdown("- item 1\n- item 2");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_ordered_list() {
        let lines = parse_markdown("1. first\n2. second");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_inline_code() {
        let lines = parse_markdown("Use `cargo build` to compile");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_empty_input() {
        let lines = parse_markdown("");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_multiline() {
        let lines = parse_markdown("Line 1\nLine 2\nLine 3");
        assert_eq!(lines.len(), 3);
    }
    
    #[test]
    fn test_italic_text() {
        let lines = parse_markdown("This is *italic* text");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 3);
    }
    
    #[test]
    fn test_multiple_inline_styles() {
        let lines = parse_markdown("**bold** and *italic* and `code`");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].spans.len() >= 5); // bold, space, italic, space, code
    }
    
    #[test]
    fn test_h4_header() {
        let lines = parse_markdown("#### Small Header");
        assert_eq!(lines.len(), 1);
    }
    
    #[test]
    fn test_asterisk_list() {
        let lines = parse_markdown("* item with asterisk");
        assert_eq!(lines.len(), 1);
    }
}
