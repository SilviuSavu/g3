//! Subagent panel renderer for the right-side split pane.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::subagent_monitor::{AgentStatus, SubagentEntry};

/// Render the subagent panel in the given area.
pub fn render_subagent_panel(
    frame: &mut Frame,
    area: Rect,
    entries: &[SubagentEntry],
    focused: bool,
    scroll_offset: usize,
) {
    let active_count = entries
        .iter()
        .filter(|e| e.status == AgentStatus::Running)
        .count();

    let border_color = if focused {
        Color::Magenta
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" Subagents ({} active) ", active_count),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));

    if entries.is_empty() {
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "No subagents",
            Style::default().fg(Color::DarkGray),
        )))
        .block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    for entry in entries.iter().skip(scroll_offset) {
        // Status icon + agent ID + model
        let status_icon = match entry.status {
            AgentStatus::Running => Span::styled("■ ", Style::default().fg(Color::Green)),
            AgentStatus::Idle => Span::styled("◐ ", Style::default().fg(Color::Yellow)),
            AgentStatus::Complete => Span::styled("□ ", Style::default().fg(Color::DarkGray)),
            AgentStatus::Failed => Span::styled("⊘ ", Style::default().fg(Color::Red)),
        };

        let agent_id_span = Span::styled(
            entry.agent_id.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

        let model_span = Span::styled(
            format!("  {}", entry.model),
            Style::default().fg(Color::Cyan),
        );

        lines.push(Line::from(vec![
            Span::raw(" "),
            status_icon,
            agent_id_span,
            model_span,
        ]));

        // Context gauge line
        let gauge = render_context_gauge(entry.context_pct);
        lines.push(Line::from(vec![
            Span::raw("   ctx "),
            Span::styled(
                format!("{:2.0}% ", entry.context_pct),
                Style::default().fg(context_gauge_color(entry.context_pct)),
            ),
            Span::styled(gauge, Style::default().fg(context_gauge_color(entry.context_pct))),
        ]));

        // Current tool or idle
        let tool_line = match &entry.last_tool {
            Some(tool) if entry.status == AgentStatus::Running => {
                Span::styled(format!("   ● {}", tool), Style::default().fg(Color::Blue))
            }
            _ if entry.status == AgentStatus::Running => {
                Span::styled("   idle", Style::default().fg(Color::DarkGray))
            }
            _ if entry.status == AgentStatus::Complete => {
                Span::styled("   done", Style::default().fg(Color::DarkGray))
            }
            _ => Span::styled("   pending", Style::default().fg(Color::DarkGray)),
        };
        lines.push(Line::from(tool_line));

        // Separator
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

/// Render a context gauge using Unicode block chars.
/// Returns a string like `████░░░░░░` (10 chars wide).
fn render_context_gauge(pct: f32) -> String {
    let filled = ((pct / 100.0) * 10.0).round() as usize;
    let filled = filled.min(10);
    let empty = 10 - filled;
    format!(
        "{}{}",
        "█".repeat(filled),
        "░".repeat(empty),
    )
}

fn context_gauge_color(pct: f32) -> Color {
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
    fn test_render_context_gauge_zero() {
        let gauge = render_context_gauge(0.0);
        assert_eq!(gauge, "░░░░░░░░░░");
    }

    #[test]
    fn test_render_context_gauge_50() {
        let gauge = render_context_gauge(50.0);
        assert_eq!(gauge, "█████░░░░░");
    }

    #[test]
    fn test_render_context_gauge_100() {
        let gauge = render_context_gauge(100.0);
        assert_eq!(gauge, "██████████");
    }

    #[test]
    fn test_context_gauge_color_low() {
        assert_eq!(context_gauge_color(30.0), Color::Green);
    }

    #[test]
    fn test_context_gauge_color_medium() {
        assert_eq!(context_gauge_color(60.0), Color::Yellow);
    }

    #[test]
    fn test_context_gauge_color_high() {
        assert_eq!(context_gauge_color(90.0), Color::Red);
    }
}
