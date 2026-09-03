//! The `Config` struct tree that mirrors `config/default.toml` (plan.md
//! §1.6) key for key. This crate has no logic beyond serde (see `lib.rs`),
//! so all the figment layering, precedence, provenance tracking and the
//! `cox config` subcommand live in `crates/cox/src/config_load.rs` and
//! `crates/cox/src/config_cmd.rs`; what lives here is only the shape every
//! layer deserializes into, plus the embedded default file those layers
//! start from.
//!
//! Every struct is `#[serde(deny_unknown_fields, default)]`: a typo in a
//! user or project `config.toml` is a hard error at load time (surfaced by
//! the loader), and any field a layer omits falls back to that struct's own
//! `Default` impl — which is hand-written, not derived, because most of
//! `default.toml`'s values are not a Rust type's zero value (`true`,
//! non-empty strings, non-zero numbers).

use std::collections::HashMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::types::{
    ApprovalPolicy, Effort, LinuxBackend, PermissionMode, SandboxMode, Thinking, Tier,
};

/// The embedded lowest-precedence config layer (plan.md §1.6/D13):
/// `crates/cox/src/config_load.rs` merges this beneath the user, project,
/// env and flag layers via `figment::providers::Toml::string`.
pub const DEFAULT_CONFIG_TOML: &str = include_str!("../../../config/default.toml");

/// The full configuration tree (plan.md §1.6), one field per top-level
/// `default.toml` table.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// `[core]`
    pub core: CoreConfig,
    /// `[tiers.cheap]` / `[tiers.code]` / `[tiers.think]`
    pub tiers: TiersConfig,
    /// `[jobs]`
    pub jobs: JobsConfig,
    /// `[providers.anthropic]` / `[providers.openai]` / `[providers.local]`
    pub providers: ProvidersConfig,
    /// `[context]`
    pub context: ContextConfig,
    /// `[permissions]`
    pub permissions: PermissionsConfig,
    /// `[sandbox]`
    pub sandbox: SandboxConfig,
    /// `[budget]`
    pub budget: BudgetConfig,
    /// `[tui]`
    pub tui: TuiConfig,
    /// `[hooks]`
    pub hooks: HooksConfig,
    /// `[mcp]`
    pub mcp: McpConfig,
    /// `[memory]`
    pub memory: MemoryConfig,
    /// `[telemetry]`
    pub telemetry: TelemetryConfig,
    /// `[record]`
    pub record: RecordConfig,
}

/// What a child process cox spawns (`bash`, hooks, stdio MCP servers)
/// inherits from cox's environment; everything else — API keys above all —
/// stays behind (D14).
pub const CHILD_ENV_ALLOWLIST: &[&str] = &[
    "PATH", "HOME", "LANG", "LC_ALL", "LC_CTYPE", "TERM", "TMPDIR", "USER", "SHELL",
];

/// `[core]` (plan.md §1.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct CoreConfig {
    /// `core.home`: where `~/.cox` lives; `COX_HOME` overrides this directly
    /// (not via the `COX_CORE_HOME` env pattern) before any layer is read.
    pub home: String,
    /// `core.workspace_roots`: empty means "git root of cwd, else cwd";
    /// `--add-dir`/`--cwd` append to this.
    pub workspace_roots: Vec<PathBuf>,
    /// `core.max_turns`: provider calls allowed per `UserTurn`.
    pub max_turns: u32,
    /// `core.parallel_tools`: max concurrent `Concurrency::Parallel` calls.
    pub parallel_tools: u32,
    /// `core.log_level`: a `tracing` filter string.
    pub log_level: String,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            home: "~/.cox".to_string(),
            workspace_roots: Vec::new(),
            max_turns: 200,
            parallel_tools: 4,
            log_level: "info".to_string(),
        }
    }
}

/// One `[tiers.<tier>]` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct TierConfig {
    /// The provider id this tier calls (matches a `[providers.*]` table).
    pub provider: String,
    /// The model id sent in the request.
    pub model: String,
    /// Reasoning effort passed to the provider.
    pub effort: Effort,
    /// Max output tokens for a call on this tier.
    pub max_tokens: u32,
    /// Extended/adaptive thinking mode; absent (`cheap`) means `off`.
    pub thinking: Thinking,
    /// Whether picking this tier requires user confirmation (`think` only;
    /// project config may not set this to `false`, plan.md §1.6).
    pub confirm: bool,
}

