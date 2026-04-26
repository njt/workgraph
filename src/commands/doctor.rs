//! `wg doctor` — environment diagnostic.
//!
//! A single command that walks the list of things workgraph needs to run
//! and reports each check's status. Aimed at the "I installed wg, something
//! doesn't work, why" case: surfacing the actual missing piece instead of
//! leaving users to diff obscure error messages against the docs.
//!
//! Most of the surface here is Windows-port adjacent — that's where the
//! gotchas live (bash vs TIMEOUT.EXE, OAuth-token-in-wrong-env-var, extended
//! path prefixes). On Unix the same checks still run but almost always go
//! green.
//!
//! Exit codes:
//!   0 — all green
//!   1 — one or more warnings but no hard errors
//!   2 — one or more errors; workgraph probably won't function correctly

use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Err,
    /// Informational — no judgement, just a fact the user might want to know.
    Info,
}

#[derive(Debug, Serialize)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl Check {
    fn ok(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { name: name.into(), status: Status::Ok, detail: detail.into(), hint: None }
    }
    fn warn(name: impl Into<String>, detail: impl Into<String>, hint: impl Into<String>) -> Self {
        Self { name: name.into(), status: Status::Warn, detail: detail.into(), hint: Some(hint.into()) }
    }
    fn err(name: impl Into<String>, detail: impl Into<String>, hint: impl Into<String>) -> Self {
        Self { name: name.into(), status: Status::Err, detail: detail.into(), hint: Some(hint.into()) }
    }
    fn info(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { name: name.into(), status: Status::Info, detail: detail.into(), hint: None }
    }
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    platform: String,
    wg_version: String,
    checks: Vec<Check>,
    summary: Summary,
}

#[derive(Debug, Serialize)]
struct Summary {
    ok: usize,
    warn: usize,
    err: usize,
    info: usize,
}

