"""Agents × providers × models on Terminal-Bench 2.0 through Harbor (T30.14).

One `harbor run` job per agent × model over the same task list, then one
per-task table from the jobs' `result.json`. Agents, providers and presets
are registries in code, so a new comparison is a new entry, not a script:

    uv run --project evals --extra tbench cox-bench --preset same-model --dry-run
    uv run --project evals --extra tbench cox-bench --preset same-model --prepare
    uv run --project evals --extra tbench cox-bench --agents cox terminus-2 \\
        --provider anthropic --model claude-sonnet-5 --tasks fix-git

Secrets never reach argv or Harbor's job config: keys stay in the process
env, which Harbor's agents read on the host. A local server gets a dummy key.
"""

import argparse
import json
import os
import shlex
import shutil
import subprocess
import sys
import urllib.request
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

CHAT, MESSAGES = "openai-chat", "anthropic-messages"
JOBS_DIR = Path.home() / ".cache" / "cox-evals" / "tb-jobs"
# colima's name for the macOS host inside its VM; Harbor adds no host alias.
CONTAINER_HOST = "host.lima.internal"


@dataclass(frozen=True)
class Provider:
    name: str
    url: str  # as seen from the host
    shapes: dict  # API shape -> path under `url`
    key_env: dict = field(default_factory=dict)  # API shape -> key env var
    local: bool = False  # a server on this machine: dummy key, host rewrite
    prepare: tuple = ()  # commands that start the server and load the model

    def base(self, shape, *, in_container):
        url = self.url + self.shapes[shape]
        if self.local and in_container:
            url = url.replace("localhost", CONTAINER_HOST).replace("127.0.0.1", CONTAINER_HOST)
        return url


PROVIDERS = {p.name: p for p in [
    # https://lmstudio.ai/docs/developer (OpenAI and Anthropic compatibility, checked 2026-09-25)
    Provider("lmstudio", "http://localhost:1234", {CHAT: "/v1", MESSAGES: ""},
             {CHAT: "OPENAI_API_KEY", MESSAGES: "ANTHROPIC_API_KEY"}, local=True,
             prepare=("lms server start", "lms load {model} --context-length {context} -y")),
    Provider("ollama", "http://localhost:11434", {CHAT: "/v1"}, {CHAT: "OPENAI_API_KEY"},
             local=True, prepare=("ollama pull {model}",)),
    Provider("anthropic", "https://api.anthropic.com", {MESSAGES: ""},
             {MESSAGES: "ANTHROPIC_API_KEY"}),
    Provider("openai", "https://api.openai.com", {CHAT: "/v1"}, {CHAT: "OPENAI_API_KEY"}),
]}


@dataclass(frozen=True)
class Run:
    """Everything one `harbor run` needs besides the shared task flags."""
    agent: str
    argv: list
    env: dict


def cox_run(provider, shape, model, opts):
    cox_provider = "anthropic" if shape == MESSAGES else ("local" if provider.local else "openai")
    argv = ["-a", "cox_evals.tbench:CoxAgent", "-m", f"{cox_provider}/{model}",
            "--ak", f"budget_usd={opts['budget_usd']}", "--ak", f"max_turns={opts['max_turns']}"]
    if opts.get("cox_bin"):
        argv += ["--ak", f"cox_bin={opts['cox_bin']}"]
    if provider.local:
        argv += ["--ak", f"base_url={provider.base(shape, in_container=True)}",
                 "--ak", f"context_window={opts['context']}"]
    return argv, {}


def claude_code_run(provider, shape, model, opts):
    # Claude Code runs in the container and reads its endpoint from the host
    # env; with ANTHROPIC_BASE_URL set Harbor passes `-m` through verbatim.
    env = {"ANTHROPIC_BASE_URL": provider.base(shape, in_container=True)} if provider.local else {}
    return ["-a", "claude-code", "-m", model], env


