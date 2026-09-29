"""Vendors the whisper model table (T54.1): the ggml files `cox voice model
download` may fetch, each pinned to one commit of the Hugging Face repository
`ggerganov/whisper.cpp` with its size and SHA-256. This module is the only
thing that writes `crates/cox-voice/data/whisper-models.json` — no row is
pasted by hand (plan.md A48, A123).

Source: the Hugging Face Hub API (no key needed for a public repository):
``GET https://huggingface.co/api/models/ggerganov/whisper.cpp`` gives the
current commit (`sha`) and the licence (`cardData.license`, also the
`license:<id>` tag); ``GET .../tree/<commit>`` lists the files at that commit,
each LFS file with `lfs.oid` (the SHA-256 of the content) and `lfs.size`.
The tree is read at the commit rather than at `main`, so the two documents
can never describe different revisions. Checked 2026-09-29: commit
`5359861c739e955e79d9a303bcbc70fb988958b1`, licence `mit`.

A download URL is ``https://huggingface.co/<repo>/resolve/<commit>/<file>``:
a moved `main` never changes what a pinned row fetches.

To refresh: `uv run --project scripts/vendor cox-vendor whisper-models`
(`--check` reports a diff and writes nothing). Re-running on a later day
against the same upstream files is a byte-for-byte no-op: `checked` moves
only when a row changes.
"""

from __future__ import annotations

import json
import re
import urllib.request
from datetime import date
from pathlib import Path
from typing import Any, Callable

REPO_ROOT = Path(__file__).resolve().parents[4]
OUT_FILE = REPO_ROOT / "crates" / "cox-voice" / "data" / "whisper-models.json"

REPOSITORY = "ggerganov/whisper.cpp"
MODEL_URL = f"https://huggingface.co/api/models/{REPOSITORY}"

# The models cox offers: English-only and multilingual, tiny to small. Larger
# ones are too slow on a laptop CPU for push-to-talk; quantized and Core ML
# files are out of scope (T54.1).
NAMES = ("tiny.en", "base.en", "small.en", "tiny", "base", "small")

_SHA256 = re.compile(r"^[0-9a-f]{64}$")
_COMMIT = re.compile(r"^[0-9a-f]{40}$")


def tree_url(commit: str) -> str:
    return f"{MODEL_URL}/tree/{commit}"


def download(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "cox-dev (https://github.com/listepo/cox)"})
    with urllib.request.urlopen(req, timeout=30) as resp:  # noqa: S310 - fixed https URL
        return resp.read()


def _json(body: bytes, what: str) -> Any:
    try:
        return json.loads(body)
    except json.JSONDecodeError as exc:
        raise ValueError(f"{what} is not valid JSON: {exc}") from exc


def commit_of(model: Any) -> str:
    commit = model.get("sha") if isinstance(model, dict) else None
    if not isinstance(commit, str) or not _COMMIT.match(commit):
        raise ValueError("model document has no 40-hex `sha` commit")
    return commit


def license_of(model: dict) -> str:
    card = model.get("cardData") or {}
    if isinstance(card.get("license"), str):
        return card["license"]
    for tag in model.get("tags") or []:
        if isinstance(tag, str) and tag.startswith("license:"):
            return tag.removeprefix("license:")
    raise ValueError("model document names no licence")


def build_table(model: dict, tree: Any, *, today: str) -> dict:
    """The table as a dict; raises ValueError on a missing file or a file
    without an LFS oid or size (a pointer-less file cannot be verified)."""
    commit = commit_of(model)
    licence = license_of(model)
    if not isinstance(tree, list):
        raise ValueError("tree document is not a list of files")
    by_path = {e.get("path"): e for e in tree if isinstance(e, dict)}
    rows = []
    for name in NAMES:
        file = f"ggml-{name}.bin"
        entry = by_path.get(file)
        if entry is None:
            raise ValueError(f"{file} is not in {REPOSITORY} at {commit}")
        lfs = entry.get("lfs") or {}
        sha256, size = lfs.get("oid"), lfs.get("size")
        if not isinstance(sha256, str) or not _SHA256.match(sha256):
            raise ValueError(f"{file} has no LFS SHA-256 oid")
        if not isinstance(size, int) or size <= 0:
            raise ValueError(f"{file} has no LFS size")
        rows.append({
            "name": name,
            "file": file,
            "url": f"https://huggingface.co/{REPOSITORY}/resolve/{commit}/{file}",
            "size": size,
            "sha256": sha256,
            "license": licence,
        })
    rows.sort(key=lambda r: r["name"])
    return {
        "checked": today,
        "commit": commit,
        "license": licence,
        "models": rows,
        "repository": REPOSITORY,
        "source": MODEL_URL,
    }


def render(table: dict) -> str:
    return json.dumps(table, indent=2, sort_keys=True) + "\n"


def run(*, check: bool = False, fetch: Callable[[str], bytes] | None = None, today: str | None = None) -> bool:
    """Fetch both documents, build the table and — unless `check` — write
    it. Returns whether the file's bytes would change. `today` is injectable
    so tests prove the date moves only with the rows."""
    get = fetch or download
    model = _json(get(MODEL_URL), "model document")
    tree = _json(get(tree_url(commit_of(model))), "tree document")
    table = build_table(model, tree, today=today or date.today().isoformat())

    current = OUT_FILE.read_text() if OUT_FILE.exists() else None
    if current is not None:
        try:
            old = json.loads(current)
        except json.JSONDecodeError:
            old = None
        if isinstance(old, dict) and {**old, "checked": table["checked"]} == table:
            table["checked"] = old.get("checked", table["checked"])
    new = render(table)
    changed = new != current
    if changed and not check:
        OUT_FILE.parent.mkdir(parents=True, exist_ok=True)
        OUT_FILE.write_text(new)
    return changed
