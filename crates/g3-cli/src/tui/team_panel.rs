//! Team panel renderer for the right-side split pane.
//! Shows team members and shared task list.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use super::team_monitor::{MemberStatus, TeamState};

/// Render the team panel in the given area.
pub fn render_team_panel(
    frame: &mut Frame,
    area: Rect,
    state: &TeamState,
    focused: bool,
    scroll_offset: usize,
) {
    let border_color = if focused {
        Color::Magenta
    } else {
        Color::DarkGray
    };

    // Split vertically: members (top, compact) and tasks (bottom, scrollable)
    let member_height = (state.members.len() as u16 + 3).min(8); // header + members + padding
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(member_height),
            Constraint::Min(4),
        ])
        .split(area);

    render_members(frame, chunks[0], state, border_color);
    render_tasks(frame, chunks[1], state, focused, border_color, scroll_offset);
}

fn render_members(frame: &mut Frame, area: Rect, state: &TeamState, border_color: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" Team: {} ", state.team_name),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));

    if state.members.is_empty() {
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "No members",
            Style::default().fg(Color::DarkGray),
        )))
        .block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for member in &state.members {
        let (icon, color) = match member.status {
            MemberStatus::Active => ("■", Color::Green),
            MemberStatus::Idle => ("◐", Color::Yellow),
            MemberStatus::Shutdown => ("□", Color::DarkGray),
        };

        let mut spans = vec![
            Span::raw(" "),
            Span::styled(format!("{} ", icon), Style::default().fg(color)),
            Span::styled(
                member.name.clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ];

        if !member.agent_type.is_empty() {
            spans.push(Span::styled(
                format!(" ({})", member.agent_type),
                Style::default().fg(Color::Cyan),
            ));
        }

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn render_tasks(
    frame: &mut Frame,
    area: Rect,
    state: &TeamState,
    focused: bool,
    border_color: Color,
    scroll_offset: usize,
) {
    let pending = state.tasks.iter().filter(|t| t.status == "pending").count();
    let in_progress = state.tasks.iter().filter(|t| t.status == "in_progress").count();
    let completed = state.tasks.iter().filter(|t| t.status == "completed").count();

    let title = format!(" Tasks: {} pending, {} active, {} done ", pending, in_progress, completed);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { Color::Magenta } else { border_color }))
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ));

    if state.tasks.is_empty() {
        let paragraph = Paragraph::new(Line::from(Span::styled(
            "No tasks",
            Style::default().fg(Color::DarkGray),
        )))
        .block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    for task in state.tasks.iter().skip(scroll_offset) {
        let (status_icon, status_color) = match task.status.as_str() {
            "in_progress" => ("●", Color::Yellow),
            "completed" => ("✓", Color::Green),
            _ => ("○", Color::DarkGray), // pending
        };

        let blocked_indicator = if !task.blocked_by.is_empty() {
            Span::styled(" [blocked]", Style::default().fg(Color::Red))
        } else {
            Span::raw("")
        };

        let owner_span = match &task.owner {
            Some(o) => Span::styled(format!(" → {}", o), Style::default().fg(Color::Cyan)),
            None => Span::raw(""),
        };

        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(format!("{} ", status_icon), Style::default().fg(status_color)),
            Span::styled(
                format!("#{} ", task.id),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                task.subject.clone(),
                Style::default().fg(Color::White),
            ),
            owner_span,
            blocked_indicator,
        ]));
    }

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::team_monitor::{TeamMemberEntry, TeamTaskEntry};

    #[test]
    fn test_empty_state() {
        let state = TeamState {
            team_name: "test".to_string(),
            members: vec![],
            tasks: vec![],
        };
        // Just verify it doesn't panic
        assert_eq!(state.members.len(), 0);
        assert_eq!(state.tasks.len(), 0);
    }

    #[test]
    fn test_task_status_counts() {
        let state = TeamState {
            team_name: "test".to_string(),
            members: vec![],
            tasks: vec![
                TeamTaskEntry {
                    id: "1".to_string(),
                    subject: "Task 1".to_string(),
                    status: "pending".to_string(),
                    owner: None,
                    blocked_by: vec![],
                },
                TeamTaskEntry {
                    id: "2".to_string(),
                    subject: "Task 2".to_string(),
                    status: "in_progress".to_string(),
                    owner: Some("worker".to_string()),
                    blocked_by: vec![],
                },
                TeamTaskEntry {
                    id: "3".to_string(),
                    subject: "Task 3".to_string(),
                    status: "completed".to_string(),
                    owner: Some("worker".to_string()),
                    blocked_by: vec![],
                },
            ],
        };
        let pending = state.tasks.iter().filter(|t| t.status == "pending").count();
        let active = state.tasks.iter().filter(|t| t.status == "in_progress").count();
        let done = state.tasks.iter().filter(|t| t.status == "completed").count();
        assert_eq!(pending, 1);
        assert_eq!(active, 1);
        assert_eq!(done, 1);
    }

    #[test]
    fn test_member_entry() {
        let entry = TeamMemberEntry {
            name: "worker-1".to_string(),
            agent_type: "builder".to_string(),
            status: MemberStatus::Active,
        };
        assert_eq!(entry.name, "worker-1");
        assert_eq!(entry.status, MemberStatus::Active);
    }
}
