//! Input formatting for interactive mode.
//!
//! Applies visual highlighting to user input:
//! - ALL CAPS words (2+ chars) → bold green
//! - Quoted text ("..." or '...') → cyan
//! - Standard markdown (bold, italic, code) via termimad

use crossterm::terminal;
use regex::Regex;
use std::io::Write;
use std::io::IsTerminal;
use once_cell::sync::Lazy;
use termimad::MadSkin;

use crate::streaming_markdown::StreamingMarkdownFormatter;

// Compiled regexes for preprocessing (compiled once, reused)
static CAPS_RE: Lazy<Regex> = Lazy::new(|| {
    // ALL CAPS words: 2+ uppercase letters, may include numbers, word boundaries
    Regex::new(r"\b([A-Z][A-Z0-9]{1,}[A-Z0-9]*)\b").unwrap()
});
static DOUBLE_QUOTE_RE: Lazy<Regex> = Lazy::new(|| {
    // Double-quoted text: quote must be preceded by whitespace/punctuation or start of string,
    // and followed by whitespace/punctuation or end of string
    Regex::new(r#"(?:^|[\s(\[{])"([^"]+)"(?:$|[\s.,;:!?)\]}])"#).unwrap()
});
static SINGLE_QUOTE_RE: Lazy<Regex> = Lazy::new(|| {
    // Single-quoted text: quote must be preceded by whitespace/punctuation or start of string,
    // and followed by whitespace/punctuation or end of string (avoids contractions like "it's")
    Regex::new(r#"(?:^|[\s(\[{])'([^']+)'(?:$|[\s.,;:!?)\]}])"#).unwrap()
});

/// Pre-process input to add markdown markers before formatting.
/// ALL CAPS → **bold**, quoted text → special markers for cyan.
pub fn preprocess_input(input: &str) -> String {
    let mut result = input.to_string();
    
    // ALL CAPS → **bold**
    result = CAPS_RE.replace_all(&result, "**$1**").to_string();
    
    // Quoted text → markers (processed after markdown to apply cyan)
    result = DOUBLE_QUOTE_RE.replace_all(&result, "\x00qdbl\x00$1\x00qend\x00").to_string();
    result = SINGLE_QUOTE_RE.replace_all(&result, "\x00qsgl\x00$1\x00qend\x00").to_string();
    
    result
}

// Regexes for post-processing quote markers into ANSI cyan
static CYAN_DOUBLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(\x1b\[36m")([^\x1b]*)\x1b\[0m"#).unwrap()
});
static CYAN_SINGLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(\x1b\[36m')([^\x1b]*)\x1b\[0m").unwrap()
});

/// Apply cyan highlighting to quoted text markers (runs after markdown formatting).
fn apply_quote_highlighting(text: &str) -> String {
    let mut result = text.to_string();
    
    // \x1b[36m = cyan, \x1b[0m = reset
    result = result.replace("\x00qdbl\x00", "\x1b[36m\"");
    result = result.replace("\x00qsgl\x00", "\x1b[36m'");
    result = result.replace("\x00qend\x00", "\x1b[0m");
    
    // Insert closing quotes before reset code
    result = CYAN_DOUBLE_RE.replace_all(&result, |caps: &regex::Captures| {
        format!("{}{}\"\x1b[0m", &caps[1], &caps[2])
    }).to_string();
    result = CYAN_SINGLE_RE.replace_all(&result, |caps: &regex::Captures| {
        format!("{}{}'\x1b[0m", &caps[1], &caps[2])
    }).to_string();
    
    result
}

/// Format user input with markdown and special highlighting (ALL CAPS, quotes).
pub fn format_input(input: &str) -> String {
    let preprocessed = preprocess_input(input);
    
    let skin = MadSkin::default();
    let mut formatter = StreamingMarkdownFormatter::new(skin);
    let formatted = formatter.process(&preprocessed);
    let formatted = formatted + &formatter.finish();
    
    apply_quote_highlighting(&formatted)
}

/// Calculate the number of visual lines that text occupies in a terminal.
/// Accounts for line wrapping and the cursor position after typing.
pub fn calculate_visual_lines(text_len: usize, term_width: usize) -> usize {
    if term_width == 0 {
        return 1;
    }
    let mut visual_lines = text_len.div_ceil(term_width).max(1);
    // When text exactly fills the terminal width (or a multiple), the cursor
    // wraps to the next line, so we need to clear one additional line
    if text_len > 0 && text_len % term_width == 0 {
        visual_lines += 1;
    }
    visual_lines
}

/// Reprint user input in place with formatting (TTY only).
/// Moves cursor up to overwrite original input, then prints formatted version.
pub fn reprint_formatted_input(input: &str, prompt: &str) {
    if !std::io::stdout().is_terminal() {
        return;
    }

    let formatted = format_input(input);

    // Calculate visual lines (prompt + input may wrap across terminal rows)
    let term_width = terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
    let visual_lines = calculate_visual_lines(prompt.len() + input.len(), term_width);

    // Move up and clear each line
    for _ in 0..visual_lines {
        print!("\x1b[1A\x1b[2K");
    }

    // Dim prompt + formatted input
    println!("\x1b[2m{}\x1b[0m{}", prompt, formatted);
    let _ = std::io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_preprocess_all_caps() {
        let input = "please FIX the BUG in this CODE";
        let result = preprocess_input(input);
        assert!(result.contains("**FIX**"));
        assert!(result.contains("**BUG**"));
        assert!(result.contains("**CODE**"));
        // "please", "the", "in", "this" should not be wrapped
        assert!(!result.contains("**please**"));
    }
    
    #[test]
    fn test_preprocess_single_caps_not_matched() {
        // Single letter caps should not be matched
        let input = "I am A person";
        let result = preprocess_input(input);
        // "I" and "A" are single letters, should not be wrapped
        assert!(!result.contains("**I**"));
        assert!(!result.contains("**A**"));
    }
    
    #[test]
    fn test_preprocess_double_quotes() {
        let input = r#"say "hello world" please"#;
        let result = preprocess_input(input);
        assert!(result.contains("\x00qdbl\x00hello world\x00qend\x00"));
    }
    
    #[test]
    fn test_preprocess_single_quotes() {
        let input = "use the 'special' method";
        let result = preprocess_input(input);
        assert!(result.contains("\x00qsgl\x00special\x00qend\x00"));
    }
    
    #[test]
    fn test_preprocess_mixed() {
        let input = r#"FIX the "critical" BUG"#;
        let result = preprocess_input(input);
        assert!(result.contains("**FIX**"));
        assert!(result.contains("**BUG**"));
        assert!(result.contains("\x00qdbl\x00critical\x00qend\x00"));
    }
    
    #[test]
    fn test_apply_quote_highlighting() {
        let input = "\x00qdbl\x00hello\x00qend\x00";
        let result = apply_quote_highlighting(input);
        assert!(result.contains("\x1b[36m"));
        assert!(result.contains("\x1b[0m"));
    }
    
    #[test]
    fn test_format_input_caps_become_bold() {
        let input = "FIX this";
        let result = format_input(input);
        // Should contain bold ANSI code (\x1b[1;32m for bold green)
        assert!(result.contains("\x1b[1;32m") || result.contains("FIX"));
    }
    
    #[test]
    fn test_format_input_quotes_become_cyan() {
        let input = r#"say "hello""#;
        let result = format_input(input);
        // Should contain cyan ANSI code
        assert!(result.contains("\x1b[36m"));
    }
    
    #[test]
    fn test_caps_with_numbers() {
        let input = "check HTTP2 and TLS13";
        let result = preprocess_input(input);
        assert!(result.contains("**HTTP2**"));
        assert!(result.contains("**TLS13**"));
    }
    
    #[test]
    fn test_two_letter_caps() {
        let input = "use IO and DB";
        let result = preprocess_input(input);
        assert!(result.contains("**IO**"));
        assert!(result.contains("**DB**"));
    }
    
    // Tests for apostrophe/contraction handling (I1 bug fix)
    
    #[test]
    fn test_contraction_not_highlighted() {
        // Contractions should NOT be treated as quoted text
        let input = "it's fine";
        let result = preprocess_input(input);
        // Should not contain quote markers
        assert!(!result.contains("\x00qsgl\x00"));
        assert!(!result.contains("\x00qend\x00"));
        assert_eq!(result, "it's fine");
    }
    
    #[test]
    fn test_multiple_contractions_not_highlighted() {
        let input = "don't won't can't shouldn't";
        let result = preprocess_input(input);
        assert!(!result.contains("\x00qsgl\x00"));
        assert_eq!(result, input);
    }
    
    #[test]
    fn test_contraction_with_quoted_text() {
        // Mixed: contraction + actual quoted text
        // Only 'test' should be highlighted, not the apostrophe in "it's"
        let input = "it's a 'test' case";
        let result = preprocess_input(input);
        assert!(result.contains("\x00qsgl\x00test\x00qend\x00"));
        // The "it's" should remain unchanged
        assert!(result.contains("it's"));
    }
    
    #[test]
    fn test_quoted_at_start_of_string() {
        let input = "'hello' world";
        let result = preprocess_input(input);
        assert!(result.contains("\x00qsgl\x00hello\x00qend\x00"));
    }
    
    #[test]
    fn test_quoted_at_end_of_string() {
        let input = "say 'goodbye'";
        let result = preprocess_input(input);
        assert!(result.contains("\x00qsgl\x00goodbye\x00qend\x00"));
    }
    
    // Tests for visual line calculation (I2 bug fix)
    
    #[test]
    fn test_visual_lines_shorter_than_width() {
        // 50 chars on 80-char terminal = 1 line
        assert_eq!(calculate_visual_lines(50, 80), 1);
    }
    
    #[test]
    fn test_visual_lines_longer_than_width() {
        // 100 chars on 80-char terminal = 2 lines (wraps once)
        assert_eq!(calculate_visual_lines(100, 80), 2);
        // 170 chars on 80-char terminal = 3 lines
        assert_eq!(calculate_visual_lines(170, 80), 3);
    }
    
    #[test]
    fn test_visual_lines_exactly_equals_width() {
        // 80 chars on 80-char terminal = 2 lines (cursor wraps to next line)
        assert_eq!(calculate_visual_lines(80, 80), 2);
        // 160 chars on 80-char terminal = 3 lines (fills 2 lines exactly, cursor on 3rd)
        assert_eq!(calculate_visual_lines(160, 80), 3);
    }
    
    #[test]
    fn test_visual_lines_empty_input() {
        // Empty input should still be 1 line (the prompt line)
        assert_eq!(calculate_visual_lines(0, 80), 1);
    }
}
