//! The runtime side of the TUI's plugin redraw model (T33.23, PL§8):
//! serves `Cmd::Plugin` requests against the live plugin hosts and answers
//! on the TUI's feed as `Msg::Plugin`. Its own module because `cox-tui`
//! never holds a plugin (it depends on the ABI types only) and `session.rs`
//! only wires the channels; the session's live hosts (T33.44) are passed in.
//! T33.25 adds `Command`/`Key`: the same request/answer shape, `cox_command`
//! or `cox_key` in place of `cox_render`.

use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cox_plugin::{Lane, PluginHost, Redraw};
use cox_protocol::plugin::{CommandIn, CommandOut, Widget};
use cox_tui::state::{Msg, PluginRequest, PluginUiMsg};
use tokio::sync::mpsc::{Receiver, Sender};

/// A render's budget (PL§8, PL§12 "render 20 ms").
pub(crate) const RENDER_DEADLINE: Duration = Duration::from_millis(20);
/// A command or a key's budget (T33.25): interactive, not render-critical,
/// so it gets the same outer cap `cox_init` and a plugin hook already use.
pub(crate) const COMMAND_DEADLINE: Duration = Duration::from_secs(5);
const RENDER: &str = "cox_render";
const COMMAND: &str = "cox_command";
const KEY: &str = "cox_key";

/// Serves `requests` on a thread of its own, since a host call blocks for
/// up to its deadline. Requests that piled up behind a slow render are
/// served once each. Ends when the TUI drops its sender or the feed closes.
pub(crate) fn serve(
    hosts: Vec<(String, Arc<PluginHost>)>,
    mut requests: Receiver<PluginRequest>,
    feed: Sender<Msg>,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("cox-plugin-ui".into())
        .spawn(move || {
            while let Some(first) = requests.blocking_recv() {
                let mut batch = vec![first];
                while let Ok(next) = requests.try_recv() {
                    if !batch.contains(&next) {
                        batch.push(next);
                    }
                }
                for request in batch {
                    let plugin = match &request {
                        PluginRequest::Render { plugin, .. }
                        | PluginRequest::Command { plugin, .. }
                        | PluginRequest::Key { plugin, .. } => plugin,
                    };
                    let host = hosts.iter().find(|(id, _)| id == plugin);
                    let msg = answer(host.map(|(_, h)| h.as_ref()), request);
                    if feed.blocking_send(Msg::Plugin(msg)).is_err() {
                        return;
                    }
                }
            }
        })
}

/// One request's answer. A render inside `RENDER_DEADLINE` is `Rendered`;
/// a timeout, an error, a missing export or an unknown plugin is `Missed`,
/// so the TUI keeps the last good segment and counts the miss. A command or
/// a key (T33.25) answers `Command`, `out: None` covering the same misses.
pub(crate) fn answer(host: Option<&PluginHost>, request: PluginRequest) -> PluginUiMsg {
    match request {
        PluginRequest::Render { plugin, input } => {
            let slot = input.slot;
            let out =
                host.map(|h| h.call::<_, Widget>(Lane::Control, RENDER, &input, RENDER_DEADLINE));
            match out {
                Some(Ok(Some(widget))) => PluginUiMsg::Rendered {
                    plugin,
                    slot,
                    widget,
                },
                _ => PluginUiMsg::Missed { plugin, slot },
            }
        }
        PluginRequest::Command { plugin, name, args } => {
            let input = CommandIn { name, args };
            PluginUiMsg::Command {
                plugin,
                out: command_call(host, COMMAND, &input),
            }
        }
        PluginRequest::Key { plugin, name } => {
            let input = CommandIn {
                name,
                args: String::new(),
            };
            PluginUiMsg::Command {
                plugin,
                out: command_call(host, KEY, &input),
            }
        }
    }
}

/// `cox_command`/`cox_key`'s call, folding a timeout, an error, a missing
/// export or an unknown plugin into one `None` (PL§4 fails open).
fn command_call(host: Option<&PluginHost>, export: &str, input: &CommandIn) -> Option<CommandOut> {
    host?
        .call::<_, CommandOut>(Lane::Control, export, input, COMMAND_DEADLINE)
        .ok()
        .flatten()
}

