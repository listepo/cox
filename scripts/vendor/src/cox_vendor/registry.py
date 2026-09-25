"""Registry of vendored files `cox-vendor` can produce (plan.md A48). Each
entry is a `run(*, check: bool, fetch=None) -> bool` callable: True if the
vendored bytes differ from what's on disk, nothing written when `check` is
set. T30.20 adds a `models` entry here (models.dev prices and model lists).
"""

from . import anthropic_spec

COMMANDS = {
    "anthropic-spec": anthropic_spec.run,
}
