"""Tests for `cox_vendor.whisper_models`: no network — the fetch is stubbed
with the Hugging Face API responses recorded under `fixtures/` (trimmed to
the files that matter, checked 2026-09-29), and OUT_FILE points at tmp_path."""

import copy
import json
from pathlib import Path

import pytest

from cox_vendor import whisper_models as wm

FIXTURES = Path(__file__).parent / "fixtures"
MODEL = json.loads((FIXTURES / "hf_whisper_cpp_model.json").read_text())
TREE = json.loads((FIXTURES / "hf_whisper_cpp_tree.json").read_text())
COMMIT = MODEL["sha"]
# Captured at import, before the autouse fixture points OUT_FILE at tmp_path.
REAL_OUT_FILE = wm.OUT_FILE


@pytest.fixture(autouse=True)
def out_file(tmp_path, monkeypatch):
    path = tmp_path / "data" / "whisper-models.json"
    monkeypatch.setattr(wm, "OUT_FILE", path)
    return path


def fetching(model=MODEL, tree=TREE):
    def fetch(url):
        if url == wm.MODEL_URL:
            return json.dumps(model).encode()
        if url == wm.tree_url(COMMIT):
            return json.dumps(tree).encode()
        raise AssertionError(f"unexpected URL {url}")

    return fetch


def written(path):
    return json.loads(path.read_text())


def test_table_pins_the_commit_in_every_url(out_file):
    assert wm.run(fetch=fetching(), today="2026-09-29")
    table = written(out_file)
    assert table["commit"] == COMMIT
    for row in table["models"]:
        assert row["url"] == f"https://huggingface.co/ggerganov/whisper.cpp/resolve/{COMMIT}/{row['file']}"
        assert "/main/" not in row["url"]


def test_rows_carry_sha256_and_size(out_file):
    wm.run(fetch=fetching(), today="2026-09-29")
    rows = {r["name"]: r for r in written(out_file)["models"]}
    assert sorted(rows) == sorted(wm.NAMES)
    tiny_en = rows["tiny.en"]
    assert tiny_en["size"] == 77704715
    assert tiny_en["sha256"] == "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f"
    assert tiny_en["license"] == "mit"
    assert all(len(r["sha256"]) == 64 and r["size"] > 0 for r in rows.values())


def test_file_without_lfs_oid_is_refused(out_file):
    tree = copy.deepcopy(TREE)
    for entry in tree:
        if entry["path"] == "ggml-base.en.bin":
            del entry["lfs"]
    with pytest.raises(ValueError, match="ggml-base.en.bin"):
        wm.run(fetch=fetching(tree=tree))
    assert not out_file.exists()


def test_file_missing_from_the_tree_is_refused(out_file):
    tree = [e for e in TREE if e["path"] != "ggml-small.bin"]
    with pytest.raises(ValueError, match="ggml-small.bin"):
        wm.run(fetch=fetching(tree=tree))
    assert not out_file.exists()


def test_output_is_stable(out_file):
    wm.run(fetch=fetching(), today="2026-09-29")
    first = out_file.read_bytes()
    # A later day against the same upstream files changes nothing, date included.
    assert not wm.run(fetch=fetching(), today="2027-01-01")
    assert out_file.read_bytes() == first
    assert [r["name"] for r in written(out_file)["models"]] == sorted(wm.NAMES)


def test_check_reports_a_diff_and_writes_nothing(out_file):
    assert wm.run(check=True, fetch=fetching(), today="2026-09-29")
    assert not out_file.exists()


def test_quantized_and_core_ml_files_are_not_offered(out_file):
    wm.run(fetch=fetching(), today="2026-09-29")
    files = {r["file"] for r in written(out_file)["models"]}
    assert "ggml-tiny.en-q5_1.bin" not in files
    assert not any(f.endswith(".zip") for f in files)


def test_default_target_is_the_crate_data_file():
    # `cox voice model` embeds this path; a stale one would vendor a file nothing reads.
    assert REAL_OUT_FILE == wm.REPO_ROOT / "crates" / "cox-voice" / "data" / "whisper-models.json"
