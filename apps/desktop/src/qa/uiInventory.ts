// UI inventory (slice E9, CI half): every visible control of the desktop app, from two sources.
//  * The action registry (A1): each action is probed in several workspace states; an action must
//    have a handler that some state enables, or be explicitly unavailable ("Not implemented: …"
//    or "Unavailable: …") in every state.
//  * A component scan of the JSX sources: every intrinsic interactive element (button, input,
//    select, textarea, a, or an element with an interactive role) must carry a handler, or be
//    disabled with a reason (`title`).
// A control that fails either rule is a violation; the CI test fails on any violation that is not
// in the allowlist (`uiInventoryAllowlist.ts`). `renderInventory` writes docs/qa/UI_INVENTORY.md.
import { NO_HANDLER, workspaceActions, type ActionDeps } from "../actions/sets";
import { keysOf } from "../actions/registry";
import { DOC_CONTEXTS, type Action, type DocContext } from "../actions/types";

export type ControlState = "enabled" | "state-dependent" | "unavailable" | "handler" | "violation";

export interface ActionRow {
  key: string;
  id: string;
  label: string;
  contexts: readonly DocContext[];
  menu?: string;
  toolbar?: string;
  keys: string;
  state: ControlState;
  /** The disabled reason when it is the same in every probed state. */
  reason?: string;
  violation?: string;
}

export interface ScannedControl {
  key: string;
  file: string;
  line: number;
  element: string;
  label: string;
  handlers: string[];
  state: ControlState;
  violation?: string;
}

export interface AllowEntry {
  /** `action:<id>` or `scan:<file>|<element>|<label>` (the `key` of the inventory row). */
  key: string;
  /** Slice that owns the fix (and removes the entry when it lands). */
  owner: string;
  why: string;
}

/** A disabled reason that says plainly that the capability does not exist yet. */
export const EXPLICIT_UNAVAILABLE = /^(Not implemented|Unavailable)\b/;

/** Functions that only answer a question (an action's enablement, the editor-operation lookup): calling one is not an effect. */
const QUERIES = new Set(["enabled", "editorOp"]);

export interface Recorder<T> {
  /** A copy of the dependencies whose effect calls are counted. */
  deps: T;
  calls: () => number;
  reset: () => void;
}

/**
 * Wraps every function of `deps` (top level, nested objects such as `grid` and `view3d`, and the
 * objects a function returns, such as the operation `editorOp` looks up) so each effect call is
 * counted; the wrapped functions still call the originals. Data (the snapshot, arrays) is kept as is.
 */
export function recorder<T extends object>(deps: T): Recorder<T> {
  let n = 0;
  const wrap = (v: unknown, key: string, depth: number): unknown => {
    if (typeof v === "function")
      return (...args: unknown[]) => {
        if (!QUERIES.has(key)) n++;
        return wrap((v as (...a: unknown[]) => unknown)(...args), "", depth + 1);
      };
    if (depth < 3 && v !== null && typeof v === "object" && !Array.isArray(v) && Object.values(v).some((x) => typeof x === "function"))
      return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, wrap(x, k, depth + 1)]));
    return v;
  };
  return {
    deps: wrap(deps, "", 0) as T,
    calls: () => n,
    reset: () => {
      n = 0;
    },
  };
}

/** A control the probe can run: a registry action (`enabled`) or a palette item (`disabled`). */
export interface Runnable {
  id: string;
  run: () => void;
  enabled?: () => true | string;
  disabled?: boolean;
}

/**
 * Ids of the items that are enabled in the recorded state but whose `run` makes no effect call:
 * a handler that does nothing. Items must be built from `rec.deps`.
 */
export function noOps(items: readonly Runnable[], rec: { calls: () => number; reset: () => void }): string[] {
  const live = items.filter((it) => (it.enabled ? it.enabled() === true : !it.disabled));
  return live
    .filter((it) => {
      rec.reset();
      it.run();
      return rec.calls() === 0;
    })
    .map((it) => it.id);
}

/**
 * Audits every registry action over the probed states (the first probe defines the set). Each
 * action enabled in a probe is also run there against recorded dependencies: one that calls
 * nothing is a violation (enabled, but does nothing). `build` is the action set (tests inject one).
 */
