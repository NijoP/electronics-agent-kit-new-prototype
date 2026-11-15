# Remediation ledger (from the installed-build evaluation)

Status key: **main** = a fix commit exists on main, code-reviewed only, not verified in a build. **open** = no fix. Nothing here is verified on the installed binary.

| ID | Pri | Finding | Evidence | Status |
|---|---|---|---|---|
| R1 | P1 | Terminal viewport 24 rows / 44.6 % of height, dark bands | t03/t04 screenshots; `stty size` 24 149 | main (`344e0c9`) |
| R2 | P1 | Split right stacks vertically, right half blank; Split down 2+1; overflow past status bar at small heights | t16/t18/t22 | main partly (`344e0c9` drops `flex-grow:0`); re-verify after build |
| R3 | P1 | Agent PTY starts 24x80 before fit; launcher types command into a bash session | t13 stub output | open; spawn PTY at fitted size / exec the agent directly (`3f37709` changes launch, unverified) |
| R4 | P2 | Tab bar wraps/clips; tabs 25 px | t13, t22 | main (`3f37709`, `4435991`), unverified |
| R5 | P2 | Workspace switcher small (25 px, underline-only active) | t01 | main (`4435991`), unverified |
| R6 | P2 | PCB empty state shows schematic guidance | e01 | main (`10eee89`), unverified |
| R7 | P2 | Explorer file names truncate ("proj...") behind badges | s07 | open |
| R8 | P2 | Escape did not close Model settings; dialog persisted across workspaces | t01 | open (needs repro) |
| R9 | P2 | Toolbar density, 24 px targets, disabled buttons without reason | s07 | main (`198a22c`), unverified |
| R10 | Sec Medium | PTY children inherit full environment (S1) | stub output | open |
| R11 | Low | Stale socket/token files after SIGTERM (S4) | harness dir | open |
| R12 | Low | Close-confirm wording "is hung up" | t21 | open |
| R13 | Info | Slow project open/recover and workspace switch (1.4-16 s, noisy) | perf doc | open, needs profiling |
| R14 | Gap | Canvas editing, explorer file ops (traversal), injection strings, IPC malformed/stale tests, dependency audit not run | inventory | open: needs another run |
