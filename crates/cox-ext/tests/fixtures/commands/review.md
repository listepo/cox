---
description: Review a file against the style guide
allowed-tools: read grep
model: haiku
argument-hint: <path> [focus]
---
Review $1 focusing on $2. Full args: $ARGUMENTS.

Branch: !`git branch --show-current`
Guide:
@STYLE.md
Contact me@example.com, not $HOME.