/// The `Redraw` a `PluginTap` calls when a plugin's `Effects.redraw` is set.
/// `try_send`: the tap's pump thread must not wait on the TUI; a redraw lost
/// to a full feed is repainted by the plugin's next one.
pub(crate) fn redraw(feed: Sender<Msg>) -> Redraw {
    Arc::new(move |plugin: &str| {
        let msg = PluginUiMsg::Redraw {
            plugin: plugin.to_string(),
        };
        let _ = feed.try_send(Msg::Plugin(msg));
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cox_plugin_api::{Limits, RenderIn, Slot};
    use std::time::Instant;

    // The extism kernel imports a module needs to write its output (P15:
    // the loader takes WAT text).
    const IMPORTS: &str = r#"
      (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
      (import "extism:host/env" "store_u8" (func $store (param i64 i32)))
      (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))"#;
    const WIDGET: &str = r#"{"text":[[{"text":"3 turns"}]]}"#;

    /// A plugin whose `cox_render` is `render`; `cox_init` answers `{}`.
    fn host(render: &str) -> PluginHost {
        let json = WIDGET.replace('"', "\\\"");
        let wat = format!(
            r#"(module {IMPORTS}
              (memory 1)
              (data (i32.const 0) "{json}")
              (data (i32.const 64) "{{}}")
              (func $out (param $at i64) (param $n i64) (local $off i64) (local $i i64)
                (local.set $off (call $alloc (local.get $n)))
                (block $done (loop $copy
                  (br_if $done (i64.ge_u (local.get $i) (local.get $n)))
                  (call $store (i64.add (local.get $off) (local.get $i))
                    (i32.load8_u (i32.wrap_i64 (i64.add (local.get $at) (local.get $i)))))
                  (local.set $i (i64.add (local.get $i) (i64.const 1)))
                  (br $copy)))
                (call $output_set (local.get $off) (local.get $n)))
              (func (export "cox_init") (result i32)
                (call $out (i64.const 64) (i64.const 2)) (i32.const 0))
              (func (export "cox_render") (result i32) {render}))"#,
        );
        // A long call cap, so only the render deadline can cut a spin short.
        let limits = Limits {
            call_ms: Some(30_000),
            ..Limits::default()
        };
        PluginHost::load("acme", wat.as_bytes(), &limits).expect("module loads")
    }

    fn request() -> PluginRequest {
        PluginRequest::Render {
            plugin: "acme".into(),
            input: RenderIn {
                slot: Slot::StatusLeft,
                width: 24,
                height: 1,
            },
        }
    }

    #[test]
    fn render_answer_carries_the_widget() {
        let body = format!(
            "(call $out (i64.const 0) (i64.const {})) (i32.const 0)",
            WIDGET.len()
        );
        let msg = answer(Some(&host(&body)), request());
        let widget: Widget = serde_json::from_str(WIDGET).expect("widget json");
        assert_eq!(
            msg,
            PluginUiMsg::Rendered {
                plugin: "acme".into(),
                slot: Slot::StatusLeft,
                widget,
            }
        );
    }

    #[test]
    fn slow_render_is_missed_at_its_deadline() {
        let spin = host("(loop $l (br $l)) (i32.const 0)");
        let started = Instant::now();
        let msg = answer(Some(&spin), request());
        assert!(matches!(msg, PluginUiMsg::Missed { .. }), "{msg:?}");
        // `call_ms` is 30 s; only the 20 ms render deadline ends it this soon.
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        assert!(matches!(
            answer(None, request()),
            PluginUiMsg::Missed { .. }
        ));
    }

    const COMMAND_JSON: &str = r#"{"kind":"prompt","text":"go"}"#;

    /// A plugin whose `export` (`cox_command` or `cox_key`) is `body`;
    /// `cox_init` answers `{}`, like `host` above.
    fn command_host(export: &str, body: &str) -> PluginHost {
        let json = COMMAND_JSON.replace('"', "\\\"");
        let wat = format!(
            r#"(module {IMPORTS}
              (memory 1)
              (data (i32.const 0) "{json}")
              (data (i32.const 64) "{{}}")
              (func $out (param $at i64) (param $n i64) (local $off i64) (local $i i64)
                (local.set $off (call $alloc (local.get $n)))
                (block $done (loop $copy
                  (br_if $done (i64.ge_u (local.get $i) (local.get $n)))
                  (call $store (i64.add (local.get $off) (local.get $i))
                    (i32.load8_u (i32.wrap_i64 (i64.add (local.get $at) (local.get $i)))))
                  (local.set $i (i64.add (local.get $i) (i64.const 1)))
                  (br $copy)))
                (call $output_set (local.get $off) (local.get $n)))
              (func (export "cox_init") (result i32)
                (call $out (i64.const 64) (i64.const 2)) (i32.const 0))
              (func (export "{export}") (result i32) {body}))"#,
        );
        let limits = Limits {
            call_ms: Some(30_000),
            ..Limits::default()
        };
        PluginHost::load("acme", wat.as_bytes(), &limits).expect("module loads")
    }

    /// T33.25: a `/<id>:<name>` command calls `cox_command` and its
    /// `CommandOut` reaches the TUI as `PluginUiMsg::Command`.
    #[test]
    fn command_answer_carries_command_out() {
        let body = format!(
            "(call $out (i64.const 0) (i64.const {})) (i32.const 0)",
            COMMAND_JSON.len()
        );
        let host = command_host("cox_command", &body);
        let request = PluginRequest::Command {
            plugin: "acme".into(),
            name: "go".into(),
            args: String::new(),
        };
        assert_eq!(
            answer(Some(&host), request),
            PluginUiMsg::Command {
                plugin: "acme".into(),
                out: Some(CommandOut::Prompt { text: "go".into() }),
            }
        );
    }

    /// T33.25: `<leader> <key>` calls `cox_key`, never `cox_command` — a
    /// plugin with only the latter fails open (`out: None`) for a `Key`
    /// request, the same as a missing export always has.
    #[test]
    fn key_request_calls_cox_key_not_cox_command() {
        let body = format!(
            "(call $out (i64.const 0) (i64.const {})) (i32.const 0)",
            COMMAND_JSON.len()
        );
        let host = command_host("cox_command", &body);
        let request = PluginRequest::Key {
            plugin: "acme".into(),
            name: "reset".into(),
        };
        assert_eq!(
            answer(Some(&host), request),
            PluginUiMsg::Command {
                plugin: "acme".into(),
                out: None,
            }
        );
    }
}
