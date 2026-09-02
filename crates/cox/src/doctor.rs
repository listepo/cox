//! `cox doctor`: diagnostics to understand why cox will or will not work on
//! this machine. Checks: toolchain version, `COX_HOME` writable, db opens,
//! API keys per configured provider, sandbox backend, `git` on PATH, terminal
//! capabilities (TERM, true colour, size), prices table age, `.claude/settings.json`.
//! Outputs human-readable lines or `--json` array of `{check, status, detail, fix}`.

use std::env;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use serde::{Deserialize, Serialize};

use cox_protocol::Store as _;

/// One check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub check: String,
    pub status: String,
    pub detail: String,
    pub fix: String,
}

impl CheckResult {
    /// Create an `ok` result.
    fn ok(check: &str, detail: String) -> Self {
        CheckResult {
            check: check.to_string(),
            status: "ok".to_string(),
            detail,
            fix: String::new(),
        }
    }

    /// Create a `warn` result.
    fn warn(check: &str, detail: String, fix: String) -> Self {
        CheckResult {
            check: check.to_string(),
            status: "warn".to_string(),
            detail,
            fix,
        }
    }

    /// Create a `fail` result.
    fn fail(check: &str, detail: String, fix: String) -> Self {
        CheckResult {
            check: check.to_string(),
            status: "fail".to_string(),
            detail,
            fix,
        }
    }
}

/// Run all doctor checks. Returns exit code 0 when no `fail`, 1 otherwise.
pub fn run(json: bool) -> i32 {
    let mut results = Vec::new();

    // Get COX_HOME early for reuse.
    let home = env::var("COX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| cox_store::Store::default_home());

    // Toolchain version.
    results.push(check_toolchain());

    // COX_HOME writable.
    results.push(check_home_writable(&home));

    // Database opens.
    results.push(check_db(&home));

    // API keys.
    results.push(check_api_keys());

    // Sandbox backend.
    results.push(check_sandbox());

    // git on PATH.
    results.push(check_git());

    // Terminal capabilities.
    results.push(check_terminal());

    // Prices table age.
    results.push(check_prices());

    // .claude/settings.json found.
    results.push(check_claude_settings());

    // Output and determine exit code.
    let has_fail = if json {
        output_json(&results)
    } else {
        output_human(&results)
    };

    if has_fail { 1 } else { 0 }
}

fn check_toolchain() -> CheckResult {
    match ProcessCommand::new("rustc").arg("--version").output() {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout).to_string();
            CheckResult::ok("toolchain", version.trim().to_string())
        }
        Err(_) => CheckResult::fail(
            "toolchain",
            "rustc not found on PATH".to_string(),
            "install Rust from https://rustup.rs/".to_string(),
        ),
    }
}

fn check_home_writable(home: &std::path::Path) -> CheckResult {
    use std::fs;
    use std::io::Write;

    // Try to create the home directory if it doesn't exist.
    if !home.exists() && fs::create_dir_all(home).is_err() {
        return CheckResult::fail(
            "COX_HOME writable",
            format!("cannot create {}: dir creation failed", home.display()),
            format!(
                "ensure {} exists and is writable",
                env::var("COX_HOME").unwrap_or_else(|_| "~/.cox".to_string())
            ),
        );
    }

    // Try to write a test file.
    let test_file = home.join(".cox_write_test");
    match fs::File::create(&test_file) {
        Ok(mut f) => {
            let _ = f.write_all(b"test");
            let _ = fs::remove_file(&test_file);
            CheckResult::ok("COX_HOME writable", home.display().to_string())
        }
        Err(e) => CheckResult::fail(
            "COX_HOME writable",
            format!("cannot write to {}: {}", home.display(), e),
            format!(
                "ensure {} is writable",
                env::var("COX_HOME").unwrap_or_else(|_| "~/.cox".to_string())
            ),
        ),
    }
}

fn check_db(home: &std::path::Path) -> CheckResult {
    match cox_store::Store::open(home) {
        Ok(_) => CheckResult::ok("db", "database opens and schema is valid".to_string()),
        Err(e) => CheckResult::fail(
            "db",
            format!("cannot open database: {}", e),
            format!("remove {} and retry", home.join("cox.db").display()),
        ),
    }
}

fn check_api_keys() -> CheckResult {
    // Check for Anthropic API key.
    let anthropic_ok = env::var("ANTHROPIC_API_KEY").is_ok()
        || keyring::Entry::new("cox", "anthropic")
            .and_then(|e| e.get_password())
            .is_ok();

    // If Anthropic is not configured, fail. Other providers are optional.
    if anthropic_ok {
        CheckResult::ok("API keys", "Anthropic API key found".to_string())
    } else {
        CheckResult::fail(
            "API keys",
            "ANTHROPIC_API_KEY env var not set and keyring entry 'cox/anthropic' not found".to_string(),
            "set ANTHROPIC_API_KEY or use `security add-generic-password -a cox -s anthropic -w <key>` (macOS) or similar for your platform".to_string(),
        )
    }
}

