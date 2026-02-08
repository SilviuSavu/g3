//! Terminal size utilities for the TUI.

/// Check if the terminal has a minimum required size.
pub fn has_minimum_size(min_width: u16, min_height: u16) -> bool {
    if let Ok((width, height)) = crossterm::terminal::size() {
        width >= min_width && height >= min_height
    } else {
        false
    }
}

/// Get the current terminal size.
pub fn get_terminal_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((80, 24))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_terminal_size_returns_tuple() {
        let (w, h) = get_terminal_size();
        assert!(w > 0);
        assert!(h > 0);
    }
}
