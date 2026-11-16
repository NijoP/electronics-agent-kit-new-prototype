# Contributing to EAK

Thanks for helping. The short version:

- Read [AGENTS.md](AGENTS.md): one design model (EAK-IR), every mutation through a typed command, no Python, unknown checks are never reported as passing, nothing is called manufacturing-ready.
- Keep changes small and complete. Every bug fix needs a regression test; a change to a shared contract (IR types, `Command`, report types, the harness tool envelope) needs an ADR in [docs/decisions](docs/decisions).
- Before opening a pull request, the CI gates must pass: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and the frontend typecheck and tests.
- Platform parity matters: Windows, Linux and macOS are equal targets.
- Do not commit secrets, copyrighted datasheets or API keys. Third-party source goes only under `third_party/licenses/` with its notices.
- By contributing you agree your contribution is licensed under the project's [O'Saasy License](LICENSE).

Bugs and ideas: open an issue using the templates. For anything about safety or credentials see [docs/SECURITY.md](docs/SECURITY.md).
