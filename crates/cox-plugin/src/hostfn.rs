//! The `cox:host/v1` host functions (PL§4, T33.9): what a guest may ask the
//! host for, each call checked against the plugin's grant and the export it
//! is called from. Separate from `host` because that module owns threads and
//! queues, this one owns what a call is allowed to do.
//!
//! The wire matches the guest SDK (`plugins/sdk/src/lib.rs`, `docs/plugins.md`):
//! every import is `(u64) -> u64`; the argument block holds one JSON value
//! (an object keyed by argument name when there are several, `null` when
//! there are none) and the reply block holds `{"Ok": T}` or
//! `{"Err": AbiError}`, so a refusal is a value the plugin can handle, never
//! a trap that ends its call.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use cox_plugin_api::{
    AbiError, InitIn, ModelCall, ModelTier, NoticeLevel, PluginManifest, SessionInfo,
};
use cox_protocol::config::PluginsConfig;
use cox_protocol::errors::CoreError;
use cox_protocol::traits::{KV_PLUGIN_LIMIT, KV_VALUE_LIMIT, ModelCaller};
use cox_protocol::types::{Level, Request, Tier};
use cox_protocol::{PluginStore, StoreError};
use cox_sanitize::redact::scrub;
use cox_sanitize::sanitize;
use extism::{CurrentPlugin, Function, PTR, UserData, Val};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::context::Context;

/// The import module every host function lives in.
pub const NAMESPACE: &str = "cox:host/v1";

/// Every import ABI v1 names. All are registered so a guest built with the
/// SDK links whatever it imports; one this host does not implement yet
/// answers `Err(Failed)` instead of failing the load.
const IMPORTS: [&str; 12] = [
    "cox_log",
    "cox_notify",
    "cox_kv_get",
    "cox_kv_put",
    "cox_kv_delete",
    "cox_context",
    "cox_invoke_tool",
    "cox_model_call",
    "cox_http",
    "cox_output",
    "cox_cancelled",
    "cox_redraw",
];

/// Log lines a plugin may write per second (PL§4 "rate-limited").
const LOG_LINES_PER_SEC: u32 = 20;
/// Notices waiting for the session to drain them; more is a refusal, so a
/// loop inside one call cannot flood the transcript.
const PENDING_NOTICES: usize = 16;
/// Longest kv key. Keys are not counted in the value quota, so without a
/// cap one key could store what the quota exists to stop.
const MAX_KEY_BYTES: usize = 256;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct LogWindow {
    start: Option<Instant>,
    lines: u32,
    dropped: u32,
}

impl LogWindow {
    /// Whether one more line fits in the current second, and how many lines
    /// the second that just closed dropped.
    fn admit(&mut self, now: Instant) -> (bool, u32) {
        let mut closed = 0;
        if self
            .start
            .is_none_or(|s| now.duration_since(s) >= Duration::from_secs(1))
        {
            closed = std::mem::take(&mut self.dropped);
            self.start = Some(now);
            self.lines = 0;
        }
        if self.lines < LOG_LINES_PER_SEC {
            self.lines += 1;
            (true, closed)
        } else {
            self.dropped += 1;
            (false, closed)
        }
    }
}

/// What one plugin's host functions act on: its id, its granted
/// capabilities, the stores behind them, and the export now running.
pub struct HostEnv {
    id: String,
    granted: BTreeSet<String>,
    store: Option<Arc<dyn PluginStore>>,
    context: Arc<Context>,
    export: Mutex<String>,
    notices: Mutex<Vec<(Level, String)>>,
    log: Mutex<LogWindow>,
    // T33.15: `cox_model_call`'s route to `cox-core`'s router/budget/ledger
    // over the `ModelCaller` trait (this crate may not depend on cox-core),
    // and the tokio handle to block this plugin's plain OS worker thread on
    // it (`PluginHost`'s call, not a tokio runtime worker: `crate::host`).
    model_caller: Option<Arc<dyn ModelCaller>>,
    runtime: Option<tokio::runtime::Handle>,
}

