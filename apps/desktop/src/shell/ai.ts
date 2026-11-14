// Offline commands. IMPORTANT: this is a deterministic local interpreter, not a model call;
// the panel labels its replies "Offline command (no AI)" and uses it only when no model
// provider is ready (real AI requests go through ai_propose in src-tauri/src/ai.rs).
// It turns a request into typed engine commands (the same `Command` values the UI and CLI
// use); nothing is applied until the user accepts the preview, and acceptance goes through
// Engine::execute like every other mutation. No model request is made from here, so no
// engineering number is ever produced or self-certified by it.
import { nextRotation } from "../editing";
import type { Command, Id, LibraryEntry, ProviderView, Snapshot } from "../types";

export interface Proposal {
  id: number;
  title: string;
  why: string;
  commands: Command[];
  /** Schematic symbols the commands touch, drawn as a preview on the canvases. */
  targets: Id[];
  destructive: boolean;
}

export interface Reply {
  text: string;
  proposal?: Proposal;
}

export interface AiContext {
  snapshot: Snapshot;
  selected: Id | null;
  library: LibraryEntry[];
}

let nextId = 1;
const proposal = (p: Omit<Proposal, "id" | "destructive"> & { destructive?: boolean }): Proposal => ({
  id: nextId++,
  destructive: false,
  ...p,
});

export const HELP =
  "I turn requests into typed engine commands you can preview first. Try: rotate, set value 10k, delete, annotate, sync, auto-place, auto-route, clear routing, or add <part name>.";

export function interpret(prompt: string, ctx: AiContext): Reply {
  const q = prompt.trim().toLowerCase();
  const sym = ctx.selected ? ctx.snapshot.project.schematic.symbols[ctx.selected] : undefined;
  if (!q) return { text: HELP };

  const needSym = (what: string): Reply => ({ text: `Select a part on the canvas first, then ask me to ${what}.` });

  if (/\brotate\b/.test(q)) {
    if (!sym) return needSym("rotate it");
    return {
      text: `Rotate ${sym.refdes} from ${sym.rotation} to ${nextRotation(sym.rotation)}.`,
      proposal: proposal({
        title: `Rotate ${sym.refdes}`,
        why: "Next 90° step.",
        commands: [{ type: "RotateSymbol", symbol_id: sym.id, rotation: nextRotation(sym.rotation) }],
        targets: [sym.id],
      }),
    };
  }
  const value = /\b(?:set\s+)?value\s+(?:to\s+)?(\S+)/.exec(prompt.trim());
  if (value) {
    if (!sym) return needSym("change its value");
    return {
      text: `Set ${sym.refdes} value from ${sym.value} to ${value[1]}. EAK recomputes any dependent engineering numbers; I do not.`,
      proposal: proposal({
        title: `Set ${sym.refdes} = ${value[1]}`,
        why: "Value edit requested.",
        commands: [{ type: "SetSymbolValue", symbol_id: sym.id, value: value[1] }],
        targets: [sym.id],
      }),
    };
  }
  if (/\bclear\b.*\brouting\b/.test(q))
    return {
      text: "Remove all routing. This is destructive; review before accepting.",
      proposal: proposal({ title: "Clear all routing", why: "Removes every track and via.", commands: [{ type: "ClearRouting", net: null }], targets: [], destructive: true }),
    };
  if (/\bdelete\b|\bremove\b/.test(q)) {
    if (!sym) return needSym("delete it");
    return {
      text: `Delete ${sym.refdes} (${sym.value}). This is destructive; review before accepting.`,
      proposal: proposal({ title: `Delete ${sym.refdes}`, why: "Removes the symbol.", commands: [{ type: "DeleteSymbol", symbol_id: sym.id }], targets: [sym.id], destructive: true }),
    };
  }
  const simple: [RegExp, string, Command, string][] = [
    [/\bannotate\b/, "Annotate schematic", { type: "Annotate" }, "Assigns reference designators."],
    [/\bsync\b/, "Sync schematic to PCB", { type: "SyncSchematicToPcb" }, "Adds missing footprints and nets to the PCB."],
    [/auto[- ]?place|\bplace all\b/, "Auto-place footprints", { type: "AutoPlace", grid: null }, "Deterministic grid placement."],
    [/auto[- ]?route|\broute\b/, "Auto-route all nets", { type: "AutoRoute", nets: null }, "Built-in router; some nets may stay unrouted."],
  ];
  for (const [re, title, command, why] of simple)
    if (re.test(q)) return { text: `${title}. ${why}`, proposal: proposal({ title, why, commands: [command], targets: [] }) };

  const add = /^(?:add|place|insert)\s+(.+)$/.exec(q);
  if (add) {
    const part = ctx.library.find((c) => c.name.toLowerCase().includes(add[1]) || c.id.toLowerCase() === add[1]);
    if (!part) return { text: `No library part matches "${add[1]}". Open Parts to browse the library.` };
    const sheet = Object.values(ctx.snapshot.project.schematic.sheets)[0];
    const n = Object.keys(ctx.snapshot.project.schematic.symbols).length;
    return {
      text: `Place ${part.name} on the first sheet. Wiring is up to you.`,
      proposal: proposal({
        title: `Place ${part.name}`,
        why: "Adds one symbol on a free grid position.",
        commands: [
          {
            type: "PlaceSymbol",
            sheet_id: sheet?.id ?? null,
            component_id: part.id,
            position: { x: 20_320_000 + (n % 6) * 12_700_000, y: 20_320_000 + Math.floor(n / 6) * 12_700_000 },
            rotation: "R0",
            refdes: null,
            value: null,
          },
        ],
        targets: [],
      }),
    };
  }
  return { text: `I could not turn that into a command. ${HELP}` };
}

/** "- before / + after" lines shown in the preview diff; computed from the snapshot, not by a model. */
export function diffLines(p: Proposal, snapshot: Snapshot, library: LibraryEntry[]): string[] {
  const syms = snapshot.project.schematic.symbols;
  return p.commands.flatMap((c): string[] => {
    switch (c.type) {
      case "RotateSymbol": {
        const s = syms[c.symbol_id];
        return [`- ${s?.refdes ?? c.symbol_id} rotation ${s?.rotation ?? "?"}`, `+ ${s?.refdes ?? c.symbol_id} rotation ${c.rotation}`];
      }
      case "SetSymbolValue": {
        const s = syms[c.symbol_id];
        return [`- ${s?.refdes ?? c.symbol_id} value ${s?.value ?? "?"}`, `+ ${s?.refdes ?? c.symbol_id} value ${c.value}`];
      }
      case "DeleteSymbol": {
        const s = syms[c.symbol_id];
        return [`- ${s?.refdes ?? c.symbol_id} (${s?.value ?? "?"})`];
      }
      case "PlaceSymbol":
        return [`+ place ${library.find((l) => l.id === c.component_id)?.name ?? c.component_id}`];
      default:
        return [`~ ${c.type}`];
    }
  });
}

/** The first provider profile that can actually be called: it has a model and, when the
 * endpoint needs one, a stored key. `null` means the panel must say no model is configured. */
export function readyProvider(ps: ProviderView[]): { id: string; label: string } | null {
  const p = ps.find((x) => x.model && (x.has_key || !x.needs_key));
  return p ? { id: p.id, label: `${p.label} · ${p.model}` } : null;
}
