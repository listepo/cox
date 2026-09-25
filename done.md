

#### T30.6 Anthropic tool calls reach the core

Model: claude-opus-5-5 · Status: done 2026-09-25 · Blocks: T30.3 · Size: ~40 · Priority: P0 · Complexity: 1
Goal: a `tool_use` block from the Anthropic stream becomes a tool call. `AnthropicStream` emits `ToolUseStart` and the input deltas but nothing on `content_block_stop`, and `turn::consume_provider` commits a call only on `ToolUseEnd`, so every Anthropic tool call is dropped and the turn ends with empty text (`end_turn`). The fixture snapshots were recorded with the bug and never show `ToolUseEnd`. Found by T30.3's first live task (`create-file`: 72 output tokens, no file).
Files: `crates/cox-provider/src/anthropic/stream.rs`, `fixtures/anthropic/live_tool_use.sse` (new, a real `claude-sonnet-5` stream), the two tool-call snapshots.
Plan: (1) `content_block_stop` of a `ToolUse` block emits `ToolUseEnd`; text/thinking blocks still emit nothing; (2) test `anthropic_stream_tool_block_stop_ends_the_call` on the live fixture: the event after the last input delta is `ToolUseEnd`, and the joined input parses to `{"path":"hello.txt","content":"hi"}`; (3) accept the updated `one_tool_call`/`parallel_tool_calls` snapshots; (4) live: the `create-file` eval task passes.
Check:
```bash
mise exec -- cargo nextest run -p cox-provider anthropic::stream
```
Done when: `python3 evals/run.py --provider anthropic --model claude-sonnet-5 --only create-file` passes.
What landed (`9c29639`): `AnthropicStream::feed` emits `ToolUseEnd` when a `content_block_stop` closes a `tool_use` block (text/thinking still emit nothing); `fixtures/anthropic/live_tool_use.sse` is a real `claude-sonnet-5` stream; test `anthropic_stream_tool_block_stop_ends_the_call`; the `one_tool_call` and `parallel_tool_calls` snapshots gained exactly one `tool_use_end` per call.
Check:
```text
$ mise exec -- cargo nextest run -p cox-provider anthropic::stream
     8 tests run: 8 passed (after `cargo insta accept`)
$ ANTHROPIC_API_KEY=<keychain cox/anthropic> python3 evals/run.py --provider anthropic --model claude-sonnet-5 --only create-file
create-file PASS $0.0237  turns=2  5.3s
1/1 passed  total cost $0.0237
$ mise exec -- cargo nextest run --workspace
     Summary [ 26.982s] 866 tests run: 866 passed, 3 skipped
$ mise exec -- cargo clippy --workspace --all-targets -- -D warnings
     clean
$ mise exec -- cargo fmt --check
     clean
```
