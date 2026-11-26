// Mock answers for the IDE-shell IPC commands (open projects, project files, git status), shared by
// the tests that render the whole app against a mocked Tauri IPC. Returns `handled: false` for any
// other command so each test keeps its own handling of the engine commands.
export function shellCmd(
  cmd: string,
  project: { id: string; name: string } | null,
): { handled: true; value: unknown } | { handled: false } {
  switch (cmd) {
    case "open_projects":
      return { handled: true, value: project ? [{ id: project.id, name: project.name, dir: "/p/demo.eak", active: true }] : [] };
    case "project_files":
      return { handled: true, value: [] };
    case "project_git_status":
      return { handled: true, value: { available: false, branch: null, entries: [] } };
    case "close_project_by_id":
      return { handled: true, value: null };
    // Project Home (S1): no default projects folder in tests, so a folder is always chosen.
    case "home_default_dir":
      return { handled: true, value: null };
    default:
      return { handled: false };
  }
}