pub fn run(dir: &Path, json: bool) -> Result<()> {
    let mut checks: Vec<Check> = Vec::new();

    checks.extend(check_host_tools());
    checks.extend(check_auth(dir));
    checks.extend(check_workgraph_dir(dir));
    checks.extend(check_daemon(dir));
    checks.extend(check_cargo_target_size(dir));
    #[cfg(windows)]
    checks.extend(check_windows_specific());

    let summary = Summary {
        ok: checks.iter().filter(|c| c.status == Status::Ok).count(),
        warn: checks.iter().filter(|c| c.status == Status::Warn).count(),
        err: checks.iter().filter(|c| c.status == Status::Err).count(),
        info: checks.iter().filter(|c| c.status == Status::Info).count(),
    };

    let exit_code = if summary.err > 0 { 2 } else if summary.warn > 0 { 1 } else { 0 };

    if json {
        let report = DoctorReport {
            platform: platform_string(),
            wg_version: env!("CARGO_PKG_VERSION").to_string(),
            checks,
            summary,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_plain(&checks, &summary);
    }

    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

fn platform_string() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn print_plain(checks: &[Check], summary: &Summary) {
    println!("=== wg doctor ===");
    println!("  Platform: {}", platform_string());
    println!("  wg version: {}", env!("CARGO_PKG_VERSION"));
    if let Ok(p) = std::env::current_exe() {
        println!("  wg exe: {}", p.display());
    }
    println!();

    for c in checks {
        let marker = match c.status {
            Status::Ok => "[OK]",
            Status::Warn => "[WARN]",
            Status::Err => "[ERR]",
            Status::Info => "[INFO]",
        };
        println!("  {:6} {}", marker, c.name);
        for line in c.detail.lines() {
            println!("           {}", line);
        }
        if let Some(h) = &c.hint {
            println!("           → {}", h);
        }
    }
    println!();
    println!(
        "  Summary: {} ok, {} warn, {} err ({} info)",
        summary.ok, summary.warn, summary.err, summary.info,
    );
}

// ── host tools ────────────────────────────────────────────────────────

fn check_host_tools() -> Vec<Check> {
    let mut out = Vec::new();

    // claude CLI
    match run_capture("claude", &["--version"]) {
        Some((true, stdout, _)) => {
            out.push(Check::ok(
                "claude CLI",
                format!("found: {}", stdout.lines().next().unwrap_or("").trim()),
            ));
        }
        Some((false, _, stderr)) => {
            out.push(Check::err(
                "claude CLI",
                format!("found but `claude --version` failed: {}", stderr.trim()),
                "Reinstall or reauthenticate via `claude login`.",
            ));
        }
        None => {
            out.push(Check::err(
                "claude CLI",
                "not found on PATH",
                "Install Claude Code, or run `claude setup-token` for a headless token.",
            ));
        }
    }

    // bash — use the same resolver the runtime uses, so doctor reflects
    // the bash wg will actually spawn (not whatever `bash` is first on
    // PATH, which on Windows is often the WSL shim).
    match workgraph::platform_bash::resolve_bash(None) {
        Ok(resolution) => {
            let bash_path_str = resolution.path.to_string_lossy().to_string();
            let source_tag = match resolution.source {
                workgraph::platform_bash::BashSource::Config => "config",
                workgraph::platform_bash::BashSource::Env => "env",
                workgraph::platform_bash::BashSource::KnownLocation => "well-known",
                workgraph::platform_bash::BashSource::Path => "PATH",
                workgraph::platform_bash::BashSource::Fallback => "fallback",
            };
            match run_capture_path(&resolution.path, &["--version"]) {
                Some((true, stdout, _)) => {
                    let first = stdout.lines().next().unwrap_or("").trim().to_string();
                    let detail = format!("[{}] {} — {}", source_tag, bash_path_str, first);
                    #[cfg(windows)]
                    {
                        if first.contains("msys") || first.contains("mingw") {
                            out.push(Check::ok("bash", format!("Git for Windows — {}", detail)));
                        } else {
                            out.push(Check::warn(
                                "bash",
                                format!("resolved to non-Git-for-Windows bash — {}", detail),
                                "Install Git for Windows. If you have it but wg picked the \
                                 wrong bash, set `[bash] path = \"C:\\\\Program Files\\\\Git\\\\bin\\\\bash.exe\"` \
                                 in `.workgraph/config.toml` or `WG_BASH_PATH` in your env.",
                            ));
                        }
                    }
                    #[cfg(not(windows))]
                    out.push(Check::ok("bash", detail));
                }
                _ => {
                    out.push(Check::err(
                        "bash",
                        format!(
                            "resolved to {} [{}] but it wouldn't run",
                            bash_path_str, source_tag
                        ),
                        "Check the file exists and is executable.",
                    ));
                }
            }
        }
        Err(e) => {
            out.push(Check::err(
                "bash",
                format!("could not resolve a usable bash: {}", e),
                "Install Git for Windows (on Windows) — workgraph wrappers require bash. \
                 You can also set `WG_BASH_PATH` or `[bash] path` in `.workgraph/config.toml`.",
            ));
        }
    }

    // git
    match run_capture("git", &["--version"]) {
        Some((true, stdout, _)) => out.push(Check::ok(
            "git",
            format!("found: {}", stdout.trim()),
        )),
        _ => out.push(Check::err(
            "git",
            "not found on PATH",
            "Install Git — workgraph uses `git worktree` for agent isolation.",
        )),
    }

    // GNU timeout vs Windows TIMEOUT.EXE.
    // On Windows, the one on PATH may be either. The Windows one is an
    // interactive pause utility; the GNU one is a command wrapper.
    // Workgraph's wrapper scripts rely on the GNU behavior.
    #[cfg(windows)]
    {
        // Modern wg uses `platform_timeout::spawn_with_timeout` internally
        // and does not depend on `timeout` being on PATH. Report what the
        // user has as informational only — Windows TIMEOUT.EXE ahead of
        // GNU coreutils is no longer a blocker.
        match run_capture("timeout", &["--version"]) {
            Some((true, stdout, _)) if stdout.contains("GNU coreutils") => {
                out.push(Check::info(
                    "timeout(1)",
                    format!("GNU: {}", stdout.lines().next().unwrap_or("").trim()),
                ));
            }
            Some(_) => {
                out.push(Check::info(
                    "timeout(1)",
                    "Windows timeout is first in PATH, which won't behave like \
                     Unix timeout if your workflow scripts call timeout. You'll \
                     need to change your PATH or hard-code the path to the \
                     correct timeout.",
                ));
            }
            None => {
                out.push(Check::info(
                    "timeout(1)",
                    "not found on PATH (not required by wg; Git for Windows \
                     provides it if needed)",
                ));
            }
        }
    }
    #[cfg(unix)]
    {
        if run_capture("timeout", &["--help"]).is_some() {
            out.push(Check::ok("timeout(1)", "found"));
        }
    }

    out
}

// ── auth ──────────────────────────────────────────────────────────────

fn check_auth(dir: &Path) -> Vec<Check> {
    let mut out = Vec::new();

    // Detect the classic env-var trap first: sk-ant-oat01-… tokens in
    // ANTHROPIC_API_KEY. The CLI sends them with the wrong header and
    // 401s every time, silently; the user sees synthetic placeholder
    // "Invalid API key" responses.
    if let Ok(v) = std::env::var("ANTHROPIC_API_KEY") {
        let v = v.trim().to_string();
        if v.is_empty() {
            // empty is fine, handled by the fall-through checks below
        } else if v.starts_with("sk-ant-oat01-") {
            out.push(Check::err(
                "ANTHROPIC_API_KEY",
                "is set to an `sk-ant-oat01-…` OAuth token",
                "OAuth tokens use Bearer auth; `ANTHROPIC_API_KEY` is sent as `x-api-key` and will \
                 always 401. Move it to `CLAUDE_CODE_OAUTH_TOKEN` instead.",
            ));
        } else if v.starts_with("sk-ant-api") {
            out.push(Check::ok(
                "ANTHROPIC_API_KEY",
                "set to a console API key",
            ));
        } else {
            out.push(Check::warn(
                "ANTHROPIC_API_KEY",
                format!("set but doesn't match known prefixes: starts with `{}…`", &v[..v.len().min(12)]),
                "Double-check the token format.",
            ));
        }
    }

    // OAuth token
    let oauth_env = std::env::var("CLAUDE_CODE_OAUTH_TOKEN").ok().filter(|v| !v.is_empty());
    if let Some(v) = oauth_env.as_ref() {
        if v.starts_with("sk-ant-oat01-") {
            out.push(Check::ok(
                "CLAUDE_CODE_OAUTH_TOKEN",
                "set to an `sk-ant-oat01-…` OAuth token",
            ));
        } else {
            out.push(Check::warn(
                "CLAUDE_CODE_OAUTH_TOKEN",
                "set but doesn't look like an `sk-ant-oat01-…` token",
                "Verify the token format; OAuth tokens use the `sk-ant-oat01-` prefix.",
            ));
        }
    }

    // MANAGED_BY_HOST / SDK refresh — the Claude-Code internal env vars
    // that leak through and misroute auth when the daemon was started from
    // inside a Claude Code session.
    if std::env::var("CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST").is_ok() {
        out.push(Check::warn(
            "CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST",
            "is set in the current shell",
            "This env var tells `claude` to prefer the host bridge (Claude Code's auth IPC) over \
             any token you configure. Fine if you're running interactively inside Claude Code, \
             bad if a detached daemon inherits it — the daemon will silently 401 on every call. \
             Start the daemon from a shell where this isn't set, or upgrade to a workgraph build \
             that strips it from spawned children (#30).",
        ));
    }

    // credentials.json from `claude login`
    if let Some(home) = dirs::home_dir() {
        let creds = home.join(".claude").join("credentials.json");
        if creds.exists() {
            out.push(Check::ok(
                "~/.claude/credentials.json",
                format!("present at {}", creds.display()),
            ));
        } else if oauth_env.is_none() {
            // Only flag as warning if there's no other auth source
            out.push(Check::warn(
                "claude login",
                "no `credentials.json` and no `CLAUDE_CODE_OAUTH_TOKEN` in env",
                "Run `claude login` for a refreshable credential, or set `CLAUDE_CODE_OAUTH_TOKEN`, \
                 or configure `[auth]` in `.workgraph/config.toml`.",
            ));
        }
    }

    // [auth] config
    let cfg_path = dir.join("config.toml");
    if cfg_path.exists()
        && let Ok(content) = std::fs::read_to_string(&cfg_path)
    {
        if content.contains("claude_code_oauth_token_file") {
            out.push(Check::ok("[auth] config", "token-file reference in .workgraph/config.toml"));
        } else if content.contains("claude_code_oauth_token") {
            out.push(Check::warn(
                "[auth] config",
                "inline token in .workgraph/config.toml",
                "Prefer `claude_code_oauth_token_file` so the token doesn't live in a file that \
                 might be committed. Make sure `.workgraph/config.toml` is gitignored if you keep \
                 it inline.",
            ));
        }
    }

    out
}

// ── workgraph dir ─────────────────────────────────────────────────────

fn check_workgraph_dir(dir: &Path) -> Vec<Check> {
    let mut out = Vec::new();

    if !dir.exists() {
        out.push(Check::err(
            ".workgraph dir",
            format!("{} does not exist", dir.display()),
            "Run `wg init` in your project root.",
        ));
        return out;
    }

    let graph = dir.join("graph.jsonl");
    if graph.exists() {
        // Rough count of task lines
        let line_count = std::fs::read_to_string(&graph)
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0);
        out.push(Check::ok(
            "graph.jsonl",
            format!("{} entries", line_count),
        ));
    } else {
        out.push(Check::warn(
            "graph.jsonl",
            format!("missing at {}", graph.display()),
            "Run `wg init` to create an empty graph, or `wg add` to start.",
        ));
    }

    out
}

// ── service daemon ────────────────────────────────────────────────────

fn check_daemon(dir: &Path) -> Vec<Check> {
    let mut out = Vec::new();

    let state_path = dir.join("service").join("state.json");
    if !state_path.exists() {
        out.push(Check::info("service daemon", "not running (no state.json)"));
        return out;
    }

    match std::fs::read_to_string(&state_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    {
        Some(v) => {
            let pid = v.get("pid").and_then(|p| p.as_u64()).unwrap_or(0);
            let socket = v.get("socket_path").and_then(|s| s.as_str()).unwrap_or("?").to_string();
            if pid > 0 {
                // state.json lingers after a crash/kill. Confirm the PID
                // is actually a live process before claiming "running".
                if is_pid_alive(pid as u32) {
                    out.push(Check::ok(
                        "service daemon",
                        format!("PID {} (alive)\nsocket: {}", pid, socket),
                    ));
                } else {
                    out.push(Check::warn(
                        "service daemon",
                        format!(
                            "state.json claims PID {} but that process is gone (stale state)",
                            pid
                        ),
                        "Remove `.workgraph/service/state.json` and start fresh with \
                         `wg service start`.",
                    ));
                }
            } else {
                out.push(Check::warn(
                    "service daemon",
                    "state.json present but no PID",
                    "Try `wg service stop --force` then `wg service start`.",
                ));
            }
        }
        None => {
            out.push(Check::warn(
                "service daemon",
                "state.json unreadable or malformed",
                "Remove `.workgraph/service/state.json` and restart with `wg service start`.",
            ));
        }
    }

    out
}

// ── windows specifics ─────────────────────────────────────────────────

#[cfg(windows)]
fn check_windows_specific() -> Vec<Check> {
    let mut out = Vec::new();

    // `cargo install --path .` puts wg.exe in ~/.cargo/bin; report if
    // current_exe looks like the verbatim-path form so users know what
    // they're seeing in logs.
    if let Ok(p) = std::env::current_exe() {
        let s = p.to_string_lossy();
        if s.starts_with(r"\\?\") {
            out.push(Check::info(
                "extended-length path",
                format!("`current_exe` is {}", s),
            ));
        }
    }

    // For comparison: what `where bash` sees on PATH (raw), alongside the
    // resolved path above. Useful when someone's asking "but why is wg
    // picking that bash when my PATH says something else" — this shows
    // both sides.
    if let Some((true, stdout, _)) = run_capture("where", &["bash"]) {
        let first = stdout.lines().next().unwrap_or("").trim().to_string();
        if !first.is_empty() {
            out.push(Check::info("PATH first bash", first));
        }
    }

    out
}

// ── helpers ───────────────────────────────────────────────────────────

/// Same as `run_capture` but takes a concrete path — used when we've
/// already resolved the binary (e.g. via `platform_bash::resolve_bash`)
/// and want to invoke exactly that one rather than whatever PATH yields.
fn run_capture_path(program: &Path, args: &[&str]) -> Option<(bool, String, String)> {
    let output = Command::new(program).args(args).output().ok()?;
    Some((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

/// Run a command; return `(success, stdout, stderr)` or `None` if the
/// binary wasn't found on PATH.
fn run_capture(program: &str, args: &[&str]) -> Option<(bool, String, String)> {
    let output = Command::new(program).args(args).output().ok()?;
    Some((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

/// Check whether a PID corresponds to a currently-running process.
///
/// Reuses the same pattern the daemon itself uses: Unix sends signal 0
/// (checks existence without delivering), Windows opens the process and
/// queries exit code.
fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: `kill(pid, 0)` is the standard existence probe — no signal
        // is actually sent; we only care about the errno.
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(windows)]
    {
        // Fall back to listing processes via tasklist: cheap enough for a
        // single check and avoids adding a winapi-surface dependency just
        // for this file.
        match run_capture("tasklist", &["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"]) {
            Some((true, stdout, _)) => !stdout.trim().is_empty() && stdout.contains(&pid.to_string()),
            _ => false,
        }
    }
}

// ── cargo target/ size ───────────────────────────────────────────────

const TARGET_SIZE_WARN_BYTES: u64 = 10 * 1024 * 1024 * 1024; // 10 GiB

fn check_cargo_target_size(dir: &Path) -> Vec<Check> {
    check_cargo_target_size_with_threshold(dir, TARGET_SIZE_WARN_BYTES)
}

fn check_cargo_target_size_with_threshold(dir: &Path, threshold: u64) -> Vec<Check> {
    let project_root = match dir.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };

    if !project_root.join("Cargo.toml").exists() {
        return Vec::new();
    }

    let target_dir = project_root.join("target");
    if !target_dir.exists() {
        return Vec::new();
    }

    let total_bytes: u64 = walkdir::WalkDir::new(&target_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();

    if total_bytes >= threshold {
        let gib = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        vec![Check::warn(
            "cargo target/ size",
            format!("target/ is {:.1} GiB", gib),
            "Run `cargo sweep --time 14` to prune old artifacts, and add \
             `debug = \"line-tables-only\"` to `[profile.dev]` in Cargo.toml \
             to reduce debug info size.",
        )]
    } else {
        Vec::new()
    }
}

// Silence "unused import" on non-Windows where some helpers only apply to
// the Windows branches above.
#[allow(dead_code)]
fn _unused_import_suppressor(_p: &PathBuf) {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_doctor_warns_on_large_target() {
        let tmp = TempDir::new().unwrap();
        // Simulate .workgraph dir — check_cargo_target_size takes this as `dir`
        let wg_dir = tmp.path().join(".workgraph");
        fs::create_dir(&wg_dir).unwrap();

        // Project root is tmp.path() — place Cargo.toml there
        fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        // Create target/ with files exceeding our test threshold
        let target_dir = tmp.path().join("target");
        fs::create_dir(&target_dir).unwrap();
        // Write a 2 KiB file; use a 1 KiB threshold
        fs::write(target_dir.join("bigfile.bin"), vec![0u8; 2048]).unwrap();

        let threshold = 1024; // 1 KiB for test
        let checks = check_cargo_target_size_with_threshold(&wg_dir, threshold);

        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].status, Status::Warn);
        assert!(checks[0].name.contains("target/"));
        assert!(checks[0].detail.contains("GiB"));
        let hint = checks[0].hint.as_ref().unwrap();
        assert!(hint.contains("cargo sweep"));
        assert!(hint.contains("line-tables-only"));
    }

    #[test]
    fn test_doctor_silent_no_cargo_toml() {
        let tmp = TempDir::new().unwrap();
        let wg_dir = tmp.path().join(".workgraph");
        fs::create_dir(&wg_dir).unwrap();
        // No Cargo.toml — should be silent
        let checks = check_cargo_target_size_with_threshold(&wg_dir, 1024);
        assert!(checks.is_empty());
    }

    #[test]
    fn test_doctor_silent_under_threshold() {
        let tmp = TempDir::new().unwrap();
        let wg_dir = tmp.path().join(".workgraph");
        fs::create_dir(&wg_dir).unwrap();

        fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        let target_dir = tmp.path().join("target");
        fs::create_dir(&target_dir).unwrap();
        // Write 100 bytes — under 1 KiB threshold
        fs::write(target_dir.join("small.bin"), vec![0u8; 100]).unwrap();

        let checks = check_cargo_target_size_with_threshold(&wg_dir, 1024);
        assert!(checks.is_empty());
    }
}