impl HostEnv {
    /// An environment that grants nothing: enough to link a module and run
    /// `cox_log`/`cox_notify`, nothing more.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            granted: BTreeSet::new(),
            store: None,
            context: Arc::new(Context::new()),
            export: Mutex::new(String::new()),
            notices: Mutex::new(Vec::new()),
            log: Mutex::new(LogWindow::default()),
            model_caller: None,
            runtime: None,
        }
    }

    /// The granted capability lines (`grant::capability_list` of a manifest
    /// that `grant::check` found `Granted`) and the kv store.
    pub fn with_grant(mut self, granted: Vec<String>, store: Arc<dyn PluginStore>) -> Self {
        self.granted = granted.into_iter().collect();
        self.store = Some(store);
        self
    }

    /// The session's folded context, shared by all of its plugins.
    pub fn with_context(mut self, context: Arc<Context>) -> Self {
        self.context = context;
        self
    }

    /// The seam `cox_model_call` routes through (T33.44 wires the real
    /// `Session` and its runtime `Handle` in at session open; without this,
    /// `cox_model_call` answers `Failed`).
    pub fn with_model_caller(
        mut self,
        caller: Arc<dyn ModelCaller>,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        self.model_caller = Some(caller);
        self.runtime = Some(runtime);
        self
    }

    /// The plugin id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Takes the notices `cox_notify` queued, already capped at `Warn` and
    /// sanitized, for the session to emit as `Event::Notice`.
    pub fn take_notices(&self) -> Vec<(Level, String)> {
        std::mem::take(&mut *lock(&self.notices))
    }

    /// Queues one notice for `take_notices`, sanitized and redacted: from
    /// `cox_notify` and from `Effects.notices` (T33.10) alike.
    pub(crate) fn notify(&self, level: NoticeLevel, text: &str) -> Result<(), AbiError> {
        let mut queue = lock(&self.notices);
        if queue.len() >= PENDING_NOTICES {
            return Err(failed("too many notices"));
        }
        let level = match level {
            NoticeLevel::Info => Level::Info,
            NoticeLevel::Warn => Level::Warn,
        };
        queue.push((level, clean(text)));
        Ok(())
    }

    /// Records the export the worker is about to call (`""` when idle).
    pub(crate) fn enter(&self, export: &str) {
        export.clone_into(&mut lock(&self.export));
    }

    /// Every `cox:host/v1` import, bound to this environment.
    pub(crate) fn functions(self: &Arc<Self>) -> Vec<Function> {
        IMPORTS
            .iter()
            .map(|&name| {
                Function::new(
                    name,
                    [PTR],
                    [PTR],
                    UserData::new(self.clone()),
                    move |plugin: &mut CurrentPlugin,
                          inputs: &[Val],
                          outputs: &mut [Val],
                          env: UserData<Arc<HostEnv>>| {
                        let env = lock(&*env.get()?).clone();
                        let reply = match inputs.first() {
                            Some(offset) => plugin
                                .memory_get_val::<&[u8]>(offset)
                                .ok()
                                .and_then(|bytes| serde_json::from_slice(bytes).ok())
                                .map_or_else(
                                    || Err(failed("the argument is not JSON")),
                                    |arg| env.dispatch(name, arg),
                                ),
                            None => Err(failed("no argument")),
                        };
                        let bytes = serde_json::to_vec(&reply)?;
                        match outputs.first_mut() {
                            Some(out) => plugin.memory_set_val(out, bytes.as_slice()),
                            None => Ok(()),
                        }
                    },
                )
                .with_namespace(NAMESPACE)
            })
            .collect()
    }

    fn dispatch(&self, import: &str, arg: Value) -> Result<Value, AbiError> {
        match import {
            "cox_log" => {
                let line: Line = parse(arg)?;
                self.log(line);
                Ok(Value::Null)
            }
            "cox_notify" => {
                self.outside_render()?;
                let line: Line = parse(arg)?;
                self.notify(cap(line.level), &line.text)?;
                Ok(Value::Null)
            }
            "cox_kv_get" => {
                let store = self.kv()?;
                let key = key(parse(arg)?)?;
                match store.kv_get(&self.id, &key).map_err(store_error)? {
                    Some(bytes) => serde_json::from_slice(&bytes)
                        .map_err(|_| failed("the stored value is not JSON")),
                    None => Ok(Value::Null),
                }
            }
            "cox_kv_put" => {
                let store = self.kv()?;
                let KvPut { key: k, value } = parse(arg)?;
                let bytes = serde_json::to_vec(&value).map_err(|e| failed(&e.to_string()))?;
                let limit = if bytes.len() > KV_VALUE_LIMIT {
                    KV_VALUE_LIMIT
                } else {
                    KV_PLUGIN_LIMIT
                };
                match store.kv_put(&self.id, &key(k)?, &bytes) {
                    Err(StoreError::QuotaExceeded) => Err(AbiError::TooLarge {
                        limit: limit as u64,
                    }),
                    other => other.map(|()| Value::Null).map_err(store_error),
                }
            }
            "cox_kv_delete" => {
                let store = self.kv()?;
                let key = key(parse(arg)?)?;
                store.kv_delete(&self.id, &key).map_err(store_error)?;
                Ok(Value::Null)
            }
            "cox_context" => {
                self.require("context")?;
                Ok(self.context.snapshot())
            }
            "cox_model_call" => self.model_call(arg),
            other => Err(failed(&format!("`{other}` is not available in this cox"))),
        }
    }

    fn require(&self, capability: &str) -> Result<(), AbiError> {
        if self.granted.contains(capability) {
            Ok(())
        } else {
            Err(AbiError::NotGranted {
                capability: capability.into(),
            })
        }
    }

    /// `cox_render` has a strict time cap, so it may only read (PL§4).
    fn outside_render(&self) -> Result<(), AbiError> {
        match lock(&self.export).as_str() {
            "cox_render" | "cox_render_item" => Err(AbiError::NotInThisContext),
            _ => Ok(()),
        }
    }

    fn kv(&self) -> Result<&dyn PluginStore, AbiError> {
        self.require("kv")?;
        self.outside_render()?;
        self.store.as_deref().ok_or_else(|| AbiError::NotGranted {
            capability: "kv".into(),
        })
    }

    fn log(&self, line: Line) {
        let (admit, dropped) = lock(&self.log).admit(Instant::now());
        let id = &self.id;
        if dropped > 0 {
            tracing::warn!(plugin = %id, dropped, "plugin log lines dropped by the rate limit");
        }
        if !admit {
            return;
        }
        let text = clean(&line.text);
        match cap(line.level) {
            NoticeLevel::Info => tracing::info!(plugin = %id, "{text}"),
            NoticeLevel::Warn => tracing::warn!(plugin = %id, "{text}"),
        }
    }

    /// `cox_model_call` (PL§7d, T33.15): the tier is clamped to the grant
    /// (`model:code` covers `model:cheap`, mirroring `grant::covered`) and
    /// never reaches `think` — `ModelTier` cannot even express it — then the
    /// call blocks this plugin's worker thread on `ModelCaller::call`, which
    /// runs it through the router and the budget gate and writes the one
    /// `usage` row (`crates/cox-core/src/plugin_model.rs`).
    fn model_call(&self, arg: Value) -> Result<Value, AbiError> {
        self.outside_render()?;
        let granted = if self.granted.contains(MODEL_CODE) {
            Tier::Code
        } else if self.granted.contains(MODEL_CHEAP) {
            Tier::Cheap
        } else {
            return Err(AbiError::NotGranted {
                capability: MODEL_CHEAP.into(),
            });
        };
        let call: ModelCall = parse(arg)?;
        let requested = match call.tier {
            ModelTier::Cheap => Tier::Cheap,
            ModelTier::Code => Tier::Code,
        };
        let tier = if tier_rank(requested) < tier_rank(granted) {
            requested
        } else {
            granted
        };
        let request: Request = serde_json::from_value(call.request)
            .map_err(|e| failed(&format!("bad request: {e}")))?;
        let caller = self
            .model_caller
            .as_ref()
            .ok_or_else(|| failed("model calls are not available in this cox"))?;
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| failed("model calls are not available in this cox"))?;
        let events = runtime
            .block_on(caller.call(&self.id, tier, request))
            .map_err(model_call_error)?;
        serde_json::to_value(events).map_err(|e| failed(&e.to_string()))
    }
}

