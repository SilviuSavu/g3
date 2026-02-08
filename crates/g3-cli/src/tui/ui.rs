//! TUI rendering functions using ratatui.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{ChatContent, ChatMessage, MessageRole, Pane, PendingPrompt};
use crate::tui::markdown;
use crate::tui::subagent_monitor::SubagentEntry;
use crate::tui::subagent_panel;
use crate::tui::tool_display;

/// Color palette for the TUI.
#[derive(Clone)]
pub struct Colors {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub text: Color,
    pub error: Color,
    pub success: Color,
    pub user: Color,
    pub assistant: Color,
    pub tool: Color,
}

impl Default for Colors {
    fn default() -> Self {
        Colors {
            primary: Color::Cyan,
            secondary: Color::DarkGray,
            accent: Color::Magenta,
            text: Color::White,
            error: Color::Red,
            success: Color::Green,
            user: Color::Yellow,
            assistant: Color::Cyan,
            tool: Color::Blue,
        }
    }
}

/// A read-only view of app state for rendering.
/// Avoids borrow checker issues with terminal.draw() taking &mut self.
pub struct AppView<'a> {
    pub colors: &'a Colors,
    pub messages: &'a [ChatMessage],
    pub input_buffer: &'a str,
    pub cursor_position: usize,
    pub context_percentage: f32,
    pub current_tool: &'a Option<String>,
    pub scroll_offset: u16,
    pub pending_prompt: &'a Option<PendingPrompt>,
    pub active_pane: &'a Pane,
    pub split_ratio: f32,
    pub subagent_entries: &'a [SubagentEntry],
    pub subagent_scroll: usize,
    pub model_name: &'a str,
    pub cost_dollars: f64,
    pub is_thinking: bool,
}

/// Render the entire TUI frame.
pub fn render(frame: &mut Frame, app: &AppView) {
    let size = frame.area();

    // Responsive: split layout if wide enough and subagents exist
    if size.width >= 120 && !app.subagent_entries.is_empty() {
        let left_pct = (app.split_ratio * 100.0) as u16;
        let right_pct = 100 - left_pct;
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(left_pct),
                Constraint::Percentage(right_pct),
            ])
            .split(size);

        render_main_pane(frame, horizontal[0], app);
        subagent_panel::render_subagent_panel(
            frame,
            horizontal[1],
            app.subagent_entries,
            *app.active_pane == Pane::Subagent,
            app.subagent_scroll,
        );
    } else {
        render_main_pane(frame, size, app);
    }

    if app.pending_prompt.is_some() {
        render_prompt_overlay(frame, frame.area(), app, app.colors);
    }
}

/// Render the main pane (chat + status bar + input).
fn render_main_pane(frame: &mut Frame, area: Rect, app: &AppView) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(area);

    render_chat(frame, chunks[0], app, app.colors);
    render_context_bar(frame, chunks[1], app, app.colors);
    render_input_box(frame, chunks[2], app, app.colors);
}

