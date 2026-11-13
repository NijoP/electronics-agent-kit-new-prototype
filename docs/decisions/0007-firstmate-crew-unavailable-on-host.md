# ADR-0007: Develop directly while the Firstmate crew cannot run on this host
- **Date:** 2026-11-13
- **Context:** The product owner asked for Firstmate-coordinated secondmates and crewmates. On the
  Windows 11 development host, Firstmate's session start never acquires its session lock under
  Git Bash (two runs, 120 s and 400 s bounds; matches the open upstream fix kunchenguid/firstmate#8425).
  Firstmate forbids spawning from a lock-refused session. The owner forbade modifying the Firstmate
  installation. No WSL distribution is installed.
- **Options considered:**
  1. Patch Firstmate locally (forbidden).
  2. Install a WSL2 Linux distribution and run Firstmate there (needs owner approval).
  3. Continue development directly in this repository.
- **Decision:** Option 3 until the owner decides on option 2 or the upstream fix lands.
- **Consequences:** Sequential development with no parallel workers. Module ownership is still
  documented in AGENTS.md so a crew can take over cleanly.
- **Source:** Owner directives (crew activation, §7 host compatibility check).
