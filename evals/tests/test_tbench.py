"""`cox_evals.tbench`: the command `CoxAgent` runs in a Terminal-Bench
container, how it installs the binary, and how the `cox run` payload
becomes Harbor's `AgentContext`."""

import asyncio
import json
import tomllib

import pytest

pytest.importorskip("harbor")

from harbor.environments.base import ExecResult  # noqa: E402
from harbor.models.agent.context import AgentContext  # noqa: E402

from cox_evals import tbench  # noqa: E402

PAYLOAD = {
    "usage": {"input_tokens": 10, "output_tokens": 80,
              "cache_read_tokens": 1000, "cache_write_tokens": 200},
    "cost_usd": 0.0123, "turns": 3, "exit_code": 0,
}


class FakeEnv:
    """A task container that runs nothing: records every exec and upload."""

    default_user = None

    def __init__(self, stdout=""):
        self.stdout = stdout
        self.execs = []
        self.uploads = []

    async def exec(self, command, cwd=None, env=None, timeout_sec=None, user=None):
        self.execs.append({"command": command, "env": env or {}, "user": user})
        out = self.stdout if tbench.REMOTE_BIN + " run" in command else ""
        return ExecResult(stdout=out, stderr="", return_code=0)

    async def upload_file(self, source_path, target_path):
        self.uploads.append((str(source_path), target_path))


def agent(tmp_path, **kwargs):
    return tbench.CoxAgent(logs_dir=tmp_path, model_name="anthropic/claude-sonnet-5", **kwargs)


def test_command_carries_model_caps_and_the_quoted_instruction():
    cmd = tbench.command("it's done", "anthropic/claude-sonnet-5", budget_usd=0.2, max_turns=30)
    assert cmd.startswith(tbench.REMOTE_BIN + " run -p 'it'\"'\"'s done'")
    assert "--provider anthropic --tier code=claude-sonnet-5" in cmd
    assert "--budget 0.2" in cmd and "--max-turns 30" in cmd
    assert "--output-format json" in cmd


def test_command_never_asks_inside_the_task_container():
    # A denied call cannot be approved by anyone in a benchmark run; the
    # container is the isolation boundary.
    cmd = tbench.command("x", "anthropic/m", budget_usd=1, max_turns=1)
    assert "--permission-mode bypass" in cmd and "--sandbox danger-full-access" in cmd
    assert "--approve never" not in cmd


def test_command_never_fails_so_harbor_keeps_the_usage():
    # Harbor raises on a non-zero exit; a denied call or a spent budget must
    # still leave the payload readable.
    assert tbench.command("x", "anthropic/m", budget_usd=1, max_turns=1).endswith("|| true")


def test_last_json_line_skips_noise_after_the_payload():
    assert tbench.last_json_line('log\n{"a": 1}\n{"b": 2}\nnot json {\n') == {"b": 2}
    assert tbench.last_json_line("") is None
    assert tbench.last_json_line(None) is None


def test_fill_context_counts_every_prompt_token_and_the_cost():
    context = AgentContext()
    tbench.fill_context(context, PAYLOAD)
    assert context.n_input_tokens == 1210
    assert context.n_cache_tokens == 1000
    assert context.n_output_tokens == 80
    assert context.cost_usd == 0.0123
    assert context.metadata["cox_exit_code"] == 0


def test_install_uploads_the_binary_and_makes_it_executable(tmp_path):
    binary = tmp_path / "cox"
    binary.write_bytes(b"\x7fELF")
    env = FakeEnv()
    asyncio.run(agent(tmp_path, cox_bin=str(binary)).install(env))
    assert env.uploads == [(str(binary), tbench.REMOTE_BIN)]
    assert env.execs[-1]["command"].endswith(f"chmod 755 {tbench.REMOTE_BIN}")
    assert env.execs[-1]["user"] == "root"


def test_install_without_a_binary_fails_before_touching_the_container(tmp_path, monkeypatch):
    monkeypatch.delenv("COX_LINUX_BIN", raising=False)
    env = FakeEnv()
    with pytest.raises(RuntimeError, match="no Linux cox binary"):
        asyncio.run(agent(tmp_path).install(env))
    assert env.uploads == [] and env.execs == []


def test_run_passes_the_key_only_to_the_cox_command_and_fills_the_context(tmp_path, monkeypatch):
    monkeypatch.setenv("ANTHROPIC_API_KEY", "sk-test")
    env = FakeEnv(stdout=json.dumps(PAYLOAD))
    context = AgentContext()
    asyncio.run(agent(tmp_path).run("fix it", env, context))
    cox_calls = [e for e in env.execs if tbench.REMOTE_BIN + " run" in e["command"]]
    assert len(cox_calls) == 1
    assert cox_calls[0]["env"]["ANTHROPIC_API_KEY"] == "sk-test"
    others = [e for e in env.execs if e not in cox_calls]
    assert all("ANTHROPIC_API_KEY" not in e["env"] for e in others)
    assert context.cost_usd == 0.0123
    assert (tmp_path / "cox-stdout.txt").read_text() == json.dumps(PAYLOAD)


def test_run_without_the_provider_key_fails_before_running_cox(tmp_path, monkeypatch):
    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    env = FakeEnv()
    with pytest.raises(RuntimeError, match="ANTHROPIC_API_KEY"):
        asyncio.run(agent(tmp_path).run("x", env, AgentContext()))
    assert env.execs == []


def test_run_without_a_payload_is_an_error(tmp_path, monkeypatch):
    monkeypatch.setenv("ANTHROPIC_API_KEY", "sk-test")
    with pytest.raises(RuntimeError, match="no JSON payload"):
        asyncio.run(agent(tmp_path).run("x", FakeEnv(stdout="panic"), AgentContext()))


def test_provider_config_points_cox_at_a_local_server():
    assert tbench.provider_config("anthropic/m", None) is None
    assert tomllib.loads(tbench.provider_config("anthropic/org/m", "http://h:1234")) == {
        "providers": {"anthropic": {"base_url": "http://h:1234"}}}
    assert tomllib.loads(tbench.provider_config("local/m", "http://h/v1", 4096)) == {
        "providers": {"local": {"base_url": "http://h/v1", "api": "chat", "model": "m",
                                "context_window": 4096}}}


def test_run_with_a_base_url_uploads_the_config_before_cox_runs(tmp_path, monkeypatch):
    monkeypatch.setenv("ANTHROPIC_API_KEY", "local")
    env = FakeEnv(stdout=json.dumps(PAYLOAD))
    asyncio.run(agent(tmp_path, base_url="http://host.lima.internal:1234").run("x", env, AgentContext()))
    assert env.uploads == [(str(tmp_path / "cox-config.toml"), f"{tbench.REMOTE_HOME}/config.toml")]
    assert "host.lima.internal" in (tmp_path / "cox-config.toml").read_text()
