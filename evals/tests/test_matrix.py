"""`cox_evals.matrix`: which API shape each agent gets from each provider,
the argv and env of every `harbor run`, and the per-task table built from
Harbor's result files."""

import io
import json

import pytest

from cox_evals import matrix

OPTS = {"context": 65536, "max_output": 8192, "cox_bin": "/bin/cox",
        "budget_usd": 1.0, "max_turns": 40}
LMSTUDIO = matrix.PROVIDERS["lmstudio"]


def flag(argv, name):
    """Values of a repeated flag, e.g. every `--ak`."""
    return [argv[i + 1] for i, a in enumerate(argv) if a == name]


def test_cox_reaches_a_local_server_through_messages_at_the_container_host():
    run = matrix.plan_run("cox", LMSTUDIO, "m", OPTS)
    assert run.argv[run.argv.index("-m") + 1] == "anthropic/m"
    assert "base_url=http://host.lima.internal:1234" in flag(run.argv, "--ak")
    assert run.env == {"ANTHROPIC_API_KEY": "local"}


def test_claude_code_gets_the_container_url_through_the_env_only():
    run = matrix.plan_run("claude-code", LMSTUDIO, "m", OPTS)
    assert run.env["ANTHROPIC_BASE_URL"] == "http://host.lima.internal:1234"
    assert not any("host.lima.internal" in a for a in run.argv)


def test_terminus_calls_from_the_host_so_it_keeps_localhost():
    run = matrix.plan_run("terminus-2", LMSTUDIO, "m", OPTS)
    assert run.argv[run.argv.index("-m") + 1] == "openai/m"
    ak = flag(run.argv, "--ak")
    assert "api_base=http://localhost:1234/v1" in ak
    info = json.loads(next(a for a in ak if a.startswith("model_info="))[len("model_info="):])
    assert info == {"max_input_tokens": 65536, "max_output_tokens": 8192}
    assert run.env == {"OPENAI_API_KEY": "local"}


def test_an_agent_the_provider_cannot_serve_fails_before_anything_runs():
    with pytest.raises(SystemExit, match="claude-code needs anthropic-messages"):
        matrix.plan_run("claude-code", matrix.PROVIDERS["ollama"], "m", OPTS)


def test_a_remote_provider_needs_its_key_and_never_copies_it(monkeypatch):
    anthropic = matrix.PROVIDERS["anthropic"]
    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    with pytest.raises(SystemExit, match="ANTHROPIC_API_KEY is not set"):
        matrix.plan_run("cox", anthropic, "claude-sonnet-5", OPTS)
    monkeypatch.setenv("ANTHROPIC_API_KEY", "sk-secret")
    for agent in ("cox", "claude-code", "terminus-2"):
        run = matrix.plan_run(agent, anthropic, "claude-sonnet-5", OPTS)
        assert run.env == {}
        assert not any("sk-secret" in a or "base_url" in a or "api_base" in a for a in run.argv)


def test_harbor_argv_selects_every_task_and_names_the_job(tmp_path):
    run = matrix.plan_run("cox", LMSTUDIO, "m", OPTS)
    argv = matrix.harbor_argv(run, ["a", "b"], job_name="j", jobs_dir=tmp_path, concurrency=1)
    assert argv[:4] == ["harbor", "run", "-d", "terminal-bench@2.0"]
    assert flag(argv, "-i") == ["a", "b"]
    assert flag(argv, "--job-name") == ["j"] and flag(argv, "-o") == [str(tmp_path)]


def test_dry_run_prints_one_command_per_agent_and_runs_nothing(monkeypatch, capsys):
    monkeypatch.setattr(matrix.subprocess, "run", lambda *a, **k: pytest.fail("ran"))
    assert matrix.main(["--preset", "t30.13", "--dry-run"]) == 0
    lines = capsys.readouterr().out.splitlines()
    assert len(lines) == 3
    assert all(line.count(" -i ") == 12 for line in lines)
    assert [line.split(" -a ")[1].split()[0] for line in lines] == [
        "cox_evals.tbench:CoxAgent", "claude-code", "terminus-2"]


def test_preflight_reports_a_model_the_server_does_not_serve(monkeypatch):
    body = json.dumps({"data": [{"id": "other"}]}).encode()
    monkeypatch.setattr(matrix.urllib.request, "urlopen", lambda *a, **k: io.BytesIO(body))
    assert "does not serve m" in matrix.preflight(LMSTUDIO, "m")
    assert matrix.preflight(LMSTUDIO, "other") is None
    assert matrix.preflight(matrix.PROVIDERS["anthropic"], "m") is None


def test_preflight_reports_a_server_that_is_down(monkeypatch):
    def down(*a, **k):
        raise OSError("connection refused")
    monkeypatch.setattr(matrix.urllib.request, "urlopen", down)
    assert "not reachable" in matrix.preflight(LMSTUDIO, "m")


def trial(job, name, reward, *, error=None):
    d = job / f"{name}__x"
    d.mkdir(parents=True)
    d.joinpath("result.json").write_text(json.dumps({
        "task_name": name,
        "verifier_result": None if error else {"rewards": {"reward": reward}},
        "agent_result": {"n_input_tokens": 100, "n_output_tokens": 20, "cost_usd": 0.5},
        "agent_execution": {"started_at": "2026-09-25T18:42:14Z",
                            "finished_at": "2026-09-25T18:43:02Z"},
        "exception_info": {"exception_type": error} if error else None,
    }))


def test_summarize_and_table_show_reward_tokens_time_and_totals(tmp_path):
    trial(tmp_path / "cox", "a", 1.0)
    trial(tmp_path / "cox", "b", 0.0)
    trial(tmp_path / "t2", "a", None, error="AgentTimeoutError")
    (tmp_path / "cox" / "result.json").write_text("{}")  # the job-level file is skipped
    results = {"cox": matrix.summarize(tmp_path / "cox"), "t2": matrix.summarize(tmp_path / "t2")}
    assert [r["task"] for r in results["cox"]] == ["a", "b"]
    assert results["cox"][0]["seconds"] == 48
    out = matrix.table(results)
    assert "| a | 1.0 · 120 tok · 48s | error: AgentTimeoutError |" in out
    assert "| b | 0.0 · 120 tok · 48s | — |" in out
    assert "| **total** | 1/2 · $1.0000 | 0/1 · $0.5000 |" in out
