//! TUI rendering functions using ratatui.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{ChatMessage, MessageRole, PendingPrompt};

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
}

/// Render the entire TUI frame.
pub fn render(frame: &mut Frame, app: &AppView) {
    let size = frame.area();
    let colors = app.colors;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(size);

    render_chat(frame, chunks[0], app, colors);
    render_context_bar(frame, chunks[1], app, colors);
    render_input_box(frame, chunks[2], app, colors);

    if app.current_tool.is_some() {
        render_tool_status(frame, chunks[0], app, colors);
    }

    if app.pending_prompt.is_some() {
        render_prompt_overlay(frame, size, app, colors);
    }
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
                lines.push(Line::from(vec![
                    Span::styled(
                        "You: ",
                        Style::default()
                            .fg(colors.user)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&msg.content, Style::default().fg(colors.text)),
                ]));
            }
            MessageRole::Assistant => {
                if msg.content.is_empty() {
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
                    let content_lines: Vec<&str> = msg.content.lines().collect();
                    for (i, line) in content_lines.iter().enumerate() {
                        if i == 0 {
                            lines.push(Line::from(vec![
                                Span::styled(
                                    "g3: ",
                                    Style::default()
                                        .fg(colors.assistant)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(*line, Style::default().fg(colors.text)),
                            ]));
                        } else {
                            lines.push(Line::from(Span::styled(
                                format!("    {}", line),
                                Style::default().fg(colors.text),
                            )));
                        }
                    }
                }
            }
            MessageRole::Tool => {
                lines.push(Line::from(Span::styled(
                    format!("  [{}]", msg.content),
                    Style::default().fg(colors.tool),
                )));
            }
            MessageRole::Error => {
                lines.push(Line::from(Span::styled(
                    format!("  Error: {}", msg.content),
                    Style::default().fg(colors.error),
                )));
            }
        }
        lines.push(Line::from(""));
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

    let status_text = if let Some(ref tool) = app.current_tool {
        format!(" Context: {:.0}% | Running: {} ", pct, tool)
    } else {
        format!(" Context: {:.0}% | Ready ", pct)
    };

    let bar = Paragraph::new(Line::from(Span::styled(
        status_text,
        Style::default().fg(color),
    )))
    .style(Style::default().bg(Color::Black));

    frame.render_widget(bar, area);
}

fn render_input_box(frame: &mut Frame, area: Rect, app: &AppView, colors: &Colors) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors.primary))
        .title(Span::styled(
            " Input ",
            Style::default()
                .fg(colors.primary)
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

    if app.pending_prompt.is_none() {
        let cursor_x = area.x + 1 + app.cursor_position as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

fn render_tool_status(frame: &mut Frame, area: Rect, app: &AppView, colors: &Colors) {
    if let Some(ref tool) = app.current_tool {
        let text = format!(" {} running... ", tool);
        let width = text.len() as u16;
        if area.width > width + 2 && area.height > 2 {
            let tool_area = Rect::new(
                area.x + area.width - width - 1,
                area.y + area.height - 1,
                width,
                1,
            );
            let widget = Paragraph::new(Span::styled(
                text,
                Style::default().fg(Color::Black).bg(colors.tool),
            ));
            frame.render_widget(widget, tool_area);
        }
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
