"""Opt-in eval harness (T12.1): one fresh tempdir per task, `setup`, a
headless `cox run`, then `check`. Cost comes from the JSON output.

    just eval --dry-run            # scripted provider, no network, no key
    just eval                      # real provider (needs a key in env)
    just eval --only create-file   # one task
    just eval --provider openai --model gpt-4o-mini
    just eval --preset verify      # T30.3: verify-before-done + test hook

`--preset verify` (T30.3) adds a harness system addendum telling the model
to run the task's tests and show the output before reporting done, and
wires `evals/hooks/verify.sh` as a `PostToolUse` hook on `edit`/
`apply_patch`/`write` (hooks stay enabled instead of the default
`--no-hooks`). The addendum goes into the per-task `COX_HOME/AGENTS.md`
and the hook into `COX_HOME/config.toml` — both already fresh per task.

Exit code is 0 only when every selected task passes.
"""

import argparse
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import tomli_w
import yaml

# The uv project root (`evals/`); tasks and hooks live beside the package,
# which uv installs editable, so `__file__` stays inside the checkout.
EVALS = Path(__file__).resolve().parents[2]
TASKS = EVALS / "tasks"
HOOKS = EVALS / "hooks"

# T30.3: harness system addendum for `--preset verify`, loaded through the
# AGENTS.md/CLAUDE.md hierarchy (cox_ext::instructions candidates() puts
# `<COX_HOME>/AGENTS.md` first, ahead of every project file).
VERIFY_ADDENDUM = (
    "Before reporting done, run the task's tests and show the output.\n"
)


def load_tasks(names):
    paths = sorted(TASKS.glob("*.yaml"))
    if names:
        wanted = set(names)
        paths = [p for p in paths if p.stem in wanted or p.stem.split("-", 1)[-1] in wanted]
    return [(p, yaml.safe_load(p.read_text())) for p in paths]


def find_cox_bin(explicit):
    if explicit:
        return explicit
    if os.environ.get("COX_BIN"):
        return os.environ["COX_BIN"]
    found = shutil.which("cox")
    if found:
        return found
    try:
        meta = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            capture_output=True, text=True, check=True, cwd=EVALS.parent,
        )
        candidate = (
            Path(json.loads(meta.stdout)["target_directory"]) / "debug" / "cox"
        )
        if candidate.exists():
            return str(candidate)
    except Exception:
        pass
    raise SystemExit("no cox binary: build first (`cargo build -p cox`) or pass --cox-bin")


def scenario_toml(turns):
    """Embedded dry-run turns to a Scripted provider scenario."""
    out = []
    for turn in turns or []:
        calls = turn.get("calls") or []
        head = {"text": turn.get("text", "")}
        if calls:
            head["tool_calls"] = [
                {"name": call["tool"], "input": call.get("input", {})} for call in calls
            ]
        out.append(head)
        if calls:
            out.append({"text": turn.get("final", "")})
    return tomli_w.dumps({"turn": out})


def write_verify_preset(home):
    """T30.3: drop the addendum + hook config into a fresh COX_HOME."""
    (home / "AGENTS.md").write_text(VERIFY_ADDENDUM)
    hook = {
        "matcher": "edit|apply_patch|write",
        "command": str(HOOKS / "verify.sh"),
        # Above the script's own 120s cap, so the cap (not this timeout)
        # is what fires on a hanging test command.
        "timeout_s": 130,
    }
    (home / "config.toml").write_text(tomli_w.dumps({"hooks": {"PostToolUse": [hook]}}))


