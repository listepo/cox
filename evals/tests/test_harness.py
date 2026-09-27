"""`cox_evals.harness`: task loading, the TOML it writes for cox, and how a
`cox run` outcome becomes a pass/fail row with cost and tokens."""

import tomllib

from cox_evals import harness

PAYLOAD = {
    "session": "s",
    "result": "done",
    "usage": {
        "input_tokens": 4,
        "output_tokens": 80,
        "cache_read_tokens": 16464,
        "cache_write_tokens": 220,
    },
    "cost_usd": 0.0047,
    "turns": 2,
    "exit_code": 0,
}


def task(**overrides):
    return {"name": "t", "prompt": "do it", "setup": "true", "check": "true", **overrides}


def run(fake_cox, t, **kwargs):
    return harness.run_task(
        t, cox_bin=fake_cox.path, dry_run=False, provider="anthropic",
        model="claude-sonnet-5", **kwargs,
    )


def test_load_tasks_reads_every_task_file():
    tasks = harness.load_tasks([])
    assert len(tasks) == len(list(harness.TASKS.glob("*.yaml")))
    assert all({"name", "prompt", "check"} <= set(t) for _, t in tasks)


def test_only_matches_the_name_with_or_without_its_number():
    by_name = harness.load_tasks(["create-file"])
    by_stem = harness.load_tasks(["01-create-file"])
    assert [p.stem for p, _ in by_name] == ["01-create-file"]
    assert by_name == by_stem


def test_scenario_toml_turns_a_call_into_a_tool_round_and_a_final_answer():
    turns = [{"text": "creating", "calls": [{"tool": "write", "input": {"path": "a", "content": "x"}}],
              "final": "done"}]
    parsed = tomllib.loads(harness.scenario_toml(turns))
    assert parsed == {"turn": [
        {"text": "creating", "tool_calls": [{"name": "write", "input": {"path": "a", "content": "x"}}]},
        {"text": "done"},
    ]}


def test_scenario_toml_keeps_quotes_and_newlines_verbatim():
    text = 'say "hi"\nthen \\ stop'
    parsed = tomllib.loads(harness.scenario_toml([{"text": text}]))
    assert parsed == {"turn": [{"text": text}]}


def test_every_task_dry_run_is_valid_toml():
    for _, t in harness.load_tasks([]):
        tomllib.loads(harness.scenario_toml(t.get("dry_run", {}).get("turns")))


def test_verify_preset_writes_the_addendum_and_a_post_tool_use_hook(tmp_path):
    harness.write_verify_preset(tmp_path)
    assert (tmp_path / "AGENTS.md").read_text() == harness.VERIFY_ADDENDUM
    hooks = tomllib.loads((tmp_path / "config.toml").read_text())["hooks"]["PostToolUse"]
    assert hooks == [{
        "matcher": "edit|apply_patch|write",
        "command": str(harness.HOOKS / "verify.sh"),
        "timeout_s": 130,
    }]
    assert (harness.HOOKS / "verify.sh").exists()


def test_a_passing_run_records_cost_turns_and_tokens(fake_cox):
    fake_cox.respond(PAYLOAD, write="hello.txt")
    res = run(fake_cox, task(check='test "$(cat hello.txt)" = hi'))
    assert res["pass"] and res["note"] == ""
    assert res["cost_usd"] == 0.0047 and res["turns"] == 2
    assert res["tokens"] == PAYLOAD["usage"]


def test_a_nonzero_cox_exit_fails_but_keeps_the_cost(fake_cox):
    # Exit 2 is a denied tool call: the file may be right, the run still fails.
    fake_cox.respond({**PAYLOAD, "exit_code": 2}, exit_code=2, write="hello.txt")
    res = run(fake_cox, task())
    assert not res["pass"]
    assert res["note"] == "cox exit 2"
    assert res["cost_usd"] == 0.0047


def test_a_failed_check_reports_its_output(fake_cox):
    fake_cox.respond(PAYLOAD)
    res = run(fake_cox, task(check="echo wrong content >&2; false"))
    assert not res["pass"]
    assert res["note"] == "check failed: wrong content"


def test_a_failed_setup_never_runs_cox(fake_cox):
    res = run(fake_cox, task(setup="false"))
    assert not res["pass"] and res["note"] == "setup failed"
    assert res["cost_usd"] == 0.0


def test_unparseable_output_is_a_zero_cost_row_not_a_crash(fake_cox, monkeypatch):
    monkeypatch.setenv("FAKE_COX_PAYLOAD", "not json")
    res = run(fake_cox, task())
    assert res["cost_usd"] == 0.0 and res["tokens"] == {}


def test_default_runs_are_hermetic_and_pass_provider_and_model(fake_cox):
    fake_cox.respond(PAYLOAD)
    run(fake_cox, task())
    argv = fake_cox.argv()
    assert {"--no-hooks", "--no-mcp"} <= set(argv)
    assert argv[argv.index("--provider") + 1] == "anthropic"
    assert argv[argv.index("--tier") + 1] == "code=claude-sonnet-5"
    assert argv[argv.index("--approve") + 1] == "never"


def test_verify_preset_turns_hooks_back_on(fake_cox):
    fake_cox.respond(PAYLOAD)
    run(fake_cox, task(), preset="verify")
    argv = fake_cox.argv()
    assert "--no-hooks" not in argv and "--no-mcp" in argv


def test_main_sums_cost_and_tokens_and_exits_nonzero_on_a_failure(fake_cox, capsys):
    fake_cox.respond({**PAYLOAD, "exit_code": 2}, exit_code=2)
    code = harness.main(["--cox-bin", fake_cox.path, "--only", "create-file", "find-word"])
    out = capsys.readouterr().out
    assert code == 1
    assert "0/2 passed  total cost $0.0094" in out
    assert "tokens in/out/cache-read/cache-write 8/160/32928/440" in out


def test_token_line_orders_the_four_ledger_counts():
    assert harness.token_line(PAYLOAD["usage"]) == "4/80/16464/220"
    assert harness.token_line({}) == "0/0/0/0"


def test_cox_bin_prefers_the_flag_then_the_env(monkeypatch):
    monkeypatch.setenv("COX_BIN", "/from/env")
    assert harness.find_cox_bin("/from/flag") == "/from/flag"
    assert harness.find_cox_bin(None) == "/from/env"


def test_dry_run_passes_end_to_end_against_the_real_binary(real_cox):
    t = dict(harness.load_tasks(["create-file"])[0][1])
    res = harness.run_task(t, cox_bin=real_cox, dry_run=True, provider=None, model=None)
    assert res["pass"], res["note"]
    assert res["tokens"]["input_tokens"] > 0
