# Getting started with cox

cox is a modular terminal coding agent in Rust. One core state machine turns
submissions into typed events; the same stream powers the TUI, headless runs,
editor clients (ACP), and MCP.

## Build and first run

Rust is pinned with [mise](https://mise.jdx.dev/). Prefer `mise exec -- cargo …`
over a global toolchain.

```bash
git clone https://github.com/listepo/cox && cd cox
mise exec -- cargo build -p cox
export ANTHROPIC_API_KEY=sk-...   # or OPENAI_API_KEY
./target/debug/cox doctor         # green except prices? you are good
./target/debug/cox -p "create hello.txt containing hi"
./target/debug/cox                # interactive TUI: Enter sends, Esc interrupts
```

In the TUI: `y` / `s` / `n` answer approval prompts, `/model` switches tiers,
`/compact` compacts context now. Headless scripts use `cox run -p`; editors use
`cox acp`; other agents can call `cox mcp`.

## Vim keys

Set `tui.vim = true` or type `/vim` to toggle vim keys in the composer. The
status line shows `-- NORMAL --`, `-- INSERT --`, `-- VISUAL --` or
`-- VISUAL LINE --`. `Enter` still sends from any mode.

| Keys | Mode | Effect |
| --- | --- | --- |
| `i` `a` `o` | normal | Insert before / after the cursor, on a new line below |
| `Esc` | insert, visual | Back to normal mode; in insert mode while a turn runs it interrupts the turn instead. In normal mode it only drops a pending command and never interrupts |
| `h j k l` `w b e` `0 ^ $` `gg G` | normal, visual | Motions; a count repeats them (`3w`, `2j`), `5G` goes to line 5 |
| `d` `c` `y` + motion | normal | Delete, change, yank over the motion (`d2w`, `c$`, `yG`) |
| `dd` `cc` `yy` | normal | Whole line; a count takes more lines (`3dd`) |
| `iw aw` `i" a"` `i' a'` `i( a(` `i[ a[` `i{ a{` | after `d c y`, visual | Text objects: inner / around a word, quotes or brackets (`ci"`, `da(`) |
| `x` `X` | normal | Delete the character under / before the cursor |
| `p` `P` | normal | Put after / before the cursor (linewise yanks put whole lines) |
| `u` `Ctrl+R` | normal | Undo / redo; one insert session is one undo step |
| `v` `V` | normal | Characterwise / linewise visual mode; `d` `c` `y` `x` act on the selection |
| `Ctrl+C` | any | Interrupts the running turn |

Not supported: `.` repeat, macros and registers.

## What to read next

| Doc | Contents |
| --- | --- |
| [config.md](config.md) | Every configuration key |
| [tools.md](tools.md) | Built-in tools, risk, subjects |
| [observability.md](observability.md) | Traces, metrics, OTLP backends |
| [ide.md](ide.md) | Zed, JetBrains, Neovim via ACP |
| [how-it-works.md](how-it-works.md) | One user turn on the event stream |
| [compat.md](compat.md) | What cox reads from `.claude/` / Codex setups |

Costs land in `cox stats`. Screenshots of TUI states live in
[screenshots/](screenshots/).

## Status

cox is under active development. APIs, configuration, and install paths are not
yet stable. Treat this tree as the current manual, not a frozen release surface.
