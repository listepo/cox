"""Tests for `cox_vendor.cli`: dispatch and `--check` exit codes, against a
fake registry entry so no network and no real vendored files are touched."""

from cox_vendor import cli


def test_check_exits_1_when_the_entry_reports_a_diff(monkeypatch, capsys):
    monkeypatch.setitem(cli.COMMANDS, "fake", lambda check: True)
    code = cli.main(["fake", "--check"])
    assert code == 1
    assert "differs" in capsys.readouterr().out


def test_check_exits_0_when_up_to_date(monkeypatch, capsys):
    monkeypatch.setitem(cli.COMMANDS, "fake", lambda check: False)
    code = cli.main(["fake", "--check"])
    assert code == 0
    assert "up to date" in capsys.readouterr().out


def test_write_mode_exits_0_whether_or_not_it_changed(monkeypatch):
    monkeypatch.setitem(cli.COMMANDS, "fake", lambda check: True)
    assert cli.main(["fake"]) == 0
    monkeypatch.setitem(cli.COMMANDS, "fake", lambda check: False)
    assert cli.main(["fake"]) == 0


def test_a_rejected_payload_exits_1_with_a_message_on_stderr(monkeypatch, capsys):
    def boom(check):
        raise ValueError("not json")

    monkeypatch.setitem(cli.COMMANDS, "fake", boom)
    code = cli.main(["fake"])
    assert code == 1
    assert "not json" in capsys.readouterr().err


def test_an_unknown_name_is_rejected_by_argparse():
    try:
        cli.main(["nonexistent"])
    except SystemExit as exc:
        assert exc.code == 2
    else:
        raise AssertionError("expected SystemExit")