def terminus_run(provider, shape, model, opts):
    # Terminus 2 calls the model from the host through LiteLLM, so it keeps
    # the host URL; `model_info` stops LiteLLM guessing a 1M context.
    prefix = "anthropic" if shape == MESSAGES else "openai"
    argv = ["-a", "terminus-2", "-m", f"{prefix}/{model}"]
    if provider.local:
        argv += ["--ak", f"api_base={provider.base(shape, in_container=False)}"]
    info = {"max_input_tokens": opts["context"], "max_output_tokens": opts["max_output"]}
    return argv + ["--ak", f"model_info={json.dumps(info)}"], {}


# Agent -> (API shapes it can use, most preferred first; argv/env builder).
AGENTS = {
    # cox prefers Messages: its OpenAI Chat path drops tool calls (ideas.md).
    "cox": ((MESSAGES, CHAT), cox_run),
    "claude-code": ((MESSAGES,), claude_code_run),
    "terminus-2": ((CHAT, MESSAGES), terminus_run),
}

PRESETS = {
    # The cox vs Claude Code vs Terminus 2 comparison (ideas.md): same
    # local model for all three agents; tasks are a seeded sample (the full
    # card: plan.md at commit 855fe68) of TB 2.0 `medium` and `hard` tasks.
    "same-model": {
        "agents": ["cox", "claude-code", "terminus-2"],
        "provider": "lmstudio", "model": "prism-ml/bonsai-27b", "context": 65536,
        "tasks": ["build-cython-ext", "build-pmars", "compile-compcert", "mteb-leaderboard",
                  "query-optimize", "regex-log", "sanitize-git-repo", "tune-mjcf",
                  "dna-assembly", "password-recovery", "path-tracing-reverse", "regex-chess"],
    },
}


def plan_run(agent, provider, model, opts):
    """Pick the API shape both sides speak and build the agent's argv/env."""
    shapes, build = AGENTS[agent]
    shape = next((s for s in shapes if s in provider.shapes), None)
    if shape is None:
        raise SystemExit(f"{agent} needs {' or '.join(shapes)}; {provider.name} serves "
                         f"{', '.join(provider.shapes)}")
    argv, env = build(provider, shape, model, opts)
    key_env = provider.key_env[shape]
    if provider.local:
        env[key_env] = "local"
    elif not os.environ.get(key_env):
        raise SystemExit(f"{key_env} is not set for {provider.name}")
    return Run(agent, argv, env)


def harbor_argv(run, tasks, *, job_name, jobs_dir, concurrency):
    argv = ["harbor", "run", "-d", "terminal-bench@2.0", *run.argv, "--force-build", "-y",
            "-n", str(concurrency), "-o", str(jobs_dir), "--job-name", job_name]
    for task in tasks:
        argv += ["-i", task]
    return argv


def preflight(provider, model):
    """A local server must be up and serve `model`; returns an error or None."""
    if not provider.local:
        return None
    path = provider.shapes.get(CHAT, "/v1")
    try:
        with urllib.request.urlopen(f"{provider.url}{path}/models", timeout=5) as resp:
            ids = [m.get("id") for m in json.load(resp).get("data", [])]
    except OSError as err:
        return f"{provider.name} is not reachable at {provider.url}: {err}"
    return None if model in ids else f"{provider.name} does not serve {model}; it has {ids}"


def summarize(job_dir):
    """One row per trial from a Harbor job's `result.json` files."""
    rows = []
    for path in sorted(Path(job_dir).glob("*/result.json")):
        trial = json.loads(path.read_text())
        ctx = trial.get("agent_result") or {}
        rewards = (trial.get("verifier_result") or {}).get("rewards") or {}
        timing = trial.get("agent_execution") or {}
        seconds = None
        if timing.get("started_at") and timing.get("finished_at"):
            seconds = round((datetime.fromisoformat(timing["finished_at"])
                             - datetime.fromisoformat(timing["started_at"])).total_seconds())
        rows.append({
            "task": trial.get("task_name") or path.parent.name.split("__")[0],
            "reward": rewards.get("reward"),
            "tokens_in": ctx.get("n_input_tokens") or 0,
            "tokens_out": ctx.get("n_output_tokens") or 0,
            "cost_usd": ctx.get("cost_usd") or 0.0,
            "seconds": seconds,
            "error": (trial.get("exception_info") or {}).get("exception_type"),
        })
    return rows


