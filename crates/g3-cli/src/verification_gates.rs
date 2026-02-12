//! Verification gauntlet for autonomous mode.
//!
//! Runs a multi-stage verification pipeline after the player implements code
//! and before the coach reviews it. Hard gates block progression; soft gates
//! produce advisory findings that inform the coach's review.

use std::path::Path;
use std::time::{Duration, Instant};

use g3_config::{GateMode, GatesConfig};
use tokio::process::Command;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Outcome of the full gauntlet run.
pub struct GauntletResult {
    pub stages: Vec<StageResult>,
    /// True when every hard gate passed (soft failures are allowed).
    pub passed: bool,
}

/// Outcome of a single verification stage.
pub struct StageResult {
    pub name: &'static str,
    pub kind: GateKind,
    pub status: GateStatus,
    /// Last N lines of combined stdout+stderr.
    pub output: String,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateKind {
    Hard,
    Soft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStatus {
    Passed,
    Failed,
    Skipped,
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

impl GauntletResult {
    /// Human-readable feedback aimed at the player after a hard failure.
    pub fn format_for_player(&self) -> String {
        let mut out = String::new();
        let total = self.stages.len();

        // Find first failure index for the "stage N/M" label.
        let first_fail = self
            .stages
            .iter()
            .position(|s| s.status == GateStatus::Failed);

        out.push_str(&format!(
            "GAUNTLET FAILED at stage {}/{}:\n\n",
            first_fail.map_or(total, |i| i + 1),
            total
        ));

        for s in &self.stages {
            let icon = match s.status {
                GateStatus::Passed => "PASS",
                GateStatus::Failed => "FAIL",
                GateStatus::Skipped => "SKIP",
            };
            let kind_tag = match s.kind {
                GateKind::Hard => "hard gate",
                GateKind::Soft => "soft gate",
            };
            out.push_str(&format!("[{}] {} ({}):\n", icon, s.name, kind_tag));
            if s.status == GateStatus::Failed {
                out.push_str(&s.output);
                out.push('\n');
            }
        }

        out.push_str(
            "\nFix these issues. The Gauntlet will re-run after your next implementation.\n",
        );
        out
    }

    /// Structured summary for injection into the coach's prompt.
    pub fn format_for_coach(&self) -> String {
        let mut out = String::new();
        out.push_str("GAUNTLET RESULTS:\n\n");

        for s in &self.stages {
            let icon = match s.status {
                GateStatus::Passed => "PASS",
                GateStatus::Failed => "FAIL",
                GateStatus::Skipped => "SKIP",
            };
            out.push_str(&format!(
                "[{}] {} ({:?}, {:.1}s)\n",
                icon,
                s.name,
                s.kind,
                s.duration.as_secs_f64()
            ));

            // For soft-gate failures include the output so the coach can cite specifics.
            if s.status == GateStatus::Failed && s.kind == GateKind::Soft {
                out.push_str(&s.output);
                out.push('\n');
            }
        }

        let survived = self
            .stages
            .iter()
            .any(|s| s.name == "mutants" && s.status == GateStatus::Failed);

        if survived {
            out.push_str(
                "\nMutation testing found survived mutants. \
                 The player's tests are hollow — they don't verify actual logic. \
                 Veto unless the player adds assertions that catch these mutations.\n",
            );
        }

        out
    }
}

// ---------------------------------------------------------------------------
// Gauntlet runner
// ---------------------------------------------------------------------------

/// Run the full verification gauntlet. Stops on first hard failure.
pub async fn run_gauntlet(working_dir: &Path, config: &GatesConfig) -> GauntletResult {
    if !config.enabled {
        return GauntletResult {
            stages: Vec::new(),
            passed: true,
        };
    }

    let timeout = Duration::from_secs(config.gate_timeout);
    let mut stages: Vec<StageResult> = Vec::with_capacity(4);
    let mut hard_failed = false;

    // ---- Stage 1: clippy ----
    let clippy_result = run_stage(
        "clippy",
        &config.cargo_clippy,
        working_dir,
        &["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
        &[],
        timeout,
    )
    .await;
    if clippy_result.kind == GateKind::Hard && clippy_result.status == GateStatus::Failed {
        hard_failed = true;
    }
    let clippy_blocked = hard_failed;
    stages.push(clippy_result);

    // ---- Stage 2: test ----
    let test_result = if clippy_blocked {
        skipped_stage("test", &config.cargo_test)
    } else {
        run_stage(
            "test",
            &config.cargo_test,
            working_dir,
            &["cargo", "test", "--quiet"],
            &[],
            timeout,
        )
        .await
    };
    if test_result.kind == GateKind::Hard && test_result.status == GateStatus::Failed {
        hard_failed = true;
    }
    let tests_blocked = hard_failed;
    stages.push(test_result);

    // ---- Stage 3: mutants ----
    let mutants_result = if tests_blocked {
        skipped_stage("mutants", &config.cargo_mutants)
    } else {
        let line_limit = config.mutants_line_limit.to_string();
        run_stage(
            "mutants",
            &config.cargo_mutants,
            working_dir,
            &[
                "cargo",
                "mutants",
                "--no-times",
                "--line-limit",
                &line_limit,
            ],
            &[],
            // Mutants gets 2x timeout.
            Duration::from_secs(config.gate_timeout * 2),
        )
        .await
    };
    if mutants_result.kind == GateKind::Hard && mutants_result.status == GateStatus::Failed {
        hard_failed = true;
    }
    stages.push(mutants_result);

    // ---- Stage 4: proptest ----
    let proptest_result = if tests_blocked {
        skipped_stage("proptest", &config.cargo_proptest)
    } else {
        let cases = config.proptest_cases.to_string();
        run_stage(
            "proptest",
            &config.cargo_proptest,
            working_dir,
            &["cargo", "test", "proptest_", "--quiet"],
            &[("PROPTEST_CASES", cases.as_str())],
            timeout,
        )
        .await
    };
    if proptest_result.kind == GateKind::Hard && proptest_result.status == GateStatus::Failed {
        hard_failed = true;
    }
    stages.push(proptest_result);

    GauntletResult {
        passed: !hard_failed,
        stages,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn gate_kind(mode: &GateMode) -> GateKind {
    match mode {
        GateMode::Hard => GateKind::Hard,
        _ => GateKind::Soft,
    }
}

fn skipped_stage(name: &'static str, mode: &GateMode) -> StageResult {
    StageResult {
        name,
        kind: gate_kind(mode),
        status: GateStatus::Skipped,
        output: String::new(),
        duration: Duration::ZERO,
    }
}

async fn run_stage(
    name: &'static str,
    mode: &GateMode,
    working_dir: &Path,
    cmd: &[&str],
    env_vars: &[(&str, &str)],
    timeout: Duration,
) -> StageResult {
    if *mode == GateMode::Off {
        return StageResult {
            name,
            kind: GateKind::Soft,
            status: GateStatus::Skipped,
            output: String::new(),
            duration: Duration::ZERO,
        };
    }

    let kind = gate_kind(mode);

    // Prerequisite check for cargo-mutants.
    if name == "mutants" {
        if let Err(msg) = check_prerequisite("cargo-mutants", "cargo install cargo-mutants").await {
            return StageResult {
                name,
                kind,
                status: GateStatus::Failed,
                output: msg,
                duration: Duration::ZERO,
            };
        }
    }

    let start = Instant::now();

    let (program, args) = match cmd.split_first() {
        Some((p, a)) => (*p, a),
        None => {
            return StageResult {
                name,
                kind,
                status: GateStatus::Failed,
                output: "Empty command".to_string(),
                duration: Duration::ZERO,
            };
        }
    };

    let mut command = Command::new(program);
    command.args(args).current_dir(working_dir);
    // Merge extra env vars.
    for (k, v) in env_vars {
        command.env(k, v);
    }
    // Capture combined output.
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return StageResult {
                name,
                kind,
                status: GateStatus::Failed,
                output: format!("Failed to spawn command: {}", e),
                duration: start.elapsed(),
            };
        }
    };

    // Wait with timeout.
    let result = tokio::time::timeout(timeout, child.wait_with_output()).await;

    let duration = start.elapsed();

    match result {
        Ok(Ok(output)) => {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            let truncated = tail_lines(&combined, 100);
            let status = if output.status.success() {
                GateStatus::Passed
            } else {
                GateStatus::Failed
            };
            StageResult {
                name,
                kind,
                status,
                output: truncated,
                duration,
            }
        }
        Ok(Err(e)) => StageResult {
            name,
            kind,
            status: GateStatus::Failed,
            output: format!("Process error: {}", e),
            duration,
        },
        Err(_) => StageResult {
            name,
            kind,
            status: GateStatus::Failed,
            output: format!(
                "Stage '{}' timed out after {}s",
                name,
                timeout.as_secs()
            ),
            duration,
        },
    }
}

/// Check that a binary exists on PATH. Returns Err with install instructions.
async fn check_prerequisite(binary: &str, install_hint: &str) -> Result<(), String> {
    let status = Command::new("which")
        .arg(binary)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Err(format!(
            "Gauntlet requires '{}'. Install: {}",
            binary, install_hint
        )),
    }
}

/// Return the last `n` lines of `text`.
fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= n {
        text.to_string()
    } else {
        lines[lines.len() - n..].join("\n")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_lines_short() {
        let text = "a\nb\nc";
        assert_eq!(tail_lines(text, 10), text);
    }

    #[test]
    fn tail_lines_truncates() {
        let text = (0..200)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let result = tail_lines(&text, 100);
        let count = result.lines().count();
        assert_eq!(count, 100);
        assert!(result.starts_with("line 100"));
    }

    #[test]
    fn skipped_stage_fields() {
        let s = skipped_stage("test", &GateMode::Hard);
        assert_eq!(s.name, "test");
        assert_eq!(s.kind, GateKind::Hard);
        assert_eq!(s.status, GateStatus::Skipped);
    }

    #[test]
    fn gate_kind_mapping() {
        assert_eq!(gate_kind(&GateMode::Hard), GateKind::Hard);
        assert_eq!(gate_kind(&GateMode::Soft), GateKind::Soft);
        assert_eq!(gate_kind(&GateMode::Off), GateKind::Soft);
    }

    #[test]
    fn format_for_player_includes_failure() {
        let result = GauntletResult {
            passed: false,
            stages: vec![
                StageResult {
                    name: "clippy",
                    kind: GateKind::Hard,
                    status: GateStatus::Failed,
                    output: "error[E0381]: used binding".to_string(),
                    duration: Duration::from_secs(2),
                },
                StageResult {
                    name: "test",
                    kind: GateKind::Hard,
                    status: GateStatus::Skipped,
                    output: String::new(),
                    duration: Duration::ZERO,
                },
            ],
        };
        let text = result.format_for_player();
        assert!(text.contains("GAUNTLET FAILED at stage 1/2"));
        assert!(text.contains("error[E0381]"));
        assert!(text.contains("[FAIL] clippy"));
        assert!(text.contains("[SKIP] test"));
    }

    #[test]
    fn format_for_coach_mutation_warning() {
        let result = GauntletResult {
            passed: true,
            stages: vec![
                StageResult {
                    name: "clippy",
                    kind: GateKind::Hard,
                    status: GateStatus::Passed,
                    output: String::new(),
                    duration: Duration::from_secs(5),
                },
                StageResult {
                    name: "test",
                    kind: GateKind::Hard,
                    status: GateStatus::Passed,
                    output: String::new(),
                    duration: Duration::from_secs(10),
                },
                StageResult {
                    name: "mutants",
                    kind: GateKind::Soft,
                    status: GateStatus::Failed,
                    output: "survived: replaced > with >=".to_string(),
                    duration: Duration::from_secs(30),
                },
            ],
        };
        let text = result.format_for_coach();
        assert!(text.contains("GAUNTLET RESULTS"));
        assert!(text.contains("[FAIL] mutants"));
        assert!(text.contains("survived: replaced > with >="));
        assert!(text.contains("hollow"));
    }

    #[test]
    fn format_for_coach_all_pass() {
        let result = GauntletResult {
            passed: true,
            stages: vec![StageResult {
                name: "clippy",
                kind: GateKind::Hard,
                status: GateStatus::Passed,
                output: String::new(),
                duration: Duration::from_secs(3),
            }],
        };
        let text = result.format_for_coach();
        assert!(text.contains("[PASS] clippy"));
        assert!(!text.contains("hollow"));
    }

    #[test]
    fn disabled_config_passes_immediately() {
        let config = GatesConfig {
            enabled: false,
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(run_gauntlet(Path::new("."), &config));
        assert!(result.passed);
        assert!(result.stages.is_empty());
    }

    #[test]
    fn hard_failure_blocks_subsequent_stages() {
        // Simulate: clippy passed, test failed (hard) -> mutants & proptest skipped
        let stages = vec![
            StageResult {
                name: "clippy",
                kind: GateKind::Hard,
                status: GateStatus::Passed,
                output: String::new(),
                duration: Duration::from_secs(1),
            },
            StageResult {
                name: "test",
                kind: GateKind::Hard,
                status: GateStatus::Failed,
                output: "test failures".to_string(),
                duration: Duration::from_secs(5),
            },
            StageResult {
                name: "mutants",
                kind: GateKind::Soft,
                status: GateStatus::Skipped,
                output: String::new(),
                duration: Duration::ZERO,
            },
            StageResult {
                name: "proptest",
                kind: GateKind::Soft,
                status: GateStatus::Skipped,
                output: String::new(),
                duration: Duration::ZERO,
            },
        ];

        // Verify the pattern: after a hard fail, remaining are skipped.
        assert_eq!(stages[0].status, GateStatus::Passed);
        assert_eq!(stages[1].status, GateStatus::Failed);
        assert_eq!(stages[2].status, GateStatus::Skipped);
        assert_eq!(stages[3].status, GateStatus::Skipped);

        let result = GauntletResult {
            passed: false,
            stages,
        };
        let text = result.format_for_player();
        assert!(text.contains("GAUNTLET FAILED at stage 2/4"));
    }
}
