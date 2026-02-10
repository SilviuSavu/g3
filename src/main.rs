use g3_cli::run;

fn main() -> anyhow::Result<()> {
    // Check for --tui flag before creating the tokio runtime.
    // The TUI is synchronous and spawns its own tokio runtime internally,
    // so it must not run inside an existing runtime.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--tui") {
        return g3_cli::run_tui();
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}