def table(results):
    """Markdown: one row per task, one column per agent."""
    agents = list(results)
    tasks = sorted({r["task"] for rows in results.values() for r in rows})
    cell = {(a, r["task"]): r for a, rows in results.items() for r in rows}
    out = ["| task | " + " | ".join(agents) + " |", "|---" * (len(agents) + 1) + "|"]
    for task in tasks:
        cells = []
        for agent in agents:
            r = cell.get((agent, task))
            if r is None:
                cells.append("—")
            elif r["error"]:
                cells.append(f"error: {r['error']}")
            else:
                cells.append(f"{r['reward']} · {r['tokens_in'] + r['tokens_out']} tok · {r['seconds']}s")
        out.append(f"| {task} | " + " | ".join(cells) + " |")
    totals = []
    for agent in agents:
        rows = results[agent]
        passed = sum(1 for r in rows if (r["reward"] or 0) >= 1)
        totals.append(f"{passed}/{len(rows)} · ${sum(r['cost_usd'] for r in rows):.4f}")
    out.append("| **total** | " + " | ".join(totals) + " |")
    return "\n".join(out)


def main(argv=None):
    parser = argparse.ArgumentParser(description="agents × providers × models on Terminal-Bench 2.0")
    parser.add_argument("--preset", choices=sorted(PRESETS))
    parser.add_argument("--agents", nargs="+", choices=sorted(AGENTS))
    parser.add_argument("--provider", choices=sorted(PROVIDERS))
    parser.add_argument("--model")
    parser.add_argument("--tasks", nargs="+")
    parser.add_argument("--context", type=int, help="context window the model is loaded with")
    parser.add_argument("--max-output", type=int, default=8192)
    parser.add_argument("--cox-bin", default=os.environ.get("COX_LINUX_BIN"))
    parser.add_argument("--budget-usd", type=float, default=1.0)
    parser.add_argument("--max-turns", type=int, default=40)
    parser.add_argument("-n", "--concurrency", type=int, default=1)
    parser.add_argument("--jobs-dir", type=Path, default=JOBS_DIR)
    parser.add_argument("--prepare", action="store_true", help="start the local server and load the model first")
    parser.add_argument("--dry-run", action="store_true", help="print the harbor commands, run nothing")
    args = parser.parse_args(argv)
    preset = PRESETS.get(args.preset, {})
    agents = args.agents or preset.get("agents")
    provider = PROVIDERS.get(args.provider or preset.get("provider"))
    model = args.model or preset.get("model")
    tasks = args.tasks or preset.get("tasks")
    if not (agents and provider and model and tasks):
        parser.error("give --preset or all of --agents, --provider, --model, --tasks")
    opts = {"context": args.context or preset.get("context", 32768), "max_output": args.max_output,
            "cox_bin": args.cox_bin, "budget_usd": args.budget_usd, "max_turns": args.max_turns}
    runs = [plan_run(agent, provider, model, opts) for agent in agents]
    stamp = datetime.now().strftime("%Y-%m-%d__%H-%M-%S")
    jobs = [(run, f"{stamp}__{run.agent}") for run in runs]
    if args.dry_run:
        for run, job in jobs:
            env = " ".join(f"{k}={v}" for k, v in run.env.items())
            cmd = shlex.join(harbor_argv(run, tasks, job_name=job, jobs_dir=args.jobs_dir,
                                         concurrency=args.concurrency))
            print(f"{env} {cmd}".strip())
        return 0
    if args.prepare:
        for step in provider.prepare:
            subprocess.run(step.format(model=model, context=opts["context"]), shell=True, check=True)
    error = preflight(provider, model)
    if error:
        raise SystemExit(error)
    if not shutil.which("harbor"):
        raise SystemExit("no `harbor` on PATH: run through `uv run --project evals --extra tbench`")
    results = {}
    for run, job in jobs:
        print(f"== {run.agent} · {provider.name}/{model} · {len(tasks)} tasks", file=sys.stderr)
        subprocess.run(harbor_argv(run, tasks, job_name=job, jobs_dir=args.jobs_dir,
                                   concurrency=args.concurrency),
                       env={**os.environ, **run.env}, check=False)
        results[run.agent] = summarize(args.jobs_dir / job)
    print(table(results))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
