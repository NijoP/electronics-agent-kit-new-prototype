import type { Command, LibraryEntry } from "./types";

/** What the editors may do: submit typed engine commands (in order) and report status. */
export interface EditCtx {
  /** Run commands one after another, each against the revision the previous one produced. */
  execute: (commands: Command | Command[]) => Promise<void>;
  busy: boolean;
  library: LibraryEntry[];
  notify: (message: string) => void;
}

/** Skip keyboard shortcuts while the user types in a form control. */
export const isTyping = (t: EventTarget | null) =>
  t instanceof HTMLInputElement || t instanceof HTMLSelectElement || t instanceof HTMLTextAreaElement;
