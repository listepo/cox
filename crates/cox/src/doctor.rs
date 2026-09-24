//! `cox doctor`: diagnostics to understand why cox will or will not work on
//! this machine. Checks: toolchain version, `COX_HOME` writable, db opens,
//! API keys per configured provider, sandbox backend, `git` on PATH, terminal
//! capabilities (TERM, true colour, size), prices table age, `.claude/settings.json`,
//! and one OAuth row per HTTP MCP server (T22.5).
//! Outputs human-readable lines or `--json` array of `{check, status, detail, fix}`.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use serde::{Deserialize, Serialize};

use cox_protocol::Store as _;
use cox_protocol::config::McpServerConfig;
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
/// `tui_theme` is `config.tui.theme`, shown (and queried when `"auto"`) by
/// `check_terminal`; `tui_caps` is `config.tui.caps` (T23.0), the `[tui.caps]`
/// overrides `check_terminal` reports alongside the detected/queried value.
/// `config` is the loaded config: `check_prefix` (T30.1) assembles the active
/// profile's prefix and reports its T1.8 estimate.
pub fn run(
    json: bool,
    mcp: &HashMap<String, McpServerConfig>,
    tui_theme: &str,
    tui_caps: &HashMap<String, bool>,
    config: &cox_protocol::Config,
) -> i32 {
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

    // Terminal capabilities, including `tui.theme = "auto"` (T22.6) and
    // `cox_tui::term::Caps` (T23.0).
    results.push(check_terminal(tui_theme, tui_caps));

    // Prices table age.
    results.push(check_prices());

    // .claude/settings.json found.
    results.push(check_claude_settings());

    // Assembled-prefix token count for the active profile (T30.1).
    results.push(check_prefix(config));

    // Keybindings file and the Claude import: bad entries and clashes (T25.5).
    results.push(check_keybindings(
        &home,
        &crate::config_load::home_dir().join(".claude"),
    ));

    // One row per HTTP MCP server: is its token usable?
    let mut names: Vec<&String> = mcp
        .iter()
        .filter(|(_, c)| c.url.is_some())
        .map(|(n, _)| n)
        .collect();
    names.sort();
    results.extend(names.into_iter().map(|name| check_mcp_auth(name)));

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

fn check_terminal(tui_theme: &str, tui_caps: &HashMap<String, bool>) -> CheckResult {
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

    // `tui.theme = "auto"` (T22.6): same OSC 11 query `run_tui` makes,
    // reported here so `doctor` explains what a session will resolve to
    // without opening one.
    details.push(match tui_theme {
        "auto" => match cox_tui::color::detect_dark(cox_tui::color::OSC11_TIMEOUT) {
            Some(true) => "theme: auto → dark (OSC 11 reply)".to_string(),
            Some(false) => "theme: auto → light (OSC 11 reply)".to_string(),
            None => "theme: auto → dark (no OSC 11 reply)".to_string(),
        },
        other => format!("theme: {other}"),
    });

    // `cox_tui::term::Caps` (T23.0): one row per field with the source that
    // decided it — `config` (`[tui.caps]`) beats `query` (the real
    // keyboard-protocol probe, tty only) beats `env` (the base guess).
    let env_fn = |key: &str| env::var(key).ok();
    let mut caps = cox_tui::term::Caps::detect(&env_fn);
    let queried = caps.query(cox_tui::term::KITTY_QUERY_TIMEOUT);
    let before_config = caps;
    caps.apply(tui_caps);
    let caps_report: Vec<String> = caps
        .fields()
        .into_iter()
        .zip(before_config.fields())
        .map(|((name, value), (_, pre_config))| {
            let source = if value != pre_config {
                "config"
            } else if name == "kitty_keyboard" && queried {
                "query"
            } else {
                "env"
            };
            format!("{name}={value} ({source})")
        })
        .collect();
    details.push(caps_report.join(", "));
    // T23.5: what `tui.notify` writes on this terminal.
    let vte = cox_tui::term::is_vte(&env_fn);
    details.push(format!(
        "notify via {}",
        cox_tui::term::notify_via(&caps, vte)
    ));

    CheckResult::ok("terminal", details.join(", "))
}

const PRICES_STALE_DAYS: u32 = 90;
const PRICES_FIX: &str = "update crates/cox-provider/prices.toml from the official page";

/// `mcp auth <name>`: `ok (expires in 3h)`, `ok (no expiry)`, `expired` or
/// `none`. `none` is fine — the server may not ask for a login at all.
fn check_mcp_auth(name: &str) -> CheckResult {
    use cox_mcp::auth::{Status, status, stored};
    let check = format!("mcp auth {name}");
    match stored(name) {
        Ok(creds) => match status(creds.as_ref(), cox_mcp::auth::now()) {
            s @ Status::Expired => {
                CheckResult::warn(&check, s.to_string(), format!("run `cox mcp login {name}`"))
            }
            s => CheckResult::ok(&check, s.to_string()),
        },
        Err(e) => CheckResult::warn(
            &check,
            format!("keyring: {e}"),
            format!("run `cox mcp login {name}` once the keyring is available"),
        ),
    }
}

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

/// T25.5: the same map the TUI would build; a warning for every entry it
/// skipped and every key two user bindings both claim in one context.
fn check_keybindings(cox_home: &std::path::Path, claude_home: &std::path::Path) -> CheckResult {
    let loaded = crate::config_load::keymap(cox_home, claude_home);
    let mut problems = loaded.warnings;
    problems.extend(loaded.keymap.conflicts());
    if problems.is_empty() {
        return CheckResult::ok("keybindings", "no conflicts".to_string());
    }
    CheckResult::warn(
        "keybindings",
        problems.join("; "),
        format!(
            "edit {} (docs/config.md, keybindings)",
            cox_home.join("keybindings.toml").display()
        ),
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

/// The assembled-prefix token count for the active profile (T30.1): an
/// empty-history request through `cox_core::assemble` priced by the T1.8
/// estimator, so `doctor` names what the next turn will actually send.
fn check_prefix(config: &cox_protocol::Config) -> CheckResult {
    let req = cox_core::assemble(&[], config, &[], std::path::Path::new("."), "");
    let tokens = cox_provider::tokens::estimate(&req).tokens;
    let profile = if config.core.profile.is_empty() {
        "default"
    } else {
        config.core.profile.as_str()
    };
    CheckResult::ok("prefix", format!("{tokens} tokens (profile {profile})"))
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
    fn doctor_warns_on_keybinding_conflicts() {
        let cox = tempfile::tempdir().expect("tempdir");
        let claude = tempfile::tempdir().expect("tempdir");
        let ok = check_keybindings(cox.path(), claude.path());
        assert_eq!(
            (ok.status.as_str(), ok.detail.as_str()),
            ("ok", "no conflicts")
        );
        std::fs::write(
            cox.path().join("keybindings.toml"),
            "send = \"ctrl+o\"\ntranscript = \"ctrl+o\"\n",
        )
        .expect("write");
        let warn = check_keybindings(cox.path(), claude.path());
        assert_eq!(warn.status, "warn");
        assert!(warn.detail.contains("send"), "{}", warn.detail);
        assert!(warn.detail.contains("transcript"), "{}", warn.detail);
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