fn render_chat(frame: &mut Frame, area: Rect, app: &AppView, colors: &Colors) {
    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default());

    if app.messages.is_empty() {
        let welcome = Paragraph::new(Text::from(vec![
            Line::from(""),
            Line::from(Span::styled(
                "g3 TUI",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Type a message and press Enter to start.",
                Style::default().fg(colors.secondary),
            )),
        ]))
        .alignment(Alignment::Center)
        .block(block);
        frame.render_widget(welcome, area);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for msg in app.messages {
        match msg.role {
            MessageRole::User => {
                let text = msg.content.as_text();
                lines.push(Line::from(vec![
                    Span::styled(
                        "You: ",
                        Style::default()
                            .fg(colors.user)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(text.to_string(), Style::default().fg(colors.text)),
                ]));
            }
            MessageRole::Assistant => {
                let text = msg.content.as_text();
                if text.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "g3: ",
                            Style::default()
                                .fg(colors.assistant)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("...", Style::default().fg(colors.secondary)),
                    ]));
                } else {
                    // Parse markdown for assistant messages
                    let md_lines = markdown::parse_markdown(text);
                    for (i, md_line) in md_lines.into_iter().enumerate() {
                        if i == 0 {
                            // Prepend "g3: " to first line
                            let mut spans = vec![Span::styled(
                                "g3: ",
                                Style::default()
                                    .fg(colors.assistant)
                                    .add_modifier(Modifier::BOLD),
                            )];
                            spans.extend(md_line.spans);
                            lines.push(Line::from(spans));
                        } else {
                            // Indent continuation lines
                            let mut spans = vec![Span::raw("    ")];
                            spans.extend(md_line.spans);
                            lines.push(Line::from(spans));
                        }
                    }
                }
            }
            MessageRole::Tool => {
                match &msg.content {
                    ChatContent::ToolCompact {
                        name,
                        path,
                        summary,
                        tokens,
                        duration_secs,
                        ..
                    } => {
                        let tool_lines = tool_display::render_tool_compact(
                            name,
                            path,
                            summary,
                            *tokens,
                            *duration_secs,
                        );
                        lines.extend(tool_lines);
                    }
                    ChatContent::ToolVerbose {
                        name,
                        path,
                        lines: tool_lines,
                        tokens,
                        duration_secs,
                        context_pct,
                    } => {
                        lines.push(tool_display::render_tool_verbose_header(name, path));
                        for line in tool_lines {
                            lines.push(tool_display::render_tool_verbose_line(line));
                        }
                        lines.push(tool_display::render_tool_verbose_footer(
                            *tokens,
                            *duration_secs,
                            *context_pct,
                        ));
                    }
                    ChatContent::Text(text) => {
                        lines.push(Line::from(Span::styled(
                            format!("  [{}]", text),
                            Style::default().fg(colors.tool),
                        )));
                    }
                }
            }
            MessageRole::Error => {
                let text = msg.content.as_text();
                lines.push(Line::from(Span::styled(
                    format!("  Error: {}", text),
                    Style::default().fg(colors.error),
                )));
            }
        }
        lines.push(Line::from(""));
    }

    // Show thinking indicator
    if app.is_thinking {
        lines.push(Line::from(vec![
            Span::styled(
                "g3: ",
                Style::default()
                    .fg(colors.assistant)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("thinking...", Style::default().fg(colors.secondary)),
        ]));
    }

    let text = Text::from(lines);
    let visible_height = area.height as usize;
    let total_lines = text.lines.len();
    let scroll = if total_lines > visible_height {
        (total_lines - visible_height) as u16 + app.scroll_offset
    } else {
        0
    };

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

fn render_context_bar(frame: &mut Frame, area: Rect, app: &AppView, colors: &Colors) {
    let pct = app.context_percentage;
    let color = if pct < 50.0 {
        colors.success
    } else if pct < 80.0 {
        Color::Yellow
    } else {
        colors.error
    };

    // Build context gauge
    let filled = ((pct / 100.0) * 10.0).round() as usize;
    let filled = filled.min(10);
    let empty = 10 - filled;
    let gauge = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

    // Build status text parts
    let mut spans: Vec<Span> = vec![
        Span::styled(" Context: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:.0}% ", pct), Style::default().fg(color)),
        Span::styled(gauge, Style::default().fg(color)),
    ];

    // Add model name if available
    if !app.model_name.is_empty() {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            app.model_name.to_string(),
            Style::default().fg(Color::Cyan),
        ));
    }

    // Add cost
    if app.cost_dollars > 0.0 {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            format!("${:.2}", app.cost_dollars),
            Style::default().fg(Color::White),
        ));
    }

    // Add current tool if running
    if let Some(ref tool) = app.current_tool {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            format!("● {}", tool),
            Style::default().fg(colors.tool),
        ));
    }

    // Add subagent count in narrow mode
    if area.width < 120 && !app.subagent_entries.is_empty() {
        let active = app
            .subagent_entries
            .iter()
            .filter(|e| e.status == crate::tui::subagent_monitor::AgentStatus::Running)
            .count();
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            format!("{} agents", active),
            Style::default().fg(Color::Magenta),
        ));
    }

    // Keyboard hints on the right
    spans.push(Span::styled(
        " | Tab:switch  Ctrl+C:quit  PgUp/Dn:scroll",
        Style::default().fg(Color::DarkGray),
    ));

    let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
    frame.render_widget(bar, area);
}

fn render_input_box(frame: &mut Frame, area: Rect, app: &AppView, colors: &Colors) {
    let border_color = if *app.active_pane == Pane::Main {
        colors.primary
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " Input ",
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ));

    let input_text = if app.input_buffer.is_empty() {
        Span::styled(
            "Type your message here...",
            Style::default().fg(colors.secondary),
        )
    } else {
        Span::styled(app.input_buffer, Style::default().fg(colors.text))
    };

    let input = Paragraph::new(Line::from(input_text)).block(block);
    frame.render_widget(input, area);

    if app.pending_prompt.is_none() && *app.active_pane == Pane::Main {
        let cursor_x = area.x + 1 + app.cursor_position as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

fn render_prompt_overlay(frame: &mut Frame, area: Rect, app: &AppView, colors: &Colors) {
    if let Some(ref prompt) = app.pending_prompt {
        let message = prompt.message();

        let width = (message.len() as u16 + 6).min(area.width.saturating_sub(4));
        let height: u16 = match prompt {
            PendingPrompt::YesNo { .. } => 5,
            PendingPrompt::Choice { options, .. } => 4 + options.len() as u16,
        };
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors.accent))
            .title(Span::styled(
                " Prompt ",
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD),
            ));

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(message, Style::default().fg(colors.text))),
            Line::from(""),
        ];

        match prompt {
            PendingPrompt::YesNo { .. } => {
                lines.push(Line::from(Span::styled(
                    "[y] Yes  [n] No",
                    Style::default()
                        .fg(colors.primary)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            PendingPrompt::Choice { options, .. } => {
                for (i, opt) in options.iter().enumerate() {
                    lines.push(Line::from(Span::styled(
                        format!("[{}] {}", i + 1, opt),
                        Style::default().fg(colors.primary),
                    )));
                }
            }
        }

        let paragraph = Paragraph::new(Text::from(lines))
            .block(block)
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, popup_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colors_default() {
        let colors = Colors::default();
        assert_eq!(colors.primary, Color::Cyan);
        assert_eq!(colors.text, Color::White);
    }
}