impl Default for TierConfig {
    /// A neutral fallback used only when a `[tiers.*]` table is present but
    /// missing an individual key that isn't already covered by the embedded
    /// `default.toml` layer (which always supplies every tier in full).
    fn default() -> Self {
        Self {
            provider: String::new(),
            model: String::new(),
            effort: Effort::Low,
            max_tokens: 0,
            thinking: Thinking::Off,
            confirm: false,
        }
    }
}

/// `[tiers]` (plan.md §1.6): the three routing tiers (plan.md D5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct TiersConfig {
    /// `[tiers.cheap]`
    pub cheap: TierConfig,
    /// `[tiers.code]`
    pub code: TierConfig,
    /// `[tiers.think]`
    pub think: TierConfig,
}

impl TiersConfig {
    /// The `[tiers.<tier>]` block for `tier`.
    pub fn get(&self, tier: crate::types::Tier) -> &TierConfig {
        match tier {
            crate::types::Tier::Cheap => &self.cheap,
            crate::types::Tier::Code => &self.code,
            crate::types::Tier::Think => &self.think,
        }
    }
}

impl Default for TiersConfig {
    fn default() -> Self {
        Self {
            cheap: TierConfig {
                provider: "anthropic".to_string(),
                model: "claude-haiku-4-5".to_string(),
                effort: Effort::Low,
                max_tokens: 4096,
                thinking: Thinking::Off,
                confirm: false,
            },
            code: TierConfig {
                provider: "anthropic".to_string(),
                model: "claude-sonnet-5".to_string(),
                effort: Effort::High,
                max_tokens: 16384,
                thinking: Thinking::Adaptive,
                confirm: false,
            },
            think: TierConfig {
                provider: "anthropic".to_string(),
                model: "claude-fable-5-1".to_string(),
                effort: Effort::High,
                max_tokens: 32768,
                thinking: Thinking::Adaptive,
                confirm: true,
            },
        }
    }
}

/// `[jobs]` (plan.md §1.6): every `Job` pinned to a `Tier`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct JobsConfig {
    /// `jobs.main`
    pub main: Tier,
    /// `jobs.plan`
    pub plan: Tier,
    /// `jobs.compact`
    pub compact: Tier,
    /// `jobs.title`
    pub title: Tier,
    /// `jobs.summarize`
    pub summarize: Tier,
    /// `jobs.commit`
    pub commit: Tier,
    /// `jobs.memory`
    pub memory: Tier,
    /// `jobs.explore`
    pub explore: Tier,
    /// `jobs.shell`
    pub shell: Tier,
    /// `jobs.hook`
    pub hook: Tier,
}

impl JobsConfig {
    /// The tier a job is pinned to (`[jobs]`, plan.md §1.4).
    pub fn tier_for(&self, job: crate::types::Job) -> crate::types::Tier {
        use crate::types::Job as J;
        match job {
            J::Main => self.main,
            J::Plan => self.plan,
            J::Compact => self.compact,
            J::Title => self.title,
            J::Summarize => self.summarize,
            J::Commit => self.commit,
            J::Memory => self.memory,
            J::Explore => self.explore,
            J::Shell => self.shell,
            J::Hook => self.hook,
        }
    }
}

impl Default for JobsConfig {
    fn default() -> Self {
        Self {
            main: Tier::Code,
            plan: Tier::Think,
            compact: Tier::Cheap,
            title: Tier::Cheap,
            summarize: Tier::Cheap,
            commit: Tier::Cheap,
            memory: Tier::Cheap,
            explore: Tier::Cheap,
            shell: Tier::Cheap,
            hook: Tier::Cheap,
        }
    }
}

/// `[providers]` (plan.md §1.6).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct ProvidersConfig {
    /// `[providers.anthropic]`
    pub anthropic: AnthropicProviderConfig,
    /// `[providers.openai]`
    pub openai: OpenAiProviderConfig,
    /// `[providers.local]`
    pub local: LocalProviderConfig,
}

/// `[providers.anthropic]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct AnthropicProviderConfig {
    /// API base URL.
    pub base_url: String,
    /// Env var holding the API key; falls back to keyring entry `cox/anthropic`.
    pub api_key_env: String,
    /// Prompt cache TTL: `"5m"` or `"1h"`.
    pub cache_ttl: String,
    /// Whether to send the `fallbacks: "default"` beta header.
    pub fallbacks: bool,
    /// Request timeout, in seconds.
    pub timeout_s: u32,
    /// Max retries for retryable errors.
    pub max_retries: u32,
}

