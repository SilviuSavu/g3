//! Turn metrics and histogram generation for performance visualization.

use std::time::Duration;

/// Metrics captured for a single turn of interaction.
#[derive(Debug, Clone)]
pub struct TurnMetrics {
    pub turn_number: usize,
    pub tokens_used: u32,
    pub wall_clock_time: Duration,
}

/// Format a Duration as human-readable elapsed time (e.g., "1h 23m 45s").
pub fn format_elapsed_time(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    match (hours, minutes, seconds) {
        (h, m, s) if h > 0 => format!("{}h {}m {}s", h, m, s),
        (_, m, s) if m > 0 => format!("{}m {}s", m, s),
        (_, _, s) if s > 0 => format!("{}s", s),
        _ => format!("{}ms", duration.as_millis()),
    }
}

/// Generate a histogram showing tokens used and wall clock time per turn.
pub fn generate_turn_histogram(turn_metrics: &[TurnMetrics]) -> String {
    if turn_metrics.is_empty() {
        return "   No turn data available".to_string();
    }

    const MAX_BAR_WIDTH: usize = 40;
    const TOKEN_CHAR: char = '█';
    const TIME_CHAR: char = '▓';

    let max_tokens = turn_metrics.iter().map(|t| t.tokens_used).max().unwrap_or(1);
    let max_time_ms = turn_metrics
        .iter()
        .map(|t| t.wall_clock_time.as_millis().min(u32::MAX as u128) as u32)
        .max()
        .unwrap_or(1);

    let mut histogram = String::new();
    histogram.push_str("\n📊 Per-Turn Performance Histogram:\n");
    histogram.push_str(&format!("   {} = Tokens Used (max: {})\n", TOKEN_CHAR, max_tokens));
    histogram.push_str(&format!(
        "   {} = Wall Clock Time (max: {:.1}s)\n\n",
        TIME_CHAR,
        max_time_ms as f64 / 1000.0
    ));

    for metrics in turn_metrics {
        let turn_time_ms = metrics.wall_clock_time.as_millis().min(u32::MAX as u128) as u32;

        let token_bar_len = scale_bar(metrics.tokens_used, max_tokens, MAX_BAR_WIDTH);
        let time_bar_len = scale_bar(turn_time_ms, max_time_ms, MAX_BAR_WIDTH);

        let time_str = format_duration_ms(turn_time_ms);
        let token_bar = TOKEN_CHAR.to_string().repeat(token_bar_len);
        let time_bar = TIME_CHAR.to_string().repeat(time_bar_len);

        histogram.push_str(&format!(
            "   Turn {:2}: {:>6} tokens │{:<40}│\n",
            metrics.turn_number, metrics.tokens_used, token_bar
        ));
        histogram.push_str(&format!("           {:>6}       │{:<40}│\n", time_str, time_bar));

        // Separator between turns (except for last)
        if metrics.turn_number != turn_metrics.last().unwrap().turn_number {
            histogram.push_str(
                "           ────────────┼────────────────────────────────────────┤\n",
            );
        }
    }

    append_summary_statistics(&mut histogram, turn_metrics);
    histogram
}

/// Scale a value to a bar length proportional to max.
fn scale_bar(value: u32, max: u32, max_width: usize) -> usize {
    if max == 0 {
        0
    } else {
        ((value as f64 / max as f64) * max_width as f64) as usize
    }
}

/// Format milliseconds as a human-readable duration string.
fn format_duration_ms(ms: u32) -> String {
    match ms {
        ms if ms < 1000 => format!("{}ms", ms),
        ms if ms < 60_000 => format!("{:.1}s", ms as f64 / 1000.0),
        ms => {
            let minutes = ms / 60_000;
            let seconds = (ms % 60_000) as f64 / 1000.0;
            format!("{}m{:.1}s", minutes, seconds)
        }
    }
}

/// Append summary statistics to the histogram output.
fn append_summary_statistics(histogram: &mut String, turn_metrics: &[TurnMetrics]) {
    let total_tokens: u32 = turn_metrics.iter().map(|t| t.tokens_used).sum();
    let total_time: Duration = turn_metrics.iter().map(|t| t.wall_clock_time).sum();
    let avg_tokens = total_tokens as f64 / turn_metrics.len() as f64;
    let avg_time_ms = total_time.as_millis() as f64 / turn_metrics.len() as f64;

    histogram.push_str("\n📈 Summary Statistics:\n");
    histogram.push_str(&format!(
        "   • Total Tokens: {} across {} turns\n",
        total_tokens,
        turn_metrics.len()
    ));
    histogram.push_str(&format!("   • Average Tokens/Turn: {:.1}\n", avg_tokens));
    histogram.push_str(&format!("   • Total Time: {:.1}s\n", total_time.as_secs_f64()));
    histogram.push_str(&format!("   • Average Time/Turn: {:.1}s\n", avg_time_ms / 1000.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_elapsed_time() {
        assert_eq!(format_elapsed_time(Duration::from_millis(500)), "500ms");
        assert_eq!(format_elapsed_time(Duration::from_secs(45)), "45s");
        assert_eq!(format_elapsed_time(Duration::from_secs(90)), "1m 30s");
        assert_eq!(format_elapsed_time(Duration::from_secs(3661)), "1h 1m 1s");
    }

    #[test]
    fn test_empty_histogram() {
        let result = generate_turn_histogram(&[]);
        assert!(result.contains("No turn data available"));
    }

    #[test]
    fn test_scale_bar() {
        assert_eq!(scale_bar(50, 100, 40), 20);
        assert_eq!(scale_bar(100, 100, 40), 40);
        assert_eq!(scale_bar(0, 100, 40), 0);
        assert_eq!(scale_bar(50, 0, 40), 0);
    }
}