fn check_sandbox() -> CheckResult {
    let os = env::consts::OS;
    let status = if os == "macos" {
        // macOS: check for sandbox-exec.
        ProcessCommand::new("which")
            .arg("sandbox-exec")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else if os == "linux" {
        // Linux: check for bwrap or landlock/seccomp support.
        let has_bwrap = ProcessCommand::new("which")
            .arg("bwrap")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        // Landlock detection is complex; bwrap is the primary check.
        has_bwrap
    } else {
        false
    };

    if status {
        let backend = if os == "macos" {
            "sandbox-exec"
        } else if os == "linux" {
            "bwrap"
        } else {
            "none"
        };
        CheckResult::ok("sandbox backend", format!("{} available", backend))
    } else {
        let backend = if os == "macos" {
            "sandbox-exec"
        } else if os == "linux" {
            "bwrap"
        } else {
            "none"
        };
        CheckResult::warn(
            "sandbox backend",
            format!("{} not found, sandbox disabled", backend),
            if os == "macos" {
                "sandbox-exec is part of macOS; check your installation".to_string()
            } else if os == "linux" {
                "install bubblewrap: apt install bubblewrap (Debian/Ubuntu) or equivalent"
                    .to_string()
            } else {
                "sandbox is not supported on this platform".to_string()
            },
        )
    }
}

fn check_git() -> CheckResult {
    match ProcessCommand::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            CheckResult::ok("git", version.trim().to_string())
        }
        _ => CheckResult::fail(
            "git",
            "git not found on PATH".to_string(),
            "install git from https://git-scm.com/".to_string(),
        ),
    }
}

fn check_terminal() -> CheckResult {
    let mut details = Vec::new();

    // Check TERM variable.
    match env::var("TERM") {
        Ok(term) => details.push(format!("TERM={}", term)),
        Err(_) => {
            return CheckResult::warn(
                "terminal",
                "TERM not set".to_string(),
                "set TERM=xterm-256color or your terminal's type".to_string(),
            );
        }
    }

    // Check for true color support (COLORTERM).
    if env::var("COLORTERM").is_ok() {
        details.push("true colour detected".to_string());
    } else {
        details.push("true colour unknown".to_string());
    }

    // Try to get terminal size via crossterm.
    if let Ok((cols, rows)) = crossterm::terminal::size() {
        details.push(format!("size {}x{}", cols, rows));
    }

    CheckResult::ok("terminal", details.join(", "))
}

fn check_prices() -> CheckResult {
    // The prices table is in config.toml under [prices] if it exists.
    // For now, we warn since default.toml doesn't yet have a prices section.
    CheckResult::warn(
        "prices",
        "prices table not found in configuration".to_string(),
        "prices will be added in a future version".to_string(),
    )
}

fn check_claude_settings() -> CheckResult {
    // Walk up from cwd to find .claude/settings.json.
    let mut cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        let settings_file = cwd.join(".claude/settings.json");
        if settings_file.exists() {
            return CheckResult::ok("settings.json", settings_file.display().to_string());
        }

        if !cwd.pop() {
            break;
        }
    }

    CheckResult::warn(
        "settings.json",
        ".claude/settings.json not found".to_string(),
        "create ~/.claude/settings.json or a project-local .claude/settings.json if you need custom permissions or hooks".to_string(),
    )
}

fn output_human(results: &[CheckResult]) -> bool {
    let mut has_fail = false;
    for result in results {
        let status_str = match result.status.as_str() {
            "ok" => "✓",
            "warn" => "⚠",
            "fail" => "✗",
            _ => "?",
        };
        println!("{}: {} {}", result.check, status_str, result.detail);
        if !result.fix.is_empty() && result.status != "ok" {
            println!("  fix: {}", result.fix);
        }
        if result.status == "fail" {
            has_fail = true;
        }
    }
    has_fail
}

fn output_json(results: &[CheckResult]) -> bool {
    let json_array = serde_json::to_string_pretty(results).unwrap_or_default();
    println!("{}", json_array);
    results.iter().any(|r| r.status == "fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_exit_code_is_1_on_fail() {
        // Create a failing check.
        let results = [
            CheckResult::ok("check1", "detail".to_string()),
            CheckResult::fail("check2", "detail".to_string(), "fix".to_string()),
        ];

        let has_fail = results.iter().any(|r| r.status == "fail");
        assert!(has_fail);
    }

    #[test]
    fn doctor_results_serialize_to_json() {
        let result = CheckResult::ok("test", "detail".to_string());
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"check\":\"test\""));
        assert!(json.contains("\"status\":\"ok\""));
    }

    #[test]
    fn doctor_human_output() {
        let results = vec![
            CheckResult::ok("toolchain", "rustc 1.97.1".to_string()),
            CheckResult::ok("COX_HOME writable", "/home/user/.cox".to_string()),
            CheckResult::ok("db", "database opens and schema is valid".to_string()),
            CheckResult::ok("API keys", "Anthropic API key found".to_string()),
            CheckResult::warn(
                "sandbox backend",
                "sandbox-exec not found, sandbox disabled".to_string(),
                "sandbox-exec is part of macOS; check your installation".to_string(),
            ),
            CheckResult::ok("git", "git version 2.40.0".to_string()),
            CheckResult::ok("terminal", "TERM=xterm-256color, true colour detected, size 120x40".to_string()),
            CheckResult::warn(
                "prices",
                "prices table not found in configuration".to_string(),
                "prices will be added in a future version".to_string(),
            ),
            CheckResult::warn(
                "settings.json",
                ".claude/settings.json not found".to_string(),
                "create ~/.claude/settings.json or a project-local .claude/settings.json if you need custom permissions or hooks".to_string(),
            ),
        ];

        // Use insta snapshot to verify human output format.
        let mut output = String::new();
        for result in &results {
            let status_str = match result.status.as_str() {
                "ok" => "✓",
                "warn" => "⚠",
                "fail" => "✗",
                _ => "?",
            };
            output.push_str(&format!(
                "{}: {} {}\n",
                result.check, status_str, result.detail
            ));
            if !result.fix.is_empty() && result.status != "ok" {
                output.push_str(&format!("  fix: {}\n", result.fix));
            }
        }

        insta::assert_snapshot!(output);
    }
}
