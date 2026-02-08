//! TUI (Text User Interface) module for g3 using ratatui.

pub mod app;
pub mod events;
pub mod markdown;
pub mod subagent_monitor;
pub mod subagent_panel;
pub mod tool_display;
pub mod tui_ui_writer;
pub mod ui;

pub use ui::Colors;

use subagent_monitor::SubagentMonitor;
use tui_ui_writer::TuiUiWriter;

/// Run the TUI application.
/// This is a synchronous function that manages its own tokio runtime internally.
pub fn run_tui() -> anyhow::Result<()> {
    // Channel: TUI -> agent thread (user input strings)
    let (agent_input_tx, agent_input_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    // Channel: agent thread -> TUI (events for rendering)
    let (tui_event_tx, tui_event_rx) =
        tokio::sync::mpsc::unbounded_channel::<tui_ui_writer::TuiEvent>();
    // Channel: subagent monitor -> TUI (subagent state updates)
    let (subagent_tx, subagent_rx) =
        tokio::sync::mpsc::unbounded_channel::<Vec<subagent_monitor::SubagentEntry>>();

    // Spawn the agent thread with its own tokio runtime
    let agent_handle = std::thread::spawn(move || {
        run_agent_thread(agent_input_rx, tui_event_tx);
    });

    // Spawn the subagent monitor thread
    let log_dir = std::env::current_dir()
        .unwrap_or_default()
        .join("logs");
    let monitor = SubagentMonitor::new(log_dir);
    let monitor_handle = std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return,
        };
        rt.block_on(monitor.run(subagent_tx));
    });

    // Run the TUI on the main thread
    let mut app = app::App::new(agent_input_tx, tui_event_rx, subagent_rx)?;
    let result = app.run();

    // TUI exited - the agent and monitor threads will terminate when their channels close
    drop(app);
    let _ = agent_handle.join();
    let _ = monitor_handle.join();

    result
}

/// Check if the TUI can run in the current environment.
pub fn can_run_tui() -> bool {
    if let Ok((width, height)) = crossterm::terminal::size() {
        width >= 80 && height >= 24
    } else {
        false
    }
}

/// Run the agent on a separate thread with its own tokio runtime.
fn run_agent_thread(
    agent_input_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    tui_event_tx: tokio::sync::mpsc::UnboundedSender<tui_ui_writer::TuiEvent>,
) {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            let _ = tui_event_tx.send(tui_ui_writer::TuiEvent::Error(format!(
                "Failed to create tokio runtime: {}",
                e
            )));
            return;
        }
    };

    rt.block_on(agent_loop(agent_input_rx, tui_event_tx));
}

/// The async agent loop that processes user input.
async fn agent_loop(
    mut agent_input_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    tui_event_tx: tokio::sync::mpsc::UnboundedSender<tui_ui_writer::TuiEvent>,
) {
    use g3_config::Config;
    use g3_core::Agent;

    // Load configuration
    let config = match Config::load(None) {
        Ok(config) => config,
        Err(e) => {
            let _ = tui_event_tx.send(tui_ui_writer::TuiEvent::Error(format!(
                "Failed to load config: {}. Run 'g3' in CLI mode first to set up.",
                e
            )));
            return;
        }
    };

    // Create TuiUiWriter
    let ui_writer = TuiUiWriter::new(tui_event_tx.clone());

    // Create agent
    let mut agent = match Agent::new_with_project_context_and_quiet(
        config,
        ui_writer,
        None,  // project_context
        false, // quiet
    )
    .await
    {
        Ok(agent) => agent,
        Err(e) => {
            let _ = tui_event_tx.send(tui_ui_writer::TuiEvent::Error(format!(
                "Failed to create agent: {}",
                e
            )));
            return;
        }
    };

    // Process user input
    while let Some(input) = agent_input_rx.recv().await {
        match agent
            .execute_task_with_timing(
                &input,
                None,   // language
                false,  // auto_execute
                false,  // show_prompt
                false,  // show_code
                false,  // show_timing
                None,   // discovery_options
            )
            .await
        {
            Ok(_result) => {
                // Response was already streamed via TuiUiWriter
            }
            Err(e) => {
                let _ = tui_event_tx.send(tui_ui_writer::TuiEvent::Error(format!(
                    "Agent error: {}",
                    e
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_run_tui() {
        let _ = can_run_tui();
    }
}
