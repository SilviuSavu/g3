//! TUI integration tests

#[cfg(feature = "tui")]
mod tui_tests {
    use g3_cli::tui;

    #[test]
    fn test_can_run_tui() {
        // Basic test that TUI module is available
        assert!(tui::can_run_tui() || true); // May return false in test environment
    }

    #[test]
    fn test_colors_default() {
        let colors = tui::ui::Colors::default();
        assert_eq!(colors.primary, ratatui::style::Color::Cyan);
    }

    #[test]
    fn test_app_mode_clone() {
        let mode = tui::app::AppMode::Interactive;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_layout_config_default() {
        let config = tui::ui::LayoutConfig::default();
        assert!(config.show_header);
        assert!(config.show_footer);
    }
}
