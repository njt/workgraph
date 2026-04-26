//! Idle-time cargo sweep: runs `cargo sweep --time 7` when the graph has been
//! quiet (no agents alive and no tasks ready) for a sustained period.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use super::DaemonLogger;

/// How long the graph must be continuously idle before triggering a sweep.
pub const IDLE_SWEEP_THRESHOLD: Duration = Duration::from_secs(30 * 60); // 30 minutes

/// Tracks idle-sweep state across coordinator ticks.
pub struct IdleSweepState {
    /// When the graph first became idle (agents_alive == 0 && tasks_ready == 0).
    /// `None` means the graph is currently active.
    idle_since: Option<Instant>,
    /// Whether we have already fired the sweep for the current idle period.
    fired: bool,
    /// Whether we have already logged the "cargo-sweep not found" message
    /// for the current idle period.
    logged_missing: bool,
}

impl IdleSweepState {
    pub fn new() -> Self {
        Self {
            idle_since: None,
            fired: false,
            logged_missing: false,
        }
    }

    /// Call every tick with the current liveness counts. If all conditions are
    /// met, spawns `cargo sweep --time 7` asynchronously and returns `true`.
    pub fn tick(
        &mut self,
        agents_alive: usize,
        tasks_ready: usize,
        project_root: &Path,
        auto_sweep_enabled: bool,
        logger: &DaemonLogger,
    ) -> bool {
        let is_idle = agents_alive == 0 && tasks_ready == 0;

        if !is_idle {
            // Graph became active — reset state for next quiet period.
            if self.idle_since.is_some() {
                self.idle_since = None;
                self.fired = false;
                self.logged_missing = false;
            }
            return false;
        }

        // Graph is idle — start or continue the idle timer.
        let idle_start = *self.idle_since.get_or_insert_with(Instant::now);

        if self.fired || !auto_sweep_enabled {
            return false;
        }

        if idle_start.elapsed() < IDLE_SWEEP_THRESHOLD {
            return false;
        }

        // Condition (a): project root contains Cargo.toml
        if !project_root.join("Cargo.toml").exists() {
            return false;
        }

        // Condition (b): cargo-sweep is on PATH
        if !is_cargo_sweep_available() {
            if !self.logged_missing {
                logger.info("cargo-sweep not found on PATH — skipping idle sweep");
                self.logged_missing = true;
            }
            return false;
        }

        // All conditions met — fire sweep asynchronously.
        self.fired = true;
        logger.info("Graph idle for 30+ minutes — running cargo sweep --time 7");
        spawn_cargo_sweep(project_root, logger);
        true
    }
}

