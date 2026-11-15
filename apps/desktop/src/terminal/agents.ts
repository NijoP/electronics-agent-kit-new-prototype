// Agent launch surface data: which coding-agent CLIs EAK knows, which are installed (the backend
// only reports launchers whose executable it found, see src-tauri/src/terminal.rs), and the argv a
// launch runs. A launch runs the executable itself on the PTY, never a shell labelled as the agent.
export interface Launcher { name: string; command: string; path: string | null }
export interface AgentRow { id: string; label: string; glyph: string; installed: boolean; command: string; path: string | null }

/** Always offered, in this order; installed or not. */
export const KNOWN_AGENTS: { id: string; label: string; glyph: string }[] = [
  { id: "pi", label: "Pi", glyph: "π" },
  { id: "opencode", label: "OpenCode", glyph: "O" },
  { id: "claude", label: "Claude Code", glyph: "C" },
  { id: "codex", label: "Codex", glyph: "X" },
  { id: "gemini", label: "Gemini", glyph: "G" },
];

export function agentRows(launchers: Launcher[]): AgentRow[] {
  const have = new Map(launchers.map((l) => [l.name, l]));
  const known = KNOWN_AGENTS.map((a) => {
    const l = have.get(a.id);
    return { ...a, installed: !!l, command: l?.command ?? a.id, path: l?.path ?? null };
  });
  const extra = launchers
    .filter((l) => !KNOWN_AGENTS.some((a) => a.id === l.name))
    .map((l) => ({ id: l.name, label: l.name, glyph: l.name.slice(0, 1).toUpperCase(), installed: true, command: l.command, path: l.path }));
  return [...known, ...extra];
}

/** argv for `terminal_create`: the resolved executable (when known) plus any declared arguments. */
export function launchArgv(a: Pick<AgentRow, "command" | "path">): string[] {
  const [prog, ...rest] = a.command.trim().split(/\s+/);
  return [a.path ?? prog, ...rest];
}

/** Short process-derived state word for a session tab. */
export const STATE_WORD: Record<string, string> = {
  running: "running",
  idle: "idle",
  waiting_for_input: "waiting",
  blocked: "blocked",
  completed: "done",
  failed: "failed",
  ended: "ended",
};