export function auditActions(probes: readonly ActionDeps[], build: (d: ActionDeps) => Action[] = workspaceActions): ActionRow[] {
  const recs = probes.map((p) => recorder(p));
  const sets = recs.map((r) => build(r.deps));
  const noop = new Set(sets.flatMap((s, i) => noOps(s, recs[i])));
  return sets[0].map((a) => {
    const results = sets.map((s) => s.find((x) => x.id === a.id)).filter((x) => x !== undefined).map((x) => x.enabled());
    const reasons = [...new Set(results.filter((r): r is string => r !== true))];
    const never = results.every((r) => r !== true);
    const handler = a.run !== NO_HANDLER;
    let state: ControlState = results.every((r) => r === true) ? "enabled" : "state-dependent";
    let violation: string | undefined;
    if (!handler) {
      if (never && reasons.length === 1 && EXPLICIT_UNAVAILABLE.test(reasons[0])) state = "unavailable";
      else {
        state = "violation";
        violation = never ? "no handler, and the disabled reason does not say it is unavailable" : "no handler, but enabled in some state";
      }
    } else if (never) {
      state = "violation";
      violation = "handler is never reachable: disabled in every probed state";
    } else if (noop.has(a.id)) {
      state = "violation";
      violation = "enabled, but running it calls nothing (a no-op handler)";
    }
    return {
      key: `action:${a.id}`,
      id: a.id,
      label: a.label,
      contexts: DOC_CONTEXTS.filter((c) => !a.docs || a.docs.includes(c)),
      menu: a.menu,
      toolbar: a.toolbar,
      keys: keysOf(a).join(", "),
      state,
      reason: never && reasons.length === 1 ? reasons[0] : undefined,
      violation,
    };
  });
}

const INTERACTIVE_TAGS = new Set(["button", "input", "select", "textarea", "a"]);
const INTERACTIVE_ROLE = /\brole=(?:"|\{\s*")(button|link|menuitem|menuitemcheckbox|menuitemradio|tab|checkbox|switch|radio|option|slider|spinbutton|combobox)"/;

/** Index of the `>` that closes the opening tag whose attributes start at `i`, or -1. */
export function tagEnd(src: string, i: number): number {
  let depth = 0;
  let quote: string | null = null;
  for (; i < src.length; i++) {
    const c = src[i];
    if (quote) {
      if (c === "\\") i++;
      else if (c === quote) quote = null;
      continue;
    }
    if (depth > 0 && c === "/" && src[i + 1] === "/") {
      const nl = src.indexOf("\n", i);
      if (nl < 0) return -1;
      i = nl;
    } else if (depth > 0 && c === "/" && src[i + 1] === "*") {
      const end = src.indexOf("*/", i + 2);
      if (end < 0) return -1;
      i = end + 1;
    } else if (c === '"' || c === "'" || c === "`") quote = c;
    else if (c === "{") depth++;
    else if (c === "}") depth--;
    else if (c === ">" && depth === 0) return i;
  }
  return -1;
}

const literal = (attrs: string, name: string): string | undefined => {
  const m = attrs.match(new RegExp(`(?:^|\\s)${name}=(?:"([^"]*)"|\\{\\s*"([^"]*)"\\s*\\})`));
  return m ? (m[1] ?? m[2]) : undefined;
};

