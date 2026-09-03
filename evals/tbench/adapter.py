"""Terminal-Bench adapter (T12.1): `CoxAgent` drives headless `cox run`
inside the harness container and returns the trajectory.

Follows the harness `BaseAgent` contract (`name()` / `perform_task`) from
the `terminal-bench` package when importable (verified against
terminal-bench 0.2.18's `base_agent.py`); otherwise local shims with the
same shape so this file self-tests without the harness installed:

    python3 evals/tbench/adapter.py --self-test   # scripted dry run, offline
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

try:  # pragma: no cover - real harness path
    from terminal_bench.agents.base_agent import AgentResult, BaseAgent
    from terminal_bench.agents.failure_mode import FailureMode
    HAVE_TB = True
except ImportError:  # pragma: no cover - self-test path
    HAVE_TB = False

    class FailureMode:  # minimal mirror of the harness enum
        NONE = "none"
        AGENT_TIMEOUT = "agent_timeout"
        PARSE_ERROR = "parse_error"
        UNKNOWN_AGENT_ERROR = "unknown_agent_error"
        AGENT_INSTALLATION_FAILED = "agent_installation_failed"

    class AgentResult:
        def __init__(self, total_input_tokens=0, total_output_tokens=0,
                     failure_mode=FailureMode.NONE, timestamped_markers=None):
            self.total_input_tokens = total_input_tokens
            self.total_output_tokens = total_output_tokens
            self.failure_mode = failure_mode
            self.timestamped_markers = timestamped_markers or []

    class BaseAgent:
        def __init__(self, **kwargs):
            self._version = kwargs.get("version", None)


class CoxAgent(BaseAgent):
    """Runs `cox run -p <instruction> --output-format json` and returns the
    trajectory. `model_name` is `provider/model` (like the opencode agent);
    the provider part selects which `*_API_KEY` must be in env."""

    @staticmethod
    def name() -> str:
        return "cox"

    def __init__(self, model_name: str = "openai/gpt-4o-mini",
                 cox_bin: str = "cox", max_turns: int = 40, **kwargs):
        super().__init__(**kwargs)
        self._provider, _, self._model = model_name.partition("/")
        self._cox_bin = cox_bin
        self._max_turns = max_turns

    @property
    def _env_keys(self):
        return {"openai": ["OPENAI_API_KEY"], "anthropic": ["ANTHROPIC_API_KEY"]}.get(
            self._provider, [])

    def perform_task(self, instruction, session, logging_dir=None):
        del logging_dir
        cox = shutil.which(self._cox_bin) or self._cox_bin
        if not (Path(cox).exists() if "/" in cox else shutil.which(cox)):
            return AgentResult(failure_mode=FailureMode.AGENT_INSTALLATION_FAILED)
        missing = [k for k in self._env_keys if k not in os.environ]
        cmd = (
            f"{shlex.quote(cox)} run -p {shlex.quote(instruction)}"
            f" --output-format json --max-turns {self._max_turns}"
            f" --approve never --permission-mode auto"
            f" --provider {shlex.quote(self._provider)}"
            f" --tier code={shlex.quote(self._model)}"
        )
        started = time.time()
        session.send_keys([cmd, "Enter"], block=True, max_timeout_sec=float("inf"))
        elapsed = time.time() - started
        if missing:
            return AgentResult(failure_mode=FailureMode.UNKNOWN_AGENT_ERROR)
        pane = session.capture_pane()
        payload = _last_json_line(pane)
        if payload is None:
            return AgentResult(failure_mode=FailureMode.PARSE_ERROR)
        usage = payload.get("usage", {})
        failure = (FailureMode.NONE if payload.get("exit_code", 1) == 0
                   else FailureMode.UNKNOWN_AGENT_ERROR)
        return AgentResult(
            total_input_tokens=int(usage.get("input_tokens", 0)),
            total_output_tokens=int(usage.get("output_tokens", 0)),
            failure_mode=failure,
            timestamped_markers=[(elapsed, "cox run finished")],
        )


def _last_json_line(pane):
    for line in reversed(pane.splitlines()):
        line = line.strip()
        if line.startswith("{") and line.endswith("}"):
            try:
                return json.loads(line)
            except ValueError:
                continue
    return None


def self_test(cox_bin):
    """One in-repo task through a local session shim (no harness, no key)."""

    class LocalSession:
        def __init__(self, work):
            self.work = work
            self.pane = ""

        def send_keys(self, keys, block=True, max_timeout_sec=None):
            del block, max_timeout_sec
            proc = subprocess.run(
                ["sh", "-c", keys[0]], cwd=self.work,
                capture_output=True, text=True, timeout=120,
                env=_scripted_env(),
            )
            self.pane = proc.stdout

        def capture_pane(self):
            return self.pane

    def _scripted_env():
        home = tempfile.mkdtemp()
        scenario = Path(home) / "scenario.toml"
        scenario.write_text('[[turn]]\ntext = "done"\n')
        return dict(os.environ, COX_HOME=home, COX_PROVIDER="scripted",
                    COX_SCENARIO=str(scenario))

    work = tempfile.mkdtemp()
    agent = CoxAgent(model_name="openai/gpt-4o-mini", cox_bin=cox_bin, max_turns=5)
    result = agent.perform_task("Reply with exactly: done", LocalSession(work))
    assert result.failure_mode == FailureMode.NONE, result
    print(f"self-test ok ({'harness' if HAVE_TB else 'shim'} base)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--cox-bin", default="cox")
    args = parser.parse_args()
    if args.self_test:
        self_test(args.cox_bin)
    else:
        parser.print_help()