impl Default for AnthropicProviderConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.anthropic.com".to_string(),
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            cache_ttl: "5m".to_string(),
            fallbacks: true,
            timeout_s: 120,
            max_retries: 4,
        }
    }
}

/// `[providers.openai]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct OpenAiProviderConfig {
    /// API base URL.
    pub base_url: String,
    /// Env var holding the API key.
    pub api_key_env: String,
    /// Which OpenAI API shape to use: `"responses"` or `"chat"`.
    pub api: String,
}

impl Default for OpenAiProviderConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            api: "responses".to_string(),
        }
    }
}

/// `[providers.local]` (Ollama/vLLM/LM Studio/OpenRouter-shaped).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct LocalProviderConfig {
    /// API base URL.
    pub base_url: String,
    /// API shape; local servers are typically `"chat"`.
    pub api: String,
    /// The model id the local server serves.
    pub model: String,
    /// Context window, since local servers usually don't report it.
    pub context_window: u32,
}

impl Default for LocalProviderConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434/v1".to_string(),
            api: "chat".to_string(),
            model: "qwen3-coder".to_string(),
            context_window: 32768,
        }
    }
}

/// `[context]` (plan.md §1.6/§1.9/§1.10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct ContextConfig {
    /// Fraction of `max_context` that triggers compaction.
    pub compact_at: f64,
    /// Turns compaction always keeps verbatim.
    pub keep_turns: u32,
    /// Turns after which a tool result is microcompacted to a pointer.
    pub microcompact_after_turns: u32,
    /// Bytes of a tool result kept visible before truncation.
    pub tool_output_visible_bytes: u32,
    /// Head lines kept when truncating a tool result.
    pub tool_output_head_lines: u32,
    /// Tail lines kept when truncating a tool result.
    pub tool_output_tail_lines: u32,
    /// Turn window used for tool-result dedup.
    pub dedup_window_turns: u32,
    /// Token budget for instruction files.
    pub instruction_budget_tokens: u32,
    /// Token budget for the memory index.
    pub memory_budget_tokens: u32,
    /// Whether non-core tools are deferred (found via `tool_search`).
    pub deferred_tools: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            compact_at: 0.75,
            keep_turns: 2,
            microcompact_after_turns: 6,
            tool_output_visible_bytes: 8192,
            tool_output_head_lines: 60,
            tool_output_tail_lines: 20,
            dedup_window_turns: 8,
            instruction_budget_tokens: 8000,
            memory_budget_tokens: 800,
            deferred_tools: true,
        }
    }
}

/// `[permissions]` (plan.md §1.6/§1.8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct PermissionsConfig {
    /// `permissions.mode`; project config may not set this to `bypass`.
    pub mode: PermissionMode,
    /// `permissions.approval`.
    pub approval: ApprovalPolicy,
    /// `allow` rule strings (plan.md §1.8 grammar).
    pub allow: Vec<String>,
    /// `ask` rule strings.
    pub ask: Vec<String>,
    /// `deny` rule strings.
    pub deny: Vec<String>,
    /// Whether to import `.claude/settings.json` permission rules (T7.5).
    pub import_claude_settings: bool,
    /// Whether an `AllowForSession` grant survives past the session.
    pub allow_for_session_persists: bool,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Default,
            approval: ApprovalPolicy::OnRequest,
            allow: Vec::new(),
            ask: Vec::new(),
            deny: vec![
                "Read(~/.ssh/**)".to_string(),
                "Read(~/.aws/**)".to_string(),
                "Bash(rm -rf /*)".to_string(),
            ],
            import_claude_settings: true,
            allow_for_session_persists: false,
        }
    }
}

/// `[sandbox]` (plan.md §1.6/D7). Not `SandboxPolicy` (`types.rs`), which is
/// the value resolved for one call; this is the on-disk config it's built from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct SandboxConfig {
    /// `sandbox.mode`; project config may not set this to `danger-full-access`.
    pub mode: SandboxMode,
    /// Whether network access is allowed.
    pub network: bool,
    /// Extra writable roots beyond the workspace.
    pub writable: Vec<PathBuf>,
    /// Paths inside the workspace that stay read-only even in `workspace-write`.
    pub readonly_in_workspace: Vec<PathBuf>,
    /// Linux backend selection: `auto` | `bwrap` | `landlock` | `none`.
    pub linux_backend: LinuxBackend,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            mode: SandboxMode::WorkspaceWrite,
            network: false,
            writable: Vec::new(),
            readonly_in_workspace: vec![
                PathBuf::from(".git"),
                PathBuf::from(".cox"),
                PathBuf::from(".claude"),
            ],
            linux_backend: LinuxBackend::Auto,
        }
    }
}

