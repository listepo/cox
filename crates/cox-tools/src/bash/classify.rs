//! `classify(command) -> Risk` for `bash` (plan.md T3.7 step 3): a
//! tree-sitter-bash walk that splits the line on `;`, `&&`, `||` and pipes
//! and keeps the riskiest segment. Separate from the runner because the
//! permission engine rates a command line before anything runs, and tests
//! drive it without a PTY.

use cox_protocol::Risk;
use tree_sitter::{Node, Parser};

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
/// Redirect targets under `/dev/` that discard or echo rather than overwrite a device.
const HARMLESS_DEVICES: &[&str] = &["/dev/null", "/dev/stdout", "/dev/stderr", "/dev/tty"];

/// The riskiest thing `command` can do, or `Exec` when it cannot be parsed.
pub fn classify(command: &str) -> Risk {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .is_err()
    {
        return Risk::Exec;
    }
    let Some(tree) = parser.parse(command, None) else {
        return Risk::Exec;
    };
    let mut risk = if tree.root_node().has_error() || command.trim().is_empty() {
        Risk::Exec
    } else {
        Risk::ReadOnly
    };
    walk(tree.root_node(), command.as_bytes(), &mut risk);
    risk
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

fn walk(node: Node, src: &[u8], risk: &mut Risk) {
    match node.kind() {
        "command" => bump(risk, command_risk(&words(node, src))),
        "pipeline" if piped_into_interpreter(node, src) => bump(risk, Risk::Destructive),
        "file_redirect" => bump(risk, redirect_risk(node, src)),
        // Anything that forks a shell or feeds a command's output back in.
        "subshell"
        | "command_substitution"
        | "process_substitution"
        | "heredoc_redirect"
        | "herestring_redirect" => bump(risk, Risk::Exec),
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, src, risk);
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
            let inner: Vec<String> = args
                .iter()
                .skip_while(|a| a.starts_with('-') || a.contains('='))
                .cloned()
                .collect();
            match (inner.is_empty(), n) {
                (true, "env") => Risk::ReadOnly,
                (true, _) => Risk::Exec,
                (false, _) => command_risk(&inner),
            }
        }
        n if READ_ONLY.contains(&n) => Risk::ReadOnly,
        _ => Risk::Exec,
    }
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
