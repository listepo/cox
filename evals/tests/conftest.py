"""Shared fixtures: a fake `cox` for harness logic, the real one for e2e.

The fake is a shell script that prints whatever `cox run` JSON payload a
test sets and records its argv, so accounting, flags and exit handling are
tested with no binary, no network and no key.
"""

import json
import os
import stat

import pytest

from cox_evals import harness

FAKE_COX = """#!/bin/sh
printf '%s\\n' "$@" > "$FAKE_COX_ARGS"
[ -n "$FAKE_COX_WRITE" ] && printf 'hi' > "$FAKE_COX_WRITE"
printf '%s\\n' "$FAKE_COX_PAYLOAD"
exit "${FAKE_COX_EXIT:-0}"
"""


class FakeCox:
    def __init__(self, path, args_file, monkeypatch):
        self.path = str(path)
        self._args_file = args_file
        self._mp = monkeypatch
        self.respond({})

    def respond(self, payload, exit_code=0, write=None):
        """What the next `cox run` prints, exits with, and writes to disk."""
        self._mp.setenv("FAKE_COX_PAYLOAD", json.dumps(payload))
        self._mp.setenv("FAKE_COX_EXIT", str(exit_code))
        if write:
            self._mp.setenv("FAKE_COX_WRITE", write)
        else:
            self._mp.delenv("FAKE_COX_WRITE", raising=False)

    def argv(self):
        return self._args_file.read_text().splitlines()


@pytest.fixture
def fake_cox(tmp_path, monkeypatch):
    path = tmp_path / "cox"
    path.write_text(FAKE_COX)
    path.chmod(path.stat().st_mode | stat.S_IEXEC)
    args_file = tmp_path / "argv"
    monkeypatch.setenv("FAKE_COX_ARGS", str(args_file))
    return FakeCox(path, args_file, monkeypatch)


@pytest.fixture
def real_cox():
    """The built binary (`COX_BIN`, `cox` on PATH, or the cargo target)."""
    try:
        path = harness.find_cox_bin(None)
    except SystemExit:
        pytest.skip("no cox binary: `cargo build -p cox` first")
    if not os.access(path, os.X_OK):
        pytest.skip(f"{path} is not executable")
    return path
