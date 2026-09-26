//! Enforces the crate dependency-direction rules from plan.md §1.1 by
//! parsing `cargo metadata` rather than hand-maintaining a second copy of
//! the graph that could drift out of sync with the workspace `Cargo.toml`s.

use std::collections::{HashMap, HashSet};
use std::process::Command;

use serde_json::Value;

/// Maps each workspace crate name to the set of *other workspace crates* it
/// depends on (external deps like `serde` or `clap` are filtered out).
fn workspace_deps() -> HashMap<String, HashSet<String>> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .expect("cargo metadata should run");
    assert!(
        output.status.success(),
        "cargo metadata exited with {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let meta: Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata output is valid json");

    let packages = meta["packages"].as_array().expect("packages array");
    let workspace_names: HashSet<String> = packages
        .iter()
        .map(|p| p["name"].as_str().expect("package name").to_string())
        .collect();

    packages
        .iter()
        .map(|pkg| {
            let name = pkg["name"].as_str().expect("package name").to_string();
            let deps = pkg["dependencies"]
                .as_array()
                .expect("dependencies array")
                .iter()
                .filter(|d| d.get("kind").and_then(|k| k.as_str()) != Some("dev"))
                .filter_map(|d| d["name"].as_str().map(str::to_string))
                .filter(|d| workspace_names.contains(d))
                .collect();
            (name, deps)
        })
        .collect()
}

/// Maps each workspace crate name to the set of *all* its declared
/// dependency names (workspace and external alike), unlike `workspace_deps`
/// which filters down to workspace crates only.
fn all_deps() -> HashMap<String, HashSet<String>> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .expect("cargo metadata should run");
    assert!(
        output.status.success(),
        "cargo metadata exited with {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let meta: Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata output is valid json");

    meta["packages"]
        .as_array()
        .expect("packages array")
        .iter()
        .map(|pkg| {
            let name = pkg["name"].as_str().expect("package name").to_string();
            let deps = pkg["dependencies"]
                .as_array()
                .expect("dependencies array")
                .iter()
                .filter_map(|d| d["name"].as_str().map(str::to_string))
                .collect();
            (name, deps)
        })
        .collect()
}

/// D9/plan.md §1.7: "No other crate may depend on `diesel`" — `cox-store`
/// is the only crate allowed to contain SQL.
#[test]
fn only_store_depends_on_diesel() {
    let deps = all_deps();
    for (crate_name, crate_deps) in &deps {
        if crate_name == "cox-store" {
            continue;
        }
        for diesel_crate in ["diesel", "diesel_migrations", "libsqlite3-sys"] {
            assert!(
                !crate_deps.contains(diesel_crate),
                "{crate_name} must not depend on {diesel_crate}; only cox-store may contain SQL"
            );
        }
    }
}

