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

    // cox-core depends only on cox-protocol among workspace crates (and may
    // depend on cox-models once a card actually wires the catalog in).
    let core_allowed: HashSet<&str> = ["cox-protocol", "cox-models"].into_iter().collect();
    assert!(
        deps["cox-core"]
            .iter()
            .all(|d| core_allowed.contains(d.as_str())),
        "cox-core may only depend on cox-protocol/cox-models among workspace crates, found {:?}",
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

    // cox-provider additionally depends on cox-models: `Priced` prices
    // every call through the catalog's `PriceTable` (T30.24).
    let provider_allowed: HashSet<&str> = ["cox-protocol", "cox-models"].into_iter().collect();
    let provider_deps = &deps["cox-provider"];
    assert!(
        !provider_deps.contains("cox-core"),
        "cox-provider must not depend on cox-core"
    );
    assert!(
        provider_deps
            .iter()
            .all(|dep| provider_allowed.contains(dep.as_str())),
        "cox-provider may only depend on cox-protocol/cox-models among workspace crates, found {provider_deps:?}"
    );

    // tools/mcp/store/ext depend only on cox-protocol: this is the rule the
    // test is named for — none of them may reach cox-core.
    let leaf_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    for crate_name in ["cox-tools", "cox-mcp", "cox-store", "cox-ext"] {
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
}
