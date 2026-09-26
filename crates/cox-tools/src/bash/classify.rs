//! `classify(command) -> Risk` and `segments(command) -> Segments` for
//! `bash` (plan.md T3.7 step 3, T36.1): one tree-sitter-bash walk that
//! splits the line on `;`, `&&`, `||`, pipes, `&` and newlines, keeps the
//! riskiest segment and lists every simple command for the permission
//! engine to match one by one. Separate from the runner because the
//! permission engine rates a command line before anything runs, and tests
//! drive it without a PTY. Parser setup (`parse_bash`) lives in
//! `cox-syntax` (T32.4: tree-sitter is the only reason that crate exists);
//! this file keeps the risk walk itself, since it is domain logic, not
//! parsing.

use cox_protocol::{Risk, Segments};
use cox_syntax::Node;

/// Commands that cannot change anything cox does not already show the model.
const READ_ONLY: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "grep",
    "rg",
    "egrep",
    "fgrep",
    "pwd",
    "echo",
    "printf",
    "wc",
    "which",
    "whereis",
    "type",
    "stat",
    "file",
    "tree",
    "diff",
    "sort",
    "uniq",
    "cut",
    "tr",
    "basename",
    "dirname",
    "realpath",
    "readlink",
    "date",
    "whoami",
    "id",
    "uname",
    "printenv",
    "cd",
    "true",
    "false",
    "test",
    "[",
    "du",
    "df",
    "ps",
    "jq",
    "less",
    "more",
    "md5sum",
    "sha256sum",
    "shasum",
    "hexdump",
    "xxd",
    "strings",
    "column",
    "nl",
    "fold",
    "paste",
    "comm",
    "tac",
    "rev",
    "seq",
    "expr",
];
/// Prefix commands whose risk is that of the command they run.
const WRAPPERS: &[&str] = &["env", "command", "nohup", "time", "nice", "xargs", "exec"];
const DOWNLOADERS: &[&str] = &["curl", "wget"];
const INTERPRETERS: &[&str] = &[
    "sh", "bash", "zsh", "dash", "ksh", "python", "python3", "perl", "ruby", "node",
];
/// Shells whose `-c` runs a string the split cannot see into.
const SHELLS: &[&str] = &["sh", "bash", "zsh", "dash", "ksh", "fish", "csh", "tcsh"];
/// Redirect targets under `/dev/` that discard or echo rather than overwrite a device.
const HARMLESS_DEVICES: &[&str] = &["/dev/null", "/dev/stdout", "/dev/stderr", "/dev/tty"];

/// The riskiest thing `command` can do, or `Exec` when it cannot be parsed.
pub fn classify(command: &str) -> Risk {
    scan(command).0
}

/// The simple commands `command` runs, for the permission engine (T36.1).
/// Opaque when the parse cannot vouch for the whole line: a parse error, no
/// command at all, a substitution, `eval`/`sh -c`, a variable assignment
/// (`PATH=… git` runs a different `git`) or an output redirect to a path.
pub fn segments(command: &str) -> Segments {
    scan(command).1
}

fn scan(command: &str) -> (Risk, Segments) {
    let opaque = Segments {
        commands: Vec::new(),
        opaque: true,
    };
    let Some(tree) = cox_syntax::parse_bash(command) else {
        return (Risk::Exec, opaque);
    };
    let broken = tree.root_node().has_error() || command.trim().is_empty();
    let mut scan = Scan {
        risk: if broken { Risk::Exec } else { Risk::ReadOnly },
        segments: Segments {
            opaque: broken,
            ..Segments::default()
        },
    };
    walk(tree.root_node(), command.as_bytes(), &mut scan);
    if scan.segments.commands.is_empty() {
        scan.segments.opaque = true;
    }
    (scan.risk, scan.segments)
}

/// What one walk collects: the call's risk and its permission segments.
struct Scan {
    risk: Risk,
    segments: Segments,
}

fn rank(r: Risk) -> u8 {
    match r {
        Risk::ReadOnly => 0,
        Risk::Write => 1,
        Risk::Exec => 2,
        Risk::Destructive => 3,
    }
}

fn bump(cur: &mut Risk, r: Risk) {
    if rank(r) > rank(*cur) {
        *cur = r;
    }
}

fn walk(node: Node, src: &[u8], scan: &mut Scan) {
    let risk = &mut scan.risk;
    match node.kind() {
        "command" => {
            let words = words(node, src);
            bump(risk, command_risk(&words));
            scan.segments.opaque |= runs_code_string(&words);
            // From the name on: a deny rule matches past a leading
            // assignment, which already makes the call opaque to allow rules.
            let from = node.child_by_field_name("name").unwrap_or(node);
            let line = src
                .get(from.start_byte()..node.end_byte())
                .unwrap_or_default();
            let line = String::from_utf8_lossy(line).trim().to_owned();
            // A `MISSING` name after a dangling `&&` has no text; the parse
            // error has already made the call opaque.
            if !line.is_empty() {
                scan.segments.commands.push(line);
            }
        }
        "pipeline" if piped_into_interpreter(node, src) => bump(risk, Risk::Destructive),
        "file_redirect" => {
            let r = redirect_risk(node, src);
            bump(risk, r);
            scan.segments.opaque |= r != Risk::ReadOnly;
        }
        // Output fed back in as words the split never sees as commands.
        "command_substitution" | "process_substitution" => {
            bump(risk, Risk::Exec);
            scan.segments.opaque = true;
        }
        // Anything that forks a shell.
        "subshell" | "heredoc_redirect" | "herestring_redirect" => bump(risk, Risk::Exec),
        // Changes what a later command resolves to (`PATH=…; git status`).
        "variable_assignment"
        | "variable_assignments"
        | "declaration_command"
        | "unset_command" => scan.segments.opaque = true,
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, src, scan);
    }
}

