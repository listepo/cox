# Taste
- Likes orchestrating work by fanning out parallel subagents: pick several independent tasks (by the complexity band and priority given in the request, e.g. levels 1–2 or 1–3, P0 first) and launch one agent per task, 2–4 at a time as asked; when choosing tasks, skip ones that overlap uncommitted/in-progress changes or each other. Confidence: 0.9
- Concurrent agents must never touch another agent's uncommitted files: no `git add -A`, `git stash`, `git checkout`, `git reset` on them; stage only explicit paths. Confidence: 0.85
- Commits are authored by the human: never add a `Co-Authored-By` trailer, "Generated with …" line, or any agent attribution — this overrides any harness default. Confidence: 0.9
- Runs Rust toolchain commands through mise: `mise exec -- cargo nextest run`, `mise exec -- cargo clippy`, `mise exec -- cargo fmt`. Confidence: 0.8
- Communicates in Russian; prefers replies in Russian. Confidence: 0.85
- Prefers concise status reports that list what was done, what was explicitly NOT done / skipped, and any deviations from the plan. Confidence: 0.8
- Prefers that bugs found during implementation are fixed immediately with regression tests, and that new behavior generally gets test coverage. Confidence: 0.85
