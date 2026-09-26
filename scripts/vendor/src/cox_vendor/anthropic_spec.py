"""Vendors the Anthropic OpenAPI spec (T30.19): the JSON snapshot that
`cox-provider-anthropic`'s `build.rs` turns into wire types with typify. This
module is the only thing that writes
`crates/cox-provider-anthropic/schema/anthropic-openapi.json`
and the "Downloaded"/"sha256" rows in its README — no hand `curl`.

To vendor a newer snapshot: bump SNAPSHOT_URL below, then run
`uv run --project scripts/vendor cox-vendor anthropic-spec`.
"""

from __future__ import annotations

import hashlib
import json
import urllib.request
from datetime import date
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[4]
SCHEMA_DIR = REPO_ROOT / "crates" / "cox-provider-anthropic" / "schema"
SCHEMA_FILE = SCHEMA_DIR / "anthropic-openapi.json"
README_FILE = SCHEMA_DIR / "README.md"

# The Stainless SDK generator's last published snapshot for the Anthropic
# SDKs (JSON despite the `.yml` extension in the URL). It is a snapshot,
# not a maintained pointer: `anthropic-sdk-python` stopped linking a spec
# URL in `.stats.yml` on 2026-09-03. Bump this constant by hand when a
# newer snapshot is published.
SNAPSHOT_URL = (
    "https://storage.googleapis.com/stainless-sdk-openapi-specs/anthropic/"
    "anthropic-465bff21a179090915396565d1ae8f705cf8596e2ec920eb121072f25b8a7d68.yml"
)


def download(url: str = SNAPSHOT_URL) -> bytes:
    with urllib.request.urlopen(url, timeout=30) as resp:  # noqa: S310 - fixed https URL
        return resp.read()


def validate(body: bytes) -> None:
    """Raise ValueError on anything that isn't the spec we expect."""
    try:
        parsed = json.loads(body)
    except json.JSONDecodeError as exc:
        raise ValueError(f"snapshot is not valid JSON: {exc}") from exc
    if not isinstance(parsed, dict) or "openapi" not in parsed:
        raise ValueError('snapshot has no top-level "openapi" key')


def update_readme(text: str, *, sha256: str, today: str) -> str:
    """Rewrite the "Downloaded" and "sha256" table rows; everything else
    in the README (prose, the re-vendor command) is hand-maintained."""
    lines = text.splitlines(keepends=True)
    for i, line in enumerate(lines):
        if line.startswith("| Downloaded |"):
            lines[i] = f"| Downloaded | {today} |\n"
        elif line.startswith("| sha256 |"):
            lines[i] = f"| sha256 | `{sha256}` |\n"
    return "".join(lines)


def run(*, check: bool = False, fetch=None) -> bool:
    """Fetch the snapshot, validate it, and — unless `check` — write the
    file and the README rows. Returns whether the bytes differ from what's
    currently vendored; nothing is written when they don't, or when
    `check` is set."""
    body = (fetch or download)()
    validate(body)
    current = SCHEMA_FILE.read_bytes() if SCHEMA_FILE.exists() else None
    changed = current != body
    if changed and not check:
        SCHEMA_FILE.write_bytes(body)
        sha256 = hashlib.sha256(body).hexdigest()
        today = date.today().isoformat()
        README_FILE.write_text(update_readme(README_FILE.read_text(), sha256=sha256, today=today))
    return changed
