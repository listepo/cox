#!/bin/bash
# Footprint benchmark (plan.md T30.2): cold start, time to first frame,
# peak RSS over a scripted replay of evals/token/sessions, and binary size.
#
#   bash scripts/footprint.sh          measure and print (writes nothing)
#   bash scripts/footprint.sh --write  measure and refresh scripts/footprint.json
#   bash scripts/footprint.sh --check  measure and fail on >20% vs the baseline
#   just footprint                     measure and print
#
# No hyperfine, no new dependencies: orchestration is bash, timing and JSON
# are python3 (already required by evals/run.py). RSS comes from
# /usr/bin/time -l (macOS, bytes) or -v (Linux, KiB) — on Linux install the
# `time` package first. Baselines are keyed by OS-arch because binaries and
# timings are per-target; --check with no entry for this machine warns and
# exits 0 so a new platform bootstraps instead of failing.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASELINE="$ROOT/scripts/footprint.json"
MODE="measure"
if [ $# -gt 1 ] || { [ $# -eq 1 ] && [ "$1" != "--write" ] && [ "$1" != "--check" ]; }; then
  echo "usage: footprint.sh [--write|--check]" >&2; exit 2
fi
[ $# -eq 1 ] && MODE="${1#--}"

OS="$(uname -s)-$(uname -m)"
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/footprint-XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT

BIN=""
for cand in "$ROOT/target/release/cox" "$ROOT/target/dist/cox"; do
  if [ -x "$cand" ]; then BIN="$cand"; break; fi
done
if [ -z "$BIN" ]; then
  echo "no release binary; building…" >&2
  (cd "$ROOT" && cargo build --release -p cox)
  BIN="$ROOT/target/release/cox"
fi

now_ns() { date +%s%N; }
median_ms() { python3 -c 'import statistics,sys; print(round(statistics.median(int(x) for x in open(sys.argv[1]))/1e6,1))' "$1"; }

# Cold start: median of 5 `cox --version` runs (spawn + clap + config load).
: > "$SCRATCH/startup"
for _ in 1 2 3 4 5; do
  s="$(now_ns)"
  COX_HOME="$SCRATCH/home" HOME="$SCRATCH/home" "$BIN" --version >/dev/null
  echo "$(( $(now_ns) - s ))" >> "$SCRATCH/startup"
done
STARTUP_MS="$(median_ms "$SCRATCH/startup")"

# First frame: spawn to first stream-json event with the scripted provider.
printf '[[turn]]\ntext = "hello"\n' > "$SCRATCH/first.toml"
: > "$SCRATCH/first"
for _ in 1 2 3; do
  s="$(now_ns)"
  (cd "$ROOT" && COX_HOME="$SCRATCH/home" HOME="$SCRATCH/home" COX_PROVIDER=scripted \
    COX_SCENARIO="$SCRATCH/first.toml" "$BIN" run -p "hi" --output-format stream-json \
    --max-turns 2 --approve never --permission-mode auto --no-hooks --no-mcp \
    > "$SCRATCH/first.jsonl" 2> "$SCRATCH/first.err") &
  pid="$!"
  for _ in $(seq 1 1000); do
    [ -s "$SCRATCH/first.jsonl" ] && break
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.01
  done
  echo "$(( $(now_ns) - s ))" >> "$SCRATCH/first"
  wait "$pid" || { echo "first-frame run failed:" >&2; cat "$SCRATCH/first.err" >&2; exit 1; }
  [ -s "$SCRATCH/first.jsonl" ] || { echo "first-frame run printed nothing" >&2; exit 1; }
done
FIRST_MS="$(median_ms "$SCRATCH/first")"

# Replay: every evals/token transcript becomes one Scripted scenario per user
# turn; turns run back to back through --resume so context grows like a live
# session (real read/grep/glob tools over a workspace copy). The corpus holds
# 30 user turns / 60 provider calls, not 50 — see research.md §4.7.
mkdir -p "$SCRATCH/replay" "$SCRATCH/ws" "$SCRATCH/home"
cp -r "$ROOT/evals/token/workspace/." "$SCRATCH/ws/"
python3 - "$ROOT/evals/token/sessions" "$SCRATCH/replay" <<'EOF'
import glob, json, os, sys
def esc(s): return str(s).replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\r', '\\r').replace('\t', '\\t')
src, dst = sys.argv[1], sys.argv[2]
turns = []
for path in sorted(glob.glob(os.path.join(src, '*.jsonl'))):
    lines = open(path).read().splitlines()
    for line in lines[1:]:
        t = json.loads(line)
        if t.get('calls'):
            head = '[[turn]]\ntext = "%s"\n' % esc(t.get('assistant', ''))
            for c in t['calls']:
                inp = ', '.join('%s = "%s"' % (k, esc(v)) for k, v in (c.get('input') or {}).items())
                head += '[[turn.tool_calls]]\nname = "%s"\ninput = { %s }\n' % (esc(c['tool']), inp)
            entries = [head, '[[turn]]\ntext = "%s"\n' % esc(t.get('final', ''))]
        else:
            entries = ['[[turn]]\ntext = "%s"\n' % esc(t.get('final') or t.get('assistant', ''))]
        turns.append((t.get('user', ''), entries))
for i, (user, entries) in enumerate(turns):
    open(os.path.join(dst, 'p%d.txt' % i), 'w').write(user)
    open(os.path.join(dst, 's%d.toml' % i), 'w').write(''.join(entries))
open(os.path.join(dst, 'count'), 'w').write('%d %d\n' % (len(turns), sum(len(e) for _, e in turns)))
EOF
read -r NTURNS NCALLS < "$SCRATCH/replay/count"

PEAK=0
SID=""
i=0
while [ "$i" -lt "$NTURNS" ]; do
  PROMPT="$(cat "$SCRATCH/replay/p$i.txt")"
  if [ "$i" -eq 0 ]; then RESUME_ARGS=""; OUT="$SCRATCH/res.json"; else RESUME_ARGS="--resume $SID"; OUT="/dev/null"; fi
  # shellcheck disable=SC2086: RESUME_ARGS is empty or one known-good flag pair
  if [ "$(uname -s)" = "Darwin" ]; then
    (cd "$SCRATCH/ws" && COX_HOME="$SCRATCH/home" HOME="$SCRATCH/home" COX_PROVIDER=scripted \
      COX_SCENARIO="$SCRATCH/replay/s$i.toml" /usr/bin/time -l "$BIN" run -p "$PROMPT" $RESUME_ARGS \
      --output-format json --max-turns 10 --approve never --permission-mode auto --no-hooks --no-mcp \
      >"$OUT" 2>"$SCRATCH/t.err") || { echo "replay turn $i failed:" >&2; tail -5 "$SCRATCH/t.err" >&2; exit 1; }
    rss="$(awk '/maximum resident set size/ {print $1}' "$SCRATCH/t.err")"
  else
    (cd "$SCRATCH/ws" && COX_HOME="$SCRATCH/home" HOME="$SCRATCH/home" COX_PROVIDER=scripted \
      COX_SCENARIO="$SCRATCH/replay/s$i.toml" /usr/bin/time -v "$BIN" run -p "$PROMPT" $RESUME_ARGS \
      --output-format json --max-turns 10 --approve never --permission-mode auto --no-hooks --no-mcp \
      >"$OUT" 2>"$SCRATCH/t.err") || { echo "replay turn $i failed:" >&2; tail -5 "$SCRATCH/t.err" >&2; exit 1; }
    rss="$(awk -F': ' '/Maximum resident set size/{printf "%.0f", $2*1024}' "$SCRATCH/t.err")"
  fi
  [ -n "$rss" ] || { echo "could not parse RSS on turn $i" >&2; exit 1; }
  [ "$rss" -gt "$PEAK" ] && PEAK="$rss"
  [ "$i" -eq 0 ] && SID="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["session"])' "$OUT")"
  i=$((i + 1))
done
RSS_MIB="$(python3 -c "print(round($PEAK/1048576,1))")"

if [ "$(uname -s)" = "Darwin" ]; then BYTES="$(stat -f%z "$BIN")"; else BYTES="$(stat -c%s "$BIN")"; fi
BIN_MIB="$(python3 -c "print(round($BYTES/1048576,1))")"

echo "cold start (cox --version, median of 5): ${STARTUP_MS} ms"
echo "first frame (scripted stream-json, median of 3): ${FIRST_MS} ms"
echo "replay RSS peak ($NTURNS turns, $NCALLS provider calls, max): ${RSS_MIB} MiB"
echo "binary ($BIN): ${BIN_MIB} MiB ($BYTES bytes)"

RESULT="$(python3 -c 'import json,sys; print(json.dumps({"startup_ms":float(sys.argv[1]),"first_frame_ms":float(sys.argv[2]),"rss_mib":float(sys.argv[3]),"binary_bytes":int(sys.argv[4]),"turns":int(sys.argv[5]),"calls":int(sys.argv[6])}))' "$STARTUP_MS" "$FIRST_MS" "$RSS_MIB" "$BYTES" "$NTURNS" "$NCALLS")"

if [ "$MODE" = "write" ]; then
  python3 - "$BASELINE" "$OS" "$RESULT" <<'EOF'
import json, sys
path, oskey, result = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
try: all_ = json.load(open(path))
except OSError: all_ = {}
all_[oskey] = result
json.dump(all_, open(path, 'w'), indent=2)
open(path, 'a').write('\n')
print('wrote %s [%s]' % (path, oskey))
EOF
elif [ "$MODE" = "check" ]; then
  python3 - "$BASELINE" "$OS" "$RESULT" <<'EOF'
import json, sys
path, oskey, result = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
try: base = json.load(open(path))[oskey]
except (OSError, KeyError):
    print('no baseline for %s — run: bash scripts/footprint.sh --write' % oskey)
    sys.exit(0)
keys = ('startup_ms', 'first_frame_ms', 'rss_mib', 'binary_bytes')
for k in keys: print('  %s: baseline %s, now %s' % (k, base[k], result[k]))
bad = [k for k in keys if result[k] > base[k] * 1.2]
if bad: print('footprint: REGRESSION >20%%: %s' % ', '.join(bad)); sys.exit(1)
print('footprint: no metric regressed >20%')
EOF
fi
