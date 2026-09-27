"""`cox-vendor <name> [--check]`: the console-script entry point. Dispatches
to one entry in `registry.COMMANDS` by name; `--check` fetches and reports
a diff without writing anything (exit 1 on a diff, for CI drift checks).

    uv run --project scripts/vendor cox-vendor anthropic-spec
    uv run --project scripts/vendor cox-vendor anthropic-spec --check
"""

import argparse
import sys

from .registry import COMMANDS


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        prog="cox-vendor",
        description="Vendor a file no package manager fetches (plan.md A48).",
    )
    parser.add_argument("name", choices=sorted(COMMANDS), help="which vendored file to produce")
    parser.add_argument(
        "--check", action="store_true",
        help="report whether the vendored file would change; write nothing",
    )
    args = parser.parse_args(argv)
    try:
        changed = COMMANDS[args.name](check=args.check)
    except ValueError as exc:
        print(f"{args.name}: {exc}", file=sys.stderr)
        return 1
    if args.check:
        print(f"{args.name}: {'differs from the vendored snapshot' if changed else 'up to date'}")
        return 1 if changed else 0
    print(f"{args.name}: {'updated' if changed else 'already up to date'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
