# Contributing

Repository prose is English. Read [`AGENTS.md`](AGENTS.md) before changing
anything.

## Checks

```bash
mise exec -- cargo fmt --check
mise exec -- cargo clippy --workspace --all-targets -- -D warnings
mise exec -- cargo test --workspace
```

User docs live in [`docs/`](docs/) and the Hugo site in [`website/`](website/).
Start from [`docs/getting-started.md`](docs/getting-started.md). Keep claims
aligned with what the crates actually expose — do not invent surfaces or metrics.

## Contributor License Agreement

Before a pull request can be merged, every committer must sign the
[Contributor License Agreement](https://github.com/pyrlyn/infra/blob/main/CLA.md)
([Russian translation](https://github.com/pyrlyn/infra/blob/main/CLA.ru.md); the English text
prevails). You keep the copyright in your work; the agreement lets the project be offered under
the GPL and under its royalty-free and commercial licenses. The `cla` check on your pull request
explains how to sign: post the comment `I have read the CLA Document and I hereby sign the CLA`.
One signature covers all pyrlyn repositories.
