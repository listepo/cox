"""`cox_evals.tbench`: how `CoxAgent` launches cox in a session and reads
the outcome back from the pane."""

import json

from cox_evals import tbench


class Pane:
    """A session that runs nothing and shows a fixed pane."""

    def __init__(self, text):
        self.text = text
        self.sent = []

    def send_keys(self, keys, block=True, max_timeout_sec=None):
        self.sent.append(keys[0])

    def capture_pane(self):
        return self.text


def agent(cox_bin):
    return tbench.CoxAgent(model_name="anthropic/claude-sonnet-5", cox_bin=cox_bin)


def test_last_json_line_skips_shell_noise_after_it():
    pane = 'prompt$ cox run\n{"a": 1}\n{"exit_code": 0}\nnot json {\nprompt$ '
    assert tbench._last_json_line(pane) == {"exit_code": 0}
    assert tbench._last_json_line("no json here") is None


def test_missing_binary_is_an_installation_failure():
    result = agent("/nonexistent/cox").perform_task("x", Pane(""))
    assert result.failure_mode == tbench.FailureMode.AGENT_INSTALLATION_FAILED


def test_missing_provider_key_is_an_agent_error(fake_cox, monkeypatch):
    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    result = agent(fake_cox.path).perform_task("x", Pane('{"exit_code": 0}'))
    assert result.failure_mode == tbench.FailureMode.UNKNOWN_AGENT_ERROR


def test_command_carries_provider_model_and_the_quoted_instruction(fake_cox, monkeypatch):
    monkeypatch.setenv("ANTHROPIC_API_KEY", "unused")
    pane = Pane(json.dumps({"exit_code": 0, "usage": {"input_tokens": 7, "output_tokens": 3}}))
    result = agent(fake_cox.path).perform_task("it's done", pane)
    (cmd,) = pane.sent
    assert "--provider anthropic" in cmd and "--tier code=claude-sonnet-5" in cmd
    assert "'it'\"'\"'s done'" in cmd
    assert result.failure_mode == tbench.FailureMode.NONE
    assert (result.total_input_tokens, result.total_output_tokens) == (7, 3)


def test_nonzero_exit_code_in_the_payload_is_an_agent_error(fake_cox, monkeypatch):
    monkeypatch.setenv("ANTHROPIC_API_KEY", "unused")
    result = agent(fake_cox.path).perform_task("x", Pane('{"exit_code": 2}'))
    assert result.failure_mode == tbench.FailureMode.UNKNOWN_AGENT_ERROR


def test_no_json_in_the_pane_is_a_parse_error(fake_cox, monkeypatch):
    monkeypatch.setenv("ANTHROPIC_API_KEY", "unused")
    result = agent(fake_cox.path).perform_task("x", Pane("cox: command crashed"))
    assert result.failure_mode == tbench.FailureMode.PARSE_ERROR


def test_self_test_passes_without_any_key(real_cox, monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    tbench.self_test(real_cox)
