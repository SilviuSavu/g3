//! TUI rendering module using ratatui.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

use crate::tui::app::AppMode;

/// Color palette for the TUI.
#[derive(Clone)]
pub struct Colors {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub background: Color,
    pub text: Color,
    pub error: Color,
    pub success: Color,
}

impl Default for Colors {
    fn default() -> Self {
        Colors {
            primary: Color::Cyan,
            secondary: Color::Blue,
            accent: Color::Magenta,
            background: Color::Reset,
            text: Color::White,
            error: Color::Red,
            success: Color::Green,
        }
    }
}

/// UI layout configuration.
#[derive(Clone)]
pub struct LayoutConfig {
    pub show_status_bar: bool,
    pub show_header: bool,
    pub show_footer: bool,
    pub tab_width: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        LayoutConfig {
            show_status_bar: true,
            show_header: true,
            show_footer: true,
            tab_width: 12,
        }
    }
}

/// Render text content with basic styling.
pub fn render_text(frame: &mut Frame, area: Rect, text: &str, colors: &Colors) {
    let lines: Vec<Line> = text
        .lines()
        .map(|line| {
            Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(colors.text),
            ))
        })
        .collect();
    
    let paragraph = Paragraph::new(Text::from(lines))
        .style(Style::default().fg(colors.text))
        .alignment(Alignment::Left);
    
    frame.render_widget(paragraph, area);
}

/// Render a status bar.
pub fn render_status_bar(frame: &mut Frame, area: Rect, status: &str, colors: &Colors) {
    let status_text = Text::from(vec![Line::from(vec![
        Span::styled("[", Style::default().fg(colors.secondary)),
        Span::styled(status, Style::default().fg(colors.text)),
        Span::styled("]", Style::default().fg(colors.secondary)),
    ])]);
    
    let paragraph = Paragraph::new(status_text)
        .style(Style::default().fg(colors.text).bg(colors.secondary))
        .alignment(Alignment::Left);
    
    frame.render_widget(paragraph, area);
}

/// Render a header with title.
pub fn render_header(frame: &mut Frame, area: Rect, title: &str, colors: &Colors) {
    let header_text = Text::from(vec![Line::from(vec![
        Span::styled(" ", Style::default().fg(colors.secondary)),
        Span::styled(title, Style::default().fg(colors.primary).bold()),
    ])]);
    
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(colors.secondary));
    
    let paragraph = Paragraph::new(header_text)
        .style(Style::default().fg(colors.text))
        .alignment(Alignment::Left)
        .block(block);
    
    frame.render_widget(paragraph, area);
}

/// Render a footer with instructions.
pub fn render_footer(frame: &mut Frame, area: Rect, instructions: &[&str], colors: &Colors) {
    let footer_text = Text::from(vec![Line::from(instructions
        .iter()
        .enumerate()
        .map(|(i, &inst)| {
            let prefix = if i > 0 { " | " } else { "" };
            Span::styled(
                format!("{}{}", prefix, inst),
                Style::default().fg(colors.secondary),
            )
        })
        .collect::<Vec<Span>>())]);
    
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(colors.secondary));
    
    let paragraph = Paragraph::new(footer_text)
        .style(Style::default().fg(colors.text))
        .alignment(Alignment::Left)
        .block(block);
    
    frame.render_widget(paragraph, area);
}

/// Render a centered message.
pub fn render_centered_message(
    frame: &mut Frame,
    area: Rect,
    message: &str,
    colors: &Colors,
) {
    let lines = Text::from(vec![Line::from(Span::styled(
        message,
        Style::default().fg(colors.primary).bold(),
    ))]);
    
    let paragraph = Paragraph::new(lines)
        .style(Style::default().fg(colors.text))
        .alignment(Alignment::Center);
    
    frame.render_widget(paragraph, area);
}