/// `eval …` or `sh -c …`, possibly behind a wrapper: a string run as code.
fn runs_code_string(words: &[String]) -> bool {
    let Some((name, args)) = words.split_first() else {
        return false;
    };
    match base(name) {
        "eval" => true,
        n if SHELLS.contains(&n) => args
            .iter()
            .any(|a| a.starts_with('-') && !a.starts_with("--") && a.contains('c')),
        n if WRAPPERS.contains(&n) => runs_code_string(wrapped(args)),
        _ => false,
    }
}

fn text(node: Node, src: &[u8]) -> String {
    node.utf8_text(src).unwrap_or_default().to_owned()
}

/// The command name followed by its arguments; leading `VAR=x` assignments
/// are dropped because they do not change what runs.
fn words(node: Node, src: &[u8]) -> Vec<String> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|c| c.kind() != "variable_assignment")
        .map(|c| text(c, src))
        .collect()
}

fn command_name(node: Node, src: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .map(|n| base(&text(n, src)).to_owned())
}

fn base(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

fn command_risk(words: &[String]) -> Risk {
    let Some((name, args)) = words.split_first() else {
        return Risk::Exec;
    };
    let name = base(name);
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let short = |c: char| {
        args.iter()
            .any(|a| a.starts_with('-') && !a.starts_with("--") && a.contains(c))
    };
    match name {
        "sudo" | "doas" | "dd" | "shutdown" | "reboot" | "halt" => Risk::Destructive,
        n if n.starts_with("mkfs") => Risk::Destructive,
        "rm" if short('r') || short('R') || has("--recursive") => Risk::Destructive,
        "chmod" | "chown" | "chgrp" if short('R') || has("--recursive") => Risk::Destructive,
        "git" => git_risk(args),
        "cargo" => match args.first().map(String::as_str) {
            Some("check" | "test" | "build" | "clippy" | "metadata" | "tree" | "doc") => {
                Risk::ReadOnly
            }
            _ => Risk::Exec,
        },
        "npm" | "pnpm" | "yarn" if args == ["test"] || args == ["run", "test"] => Risk::ReadOnly,
        "find" if has("-delete") || has("-exec") || has("-execdir") || has("-ok") => Risk::Exec,
        "find" => Risk::ReadOnly,
        n if WRAPPERS.contains(&n) => {
            let inner = wrapped(args);
            match (inner.is_empty(), n) {
                (true, "env") => Risk::ReadOnly,
                (true, _) => Risk::Exec,
                (false, _) => command_risk(inner),
            }
        }
        n if READ_ONLY.contains(&n) => Risk::ReadOnly,
        _ => Risk::Exec,
    }
}

/// A wrapper's arguments from the command it runs on: its own flags and
/// `env`-style assignments come first.
fn wrapped(args: &[String]) -> &[String] {
    let own = args
        .iter()
        .take_while(|a| a.starts_with('-') || a.contains('='))
        .count();
    args.get(own..).unwrap_or_default()
}

fn git_risk(args: &[String]) -> Risk {
    let mut it = args.iter();
    let mut sub = None;
    while let Some(a) = it.next() {
        if a == "-C" || a == "-c" {
            it.next();
        } else if !a.starts_with('-') {
            sub = Some(a.as_str());
            break;
        }
    }
    let has = |flag: &str| args.iter().any(|a| a == flag);
    match sub {
        Some("status" | "diff" | "log" | "show" | "blame" | "rev-parse" | "ls-files") => {
            Risk::ReadOnly
        }
        Some("push") if has("--force") || has("-f") || has("--force-with-lease") => {
            Risk::Destructive
        }
        Some("reset") if has("--hard") => Risk::Destructive,
        Some("clean") => Risk::Destructive,
        _ => Risk::Exec,
    }
}

/// `curl … | sh` and friends: remote code straight into an interpreter.
fn piped_into_interpreter(node: Node, src: &[u8]) -> bool {
    let mut cursor = node.walk();
    let names: Vec<String> = node
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "command")
        .filter_map(|c| command_name(c, src))
        .collect();
    names
        .iter()
        .position(|n| DOWNLOADERS.contains(&n.as_str()))
        .is_some_and(|i| {
            names[i + 1..]
                .iter()
                .any(|n| INTERPRETERS.contains(&n.as_str()))
        })
}

fn redirect_risk(node: Node, src: &[u8]) -> Risk {
    let dest = node
        .child_by_field_name("destination")
        .map(|d| text(d, src))
        .unwrap_or_default();
    let mut cursor = node.walk();
    let op = node
        .children(&mut cursor)
        .find(|c| !c.is_named())
        .map(|c| text(c, src))
        .unwrap_or_default();
    // Reading stdin or duplicating a descriptor changes no file.
    if (op.contains('<') && !op.contains('>')) || dest.chars().all(|c| c.is_ascii_digit()) {
        return Risk::ReadOnly;
    }
    if dest.starts_with("/dev/") {
        return if HARMLESS_DEVICES.contains(&dest.as_str()) {
            Risk::ReadOnly
        } else {
            Risk::Destructive
        };
    }
    Risk::Exec
}
