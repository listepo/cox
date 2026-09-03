# Security policy

## Trust boundaries

Everything the model, a tool, an MCP server, a hook, a skill file or a
repository writes is untrusted input. Four guards hold the line; simplicity
never removes one of them:

- `cox_core::permission::Engine` — the single place a tool call is allowed,
  denied or escalated. A tool never checks its own permission.
- `cox_tools::path::confine` — every path from the model passes through it;
  rejects escapes from the workspace roots.
- `cox_tools::sandbox::Policy` — a shell command runs under the platform
  sandbox (Seatbelt on macOS, bubblewrap else Landlock + seccomp on Linux)
  unless the user chose `danger-full-access` for that session. Windows has
  no sandbox: loud warning, `on-request` forced.
- `cox_tui::text::sanitize` — strips escape sequences and bidi overrides
  from anything the model or a tool prints. A tool result is the one place
  cox shows a whole file someone else wrote.

Broken hooks, skills, MCP servers and plugins are warned about and skipped,
never fatal (fail open on extensions). `cargo deny` + `cargo audit` run on
every PR; the four input parsers (SSE, V4A, frontmatter, permission rules)
fuzz nightly (see `fuzz/` and `.github/workflows/nightly.yml`).

## Reporting a vulnerability

Open a private security advisory at
<https://github.com/listepo/cox/security/advisories/new> — do not file a
public issue for anything that leaks credentials, escapes the sandbox or
the workspace, or runs code outside an approved tool call. Say which guard
above you believe failed and how to reproduce it; you will hear back within
a week.
