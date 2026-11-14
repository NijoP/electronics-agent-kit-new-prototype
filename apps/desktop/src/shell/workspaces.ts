// Loads every `src/workspaces/*.workspace.ts(x)` module so a feature can register its activity-bar
// workspace or bottom-panel tab (see registry.ts) by adding one file, without editing the shell.
// Imported once, for its side effects, from main.tsx and the shell.
const modules = import.meta.glob("../workspaces/*.workspace.{ts,tsx}", { eager: true });

/** Number of workspace modules found (for the diagnostics log). */
export const workspaceModuleCount = Object.keys(modules).length;