def run_task(task, *, cox_bin, dry_run, provider, model, preset=None):
    work = Path(tempfile.mkdtemp(prefix="cox-eval-"))
    home = Path(tempfile.mkdtemp(prefix="cox-eval-home-"))
    scenario_file = None
    env = dict(os.environ, COX_HOME=str(home), HOME=str(home))
    if preset == "verify":
        write_verify_preset(home)
    if dry_run:
        scenario_file = work / "scenario.toml"
        scenario_file.write_text(scenario_toml(task.get("dry_run", {}).get("turns")))
        env["COX_PROVIDER"] = "scripted"
        env["COX_SCENARIO"] = str(scenario_file)
    started = time.time()
    setup = subprocess.run(
        ["sh", "-c", task.get("setup") or "true"],
        cwd=work, capture_output=True, text=True,
    )
    if setup.returncode != 0:
        return result(task, False, 0.0, 0, time.time() - started, "setup failed")
    cmd = [
        cox_bin, "run", "-p", task["prompt"],
        "--output-format", "json", "--max-turns", "40",
        "--approve", "never", "--permission-mode", "auto",
        # Hermetic evals: ambient MCP servers would add seconds of startup
        # noise (and nondeterminism) to every task. Hooks stay off too,
        # except under `--preset verify`, which wires its own PostToolUse
        # hook through this task's fresh COX_HOME/config.toml.
        "--no-mcp",
    ]
    if preset != "verify":
        cmd.append("--no-hooks")
    if provider:
        cmd += ["--provider", provider]
    if model:
        cmd += ["--tier", f"code={model}"]
    out_file = work / "cox-out.json"
    try:
        proc = subprocess.run(
            cmd, cwd=work, env=env, capture_output=True, text=True,
            timeout=task.get("timeout_s", 300),
        )
    except subprocess.TimeoutExpired:
        return result(task, False, 0.0, 0, time.time() - started, "timeout")
    out_file.write_text(proc.stdout)
    cost, turns, tokens = 0.0, 0, {}
    try:
        payload = json.loads(proc.stdout or "{}")
        cost = float(payload.get("cost_usd", 0.0))
        turns = int(payload.get("turns", 0))
        tokens = {k: int(v) for k, v in (payload.get("usage") or {}).items()}
    except (ValueError, TypeError, AttributeError):
        pass
    if proc.returncode != 0:
        return result(task, False, cost, turns, time.time() - started,
                       f"cox exit {proc.returncode}", tokens)
    check_env = dict(os.environ, COX_OUT=str(out_file))
    check = subprocess.run(["sh", "-c", task.get("check") or "true"],
                           cwd=work, env=check_env, capture_output=True, text=True)
    ok = check.returncode == 0
    return result(task, ok, cost, turns, time.time() - started,
                  "" if ok else f"check failed: {check.stderr.strip() or check.stdout.strip()}",
                  tokens)


def result(task, ok, cost, turns, seconds, note, tokens=None):
    return {"name": task["name"], "pass": ok, "cost_usd": cost,
            "turns": turns, "seconds": round(seconds, 1), "note": note,
            "tokens": tokens or {}}


TOKEN_KEYS = ("input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens")


def token_line(tokens):
    """`in/out/cache-read/cache-write`, the four counts the ledger keeps."""
    return "/".join(str(tokens.get(k, 0)) for k in TOKEN_KEYS)


def main(argv=None):
    parser = argparse.ArgumentParser(description="cox eval harness")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--only", nargs="*", default=[])
    parser.add_argument("--cox-bin", default=None)
    parser.add_argument("--provider", default=None)
    parser.add_argument("--model", default=None)
    parser.add_argument(
        "--preset", choices=["verify"], default=None,
        help="verify: add the run-tests-before-done addendum and the "
             "PostToolUse test-runner hook (T30.3)",
    )
    args = parser.parse_args(argv)
    cox_bin = find_cox_bin(args.cox_bin)
    tasks = load_tasks(args.only)
    if not tasks:
        raise SystemExit("no tasks selected")
    print(f"cox: {cox_bin}  tasks: {len(tasks)}"
          f"  mode: {'dry-run (scripted)' if args.dry_run else 'live'}"
          f"{f'  preset: {args.preset}' if args.preset else ''}")
    results = []
    for path, task in tasks:
        res = run_task(task, cox_bin=cox_bin, dry_run=args.dry_run,
                       provider=args.provider, model=args.model,
                       preset=args.preset)
        results.append(res)
        flag = "PASS" if res["pass"] else "FAIL"
        print(f'{res["name"]:20} {flag:4}  ${res["cost_usd"]:.4f}'
              f'  turns={res["turns"]}  tok={token_line(res["tokens"])}'
              f'  {res["seconds"]}s  {res["note"]}')
    passed = sum(1 for r in results if r["pass"])
    total_cost = sum(r["cost_usd"] for r in results)
    total_tokens = {k: sum(r["tokens"].get(k, 0) for r in results) for k in TOKEN_KEYS}
    print(f"{passed}/{len(results)} passed  total cost ${total_cost:.4f}"
          f"  tokens in/out/cache-read/cache-write {token_line(total_tokens)}")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
