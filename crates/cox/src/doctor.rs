//! `cox doctor`: diagnostics to understand why cox will or will not work on
//! this machine. Checks: toolchain version, `COX_HOME` writable, db opens,
//! API keys per configured provider, sandbox backend, `git` on PATH, terminal
//! capabilities (TERM, true colour, size), prices table age, whether every
//! configured model has a catalog price (T30.27), what LM Studio runs when
//! it is the code tier's provider (T30.16), `.claude/settings.json`,
//! and one OAuth row per HTTP MCP server (T22.5).
//! Outputs human-readable lines or `--json` array of `{check, status, detail, fix}`.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use serde::{Deserialize, Serialize};

use cox_protocol::Store as _;
use cox_protocol::config::McpServerConfig;
use cox_provider::usage::{Price, load_price_table};

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
    results.push(check_api_keys(config));

    // Sandbox backend.
    results.push(check_sandbox());

    // git on PATH.
    results.push(check_git());

    // Terminal capabilities, including `tui.theme = "auto"` (T22.6) and
    // `cox_tui::term::Caps` (T23.0).
    results.push(check_terminal(tui_theme, tui_caps));

    // Prices table age.
    results.push(check_prices());

    // What LM Studio runs for the code tier (T30.16), only when it is the
    // code tier's provider: nobody else needs port 1234 probed.
    if config.tiers.code.provider == "lmstudio" {
        results.push(check_lmstudio(config));
    }

    // Every model reachable from [tiers.*] or [providers.*].models has a
    // catalog price (T30.27).
    results.push(check_catalog_prices(config));

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

/// What the `code` tier's provider needs for a key (T30.21).
#[derive(Debug, PartialEq)]
enum KeyRequirement<'a> {
    /// The section's name, the env var its `api_key_env` names, and whether
    /// a missing key is fatal: Anthropic and Jev fail without one;
    /// OpenAI-shaped sections run keyless against a local server.
    Key(&'a str, &'a str, bool),
    /// `local` never sends a key.
    None,
    /// No `[providers.<name>]` section: the session refuses to start, so
    /// doctor must not call this "needs no key".
    UnknownProvider(&'a str),
}

fn key_requirement(config: &cox_protocol::Config) -> KeyRequirement<'_> {
    let section = config.tiers.code.provider.as_str();
    let p = &config.providers;
    match section {
        "anthropic" => KeyRequirement::Key(section, p.anthropic.api_key_env.as_str(), true),
        "typesafe" => KeyRequirement::Key(section, p.typesafe.api_key_env.as_str(), true),
        "openai" => KeyRequirement::Key(section, p.openai.api_key_env.as_str(), false),
        "local" => KeyRequirement::None,
        // T30.15: same optional-key shape as `openai` — LM Studio runs
        // keyless unless "Require Authentication" is on.
        "lmstudio" => KeyRequirement::Key(section, p.lmstudio.api_key_env.as_str(), false),
        _ => match p.custom.get(section) {
            Some(c) => KeyRequirement::Key(section, c.api_key_env.as_str(), false),
            None => KeyRequirement::UnknownProvider(section),
        },
    }
}

/// [`check_api_keys`]'s body with the credential lookup injected, so a test
/// can exercise every branch (unknown provider, keyless, found, missing)
/// without ever touching the real keyring (A49, T30.28).
fn check_api_keys_with(
    config: &cox_protocol::Config,
    resolve: impl FnOnce(&str, &str) -> Result<String, cox_protocol::errors::ProviderError>,
) -> CheckResult {
    let (section, env_var, required) = match key_requirement(config) {
        KeyRequirement::Key(section, env_var, required) => (section, env_var, required),
        KeyRequirement::None => {
            return CheckResult::ok(
                "API keys",
                "the code tier's provider needs no key".to_string(),
            );
        }
        KeyRequirement::UnknownProvider(section) => {
            return CheckResult::fail(
                "API keys",
                format!("tiers.code.provider `{section}` has no [providers.{section}] section"),
                format!(
                    "add [providers.{section}] or point tiers.code.provider at a configured provider"
                ),
            );
        }
    };
    if resolve(env_var, section).is_ok() {
        return CheckResult::ok("API keys", format!("{section} key found"));
    }
    let detail = format!("{env_var} is not set and keyring entry 'cox/{section}' not found");
    let fix = format!(
        "set {env_var} or run `security add-generic-password -s cox -a {section} -w` (macOS) or your platform's keyring equivalent"
    );
    if required {
        CheckResult::fail("API keys", detail, fix)
    } else {
        CheckResult::warn(
            "API keys",
            format!("{detail}; requests go out without a key"),
            fix,
        )
    }
}

