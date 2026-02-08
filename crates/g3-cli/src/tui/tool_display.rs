use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Render a compact single-line format for a tool call
/// Format: `  ● name | path | summary | tokens ◉ duration`
pub fn render_tool_compact(
    name: &str,
    path: &str,
    summary: &str,
    tokens: u32,
    duration_secs: f64,
) -> Vec<Line<'static>> {
    let mut spans = vec![
        Span::raw("  "),
        Span::styled("●", Style::default().fg(Color::Blue)),
        Span::raw(" "),
        Span::styled(
            name.to_string(),
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        ),
    ];

    // Add path if not empty
    if !path.is_empty() {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            path.to_string(),
            Style::default().fg(Color::White),
        ));
    }

    // Add summary if not empty
    if !summary.is_empty() {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            summary.to_string(),
            Style::default().fg(Color::Cyan),
        ));
    }

    // Add tokens and duration
    spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(
        format!("{}", tokens),
        Style::default().fg(Color::DarkGray),
    ));
    spans.push(Span::styled(" ◉ ", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(
        format_duration(duration_secs),
        Style::default().fg(duration_color(duration_secs)),
    ));

    vec![Line::from(spans)]
}

/// Render verbose header with box drawing
/// Format: `  ┌─ name | path`
pub fn render_tool_verbose_header(name: &str, path: &str) -> Line<'static> {
    let mut spans = vec![
        Span::styled("  ┌─ ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            name.to_string(),
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        ),
    ];

    if !path.is_empty() {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            path.to_string(),
            Style::default().fg(Color::White),
        ));
    }

    Line::from(spans)
}

/// Render a verbose content line
/// Format: `  │ text`
pub fn render_tool_verbose_line(text: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
        Span::styled(text.to_string(), Style::default().fg(Color::White)),
    ])
}

/// Render verbose footer with metrics
/// Format: `  └─ tokens ◉ duration | ctx%`
pub fn render_tool_verbose_footer(tokens: u32, duration_secs: f64, ctx_pct: f32) -> Line<'static> {
    Line::from(vec![
        Span::styled("  └─ ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", tokens),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(" ◉ ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format_duration(duration_secs),
            Style::default().fg(duration_color(duration_secs)),
        ),
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.1}%", ctx_pct),
            Style::default().fg(context_color(ctx_pct)),
        ),
    ])
}

/// Get color based on duration
pub fn duration_color(duration_secs: f64) -> Color {
    if duration_secs < 1.0 {
        Color::White
    } else if duration_secs < 60.0 {
        Color::Yellow
    } else if duration_secs < 300.0 {
        Color::Rgb(208, 135, 112) // orange
    } else {
        Color::Red
    }
}

/// Format duration as human-readable string
pub fn format_duration(secs: f64) -> String {
    if secs < 0.1 {
        "0.0s".to_string()
    } else if secs < 60.0 {
        format!("{:.1}s", secs)
    } else if secs < 3600.0 {
        let minutes = (secs / 60.0).floor() as u32;
        let seconds = (secs % 60.0).floor() as u32;
        format!("{}m {}s", minutes, seconds)
    } else {
        let hours = (secs / 3600.0).floor() as u32;
        let minutes = ((secs % 3600.0) / 60.0).floor() as u32;
        format!("{}h {}m", hours, minutes)
    }
}

/// Get color based on context percentage
pub fn context_color(pct: f32) -> Color {
    if pct < 50.0 {
        Color::Green
    } else if pct < 80.0 {
        Color::Yellow
    } else {
        Color::Red
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_subsecond() {
        assert_eq!(format_duration(0.3), "0.3s");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(2.1), "2.1s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(83.0), "1m 23s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(3900.0), "1h 5m");
    }

    #[test]
    fn test_duration_color_fast() {
        assert_eq!(duration_color(0.5), Color::White);
    }

    #[test]
    fn test_duration_color_medium() {
        assert_eq!(duration_color(30.0), Color::Yellow);
    }

    #[test]
    fn test_duration_color_slow() {
        assert_eq!(duration_color(400.0), Color::Red);
    }

    #[test]
    fn test_compact_with_all_fields() {
        let lines = render_tool_compact("read_file", "./src/main.rs", "(45 lines)", 234, 1.2);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_compact_empty_path() {
        let lines = render_tool_compact("remember", "", "saved", 50, 0.3);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_verbose_header() {
        let line = render_tool_verbose_header("read_file", "./src/auth.rs");
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_verbose_footer() {
        let line = render_tool_verbose_footer(234, 1.2, 45.0);
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_context_color() {
        assert_eq!(context_color(30.0), Color::Green);
        assert_eq!(context_color(60.0), Color::Yellow);
        assert_eq!(context_color(90.0), Color::Red);
    }
}
