# Changelog

## 0.1.0 (unreleased) - M1 engineering foundation
- EAK-IR v1, built-in starter library, schema gate and integrity checks.
- Engineering core: IPC sizing rules, circuit calcs, netlist, copper connectivity, zone fill, router, placer.
- Verification engine: ERC (13 rules), DRC (13 rules), mechanical (3 rules) with revision-bound evidence.
- Runtime: typed command engine, approvals, transactions, undo/redo, journal + crash recovery, checkpoints,
  process supervisor, task model, SQLite index.
- Manufacturing compiler: Gerber, Excellon, BOM, CPL, SHA-256 manifest, release gate, package verification.
- Three-OS CI workflow and Python-free policy scan.

## Unreleased - Intelligence Engine core (`eak-intelligence`)
- Provider trait with OpenAI-compatible (`/chat/completions`), Anthropic Messages (`/v1/messages`) and
  Ollama (`/api/chat`) adapters. The HTTPS transport (ureq + rustls) is optional (Cargo feature `remote-http`,
  off by default; without it remote providers return `TransportNotCompiledIn`). It is cancellable, with bounded retries
  and a body-size limit.
- API keys come only from user configuration or an environment variable. `ApiKey` is never
  serialised and `Debug` is redacted.
- Typed tools for all 38 `eak_runtime::Command` variants. Tool names and operation classes come from
  the enum; schemas are tested against the serde shape.
- Read-only tools: design summary, library, netlist, ERC and DRC reports, and calculators backed by
  `eak-engineering`.
- Bounded, cancellable agent loop with Manual, Assisted and Autonomous modes:
  - Each command gets a dry-run preview and needs an approval covering its `OperationClass`. It is
    submitted as an `Agent`-actor `CommandRequest`.
  - Stale views and stale plans are refused.
  - The run log carries actions and events.
- `ScriptedProvider` mock for network-free tests.