/// `[budget]` (plan.md §1.6/§1.9). Project config may not raise any field
/// here above the user/default value (plan.md §1.6 guard list).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct BudgetConfig {
    /// Session spend cap, in USD.
    pub session_usd: f64,
    /// Monthly spend cap, in USD.
    pub monthly_usd: f64,
    /// Fraction of a cap that raises a `Level::Budget` notice.
    pub warn_at: f64,
    /// Whether `cheap`-tier calls count against the budget.
    pub cheap_counts: bool,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            session_usd: 5.0,
            monthly_usd: 100.0,
            warn_at: 0.8,
            cheap_counts: true,
        }
    }
}

/// `[tui]` (plan.md §1.6/§1.13).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct TuiConfig {
    /// Vim keybindings in the editor.
    pub vim: bool,
    /// `auto` | `dark` | `light`.
    pub theme: String,
    /// Whether the TUI renders inline (vs. alt-screen).
    pub inline: bool,
    /// `collapsed` | `hidden` | `full`.
    pub show_thinking: String,
    /// Whether mouse input (scroll, click) is enabled.
    pub mouse: bool,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            vim: false,
            theme: "auto".to_string(),
            inline: true,
            show_thinking: "collapsed".to_string(),
            mouse: true,
        }
    }
}

/// One `[[hooks.<Event>]]` entry.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct HookConfig {
    /// Tool/subject matcher (Claude Code hook matcher syntax), if any.
    pub matcher: Option<String>,
    /// The command to run.
    pub command: String,
    /// Per-hook timeout override, in seconds; falls back to `hooks.timeout_s`.
    pub timeout_s: Option<u32>,
}

/// `[hooks]` (plan.md §1.6/§1.10/D14). The fixed `timeout_s`/`fail_open`
/// keys plus every `[[hooks.<Event>]]` table, captured generically since
/// the event name is the TOML key (`PreToolUse`, `PostToolUse`, ...) rather
/// than a fixed field.
///
/// `deny_unknown_fields` is intentionally *not* set here: the flattened
/// `events` map is exactly what would otherwise be "unknown fields", so the
/// two are mutually exclusive for this one struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct HooksConfig {
    /// Max seconds a hook process may run before it's treated as failed.
    pub timeout_s: u32,
    /// Whether a broken hook is skipped (warned) instead of fatal (AGENTS.md).
    pub fail_open: bool,
    /// `--no-hooks` sets this to `false`; not a `default.toml` key.
    pub enabled: bool,
    /// Every `[[hooks.<Event>]]` array, keyed by event name.
    #[serde(flatten)]
    pub events: HashMap<String, Vec<HookConfig>>,
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            timeout_s: 60,
            fail_open: true,
            enabled: true,
            events: HashMap::new(),
        }
    }
}

/// One `[mcp.servers.<name>]` entry (same shape as `.mcp.json`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct McpServerConfig {
    /// Stdio launch command, for a local server.
    pub command: Option<String>,
    /// Arguments to `command`.
    pub args: Vec<String>,
    /// Remote URL, for an HTTP/SSE server (mutually exclusive with `command`).
    pub url: Option<String>,
    /// Extra environment variables for a stdio server.
    pub env: HashMap<String, String>,
}

/// `[mcp]` (plan.md §1.6/§1.1). `deny_unknown_fields` is not set for the
/// same reason as `HooksConfig`: `servers` is a flattened catch-all for
/// arbitrary server names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct McpConfig {
    /// Per-call timeout, in seconds.
    pub timeout_s: u32,
    /// Whether MCP tools are deferred (found via `tool_search`) by default.
    pub deferred: bool,
    /// `--no-mcp` sets this to `false`; not a `default.toml` key.
    pub enabled: bool,
    /// `[mcp.servers.<name>]` entries.
    pub servers: HashMap<String, McpServerConfig>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            timeout_s: 30,
            deferred: true,
            enabled: true,
            servers: HashMap::new(),
        }
    }
}

/// `[memory]` (plan.md §1.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct MemoryConfig {
    /// Whether the memory index is read into context.
    pub enabled: bool,
    /// Whether end-of-session extraction runs (on the `memory` job's tier).
    pub extract: bool,
    /// Override for `~/.cox/projects/<slug>/memory`; empty means default.
    pub dir: String,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extract: false,
            dir: String::new(),
        }
    }
}

