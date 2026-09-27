"""Tests for `cox_vendor.anthropic_spec`: no network — every test stubs the
download with a `fetch` callable, and SCHEMA_FILE/README_FILE are pointed at
a tmp_path fixture instead of the real vendored files."""

import hashlib
import json

import pytest

from cox_vendor import anthropic_spec as spec

# Captured at import, before the autouse fixture points SCHEMA_FILE at tmp_path.
REAL_SCHEMA_FILE = spec.SCHEMA_FILE

SPEC_BODY = json.dumps({"openapi": "3.1.0", "paths": {}}, sort_keys=True).encode()

README_TEMPLATE = """# Vendored API specs

## `anthropic-openapi.json`

| | |
| --- | --- |
| Source | https://example.invalid/spec.yml |
| Downloaded | 2020-01-01 |
| sha256 | `deadbeef` |
"""


@pytest.fixture(autouse=True)
def isolated_files(tmp_path, monkeypatch):
    schema = tmp_path / "anthropic-openapi.json"
    readme = tmp_path / "README.md"
    readme.write_text(README_TEMPLATE)
    monkeypatch.setattr(spec, "SCHEMA_FILE", schema)
    monkeypatch.setattr(spec, "README_FILE", readme)
    return schema, readme


def fetching(body=SPEC_BODY):
    return lambda: body


def test_first_run_writes_the_file_and_the_readme_rows(isolated_files):
    schema, readme = isolated_files
    changed = spec.run(fetch=fetching())
    assert changed
    assert schema.read_bytes() == SPEC_BODY
    text = readme.read_text()
    assert f"| sha256 | `{hashlib.sha256(SPEC_BODY).hexdigest()}` |" in text
    assert "2020-01-01" not in text
    assert "deadbeef" not in text


def test_rerun_with_the_same_bytes_is_a_no_op(isolated_files):
    _, readme = isolated_files
    spec.run(fetch=fetching())
    before = readme.read_text()
    changed = spec.run(fetch=fetching())
    assert not changed
    assert readme.read_text() == before


def test_check_reports_a_diff_and_writes_nothing(isolated_files):
    schema, readme = isolated_files
    changed = spec.run(check=True, fetch=fetching())
    assert changed
    assert not schema.exists()
    assert "2020-01-01" in readme.read_text()


def test_check_after_a_real_run_reports_no_diff(isolated_files):
    spec.run(fetch=fetching())
    assert spec.run(check=True, fetch=fetching()) is False


def test_non_json_body_is_rejected_and_nothing_is_written(isolated_files):
    schema, readme = isolated_files
    with pytest.raises(ValueError):
        spec.run(fetch=fetching(b"not json"))
    assert not schema.exists()
    assert "2020-01-01" in readme.read_text()


def test_json_without_an_openapi_key_is_rejected(isolated_files):
    schema, _ = isolated_files
    with pytest.raises(ValueError):
        spec.run(fetch=fetching(json.dumps({"foo": 1}).encode()))
    assert not schema.exists()


def test_validate_accepts_a_minimal_openapi_document():
    spec.validate(SPEC_BODY)  # no raise


def test_default_target_is_the_file_build_rs_reads():
    # T32.13 moved the spec with the Anthropic wire; a stale path would make
    # the script write a file no build reads.
    crate = spec.REPO_ROOT / "crates" / "cox-provider-anthropic"
    assert REAL_SCHEMA_FILE == crate / "schema" / "anthropic-openapi.json"
    assert REAL_SCHEMA_FILE.is_file()
    assert '"schema/anthropic-openapi.json"' in (crate / "build.rs").read_text()
