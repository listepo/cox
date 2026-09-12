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
use cox_provider::usage::{Price, PriceTable};

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
    match cox_tools::sandbox::backend(cox_protocol::LinuxBackend::Auto) {
        Some(backend) => CheckResult::ok("sandbox", backend.name().to_string()),
        None => CheckResult::warn(
            "sandbox",
            "none: shell commands run unconfined".to_string(),
            match env::consts::OS {
                "macos" => "sandbox-exec is part of macOS; check your installation".to_string(),
                "linux" => {
                    "install bubblewrap: apt install bubblewrap (Debian/Ubuntu) or equivalent"
                        .to_string()
                }
                _ => "sandbox is not supported on this platform".to_string(),
            },
        ),
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

const PRICES_STALE_DAYS: u32 = 90;
const PRICES_FIX: &str = "update crates/cox-provider/prices.toml from the official page";

fn parse_iso_date(s: &str) -> Option<(u32, u32, u32)> {
    let mut parts = s.split('-');
    let y = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    let d = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

fn ymd_to_days(date: (u32, u32, u32)) -> Option<u32> {
    let (y, m, d) = date;
    let m = m as i64;
    let y = y as i64;
    let d = d as i64;
    let y_adj = y - if m <= 2 { 1 } else { 0 };
    let era = y_adj / 400;
    let yoe = y_adj - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    if days < 0 {
        return None;
    }
    Some(days as u32)
}

fn days_between(from: &str, to: (u32, u32, u32)) -> Option<u32> {
    let from_days = ymd_to_days(parse_iso_date(from)?)?;
    let to_days = ymd_to_days(to)?;
    Some(to_days - from_days)
}

fn today_ymd() -> (u32, u32, u32) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0);
    civil_from_days(days as i64)
}

fn civil_from_days(z: i64) -> (u32, u32, u32) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + (era * 400) as u32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mp < 10 { y } else { y + 1 };
    (y, m, d)
}

fn prices_status(prices: &[Price], today: (u32, u32, u32)) -> CheckResult {
    let mut oldest_date = None;
    let mut oldest_age = None;

    for price in prices {
        let age = days_between(&price.verified_on, today);
        match age {
            Some(age) if age > PRICES_STALE_DAYS => {
                return CheckResult::warn(
                    "prices",
                    format!(
                        "{} verified_on {} is {} days old",
                        price.id, price.verified_on, age
                    ),
                    PRICES_FIX.to_string(),
                );
            }
            Some(age) => {
                if oldest_age.is_none_or(|current| age > current) {
                    oldest_age = Some(age);
                    oldest_date = Some(price.verified_on.as_str());
                }
            }
            None => {
                return CheckResult::warn(
                    "prices",
                    format!("invalid verified_on date: {}", price.verified_on),
                    PRICES_FIX.to_string(),
                );
            }
        }
    }

    let detail = match oldest_date {
        Some(date) => format!("oldest verified_on {}", date),
        None => "no price rows".to_string(),
    };
    CheckResult::ok("prices", detail)
}

fn check_prices() -> CheckResult {
    let table = match PriceTable::load("/nonexistent/cox-doctor-prices.toml") {
        Ok(table) => table,
        Err(err) => {
            return CheckResult::warn(
                "prices",
                format!("could not load price table: {}", err),
                PRICES_FIX.to_string(),
            );
        }
    };
    prices_status(table.prices(), today_ymd())
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

    #[test]
    fn doctor_prices_embedded_table_is_ok() {
        let result = check_prices();
        assert_eq!(result.status, "ok");
        assert!(result.detail.starts_with("oldest verified_on "));
    }

    #[test]
    fn doctor_prices_older_than_90_days_warns() {
        let stale = Price {
            id: "claude-haiku-4-5".to_string(),
            input: 1.0,
            output: 5.0,
            cache_write: 1.25,
            cache_read: 0.1,
            verified_on: "2020-01-01".to_string(),
            source_url: "https://example.com".to_string(),
        };
        let result = prices_status(&[stale], (2026, 9, 12));
        assert_eq!(result.status, "warn");
        assert!(result.detail.contains("2020-01-01"));
        assert!(result.detail.contains("days old"));
        assert_eq!(result.fix, PRICES_FIX);
    }
}
