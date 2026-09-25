"""Registry of vendored files `cox-vendor` can produce (plan.md A48). Each
entry is a `run(*, check: bool, fetch=None) -> bool` callable: True if the
vendored bytes differ from what's on disk, nothing written when `check` is
set.
"""

from . import anthropic_spec, models

COMMANDS = {
    "anthropic-spec": anthropic_spec.run,
    "models": models.run,
}