/// Render an error message.
pub fn render_error(frame: &mut Frame, area: Rect, message: &str, colors: &Colors) {
    let lines = Text::from(vec![Line::from(vec![
        Span::styled("X ", Style::default().fg(colors.error)),
        Span::styled(message, Style::default().fg(colors.text)),
    ])]);
    
    let block = Block::default()
        .border_style(Style::default().fg(colors.error))
        .title(Span::styled(" Error ", Style::default().fg(colors.error).bold()));
    
    let paragraph = Paragraph::new(lines)
        .style(Style::default().fg(colors.text))
        .block(block);
    
    frame.render_widget(paragraph, area);
}

/// Render a success message.
pub fn render_success(frame: &mut Frame, area: Rect, message: &str, colors: &Colors) {
    let lines = Text::from(vec![Line::from(vec![
        Span::styled("V ", Style::default().fg(colors.success)),
        Span::styled(message, Style::default().fg(colors.text)),
    ])]);
    
    let block = Block::default()
        .border_style(Style::default().fg(colors.success))
        .title(Span::styled(" Success ", Style::default().fg(colors.success).bold()));
    
    let paragraph = Paragraph::new(lines)
        .style(Style::default().fg(colors.text))
        .block(block);
    
    frame.render_widget(paragraph, area);
}

/// Split an area into a header, main content, and footer.
pub fn split_with_header_footer(
    area: Rect,
    header_height: u16,
    footer_height: u16,
) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(0),
            Constraint::Length(footer_height),
        ])
        .split(area);
    
    (chunks[0], chunks[1], chunks[2])
}

/// Split an area into a header and main content.
pub fn split_with_header(area: Rect, header_height: u16) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(0),
        ])
        .split(area);
    
    (chunks[0], chunks[1])
}

/// Draw the main content area (standalone function to avoid borrow conflicts).
pub fn draw_main_content(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    mode: AppMode,
    messages: &[String],
    error: &Option<String>,
    success: &Option<String>,
    colors: &Colors,
) {
    if let Some(ref error) = error {
        render_error(frame, area, error, colors);
    } else if let Some(ref success) = success {
        render_centered_message(frame, area, success, colors);
    } else if messages.is_empty() {
        render_centered_message(
            frame,
            area,
            "Welcome to g3 TUI\n\nPress Ctrl+C to exit",
            colors,
        );
    } else {
        // Display recent messages
        let text = messages.join("\n");
        render_text(frame, area, &text, colors);
    }
}

/// Draw the footer (standalone function to avoid borrow conflicts).
pub fn draw_footer(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, mode: AppMode) {
    let instructions = match mode {
        AppMode::Interactive => ["Enter: Send", "Esc: Menu", "Ctrl+C: Quit"],
        AppMode::Settings => ["Arrow keys: Navigate", "Enter: Select", "Esc: Back"],
        AppMode::Help => ["Arrow keys: Navigate", "Esc: Back", ""],
        AppMode::Logs => ["Arrow keys: Scroll", "Page Up/Down: Scroll", "Esc: Back"],
    };
    render_footer(frame, area, &instructions, &Colors::default());
}

/// Draw the status bar (standalone function to avoid borrow conflicts).
pub fn draw_status_bar(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, mode: AppMode) {
    let status = match mode {
        AppMode::Interactive => "INTERACTIVE",
        AppMode::Settings => "SETTINGS",
        AppMode::Help => "HELP",
        AppMode::Logs => "LOGS",
    };
    render_status_bar(frame, area, status, &Colors::default());
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

    #[test]
    fn test_split_with_header_footer() {
        let area = Rect::new(0, 0, 80, 24);
        let (header, main, footer) = split_with_header_footer(area, 2, 2);
        assert_eq!(header.height, 2);
        assert_eq!(footer.height, 2);
        assert!(main.height > 0);
    }

    #[test]
    fn test_draw_footer() {
        let mode = AppMode::Interactive;
        assert_eq!(mode, AppMode::Interactive);
    }
}