/** Scans one TSX source for intrinsic interactive elements. `file` is the path shown in the inventory. */
export function scanSource(file: string, src: string): ScannedControl[] {
  const out: ScannedControl[] = [];
  const open = /<([a-z][a-zA-Z0-9]*)(?=[\s>/])/g;
  for (let m = open.exec(src); m; m = open.exec(src)) {
    const before = src[m.index - 1] ?? "";
    if (/[\w$.]/.test(before)) continue; // a generic such as Array<string>, not a JSX tag
    const end = tagEnd(src, m.index + m[0].length);
    if (end < 0) continue;
    const attrs = src.slice(m.index + m[0].length, end);
    const tag = m[1];
    const role = attrs.match(INTERACTIVE_ROLE)?.[1];
    if (!INTERACTIVE_TAGS.has(tag) && !role) continue;
    const type = tag === "input" ? (literal(attrs, "type") ?? "text") : undefined;
    const element = `${tag}${type ? `[type=${type}]` : ""}${role && role !== tag ? `[role=${role}]` : ""}`;
    const handlers = [...new Set([...attrs.matchAll(/(?:^|\s)(on[A-Z]\w*)=/g)].map((h) => h[1]))];
    if (/\{\s*\.\.\./.test(attrs)) handlers.push("{...props}");
    if (tag === "a" && /\bhref=/.test(attrs)) handlers.push("href");
    if (literal(attrs, "type") === "submit") handlers.push("form submit");
    if (/(?:^|\s)readOnly(?=[\s=/]|$)/.test(attrs)) handlers.push("readOnly");
    if (type === "hidden") handlers.push("hidden");
    const disabled = /(?:^|\s)(?:disabled|aria-disabled)(?=[\s=/]|$)/.test(attrs);
    const constDisabled = /(?:^|\s)disabled(?=\s|\/|$)|(?:^|\s)disabled=\{\s*true\s*\}/.test(attrs);
    const reason = /(?:^|\s)title=/.test(attrs);
    let text = "";
    if (!attrs.trimEnd().endsWith("/")) {
      const t = src.slice(end + 1).match(/^\s*([^<{]*)/);
      text = (t?.[1] ?? "").replace(/\s+/g, " ").trim();
    }
    const label =
      literal(attrs, "aria-label") ?? literal(attrs, "title") ?? (text ? text.slice(0, 48) : undefined) ?? literal(attrs, "placeholder") ?? (/aria-label=|title=/.test(attrs) ? "(dynamic)" : "(no label)");
    let state: ControlState = "handler";
    let violation: string | undefined;
    if (handlers.length === 0) {
      if (disabled && reason) state = "unavailable";
      else {
        state = "violation";
        violation = disabled ? "no handler and disabled without a reason (title)" : "no handler and not disabled";
      }
    } else if (constDisabled && !reason) {
      state = "violation";
      violation = "always disabled, without a reason (title)";
    } else if (constDisabled) state = "unavailable";
    const line = src.slice(0, m.index).split("\n").length;
    out.push({ key: `scan:${file}|${element}|${label}`, file, line, element, label, handlers, state, violation });
  }
  return out;
}

export interface Audit {
  actions: ActionRow[];
  controls: ScannedControl[];
  allow: readonly AllowEntry[];
  /** Quick-open items (Go to part, net or sheet…) probed like the actions: every id, and the ones that ran a no-op. */
  quickOpen: { ids: string[]; noOps: string[] };
}

/** Violations not covered by the allowlist (what fails CI). */
export const unallowed = (a: Audit): string[] => {
  const ok = new Set(a.allow.map((e) => e.key));
  return [
    ...a.actions.filter((r) => r.violation && !ok.has(r.key)).map((r) => `${r.key}: ${r.violation}`),
    ...a.controls.filter((c) => c.violation && !ok.has(c.key)).map((c) => `${c.key} (${c.file}:${c.line}): ${c.violation}`),
    ...a.quickOpen.noOps.filter((id) => !ok.has(`quick:${id}`)).map((id) => `quick:${id}: enabled, but running it calls nothing (a no-op handler)`),
  ];
};

/** Allowlist entries that no longer match a violation (the owner should remove them). */
export const staleAllow = (a: Audit): AllowEntry[] => {
  const bad = new Set([...a.actions, ...a.controls].filter((r) => r.violation).map((r) => r.key).concat(a.quickOpen.noOps.map((id) => `quick:${id}`)));
  return a.allow.filter((e) => !bad.has(e.key));
};

const cell = (s: string | undefined) => (s ?? "").replace(/\|/g, "\\|").replace(/\n/g, " ");

/** The generated docs/qa/UI_INVENTORY.md (no line numbers, so unrelated edits do not change it). */
export function renderInventory(a: Audit): string {
  const ok = new Set(a.allow.map((e) => e.key));
  const mark = (r: { key: string; state: ControlState; violation?: string }) => (r.violation ? (ok.has(r.key) ? `allowlisted: ${r.violation}` : `**VIOLATION**: ${r.violation}`) : r.state);
  const count = (rows: { state: ControlState; violation?: string; key: string }[]) => {
    const v = rows.filter((r) => r.violation);
    return `${rows.length} controls: ${rows.filter((r) => r.state === "unavailable").length} unavailable with a reason, ${v.length} violations (${v.filter((r) => ok.has(r.key)).length} allowlisted)`;
  };
  // the scan is grouped: identical controls of one file are one row with a count
  const groups = new Map<string, { c: ScannedControl; n: number }>();
  for (const c of a.controls) {
    const k = `${c.key}|${c.handlers.join(",")}|${c.state}|${c.violation ?? ""}`;
    const g = groups.get(k);
    if (g) g.n++;
    else groups.set(k, { c, n: 1 });
  }
  const stale = staleAllow(a);
  return [
    "# UI inventory (generated)",
    "",
    "Generated by `apps/desktop/src/qa/uiInventory.test.ts` (slice E9) from the action registry",
    "(`apps/desktop/src/actions/`) and a scan of the JSX sources. Do not edit by hand: CI regenerates it",
    "(`UPDATE_UI_INVENTORY=1 npx vitest run src/qa/uiInventory.test.ts` in `apps/desktop`), shows the",
    "diff in the job summary and uploads the regenerated file as the `ui-inventory` artifact.",
    "",
    "Rules (the unit test fails on a violation that is not allowlisted in `src/qa/uiInventoryAllowlist.ts`):",
    "- a registry action has a handler that some probed state enables, or is disabled in every state",
    '  with a reason that starts with "Not implemented" or "Unavailable";',
    "- every action enabled in a probed state, and every quick-open item (Go to part, net or sheet…),",
    "  is run against recorded dependencies there and must call at least one (no no-op handlers);",
    "- a scanned interactive element (`button`, `input`, `select`, `textarea`, `a`, or an interactive",
    "  `role`) has a handler (`on…`, spread props, `href`, submit, `readOnly`), or is disabled with a",
    "  `title` that gives the reason. The scan is static: a component handler that exists but does",
    "  nothing is not detected here; that is what the UI tests are for.",
    "",
    "## Summary",
    "",
    `- Action registry: ${count(a.actions)}.`,
    `- Component scan: ${count(a.controls)}.`,
    `- Quick open: ${a.quickOpen.ids.length} items probed, ${a.quickOpen.noOps.length} no-op handlers${a.quickOpen.noOps.length ? ` (${a.quickOpen.noOps.map((id) => `\`${id}\``).join(", ")})` : ""}.`,
    `- Allowlist: ${a.allow.length} entries${stale.length ? `, ${stale.length} stale (no longer a violation; remove them): ${stale.map((e) => `\`${e.key}\``).join(", ")}` : ""}.`,
    "",
    "## Action registry",
    "",
    "| Id | Label | Documents | Menu | Toolbar | Keys | State |",
    "|---|---|---|---|---|---|---|",
    ...a.actions.map((r) => `| \`${r.id}\` | ${cell(r.label)} | ${r.contexts.join(", ")} | ${r.menu ?? ""} | ${r.toolbar ?? ""} | ${cell(r.keys)} | ${cell(mark(r))}${r.reason ? ` (${cell(r.reason)})` : ""} |`),
    "",
    "## Component scan",
    "",
    "| File | Element | Label | Handlers | Count | State |",
    "|---|---|---|---|---|---|",
    ...[...groups.values()].map(({ c, n }) => `| ${c.file} | ${c.element} | ${cell(c.label)} | ${cell(c.handlers.join(", "))} | ${n} | ${cell(mark(c))} |`),
    "",
    "## Allowlist",
    "",
    "| Key | Owner | Why |",
    "|---|---|---|",
    ...a.allow.map((e) => `| \`${cell(e.key)}\` | ${e.owner} | ${cell(e.why)} |`),
    "",
  ].join("\n");
}