/// `model:cheap` and `model:code` (mirrors `grant.rs`'s private consts; not
/// imported because this crate's `dispatch` should not need `grant::covered`
/// for a single two-tier comparison).
const MODEL_CHEAP: &str = "model:cheap";
const MODEL_CODE: &str = "model:code";

/// Lower is cheaper; `think` has no `ModelTier` variant so it never appears.
fn tier_rank(tier: Tier) -> u8 {
    match tier {
        Tier::Cheap => 0,
        Tier::Code => 1,
        Tier::Think => 2,
    }
}

fn model_call_error(error: CoreError) -> AbiError {
    match error {
        CoreError::Budget { .. } => AbiError::Budget,
        other => failed(&other.to_string()),
    }
}

/// `cox_log`/`cox_notify`'s arguments. The level stays raw JSON so a level
/// past `warn` is capped rather than refused.
#[derive(Deserialize)]
struct Line {
    #[serde(default)]
    level: Value,
    text: String,
}

/// `cox_kv_put`'s arguments.
#[derive(Deserialize)]
struct KvPut {
    key: String,
    value: Value,
}

/// A plugin can never raise `Budget` or `Security` (PL§4): anything but
/// `info` becomes `warn`.
fn cap(level: Value) -> NoticeLevel {
    serde_json::from_value(level).unwrap_or(NoticeLevel::Warn)
}

