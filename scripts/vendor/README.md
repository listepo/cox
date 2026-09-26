# cox-vendor

The one entry point for a file no package manager fetches: a vendored API
spec, a price or model table, any JSON/YAML data pulled from outside the
repo (`plan.md` A48). No hand `curl`, no pasted rows — every such file is
produced by a saved, tested Python script here and re-run to update it.

## Commands

```bash
uv run --project scripts/vendor cox-vendor anthropic-spec           # fetch, validate, write
uv run --project scripts/vendor cox-vendor anthropic-spec --check   # report a diff, write nothing, exit 1 if stale

uv run --project scripts/vendor cox-vendor models           # regenerate prices.toml + default.toml from models.dev
uv run --project scripts/vendor cox-vendor models --check   # report a diff, write nothing, exit 1 if stale
```

`models` (T30.20) regenerates the `[[model]]` rows of
`crates/cox-provider/prices.toml` and the `[providers.*].models` arrays of
`crates/cox-protocol/default.toml` from `https://models.dev/api.json`. It
only ever updates ids the two files already list — an id models.dev
doesn't have, or whose provider section has no models.dev counterpart
(`local`, `typesafe`), is reported on stdout and left unchanged, never
dropped. Anthropic's four native rows are cross-checked against models.dev
but never overwritten from it (the vendor pricing page outranks a mirror);
a difference is reported for a human to re-verify by hand.

Or through the task runner: `just vendor anthropic-spec`, `just vendor anthropic-spec --check`, `just vendor models --check`.

## Adding a vendored file

1. Add a module `src/cox_vendor/<name>.py` with:
   - the source URL or API as a module-level constant, so bumping it later
     is a one-line diff;
   - a `run(*, check: bool = False, fetch=None) -> bool` that calls
     `fetch()` (defaulting to its own `download()`) to get the bytes,
     validates them (raise `ValueError` and write nothing on anything
     unexpected), writes the target file(s) unless `check`, and returns
     whether the bytes changed.
2. Register it in `src/cox_vendor/registry.py`'s `COMMANDS` dict, keyed by
   the name used on the command line.
3. Add tests under `tests/` with no network: stub `fetch`, and cover a
   first write, a same-bytes no-op re-run, `--check` on both a diff and no
   diff, and rejection of a malformed payload with nothing written.

## Tests

```bash
just vendor-test
```
