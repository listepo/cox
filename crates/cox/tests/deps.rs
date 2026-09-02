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

    // cox-core depends only on cox-protocol among workspace crates.
    let core_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    assert!(
        deps["cox-core"]
            .iter()
            .all(|d| core_allowed.contains(d.as_str())),
        "cox-core may only depend on cox-protocol among workspace crates, found {:?}",
        deps["cox-core"]
    );

    // cox-tui and cox-acp may depend on cox-core and cox-protocol, nothing else.
    let surface_allowed: HashSet<&str> = ["cox-core", "cox-protocol"].into_iter().collect();
    for crate_name in ["cox-tui", "cox-acp"] {
        let d = &deps[crate_name];
        assert!(
            d.iter().all(|dep| surface_allowed.contains(dep.as_str())),
            "{crate_name} may only depend on cox-core/cox-protocol among workspace crates, found {d:?}"
        );
    }

    // provider/tools/mcp/store/ext depend only on cox-protocol: this is the
    // rule the test is named for — none of them may reach cox-core.
    let leaf_allowed: HashSet<&str> = ["cox-protocol"].into_iter().collect();
    for crate_name in [
        "cox-provider",
        "cox-tools",
        "cox-mcp",
        "cox-store",
        "cox-ext",
    ] {
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