/// Plugin text leaving the host: escapes stripped, secret shapes redacted.
fn clean(text: &str) -> String {
    let safe = sanitize(text);
    scrub(&safe).into_owned()
}

fn key(key: String) -> Result<String, AbiError> {
    if key.len() > MAX_KEY_BYTES {
        return Err(AbiError::TooLarge {
            limit: MAX_KEY_BYTES as u64,
        });
    }
    Ok(key)
}

fn parse<T: DeserializeOwned>(arg: Value) -> Result<T, AbiError> {
    serde_json::from_value(arg).map_err(|e| failed(&format!("bad argument: {e}")))
}

fn failed(message: &str) -> AbiError {
    AbiError::Failed {
        message: message.into(),
    }
}

fn store_error(e: StoreError) -> AbiError {
    failed(&e.to_string())
}

/// `cox_init`'s input (PL§4): the plugin's own `[plugins.<id>]` table (an
/// empty table when there is none) and the capabilities it was granted,
/// which under `Verdict::Granted` are exactly the ones its manifest asks for.
pub fn init_input(
    manifest: &PluginManifest,
    plugins: &PluginsConfig,
    session: SessionInfo,
) -> InitIn {
    InitIn {
        api: cox_plugin_api::API_MAJOR,
        plugin_id: manifest.id.clone(),
        config: plugins
            .entries
            .get(&manifest.id)
            .cloned()
            .unwrap_or_else(|| json!({})),
        session,
        granted: serde_json::to_value(&manifest.capabilities).unwrap_or_else(|_| json!({})),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;

    use cox_plugin_api::{Capabilities, Limits};
    use cox_protocol::{GrantScope, PluginGrant};

    use super::*;
    use crate::host::{Lane, PluginHost};

    /// An in-memory `PluginStore`: only kv is exercised here.
    #[derive(Default)]
    pub(crate) struct MemKv(Mutex<HashMap<(String, String), Vec<u8>>>);

    impl PluginStore for MemKv {
        fn grant_get(
            &self,
            _: &str,
            _: &GrantScope,
            _: &str,
        ) -> Result<Option<PluginGrant>, StoreError> {
            Ok(None)
        }
        fn grant_put(&self, _: &PluginGrant) -> Result<(), StoreError> {
            Ok(())
        }
        fn grant_set_enabled(
            &self,
            _: &str,
            _: &GrantScope,
            _: &str,
            _: bool,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        fn grants_delete(&self, _: &str) -> Result<(), StoreError> {
            Ok(())
        }
        fn kv_get(&self, id: &str, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
            Ok(lock(&self.0).get(&(id.into(), key.into())).cloned())
        }
        fn kv_put(&self, id: &str, key: &str, value: &[u8]) -> Result<(), StoreError> {
            lock(&self.0).insert((id.into(), key.into()), value.to_vec());
            Ok(())
        }
        fn kv_delete(&self, id: &str, key: &str) -> Result<(), StoreError> {
            lock(&self.0).remove(&(id.into(), key.into()));
            Ok(())
        }
        fn kv_delete_all(&self, _: &str) -> Result<(), StoreError> {
            Ok(())
        }
    }

    /// A guest whose `cox_command` and `cox_render` both pass their input
    /// to the `cox:host/v1` import `import` and return its reply block.
    fn relay(import: &str) -> Vec<u8> {
        format!(
            r#"(module
              (import "extism:host/env" "input_length" (func $input_length (result i64)))
              (import "extism:host/env" "input_load_u8" (func $load (param i64) (result i32)))
              (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
              (import "extism:host/env" "store_u8" (func $store (param i64 i32)))
              (import "extism:host/env" "length" (func $length (param i64) (result i64)))
              (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))
              (import "cox:host/v1" "{import}" (func $host (param i64) (result i64)))
              (func $relay (result i32) (local $n i64) (local $off i64) (local $i i64) (local $r i64)
                (local.set $n (call $input_length))
                (local.set $off (call $alloc (local.get $n)))
                (block $done (loop $copy
                  (br_if $done (i64.ge_u (local.get $i) (local.get $n)))
                  (call $store (i64.add (local.get $off) (local.get $i)) (call $load (local.get $i)))
                  (local.set $i (i64.add (local.get $i) (i64.const 1)))
                  (br $copy)))
                (local.set $r (call $host (local.get $off)))
                (call $output_set (local.get $r) (call $length (local.get $r)))
                (i32.const 0))
              (func (export "cox_init") (result i32) (i32.const 0))
              (export "cox_command" (func $relay))
              (export "cox_render" (func $relay)))"#
        )
        .into_bytes()
    }

    fn call(env: &Arc<HostEnv>, import: &str, export: &str, arg: Value) -> Value {
        let host = PluginHost::load_with(&relay(import), &Limits::default(), env.clone())
            .expect("relay loads");
        host.call::<_, Value>(Lane::Control, export, &arg, Duration::from_secs(5))
            .expect("relay call")
            .expect("relay export exists")
    }

    #[test]
    fn notify_cannot_raise_security_level() {
        let env = Arc::new(HostEnv::new("t"));
        for level in ["security", "budget", "warn"] {
            let reply = call(
                &env,
                "cox_notify",
                "cox_command",
                json!({ "level": level, "text": "\u{1b}[2Jpwned" }),
            );
            assert_eq!(reply, json!({ "Ok": null }), "{level}");
        }
        call(
            &env,
            "cox_notify",
            "cox_command",
            json!({ "level": "info", "text": "hi" }),
        );
        let notices = env.take_notices();
        assert_eq!(notices.len(), 4);
        assert!(
            notices[..3]
                .iter()
                .all(|n| *n == (Level::Warn, "pwned".into())),
            "{notices:?}"
        );
        assert_eq!(notices[3], (Level::Info, "hi".into()));
        // `cox_render` may only read.
        let reply = call(
            &env,
            "cox_notify",
            "cox_render",
            json!({ "level": "info", "text": "x" }),
        );
        assert_eq!(reply, json!({ "Err": { "kind": "not_in_this_context" } }));
        assert!(env.take_notices().is_empty());
    }

    #[test]
    fn kv_denied_without_capability() {
        let store = Arc::new(MemKv::default());
        let put = json!({ "key": "turns", "value": 3 });

        let ungranted =
            Arc::new(HostEnv::new("t").with_grant(vec!["context".into()], store.clone()));
        let reply = call(&ungranted, "cox_kv_put", "cox_command", put.clone());
        assert_eq!(
            reply,
            json!({ "Err": { "kind": "not_granted", "capability": "kv" } })
        );
        assert!(lock(&store.0).is_empty(), "a refused put wrote");

        let granted = Arc::new(HostEnv::new("t").with_grant(vec!["kv".into()], store.clone()));
        assert_eq!(
            call(&granted, "cox_kv_put", "cox_command", put),
            json!({ "Ok": null })
        );
        assert_eq!(
            call(&granted, "cox_kv_get", "cox_command", json!("turns")),
            json!({ "Ok": 3 })
        );
        assert_eq!(
            call(&granted, "cox_kv_get", "cox_render", json!("turns")),
            json!({ "Err": { "kind": "not_in_this_context" } })
        );
        assert_eq!(
            call(&granted, "cox_kv_delete", "cox_command", json!("turns")),
            json!({ "Ok": null })
        );
        assert_eq!(
            call(&granted, "cox_kv_get", "cox_command", json!("turns")),
            json!({ "Ok": null })
        );
    }

    #[test]
    fn context_needs_its_capability_and_is_allowed_in_render() {
        let store = Arc::new(MemKv::default());
        let none = Arc::new(HostEnv::new("t"));
        assert_eq!(
            call(&none, "cox_context", "cox_command", Value::Null),
            json!({ "Err": { "kind": "not_granted", "capability": "context" } })
        );
        let env = Arc::new(HostEnv::new("t").with_grant(vec!["context".into()], store));
        let reply = call(&env, "cox_context", "cox_render", Value::Null);
        assert!(reply["Ok"]["items"].is_array(), "{reply}");
    }

    #[test]
    fn unimplemented_import_links_and_answers_failed() {
        let env = Arc::new(HostEnv::new("t"));
        let reply = call(&env, "cox_http", "cox_command", json!({}));
        assert_eq!(reply["Err"]["kind"], "failed", "{reply}");
    }

    #[test]
    fn log_is_rate_limited_per_second() {
        let mut w = LogWindow::default();
        let t0 = Instant::now();
        let admitted = (0..LOG_LINES_PER_SEC + 5).filter(|_| w.admit(t0).0).count();
        assert_eq!(admitted, LOG_LINES_PER_SEC as usize);
        // The next second admits again and reports what the last one dropped.
        assert_eq!(w.admit(t0 + Duration::from_secs(1)), (true, 5));
    }

    #[test]
    fn init_config_is_the_plugins_own_table() {
        let manifest = PluginManifest {
            api: 1,
            id: "jev".into(),
            version: "0.1.0".into(),
            name: "Jev".into(),
            description: String::new(),
            wasm: "plugin.wasm".into(),
            wasi: false,
            limits: Limits::default(),
            capabilities: Capabilities {
                kv: true,
                ..Capabilities::default()
            },
            provider: Vec::new(),
            models: Vec::new(),
            mcp: Vec::new(),
            external_agents: Vec::new(),
        };
        let mut plugins = PluginsConfig::default();
        plugins
            .entries
            .insert("jev".into(), json!({ "route": "cheap" }));
        plugins
            .entries
            .insert("other".into(), json!({ "secret": 1 }));
        let session = SessionInfo {
            id: "s1".into(),
            cwd: "/w".into(),
        };
        let init = init_input(&manifest, &plugins, session.clone());
        assert_eq!(init.config, json!({ "route": "cheap" }));
        assert_eq!(init.granted["kv"], true);
        let bare = init_input(&manifest, &PluginsConfig::default(), session);
        assert_eq!(bare.config, json!({}));
    }

    /// A `ModelCaller` that records the tier it was called at and answers
    /// with one `TextDelta`, standing in for `cox-core`'s `Session` (T33.15
    /// wires the real one; T33.44 puts it in a live `HostEnv`).
    #[derive(Default)]
    struct FakeCaller {
        seen_tier: Mutex<Option<Tier>>,
    }

    #[async_trait::async_trait]
    impl ModelCaller for FakeCaller {
        async fn call(
            &self,
            _id: &str,
            tier: Tier,
            _request: Request,
        ) -> Result<Vec<cox_protocol::types::ProviderEvent>, CoreError> {
            *lock(&self.seen_tier) = Some(tier);
            Ok(vec![cox_protocol::types::ProviderEvent::TextDelta {
                text: "ok".into(),
            }])
        }
    }

    fn model_call_request(tier: ModelTier) -> Value {
        let request = Request {
            tier: Tier::Cheap,
            job: cox_protocol::types::Job::Main,
            model: cox_protocol::types::ModelId(String::new()),
            system: vec![cox_protocol::types::SystemBlock {
                text: "you are a plugin's own prompt".into(),
                cache: false,
            }],
            tools: vec![],
            messages: vec![],
            effort: cox_protocol::types::Effort::Low,
            max_tokens: 64,
            thinking: cox_protocol::types::Thinking::Off,
            cache_breakpoints: vec![],
            stop_sequences: vec![],
        };
        json!({
            "tier": tier,
            "request": serde_json::to_value(request).expect("request serializes"),
        })
    }

    fn env_with_caller(granted: Vec<String>) -> (Arc<HostEnv>, Arc<FakeCaller>) {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let caller = Arc::new(FakeCaller::default());
        let env = Arc::new(
            HostEnv::new("t")
                .with_grant(granted, Arc::new(MemKv::default()))
                .with_model_caller(caller.clone(), rt.handle().clone()),
        );
        // Leaking the runtime keeps its handle alive for the env's lifetime;
        // the test process exits right after, so nothing outlives it.
        std::mem::forget(rt);
        (env, caller)
    }

    #[test]
    fn model_call_denied_without_capability() {
        let env = Arc::new(HostEnv::new("t"));
        let reply = call(
            &env,
            "cox_model_call",
            "cox_command",
            model_call_request(ModelTier::Cheap),
        );
        assert_eq!(
            reply,
            json!({ "Err": { "kind": "not_granted", "capability": "model:cheap" } })
        );
    }

    #[test]
    fn model_call_denied_in_render() {
        let (env, _caller) = env_with_caller(vec!["model:cheap".into()]);
        let reply = call(
            &env,
            "cox_model_call",
            "cox_render",
            model_call_request(ModelTier::Cheap),
        );
        assert_eq!(reply, json!({ "Err": { "kind": "not_in_this_context" } }));
    }

    #[test]
    fn model_call_clamps_tier_to_the_grant() {
        let (env, caller) = env_with_caller(vec!["model:cheap".into()]);
        let reply = call(
            &env,
            "cox_model_call",
            "cox_command",
            model_call_request(ModelTier::Code),
        );
        assert!(reply["Ok"].is_array(), "{reply}");
        assert_eq!(*lock(&caller.seen_tier), Some(Tier::Cheap));
    }

    #[test]
    fn model_call_round_trips_at_the_granted_tier() {
        let (env, caller) = env_with_caller(vec!["model:code".into()]);
        let reply = call(
            &env,
            "cox_model_call",
            "cox_command",
            model_call_request(ModelTier::Code),
        );
        assert_eq!(
            reply,
            json!({ "Ok": [{ "type": "text_delta", "text": "ok" }] })
        );
        assert_eq!(*lock(&caller.seen_tier), Some(Tier::Code));
    }
}
