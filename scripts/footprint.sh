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
# timings are per-target, plus `<OS-arch>-ci` for CI runners (T30.4); --check
# with no entry for this machine warns and exits 0 so a new platform
# bootstraps instead of failing.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASELINE="$ROOT/scripts/footprint.json"
MODE="measure"
if [ $# -gt 1 ] || { [ $# -eq 1 ] && [ "$1" != "--write" ] && [ "$1" != "--check" ]; }; then
  echo "usage: footprint.sh [--write|--check]" >&2; exit 2
fi
[ $# -eq 1 ] && MODE="${1#--}"

# A CI runner is a different machine from any laptop the baseline was
# written on, so CI (GitHub sets CI=true) compares against its own `-ci` key.
OS="$(uname -s)-$(uname -m)${CI:+-ci}"
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
# Timings report the best (minimum) of N runs, which is what --check gates on:
# scheduler and I/O noise on a shared runner only ever adds time, so the
# minimum is stable where the median of a few runs swings 2-3x between runs
# of the same commit (T30.4 follow-up). The median is printed for context.
min_ms() { python3 -c 'import sys; print(round(min(int(x) for x in open(sys.argv[1]))/1e6,1))' "$1"; }
median_ms() { python3 -c 'import statistics,sys; print(round(statistics.median(int(x) for x in open(sys.argv[1]))/1e6,1))' "$1"; }

# Calibration: best of 11 `/usr/bin/true` spawns under the same env. Shared
# CI runners differ from each other (best-of-11 cold start seen from 7.0 to
# 12.0 ms on identical commits), so --check scales timing baselines that carry
# a `calib_ms` by how much slower this machine spawns a process than the one
# that wrote the baseline. It only ever loosens, never tightens, the gate.
: > "$SCRATCH/calib"
for _ in $(seq 1 11); do
  s="$(now_ns)"
  COX_HOME="$SCRATCH/home" HOME="$SCRATCH/home" /usr/bin/true
  echo "$(( $(now_ns) - s ))" >> "$SCRATCH/calib"
done
CALIB_MS="$(min_ms "$SCRATCH/calib")"

# Cold start: best of 11 `cox --version` runs (spawn + clap + config load).
: > "$SCRATCH/startup"
for _ in $(seq 1 11); do
  s="$(now_ns)"
  COX_HOME="$SCRATCH/home" HOME="$SCRATCH/home" "$BIN" --version >/dev/null
  echo "$(( $(now_ns) - s ))" >> "$SCRATCH/startup"
done
STARTUP_MS="$(min_ms "$SCRATCH/startup")"
STARTUP_MED="$(median_ms "$SCRATCH/startup")"

# First frame: spawn to first stream-json event with the scripted provider.
printf '[[turn]]\ntext = "hello"\n' > "$SCRATCH/first.toml"
: > "$SCRATCH/first"
for _ in $(seq 1 7); do
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
FIRST_MS="$(min_ms "$SCRATCH/first")"
FIRST_MED="$(median_ms "$SCRATCH/first")"

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

echo "process spawn (/usr/bin/true, best of 11): ${CALIB_MS} ms"
echo "cold start (cox --version, best of 11): ${STARTUP_MS} ms (median ${STARTUP_MED} ms)"
echo "first frame (scripted stream-json, best of 7): ${FIRST_MS} ms (median ${FIRST_MED} ms)"
echo "replay RSS peak ($NTURNS turns, $NCALLS provider calls, max): ${RSS_MIB} MiB"
echo "binary ($BIN): ${BIN_MIB} MiB ($BYTES bytes)"

RESULT="$(python3 -c 'import json,sys; print(json.dumps({"startup_ms":float(sys.argv[1]),"first_frame_ms":float(sys.argv[2]),"rss_mib":float(sys.argv[3]),"binary_bytes":int(sys.argv[4]),"turns":int(sys.argv[5]),"calls":int(sys.argv[6]),"calib_ms":float(sys.argv[7])}))' "$STARTUP_MS" "$FIRST_MS" "$RSS_MIB" "$BYTES" "$NTURNS" "$NCALLS" "$CALIB_MS")"

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
timing = ('startup_ms', 'first_frame_ms')
scale = 1.0
if base.get('calib_ms'):
    scale = max(1.0, result['calib_ms'] / base['calib_ms'])
    print('  calib_ms: baseline %s, now %s (timing baselines x%.2f)' % (base['calib_ms'], result['calib_ms'], scale))
limit = {k: base[k] * (scale if k in timing else 1.0) for k in keys}
for k in keys: print('  %s: baseline %s, now %s' % (k, round(limit[k], 1), result[k]))
bad = [k for k in keys if result[k] > limit[k] * 1.2]
if bad: print('footprint: REGRESSION >20%%: %s' % ', '.join(bad)); sys.exit(1)
print('footprint: no metric regressed >20%')
EOF
fi
