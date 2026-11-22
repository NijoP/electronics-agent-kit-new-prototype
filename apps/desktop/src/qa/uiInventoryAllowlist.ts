// Known UI-inventory violations that another slice is fixing, so main stays green meanwhile.
// Each entry names the owning slice; that slice removes the entry in the PR that fixes the
// control. A stale entry (no longer a violation) is listed in docs/qa/UI_INVENTORY.md.
// Do not add an entry to silence a new control: give it a handler or an explicit unavailable state.
import type { AllowEntry } from "./uiInventory";

export const UI_INVENTORY_ALLOWLIST: readonly AllowEntry[] = [
];