/// Resolves the key exactly as the provider will (`cox_provider::http::resolve_key`:
/// the section's env var, then keyring `cox/<section>`), so doctor and the
/// session never disagree about whether a key exists.
fn check_api_keys(config: &cox_protocol::Config) -> CheckResult {
    check_api_keys_with(config, cox_provider::http::resolve_key)
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
const PRICES_FIX: &str =
    "regenerate crates/cox-provider/prices.toml with `just vendor models` (plan.md A48)";

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
    let table = match load_price_table("/nonexistent/cox-doctor-prices.toml") {
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

const CATALOG_PRICES_FIX: &str = "run `uv run --project scripts/vendor cox-vendor models` to add a price row for these models (plan.md A48)";

/// [`check_catalog_prices`]'s body with the catalog already built, so a
/// test can hand it a small `ids` list instead of a whole `Config`
/// (`docs/design/providers.md` § Target shape item 5, T30.27).
fn catalog_prices_status(catalog: &cox_models::Catalog, ids: &[String]) -> CheckResult {
    let mut unpriced: Vec<&str> = ids
        .iter()
        .filter(|id| {
            catalog
                .get(id.as_str())
                .is_none_or(|row| row.price.is_none())
        })
        .map(String::as_str)
        .collect();
    unpriced.sort();
    unpriced.dedup();
    if unpriced.is_empty() {
        return CheckResult::ok(
            "catalog prices",
            format!("{} configured models priced", ids.len()),
        );
    }
    CheckResult::warn(
        "catalog prices",
        format!("no catalog price for: {}", unpriced.join(", ")),
        CATALOG_PRICES_FIX.to_string(),
    )
}

/// A model reachable from `[tiers.*]` or `[providers.*].models` — every
/// section, native and compatible — with no catalog price: the sync check
/// between a user's own config and `prices.toml`
/// (`docs/design/providers.md` § Target shape item 5, T30.27).
/// `Config::configured_model_ids` (cox-protocol) is the same model
/// enumeration `cox_models::price`'s `usage_prices_cover_every_configured_model`
/// test uses for `default.toml`, so the two checks can never disagree; this
/// row also covers a user's own `[providers.*]` config, which that test
/// never sees.
fn check_catalog_prices(config: &cox_protocol::Config) -> CheckResult {
    let ids = config.configured_model_ids();
    match cox_models::Catalog::load(config, None) {
        Ok(catalog) => catalog_prices_status(&catalog, &ids),
        Err(e) => CheckResult::fail(
            "catalog prices",
            format!("could not build model catalog: {e}"),
            "check [providers.*] for a malformed models entry".to_string(),
        ),
    }
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

/// One result as the human output prints it: the status line, then a
/// `fix:` line unless it passed.
fn human(result: &CheckResult) -> String {
    let status_str = match result.status.as_str() {
        "ok" => "✓",
        "warn" => "⚠",
        "fail" => "✗",
        _ => "?",
    };
    let mut out = format!("{}: {} {}\n", result.check, status_str, result.detail);
    if !result.fix.is_empty() && result.status != "ok" {
        out.push_str(&format!("  fix: {}\n", result.fix));
    }
    out
}

fn output_human(results: &[CheckResult]) -> bool {
    for result in results {
        print!("{}", human(result));
    }
    results.iter().any(|r| r.status == "fail")
}

/// How long doctor waits on LM Studio: a hung server must not hang doctor.
const LMSTUDIO_DOCTOR_TIMEOUT_S: u32 = 5;

/// Asks LM Studio's native API about the code tier's model (T30.16) with
/// the key the session would use. Read-only: doctor never loads a model.
fn check_lmstudio(config: &cox_protocol::Config) -> CheckResult {
    let l = &config.providers.lmstudio;
    let mut transport = l.transport();
    transport.timeout_s = LMSTUDIO_DOCTOR_TIMEOUT_S;
    let model = crate::session::lmstudio_model(config);
    let key = cox_provider::http::resolve_key(&transport.api_key_env, "lmstudio").ok();
    let list = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| cox_protocol::errors::ProviderError::Network)
        .and_then(|rt| {
            let client = cox_provider::lmstudio::LmStudio::new(&transport, key)?;
            rt.block_on(client.models())
        });
    lmstudio_row(&transport.base_url, model, list)
}

/// [`check_lmstudio`]'s verdict from the model list alone, so every branch
/// is testable without a server.
fn lmstudio_row(
    base_url: &str,
    model: &str,
    list: Result<cox_provider::lmstudio::ModelList, cox_protocol::errors::ProviderError>,
) -> CheckResult {
    const CHECK: &str = "LM Studio";
    let list = match list {
        Ok(list) => list,
        Err(e) => {
            return CheckResult::fail(
                CHECK,
                format!("{base_url} unreachable or rejected the model list: {e}"),
                "start the server (`lms server start`) or fix providers.lmstudio.base_url"
                    .to_string(),
            );
        }
    };
    let Some(m) = list.find(model) else {
        return CheckResult::fail(
            CHECK,
            format!("{base_url} reachable; `{model}` is not downloaded"),
            format!("`lms get {model}`, or point tiers.code.model at a listed model"),
        );
    };
    let max = m
        .max_context_length
        .map_or_else(|| "?".to_string(), |n| n.to_string());
    let tools = match m.tool_use() {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    };
    match m.loaded_context() {
        Some(loaded) => {
            let detail = format!(
                "{base_url} reachable; `{model}` loaded, context {loaded} loaded / {max} max; tool use: {tools}"
            );
            if m.tool_use() == Some(true) {
                CheckResult::ok(CHECK, detail)
            } else {
                CheckResult::warn(
                    CHECK,
                    detail,
                    "pick a model trained for tool use; cox drives a tool loop".to_string(),
                )
            }
        }
        None => CheckResult::warn(
            CHECK,
            format!(
                "{base_url} reachable; `{model}` not loaded (max context {max}); tool use: {tools}"
            ),
            format!("`lms load {model}`, or set providers.lmstudio.load = true"),
        ),
    }
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
    fn doctor_checks_the_key_the_code_tier_provider_names() {
        let mut config = cox_protocol::Config::default();
        config.tiers.code.provider = "anthropic".into();
        config.providers.anthropic.api_key_env = "MY_ANTHROPIC_KEY".into();
        assert_eq!(
            key_requirement(&config),
            KeyRequirement::Key("anthropic", "MY_ANTHROPIC_KEY", true)
        );
        config.tiers.code.provider = "local".into();
        assert_eq!(key_requirement(&config), KeyRequirement::None);
        config.tiers.code.provider = "lmstudio".into();
        config.providers.lmstudio.api_key_env = "LM_API_TOKEN".into();
        assert_eq!(
            key_requirement(&config),
            KeyRequirement::Key("lmstudio", "LM_API_TOKEN", false)
        );
        config.tiers.code.provider = "deepseek".into();
        config.providers.custom.insert(
            "deepseek".into(),
            cox_protocol::config::CompatibleProviderConfig {
                api_key_env: "DEEPSEEK_API_KEY".into(),
                ..Default::default()
            },
        );
        assert_eq!(
            key_requirement(&config),
            KeyRequirement::Key("deepseek", "DEEPSEEK_API_KEY", false)
        );
    }

    #[test]
    fn doctor_fails_when_the_code_tier_names_an_unknown_provider() {
        // The session refuses this config ("unknown provider"); doctor must
        // not report it as a provider that needs no key. Returns before any
        // keyring lookup — enforced here by a lookup that panics if called
        // (A49, T30.28).
        let mut config = cox_protocol::Config::default();
        config.tiers.code.provider = "nosuch".into();
        let result = check_api_keys_with(&config, |_, _| {
            panic!("an unknown provider must fail before any key is resolved")
        });
        assert_eq!(result.status, "fail", "{}", result.detail);
        assert!(
            result.detail.contains("[providers.nosuch]"),
            "{}",
            result.detail
        );
    }

    #[test]
    fn doctor_warns_not_fails_when_a_keyless_section_has_no_key() {
        // A section name no keyring holds and an env var nobody sets —
        // simulated with an injected lookup rather than the real keyring
        // (A49, T30.28).
        let section = "cox-doctor-test-keyless";
        let mut config = cox_protocol::Config::default();
        config.tiers.code.provider = section.into();
        config.providers.custom.insert(
            section.into(),
            cox_protocol::config::CompatibleProviderConfig {
                api_key_env: "COX_DOCTOR_TEST_UNSET_KEY".into(),
                ..Default::default()
            },
        );
        let result = check_api_keys_with(&config, |_, _| {
            Err(cox_protocol::errors::ProviderError::Auth)
        });
        assert_eq!(result.status, "warn", "{}", result.detail);
        // The keyring hint names service `cox`, account `<section>` — the
        // order `keyring::Entry::new("cox", section)` reads.
        assert!(
            result.fix.contains(&format!("-s cox -a {section}")),
            "{:?}",
            result.fix
        );
    }

    #[test]
    fn doctor_results_serialize_to_json() {
        let result = CheckResult::ok("test", "detail".to_string());
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"check\":\"test\""));
        assert!(json.contains("\"status\":\"ok\""));
    }

    /// T30.16: the LM Studio row for a loaded model, an unloaded one, one
    /// the server does not list, and a server that is down.
    #[test]
    fn doctor_lmstudio_rows() {
        let raw = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/lmstudio/models.json"),
        )
        .expect("fixture");
        let list: cox_provider::lmstudio::ModelList =
            serde_json::from_str(&raw).expect("fixture parses");
        let mut unloaded = list.clone();
        for m in &mut unloaded.models {
            m.loaded_instances.clear();
        }
        let url = "http://localhost:1234";
        let rows = [
            lmstudio_row(url, "prism-ml/bonsai-27b", Ok(list.clone())),
            lmstudio_row(url, "prism-ml/bonsai-27b", Ok(unloaded)),
            lmstudio_row(
                url,
                "text-embedding-nomic-embed-text-v1.5",
                Ok(list.clone()),
            ),
            lmstudio_row(url, "nope/absent", Ok(list)),
            lmstudio_row(
                url,
                "prism-ml/bonsai-27b",
                Err(cox_protocol::errors::ProviderError::Network),
            ),
        ];
        assert_eq!(
            rows.iter().map(|r| r.status.as_str()).collect::<Vec<_>>(),
            ["ok", "warn", "warn", "fail", "fail"]
        );
        insta::assert_snapshot!(rows.iter().map(human).collect::<String>());
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

    #[test]
    fn catalog_prices_check_is_ok_on_the_default_config() {
        let config = cox_protocol::Config::default();
        let result = check_catalog_prices(&config);
        assert_eq!(result.status, "ok", "{}", result.detail);
    }

    #[test]
    fn catalog_prices_check_warns_and_names_an_unpriced_model() {
        // A model reachable from `[providers.*].models` with no row in
        // `prices.toml` (built-in or user) is a warning, not a failure —
        // an unpriced model still runs, just costed 0 and `estimated`
        // (`cox_models::PriceTable::apply`).
        let mut config = cox_protocol::Config::default();
        config
            .providers
            .anthropic
            .models
            .push(cox_protocol::config::ProviderModel {
                id: "claude-doctor-test-unpriced".into(),
                context_window: 100_000,
                efforts: vec![],
                ..Default::default()
            });
        let result = check_catalog_prices(&config);
        assert_eq!(result.status, "warn", "{}", result.detail);
        assert!(
            result.detail.contains("claude-doctor-test-unpriced"),
            "{}",
            result.detail
        );
        assert_eq!(result.fix, CATALOG_PRICES_FIX);
    }

    #[test]
    fn catalog_prices_check_names_every_unpriced_model_reachable_from_tiers() {
        // `[tiers.*].model` is the other reachability path the goal names,
        // alongside `[providers.*].models`.
        let mut config = cox_protocol::Config::default();
        config.tiers.cheap.model = "claude-doctor-test-tier-unpriced".into();
        let result = check_catalog_prices(&config);
        assert_eq!(result.status, "warn", "{}", result.detail);
        assert!(
            result.detail.contains("claude-doctor-test-tier-unpriced"),
            "{}",
            result.detail
        );
    }
}