/// Check whether `cargo-sweep` is available on PATH.
fn is_cargo_sweep_available() -> bool {
    // `cargo sweep --version` is a quick non-destructive probe.
    Command::new("cargo-sweep")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Spawn `cargo sweep --time 7` in a fire-and-forget background thread
/// so it doesn't block the coordinator tick loop.
fn spawn_cargo_sweep(project_root: &Path, logger: &DaemonLogger) {
    let root = project_root.to_path_buf();
    let logger = logger.clone();
    std::thread::spawn(move || {
        let result = Command::new("cargo")
            .args(["sweep", "--time", "7"])
            .current_dir(&root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output();
        match result {
            Ok(output) if output.status.success() => {
                logger.info("cargo sweep --time 7 completed successfully");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                logger.warn(&format!(
                    "cargo sweep exited with {}: {}",
                    output.status,
                    stderr.trim()
                ));
            }
            Err(e) => {
                logger.warn(&format!("Failed to run cargo sweep: {}", e));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a DaemonLogger writing to a temp directory.
    fn test_logger(dir: &Path) -> DaemonLogger {
        let service_dir = dir.join("service");
        fs::create_dir_all(&service_dir).unwrap();
        DaemonLogger::open(dir).unwrap()
    }

    /// Read the daemon log contents.
    fn read_log(dir: &Path) -> String {
        fs::read_to_string(dir.join("service/daemon.log")).unwrap_or_default()
    }

    #[test]
    fn test_coordinator_runs_cargo_sweep_when_idle() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Create Cargo.toml so condition (a) passes
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        let logger = test_logger(root);

        let mut state = IdleSweepState::new();

        // First tick: graph just became idle — timer starts but threshold not met
        let fired = state.tick(0, 0, root, true, &logger);
        assert!(!fired, "should not fire immediately");

        // Simulate 30+ minutes passing by backdating idle_since
        state.idle_since = Some(Instant::now() - IDLE_SWEEP_THRESHOLD - Duration::from_secs(1));

        // cargo-sweep may or may not be on PATH in the test environment.
        // We test the state machine logic: if cargo-sweep IS available, it should
        // fire exactly once and set the `fired` flag.
        let fired = state.tick(0, 0, root, true, &logger);

        if is_cargo_sweep_available() {
            assert!(fired, "should fire when idle 30+ min with cargo-sweep on PATH");
            assert!(state.fired, "fired flag should be set");

            // Second tick: should NOT fire again (flag prevents it)
            let fired_again = state.tick(0, 0, root, true, &logger);
            assert!(!fired_again, "should not fire a second time in same idle period");
        } else {
            // cargo-sweep not on PATH — sweep should NOT fire, but should log once
            assert!(!fired, "should not fire without cargo-sweep on PATH");
            assert!(state.logged_missing, "should have logged missing cargo-sweep");
            let log = read_log(root);
            assert!(
                log.contains("cargo-sweep not found"),
                "log should mention cargo-sweep not found"
            );
        }
    }

    #[test]
    fn test_coordinator_skips_sweep_when_agents_alive() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        let logger = test_logger(root);

        let mut state = IdleSweepState::new();
        // Backdate idle_since well past threshold
        state.idle_since = Some(Instant::now() - IDLE_SWEEP_THRESHOLD - Duration::from_secs(60));

        // agents_alive > 0 — sweep must NOT fire
        let fired = state.tick(1, 0, root, true, &logger);
        assert!(!fired, "should not fire when agents are alive");
        assert!(
            state.idle_since.is_none(),
            "idle_since should reset when agents are active"
        );
    }

    #[test]
    fn test_coordinator_skips_sweep_when_cargo_sweep_missing() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        let logger = test_logger(root);

        let mut state = IdleSweepState::new();
        state.idle_since = Some(Instant::now() - IDLE_SWEEP_THRESHOLD - Duration::from_secs(60));

        // Manipulate PATH to ensure cargo-sweep is NOT available
        let original_path = std::env::var("PATH").unwrap_or_default();
        // SAFETY: test runs single-threaded; we restore PATH immediately after.
        unsafe { std::env::set_var("PATH", "") };
        let fired = state.tick(0, 0, root, true, &logger);
        unsafe { std::env::set_var("PATH", &original_path) };

        assert!(!fired, "should not fire without cargo-sweep");
        assert!(state.logged_missing, "should have logged missing");

        // Second tick with empty PATH — should NOT log again (no spam)
        unsafe { std::env::set_var("PATH", "") };
        let _ = state.tick(0, 0, root, true, &logger);
        unsafe { std::env::set_var("PATH", &original_path) };

        let log = read_log(root);
        let count = log.matches("cargo-sweep not found").count();
        assert_eq!(count, 1, "should only log missing message once, got {}", count);
    }

    #[test]
    fn test_coordinator_respects_auto_sweep_disabled() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        let logger = test_logger(root);

        let mut state = IdleSweepState::new();
        state.idle_since = Some(Instant::now() - IDLE_SWEEP_THRESHOLD - Duration::from_secs(60));

        // auto_sweep_enabled = false — should never fire
        let fired = state.tick(0, 0, root, false, &logger);
        assert!(!fired, "should not fire when auto_sweep is disabled");
        assert!(!state.fired, "fired flag should stay false");
    }

    #[test]
    fn test_idle_sweep_resets_on_activity() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        let logger = test_logger(root);

        let mut state = IdleSweepState::new();

        // Become idle
        state.tick(0, 0, root, true, &logger);
        assert!(state.idle_since.is_some());

        // Agent becomes active — should reset
        state.tick(1, 0, root, true, &logger);
        assert!(state.idle_since.is_none());
        assert!(!state.fired);
        assert!(!state.logged_missing);
    }
}
