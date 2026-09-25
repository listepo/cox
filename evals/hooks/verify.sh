#!/usr/bin/env sh
# T30.3 PostToolUse hook (matcher: edit|apply_patch|write), wired by
# `evals/run.py --preset verify` through the task's COX_HOME/config.toml.
#
# Detects the project's test command (in order: `just test`, `cargo
# nextest run`, `npm test`, `pytest`), runs it under `sh -c` with a 120s
# cap, and on failure reports the output tail back to the model as
# `additionalContext` in cox's hook JSON output (cox_ext::hooks::verdict).
#
# Fails open per AGENTS.md D14: no test command detected, a passing run,
# or a missing python3 (needed to emit valid JSON) all exit 0 with no
# stdout, which cox reads as HookOutcome::Continue.

set -eu

TIMEOUT_S=120
TAIL_LINES=60

# The hook payload arrives on stdin; nothing here needs tool_name/
# tool_input, but the pipe must still be drained.
cat >/dev/null 2>&1 || true

detect_cmd() {
    if { [ -f Justfile ] || [ -f justfile ]; } \
        && command -v just >/dev/null 2>&1 \
        && just --list 2>/dev/null | grep -qE '(^|[[:space:]])test([[:space:]]|$)'; then
        printf '%s' "just test"
        return 0
    fi
    if [ -f Cargo.toml ] && command -v cargo >/dev/null 2>&1 \
        && cargo nextest --version >/dev/null 2>&1; then
        printf '%s' "cargo nextest run"
        return 0
    fi
    if [ -f package.json ] && command -v npm >/dev/null 2>&1 \
        && grep -q '"test"[[:space:]]*:' package.json 2>/dev/null; then
        printf '%s' "npm test"
        return 0
    fi
    if command -v pytest >/dev/null 2>&1 \
        && { [ -f pytest.ini ] || [ -f pyproject.toml ] || [ -f setup.cfg ] || [ -d tests ]; }; then
        printf '%s' "pytest"
        return 0
    fi
    return 1
}

CMD=$(detect_cmd) || exit 0

TIMEOUT_BIN=""
if command -v timeout >/dev/null 2>&1; then
    TIMEOUT_BIN="timeout"
elif command -v gtimeout >/dev/null 2>&1; then
    TIMEOUT_BIN="gtimeout"
fi

OUT_FILE=$(mktemp)
trap 'rm -f "$OUT_FILE"' EXIT

STATUS=0
if [ -n "$TIMEOUT_BIN" ]; then
    "$TIMEOUT_BIN" "${TIMEOUT_S}s" sh -c "$CMD" >"$OUT_FILE" 2>&1 || STATUS=$?
else
    # No `timeout` binary: fall back to a background job plus a watcher
    # that TERMs it after the cap.
    sh -c "$CMD" >"$OUT_FILE" 2>&1 &
    CMD_PID=$!
    ( sleep "$TIMEOUT_S"; kill -TERM "$CMD_PID" 2>/dev/null ) &
    WATCHER_PID=$!
    if wait "$CMD_PID"; then
        STATUS=0
    else
        STATUS=$?
    fi
    kill "$WATCHER_PID" 2>/dev/null || true
    wait "$WATCHER_PID" 2>/dev/null || true
fi

if [ "$STATUS" -eq 0 ]; then
    exit 0
fi

# Emit valid JSON only if python3 is available; otherwise fail open.
command -v python3 >/dev/null 2>&1 || exit 0

tail -n "$TAIL_LINES" "$OUT_FILE" 2>/dev/null | python3 -c '
import json
import sys

cmd, status = sys.argv[1], sys.argv[2]
tail = sys.stdin.read()
message = f"`{cmd}` failed (exit {status}):\n{tail}"
print(json.dumps({"hookSpecificOutput": {"additionalContext": message}}))
' "$CMD" "$STATUS"
