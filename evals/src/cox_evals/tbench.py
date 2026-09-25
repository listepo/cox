"""Terminal-Bench 2.0 agent for Harbor (T30.9): uploads a Linux `cox` build
into the task container and runs one headless `cox run` there.

TB 2.0 runs through Harbor (`harbor run -d terminal-bench@2.0`); an agent is
a `BaseInstalledAgent` whose `install` puts the binary in the container and
whose `run` executes it and fills `AgentContext` with tokens and cost. cox
is one self-contained binary, so installing is an upload plus `chmod`.

    uv run --project evals --extra tbench harbor run -d terminal-bench@2.0 \\
        -a cox_evals.tbench:CoxAgent -m anthropic/claude-sonnet-5 \\
        --force-build -i fix-git \\
        --ak cox_bin=<linux cox> --ak budget_usd=0.2

The provider key is read from the host env when `run` starts and passed only
to that one command, never written into the container.
"""

import json
import os
import shlex
from pathlib import Path

from harbor.agents.installed.base import BaseInstalledAgent
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext

# `/installed-agent` is the directory Harbor's own `setup` creates for
# installed agents, so the binary lands where Harbor expects agent files.
REMOTE_BIN = "/installed-agent/cox"
# A fresh COX_HOME per container: no host config, no ledger from other runs.
REMOTE_HOME = "/tmp/cox-home"
KEY_ENV = {"anthropic": "ANTHROPIC_API_KEY", "openai": "OPENAI_API_KEY"}


def command(instruction, model, *, budget_usd, max_turns):
    """The one shell command `run` executes in the task container."""
    provider, _, name = model.partition("/")
    return (
        f"{REMOTE_BIN} run -p {shlex.quote(instruction)}"
        f" --output-format json --max-turns {int(max_turns)} --budget {float(budget_usd)}"
        " --approve never --permission-mode auto --no-mcp --no-hooks"
        # The task container is the isolation boundary; cox's own sandbox
        # needs bwrap or Landlock, which TB images do not promise.
        " --sandbox danger-full-access"
        f" --provider {shlex.quote(provider)} --tier code={shlex.quote(name)}"
        # cox exits non-zero on a denied call or a spent budget, and Harbor
        # raises on any non-zero exit — the usage JSON would be lost with it.
        " || true"
    )


def last_json_line(text):
    """The `cox run` payload: the last line of stdout that parses as an object."""
    for line in reversed((text or "").splitlines()):
        line = line.strip()
        if line.startswith("{") and line.endswith("}"):
            try:
                return json.loads(line)
            except ValueError:
                continue
    return None


def fill_context(context, payload):
    """Copy tokens and cost from the payload into Harbor's `AgentContext`.

    `n_input_tokens` is every prompt token (fresh + cache read + cache
    write), matching how Harbor's own agents sum prompt tokens;
    `n_cache_tokens` is the cache-read share of it.
    """
    usage = payload.get("usage") or {}
    fresh = int(usage.get("input_tokens", 0))
    read = int(usage.get("cache_read_tokens", 0))
    write = int(usage.get("cache_write_tokens", 0))
    context.n_input_tokens = fresh + read + write
    context.n_cache_tokens = read
    context.n_output_tokens = int(usage.get("output_tokens", 0))
    context.cost_usd = float(payload.get("cost_usd", 0.0))
    context.metadata = {
        **(context.metadata or {}),
        "cox_exit_code": payload.get("exit_code"),
        "cox_turns": payload.get("turns"),
        "cox_usage": usage,
    }


class CoxAgent(BaseInstalledAgent):
    """`cox run` inside the task container; one call per trial."""

    @staticmethod
    def name() -> str:
        return "cox"

    def __init__(self, *args, cox_bin=None, budget_usd=0.2, max_turns=40, **kwargs):
        super().__init__(*args, **kwargs)
        self._cox_bin = cox_bin or os.environ.get("COX_LINUX_BIN")
        self._budget_usd = float(budget_usd)
        self._max_turns = int(max_turns)

    def get_version_command(self) -> str | None:
        return f"{REMOTE_BIN} --version"

    async def install(self, environment: BaseEnvironment) -> None:
        if not self._cox_bin or not Path(self._cox_bin).is_file():
            raise RuntimeError(
                "no Linux cox binary: pass --ak cox_bin=<path> or set COX_LINUX_BIN"
            )
        await environment.upload_file(self._cox_bin, REMOTE_BIN)
        await self.exec_as_root(environment, command=f"chmod 755 {REMOTE_BIN}")

    async def run(self, instruction: str, environment: BaseEnvironment,
                  context: AgentContext) -> None:
        model = self.model_name or "anthropic/claude-sonnet-5"
        key_name = KEY_ENV.get(model.partition("/")[0])
        env = {"COX_HOME": REMOTE_HOME}
        if key_name:
            key = os.environ.get(key_name)
            if not key:
                raise RuntimeError(f"{key_name} is not set on the host")
            env[key_name] = key
        await self.exec_as_agent(environment, command=f"mkdir -p {REMOTE_HOME}")
        result = await self.exec_as_agent(
            environment,
            command=command(self.render_instruction(instruction), model,
                            budget_usd=self._budget_usd, max_turns=self._max_turns),
            env=env,
        )
        (self.logs_dir / "cox-stdout.txt").write_text(result.stdout or "")
        (self.logs_dir / "cox-stderr.txt").write_text(result.stderr or "")
        payload = last_json_line(result.stdout)
        if payload is None:
            raise RuntimeError("cox printed no JSON payload; see cox-stderr.txt")
        fill_context(context, payload)
