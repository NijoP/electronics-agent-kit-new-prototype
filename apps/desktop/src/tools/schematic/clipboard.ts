// Schematic clipboard (slice B3): one per app window, shared by every sheet editor, so a copy on
// one sheet pastes on another. It holds the copied objects by value (`Clip`), never ids: a paste
// is new objects with fresh ids and designators, and nets carry over by label name only.
import type { Clip } from "./selection";

let clip: Clip | null = null;

export const clipboard = (): Clip | null => clip;
export function setClipboard(c: Clip | null): void {
  clip = c;
}
