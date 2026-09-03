#!/usr/bin/env python3
"""Opt-in eval harness (T12.1): one fresh tempdir per task, `setup`, a
headless `cox run`, then `check`. Cost comes from the JSON output.

    just eval --dry-run            # scripted provider, no network, no key
    just eval                      # real provider (needs a key in env)
    just eval --only create-file   # one task
    just eval --provider openai --model gpt-4o-mini

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

import yaml

EVALS = Path(__file__).resolve().parent
TASKS = EVALS / "tasks"


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


def toml_escape(text):
    return (
        text.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )


def toml_value(value):
    if isinstance(value, str):
        return f'"{toml_escape(value)}"'
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(v) for v in value) + "]"
    if isinstance(value, dict):
        inner = ", ".join(f"{k} = {toml_value(v)}" for k, v in value.items())
        return "{ " + inner + " }"
    raise TypeError(f"unsupported scenario value: {value!r}")


def scenario_toml(turns):
    """Embedded dry-run turns to a Scripted provider scenario."""
    out = []
    for turn in turns or []:
        calls = turn.get("calls") or []
        if not calls:
            out.append(f'[[turn]]\ntext = "{toml_escape(turn.get("text", ""))}"\n')
            continue
        out.append(f'[[turn]]\ntext = "{toml_escape(turn.get("text", ""))}"\n')
        for call in calls:
            out.append(f'[[turn.tool_calls]]\nname = "{toml_escape(call["tool"])}"\n')
            out.append(f"input = {toml_value(call.get('input', {}))}\n")
        out.append(f'[[turn]]\ntext = "{toml_escape(turn.get("final", ""))}"\n')
    return "".join(out)


def run_task(task, *, cox_bin, dry_run, provider, model):
    work = Path(tempfile.mkdtemp(prefix="cox-eval-"))
    home = Path(tempfile.mkdtemp(prefix="cox-eval-home-"))
    scenario_file = None
    env = dict(os.environ, COX_HOME=str(home), HOME=str(home))
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
        # Hermetic evals: ambient hooks and MCP servers would add seconds
        # of startup noise (and nondeterminism) to every task.
        "--no-hooks", "--no-mcp",
    ]
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
    cost, turns = 0.0, 0
    try:
        payload = json.loads(proc.stdout or "{}")
        cost = float(payload.get("cost_usd", 0.0))
        turns = int(payload.get("turns", 0))
    except (ValueError, TypeError):
        pass
    if proc.returncode != 0:
        return result(task, False, cost, turns, time.time() - started,
                       f"cox exit {proc.returncode}")
    check_env = dict(os.environ, COX_OUT=str(out_file))
    check = subprocess.run(["sh", "-c", task.get("check") or "true"],
                           cwd=work, env=check_env, capture_output=True, text=True)
    ok = check.returncode == 0
    return result(task, ok, cost, turns, time.time() - started,
                  "" if ok else f"check failed: {check.stderr.strip() or check.stdout.strip()}")


def result(task, ok, cost, turns, seconds, note):
    return {"name": task["name"], "pass": ok, "cost_usd": cost,
            "turns": turns, "seconds": round(seconds, 1), "note": note}


def main(argv=None):
    parser = argparse.ArgumentParser(description="cox eval harness")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--only", nargs="*", default=[])
    parser.add_argument("--cox-bin", default=None)
    parser.add_argument("--provider", default=None)
    parser.add_argument("--model", default=None)
    args = parser.parse_args(argv)
    cox_bin = find_cox_bin(args.cox_bin)
    tasks = load_tasks(args.only)
    if not tasks:
        raise SystemExit("no tasks selected")
    print(f"cox: {cox_bin}  tasks: {len(tasks)}"
          f"  mode: {'dry-run (scripted)' if args.dry_run else 'live'}")
    results = []
    for path, task in tasks:
        res = run_task(task, cox_bin=cox_bin, dry_run=args.dry_run,
                       provider=args.provider, model=args.model)
        results.append(res)
        flag = "PASS" if res["pass"] else "FAIL"
        print(f'{res["name"]:20} {flag:4}  ${res["cost_usd"]:.4f}'
              f'  turns={res["turns"]}  {res["seconds"]}s  {res["note"]}')
    passed = sum(1 for r in results if r["pass"])
    total_cost = sum(r["cost_usd"] for r in results)
    print(f"{passed}/{len(results)} passed  total cost ${total_cost:.4f}")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