#[test]
fn no_crate_below_cox_depends_on_core() {
    let deps = workspace_deps();

    // cox-protocol is the base: no workspace-crate dependencies at all.
    assert!(
        deps["cox-protocol"].is_empty(),
        "cox-protocol must not depend on any other workspace crate, found {:?}",
        deps["cox-protocol"]
    );

    // cox-models (T30.24: the model/price catalog) depends only on
    // cox-protocol among workspace crates.
    let models_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    assert!(
        deps["cox-models"]
            .iter()
            .all(|d| models_allowed.contains(d.as_str())),
        "cox-models may only depend on cox-protocol among workspace crates, found {:?}",
        deps["cox-models"]
    );

    // cox-sanitize (T32.1) is a pure trust guard, same as cox-protocol: no
    // workspace-crate dependencies at all.
    assert!(
        deps["cox-sanitize"].is_empty(),
        "cox-sanitize must not depend on any other workspace crate, found {:?}",
        deps["cox-sanitize"]
    );

    // cox-sandbox (T32.3) is a trust guard that depends only on
    // cox-protocol among workspace crates (SandboxPolicy, SandboxMode,
    // LinuxBackend, ToolError).
    let sandbox_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    assert!(
        deps["cox-sandbox"]
            .iter()
            .all(|d| sandbox_allowed.contains(d.as_str())),
        "cox-sandbox may only depend on cox-protocol among workspace crates, found {:?}",
        deps["cox-sandbox"]
    );

    // cox-syntax (T32.4) is a pure parsing engine (tree-sitter and its
    // five grammars): no workspace-crate dependencies at all.
    assert!(
        deps["cox-syntax"].is_empty(),
        "cox-syntax must not depend on any other workspace crate, found {:?}",
        deps["cox-syntax"]
    );

    // cox-tokens (T32.10) is a pure leaf that depends only on cox-protocol
    // among workspace crates (ProviderError, Content, Request) — it takes a
    // plain reqwest::Client/HeaderMap rather than any cox-provider type, so
    // it never depends back on cox-provider.
    let tokens_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    assert!(
        deps["cox-tokens"]
            .iter()
            .all(|d| tokens_allowed.contains(d.as_str())),
        "cox-tokens may only depend on cox-protocol among workspace crates, found {:?}",
        deps["cox-tokens"]
    );

    // cox-permission (T32.8) is a pure trust guard that depends only on
    // cox-protocol among workspace crates (rule grammar, `Engine::decide`).
    let permission_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    assert!(
        deps["cox-permission"]
            .iter()
            .all(|d| permission_allowed.contains(d.as_str())),
        "cox-permission may only depend on cox-protocol among workspace crates, found {:?}",
        deps["cox-permission"]
    );

    // cox-config (T32.16), the one config owner, depends only on
    // cox-protocol among workspace crates (`Config`, `CoreError`): the CLI
    // flag layer and the `.claude/settings.json` import (cox-ext) are passed
    // in by `crates/cox`.
    let config_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    assert!(
        deps["cox-config"]
            .iter()
            .all(|d| config_allowed.contains(d.as_str())),
        "cox-config may only depend on cox-protocol among workspace crates, found {:?}",
        deps["cox-config"]
    );

    // cox-core depends only on cox-protocol among workspace crates (and may
    // depend on cox-models once a card actually wires the catalog in, and
    // on cox-permission, T32.8, re-exported at the old `permission` path).
    let core_allowed: HashSet<&str> = ["cox-protocol", "cox-models", "cox-permission"]
        .into_iter()
        .collect();
    assert!(
        deps["cox-core"]
            .iter()
            .all(|d| core_allowed.contains(d.as_str())),
        "cox-core may only depend on cox-protocol/cox-models/cox-permission among workspace crates, found {:?}",
        deps["cox-core"]
    );

    // cox-tui and cox-acp may depend on cox-core, cox-protocol and
    // cox-sanitize (T32.1's guard), nothing else.
    let surface_allowed: HashSet<&str> = ["cox-core", "cox-protocol", "cox-sanitize"]
        .into_iter()
        .collect();
    for crate_name in ["cox-tui", "cox-acp"] {
        let d = &deps[crate_name];
        assert!(
            d.iter().all(|dep| surface_allowed.contains(dep.as_str())),
            "{crate_name} may only depend on cox-core/cox-protocol/cox-sanitize among workspace crates, found {d:?}"
        );
    }

    // cox-provider-http (T32.12) is a pure leaf shared by every wire
    // (http.rs/retry.rs/sse.rs): connection setup, credential resolution,
    // error mapping, SSE framing and retry/backoff. It depends only on
    // cox-protocol among workspace crates.
    let provider_http_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    assert!(
        deps["cox-provider-http"]
            .iter()
            .all(|d| provider_http_allowed.contains(d.as_str())),
        "cox-provider-http may only depend on cox-protocol among workspace crates, found {:?}",
        deps["cox-provider-http"]
    );

    // cox-provider additionally depends on cox-models (`Priced` prices every
    // call through the catalog's `PriceTable`, T30.24), cox-tokens
    // (re-exported at the old `tokens` path, T32.10), cox-provider-http
    // (re-exported at the old `http`/`retry`/`sse` paths, T32.12) and
    // cox-provider-testkit (T32.11): `scripted`/`replay` are thin glue over
    // the pure scenario/cassette helpers moved there, and `from_env` uses
    // them in production (COX_PROVIDER=scripted|replay), not just in tests.
    let provider_allowed: HashSet<&str> = [
        "cox-protocol",
        "cox-models",
        "cox-tokens",
        "cox-provider-http",
        "cox-provider-testkit",
    ]
    .into_iter()
    .collect();
    let provider_deps = &deps["cox-provider"];
    assert!(
        !provider_deps.contains("cox-core"),
        "cox-provider must not depend on cox-core"
    );
    assert!(
        provider_deps
            .iter()
            .all(|dep| provider_allowed.contains(dep.as_str())),
        "cox-provider may only depend on cox-protocol/cox-models/cox-tokens/cox-provider-http/cox-provider-testkit among workspace crates, found {provider_deps:?}"
    );

    // cox-provider-testkit (T32.11) is a pure leaf, same shape as
    // cox-patch/cox-sanitize/cox-syntax: no workspace-crate dependencies
    // beyond cox-protocol, so it never depends back on cox-provider (which
    // would cycle with cox-provider's `pub use` re-export of it).
    let testkit_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    assert!(
        deps["cox-provider-testkit"]
            .iter()
            .all(|d| testkit_allowed.contains(d.as_str())),
        "cox-provider-testkit may only depend on cox-protocol among workspace crates, found {:?}",
        deps["cox-provider-testkit"]
    );

    // cox-patch (T32.6) is the V4A parse/match/stage engine: a pure leaf,
    // same shape as cox-models/cox-sanitize. `ApplyPatchTool` — the `Tool`
    // impl that calls `path::confine` and `write::atomic_write` — stays in
    // cox-tools so `confine` keeps its single call site.
    let patch_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    assert!(
        deps["cox-patch"]
            .iter()
            .all(|d| patch_allowed.contains(d.as_str())),
        "cox-patch may only depend on cox-protocol among workspace crates, found {:?}",
        deps["cox-patch"]
    );

    // mcp/store/ext depend only on cox-protocol: this is the rule the test
    // is named for — none of them may reach cox-core.
    let leaf_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    for crate_name in ["cox-mcp", "cox-store", "cox-ext"] {
        let d = &deps[crate_name];
        assert!(
            !d.contains("cox-core"),
            "{crate_name} must not depend on cox-core"
        );
        assert!(
            d.iter().all(|dep| leaf_allowed.contains(dep.as_str())),
            "{crate_name} may only depend on cox-protocol among workspace crates, found {d:?}"
        );
    }

    // cox-tools additionally depends on cox-sandbox (T32.3: path::confine
    // and the sandbox backends), cox-patch (T32.6: the V4A engine) and
    // cox-syntax (T32.4: outline and parse_bash).
    let tools_allowed: HashSet<&str> = ["cox-protocol", "cox-sandbox", "cox-patch", "cox-syntax"]
        .into_iter()
        .collect();
    let tools_deps = &deps["cox-tools"];
    assert!(
        !tools_deps.contains("cox-core"),
        "cox-tools must not depend on cox-core"
    );
    assert!(
        tools_deps
            .iter()
            .all(|dep| tools_allowed.contains(dep.as_str())),
        "cox-tools may only depend on cox-protocol/cox-sandbox/cox-patch/cox-syntax among workspace crates, found {tools_deps:?}"
    );
}