/// `[telemetry]` (plan.md §1.6).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct TelemetryConfig {
    /// Whether an OpenTelemetry exporter is enabled.
    pub otel: bool,
    /// OTLP endpoint; empty means the exporter's own default.
    pub endpoint: String,
}

/// `[record]` (`cox record`, plan.md §1.12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct RecordConfig {
    /// Whether secrets are redacted from a re-recorded cassette.
    pub redact: bool,
}

impl Default for RecordConfig {
    fn default() -> Self {
        Self { redact: true }
    }
}

/// Renders `docs/config.md` from `default.toml`'s own text: a `##` heading
/// per `[section]`/`[section.sub]` table, and one bullet per `key = value`
/// line carrying its trailing `# comment`, if any. Deliberately a line-level
/// scan rather than a full TOML parse — `default.toml`'s shape (one table
/// header or one `key = value [# comment]` per line, no multi-line values)
/// is ours to keep simple, and this avoids adding a `toml`-parsing
/// dependency to `cox-protocol` for a docs generator.
#[cfg(test)]
fn generate_config_docs(toml: &str) -> String {
    let mut out = String::from(
        "# cox configuration reference\n\n\
         Generated from `config/default.toml` by a test in `cox-protocol/src/config.rs`; do not hand-edit.\n\n",
    );
    for line in toml.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            out.push_str(&format!("## `[{section}]`\n\n"));
            continue;
        }
        let (body, comment) = match line.split_once('#') {
            Some((b, c)) => (b.trim(), Some(c.trim())),
            None => (line, None),
        };
        let Some((key, value)) = body.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match comment {
            Some(c) => out.push_str(&format!("- `{key}` = `{value}` — {c}\n")),
            None => out.push_str(&format!("- `{key}` = `{value}`\n")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn config_docs_config_md_matches_default_toml() {
        let generated = generate_config_docs(DEFAULT_CONFIG_TOML);
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/config.md");
        match std::fs::read_to_string(&path) {
            Ok(committed) => assert_eq!(
                committed, generated,
                "docs/config.md is stale; regenerate it (see this test) and commit it"
            ),
            Err(_) => {
                // First run: create it. `git status` will show it as new/changed for review.
                std::fs::write(&path, &generated).expect("write docs/config.md");
            }
        }
    }

    #[test]
    fn config_default_matches_hand_built_defaults() {
        // `Config::default()` is exercised directly (not via figment, which
        // is a `cox`-crate concern) to prove every hand-written `Default`
        // impl above actually compiles into a coherent tree and that the
        // `#[serde(default)]` container attributes have something sane to
        // fall back to.
        let cfg = Config::default();
        assert_eq!(cfg.core.home, "~/.cox");
        assert_eq!(cfg.tiers.code.model, "claude-sonnet-5");
        assert_eq!(cfg.tiers.cheap.thinking, Thinking::Off);
        assert!(cfg.tiers.think.confirm);
        assert_eq!(cfg.jobs.main, Tier::Code);
        assert_eq!(cfg.providers.anthropic.max_retries, 4);
        assert_eq!(
            cfg.permissions.deny,
            vec!["Read(~/.ssh/**)", "Read(~/.aws/**)", "Bash(rm -rf /*)"]
        );
        assert_eq!(cfg.sandbox.mode, SandboxMode::WorkspaceWrite);
        assert!(cfg.hooks.events.is_empty());
        assert!(cfg.mcp.servers.is_empty());
    }

    #[test]
    fn config_json_roundtrip() {
        let cfg = Config::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: Config = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cfg, back);
    }

    #[test]
    fn config_hooks_deny_unknown_but_accept_event_arrays() {
        // A genuinely unknown top-level key is still rejected...
        let bad = r#"{"timeout_s": 1, "fail_open": true, "enabled": true}"#;
        let cfg: HooksConfig = serde_json::from_str(bad).expect("known fields only");
        assert!(cfg.events.is_empty());

        // ...while an event-shaped key is captured, not rejected.
        let with_event = r#"{
            "timeout_s": 1, "fail_open": true, "enabled": true,
            "PreToolUse": [{"matcher": "Bash", "command": "echo hi"}]
        }"#;
        let cfg: HooksConfig = serde_json::from_str(with_event).expect("flatten captures it");
        assert_eq!(cfg.events["PreToolUse"][0].command, "echo hi");
    }
}
