

#### T30.7 Evals as a Python package

Model: claude-opus-5-5 · Status: done 2026-09-25 · Blocks: T30.8, T30.9 · Size: ~150 · Priority: P1 · Complexity: 2
Goal: the eval scripts are one uv-managed package instead of loose files with no manifest: `evals/pyproject.toml` (`cox-evals`, `uv_build`), `evals/src/cox_evals/{harness,tbench}.py`, locked in `evals/uv.lock`, run as `uv run --project evals cox-evals …`. Python stays because a Harbor/Terminal-Bench agent has to be a Python class (T30.9).
Files: `evals/pyproject.toml`, `evals/uv.lock`, `evals/.python-version`, `evals/src/cox_evals/__init__.py`, `harness.py` (was `evals/run.py`), `tbench.py` (was `evals/tbench/adapter.py`), `justfile`, `toolchain.md`, `rust.md` untouched. A package move cannot fit three files; the diff is mostly renames.
Plan: (1) `git mv` both scripts into `src/cox_evals/` so history follows; (2) the hand-rolled TOML writer (`toml_escape`/`toml_value`/string-built `scenario_toml` and hook config) becomes `tomli-w`, which serialises the same tables; (3) task and hook paths resolve from the package's project root (`EVALS`), unchanged on disk; (4) `just eval` → `uv run --project evals cox-evals {{args}}`; (5) `toolchain.md`: uv, Python, and a `uv` package table (pyyaml, tomli-w, pytest); (6) active docs that name `evals/run.py` (plan T30.3 Check, `research.md` §5.3 reproduce line) point at the new command; `done.md` keeps its history.
Check:
```bash
COX_PROVIDER=scripted uv run --project evals cox-evals --dry-run
```
Done when: the dry run is 10/10 like before the move and `just eval --dry-run` works.
What landed (`1d8283a`): `evals/pyproject.toml` (`cox-evals` 0.1.0, `uv_build`, script `cox-evals = cox_evals.harness:main`), `evals/.python-version` (3.14), `evals/uv.lock`; `evals/run.py` → `evals/src/cox_evals/harness.py` and `evals/tbench/adapter.py` → `evals/src/cox_evals/tbench.py` via `git mv`; `EVALS` now resolves to the uv project root. `tomli-w` replaced `toml_escape`/`toml_value` and the string-built scenario and hook TOML: parsed with `tomllib`, the output for all 10 tasks and the hook config is identical to the old writer's. `just eval` runs `uv run --project evals cox-evals`. `toolchain.md` gained uv, python and a `uv (evals/)` package table. Active references (T30.3 Check, `research.md` §5.3 reproduce line, `hooks/verify.sh` header) point at the new command.
Deviations: more than three files, as the card said (a package move is mostly renames). Found and fixed on the way: `python -m cox_evals.tbench --self-test` failed on any machine without `OPENAI_API_KEY` exported, before the move too (`perform_task` refuses to start without the provider's key even though the scripted provider never reads it); the self-test now sets a placeholder. `research.md` §5.3's T12.1 paragraph still names the old paths; it records that run and stays as written.
Check:
```text
$ COX_PROVIDER=scripted uv run --project evals cox-evals --dry-run
10/10 passed  total cost $0.0000  tokens in/out/cache-read/cache-write 70362/120/0/0
$ just eval --dry-run --only create-file
1/1 passed  total cost $0.0000  tokens in/out/cache-read/cache-write 7401/13/0/0
$ env -u OPENAI_API_KEY uv run --project evals python -m cox_evals.tbench --self-test --cox-bin target/debug/cox
self-test ok (shim base)
```
