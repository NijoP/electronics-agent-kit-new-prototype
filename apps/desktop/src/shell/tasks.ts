// Task list for the bottom panel: the long-running operations this session started (checks,
// exports, Update PCB) with their real outcome. Entries are created when the operation starts and
// finished when it settles; nothing is invented.
import { useCallback, useRef, useState } from "react";

export interface Task {
  id: number;
  label: string;
  status: "running" | "done" | "failed";
  startedAt: number;
  endedAt?: number;
  detail?: string;
}

const MAX_TASKS = 50;

export function useTasks() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const seq = useRef(1);

  /** Run `f`, recording it as a task. Rethrows so the caller's own error handling still runs. */
  const track = useCallback(async <T,>(label: string, f: () => Promise<T>, detail?: (r: T) => string | undefined): Promise<T> => {
    const id = seq.current++;
    const patch = (p: Partial<Task>) => setTasks((old) => old.map((t) => (t.id === id ? { ...t, ...p } : t)));
    setTasks((old) => [...old, { id, label, status: "running" as const, startedAt: Date.now() }].slice(-MAX_TASKS));
    try {
      const r = await f();
      patch({ status: "done", endedAt: Date.now(), detail: detail?.(r) });
      return r;
    } catch (e) {
      patch({ status: "failed", endedAt: Date.now(), detail: String(e).replace(/^Error: /, "") });
      throw e;
    }
  }, []);

  const clearFinished = useCallback(() => setTasks((old) => old.filter((t) => t.status === "running")), []);
  return { tasks, track, clearFinished };
}
