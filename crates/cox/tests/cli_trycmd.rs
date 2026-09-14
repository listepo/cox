//! Headless `cox run -p` full-output fixtures (trycmd).
//!
//! Why trycmd here and insta elsewhere: a headless run is one process with
//! full stdout/stderr plus an exit code — a herd of blunt cases, which is
//! what trycmd is for. TUI widgets and transcripts stay `insta` snapshots
//! (plan.md D10); interactive stdin driving and filesystem side effects stay
//! `assert_cmd` in `run_cli.rs`, which can predicate on them.
//!
//! Each case owns `target/trycmd/<case>/{home,work}` (recreated below before
//! the run) so parallel cases never share a store; the paths are
//! package-root-relative so the fixtures stay machine-independent.
//! `COX_HOME`/`HOME` are scratch, never the real `~/.cox` (AGENTS.md).

use std::path::Path;

const CASES: &[&str] = &[
    "run_text",
    "run_json",
    "run_stream_json",
    "run_denied",
    "run_auto",
    "run_bad_format",
];

#[test]
fn headless_run_output() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/trycmd");
    for case in CASES {
        let dir = root.join(case);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("clear stale trycmd scratch");
        }
        std::fs::create_dir_all(dir.join("home")).expect("trycmd home scratch");
        std::fs::create_dir_all(dir.join("work")).expect("trycmd work scratch");
    }
    trycmd::TestCases::new().case("tests/trycmd/*.toml");
}
