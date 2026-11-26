// Settings > Settings… (Phase 1 D4): one dialog with a searchable tree of pages. Every control
// writes a real store: the registry key map (`keymap.ts`), the canvas colour overrides
// (`canvasColours.ts`), the per-editor grid store (`grid.tsx`, which the editors open with) and the
// theme. The hotkey table is generated from the action registry, so it lists exactly the actions
// the menus, toolbar and palette show.
import { useEffect, useMemo, useRef, useState, type MutableRefObject } from "react";
import type { Action, DocContext } from "../actions/types";
import { formatShortcut } from "../design/shortcuts";
import { GRID_KINDS, GridControls, updateGrid, type GridKind } from "../grid";
import { CANVAS_OBJECTS, COLOUR_PRESETS, overrideOf, resetColour, resetColours, setColour, setColourPreset, useColours, type ColourPreset } from "./canvasColours";
import { POWER_STYLES, powerPortProblem, resetPowerPorts, setPowerPorts, usePowerPorts } from "./powerPorts";
import type { PowerStyle } from "../types";
import { bind, clearAction, effectiveKeys, normalize, PRESETS, presetKeys, resetAction, resetAll, setPreset, specOf, useKeymap, type PresetId } from "./keymap";
import { AiToolsSettings } from "./AiToolsSettings";

type Page = "keys" | "colours" | "theme" | "grid-schematic" | "grid-pcb" | "power" | "model" | "ai-tools";
interface Node {
  id: Page;
  group: string;
  label: string;
  /** Extra words the tree search matches. */
  words: string;
}
const TREE: readonly Node[] = [
  { id: "keys", group: "Keyboard", label: "Hotkeys", words: "shortcut key binding preset easyeda conflict" },
  { id: "theme", group: "Appearance", label: "Theme", words: "dark light" },
  { id: "colours", group: "Appearance", label: "Canvas colours", words: "colour color paper print wire sheet board pad via" },
  { id: "grid-schematic", group: "Editors", label: "Schematic grid", words: "grid snap unit mm mil inch spacing default" },
  { id: "grid-pcb", group: "Editors", label: "PCB grid", words: "grid snap unit mm mil inch spacing default" },
  { id: "power", group: "Editors", label: "Power ports", words: "power port gnd ground rail vcc agnd analog chassis net flag schematic" },
  { id: "model", group: "Assistant", label: "Model provider", words: "ai model provider api" },
  { id: "ai-tools", group: "Assistant", label: "AI tools", words: "ai agent claude code codex opencode gemini kimi cli mcp connect sign in login session terminal headless" },
];

const CTX_LABEL: Record<DocContext, string> = { home: "Project", schematic: "Schematic", pcb: "PCB", "3d": "3D", file: "File" };
/** Generated per-item actions (recent projects, library parts, documents, projects) are not rebindable. */
const rebindable = (a: Action) => !/^(recent|part|open|proj)-/.test(a.id);
const showKeys = (ks: readonly string[]) => (ks.length ? ks.map((k) => formatShortcut(k)).join(" / ") : "—");

export interface SettingsProps {
  actions: readonly Action[];
  theme: "dark" | "light";
  onTheme: () => void;
  /** Opens the model provider settings (their own dialog). */
  onModel: () => void;
  onClose: () => void;
}

export function SettingsDialog({ actions, theme, onTheme, onModel, onClose }: SettingsProps) {
  const [query, setQuery] = useState("");
  const [page, setPage] = useState<Page>("keys");
  const capturing = useRef(false);
  // Escape closes the dialog, except while a key is being recorded (then it cancels the recording).
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || capturing.current) return;
      e.preventDefault();
      e.stopPropagation();
      close.current();
    };
    window.addEventListener("keydown", h, true);
    return () => window.removeEventListener("keydown", h, true);
  }, []);

  const q = query.trim().toLowerCase();
  const nodes = TREE.filter((n) => !q || `${n.group} ${n.label} ${n.words}`.toLowerCase().includes(q));
  const groups = [...new Set(nodes.map((n) => n.group))];
  const shown = nodes.some((n) => n.id === page) ? page : (nodes[0]?.id ?? page);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal settings" role="dialog" aria-modal="true" aria-label="Settings" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>Settings</h2>
          <button type="button" onClick={onClose}>Close</button>
        </div>
        <div className="settings-body">
          <nav className="settings-nav" aria-label="Settings pages">
            <input type="search" aria-label="Search settings" placeholder="Search settings" value={query} onChange={(e) => setQuery(e.target.value)} />
            <ul role="tree" aria-label="Settings tree">
              {groups.map((g) => (
                <li key={g} role="treeitem" aria-expanded="true" aria-selected="false">
                  <span className="settings-group">{g}</span>
                  <ul role="group">
                    {nodes
                      .filter((n) => n.group === g)
                      .map((n) => (
                        <li key={n.id} role="treeitem" aria-selected={n.id === shown}>
                          <button type="button" className={n.id === shown ? "active" : undefined} onClick={() => setPage(n.id)}>
                            {n.label}
                          </button>
                        </li>
                      ))}
                  </ul>
                </li>
              ))}
            </ul>
            {nodes.length === 0 && <p className="muted">No setting matches</p>}
          </nav>
          <section className="settings-page" aria-label={TREE.find((n) => n.id === shown)?.label}>
            {shown === "keys" && <Hotkeys actions={actions} capturing={capturing} />}
            {shown === "theme" && (
              <>
                <h3>Theme</h3>
                <label className="inline">
                  <input type="checkbox" checked={theme === "light"} onChange={onTheme} /> Light theme
                </label>
              </>
            )}
            {shown === "colours" && <Colours />}
            {(shown === "grid-schematic" || shown === "grid-pcb") && <GridPage kind={shown === "grid-pcb" ? "pcb" : "schematic"} />}
            {shown === "power" && <PowerPortsPage />}
            {shown === "ai-tools" && <AiToolsSettings />}
            {shown === "model" && (
              <>
                <h3>Model provider</h3>
                <p className="muted">Providers, models and API keys have their own dialog.</p>
                <button type="button" onClick={onModel}>Open model settings…</button>
              </>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}

function Hotkeys({ actions, capturing }: { actions: readonly Action[]; capturing: MutableRefObject<boolean> }) {
  const km = useKeymap();
  const [filter, setFilter] = useState("");
  const [keyFilter, setKeyFilter] = useState<string | null>(null);
  /** "search": the next key press fills the key filter; an action id: the next key press rebinds it. */
  const [recording, setRecording] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const rows = useMemo(() => {
    const seen = new Set<string>();
    return actions.filter((a) => {
      if (!rebindable(a) || seen.has(a.id)) return false;
      seen.add(a.id);
      return true;
    });
  }, [actions]);

  useEffect(() => {
    capturing.current = recording !== null;
    if (!recording) return;
    const h = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecording(null);
        setMessage({ ok: true, text: "Recording cancelled" });
        return;
      }
      const spec = specOf(e);
      if (!spec) return;
      setRecording(null);
      if (recording === "search") {
        setKeyFilter(normalize(spec));
        return;
      }
      const a = rows.find((r) => r.id === recording);
      if (!a) return;
      const why = bind(actions, a, spec);
      const shown = formatShortcut(normalize(spec));
      setMessage(why ? { ok: false, text: `${shown} was not bound to "${a.label}": ${why}.` } : { ok: true, text: `"${a.label}" is now ${shown}.` });
    };
    window.addEventListener("keydown", h, true);
    return () => {
      window.removeEventListener("keydown", h, true);
      capturing.current = false;
    };
  }, [recording, rows, actions, capturing]);

  const f = filter.trim().toLowerCase();
  const list = rows.filter((a) => {
    const ks = effectiveKeys(a);
    if (keyFilter && !ks.some((k) => normalize(k) === keyFilter)) return false;
    return !f || a.label.toLowerCase().includes(f) || a.id.includes(f) || ks.some((k) => k.toLowerCase().includes(f) || formatShortcut(k).toLowerCase().includes(f));
  });
  const changed = Object.keys(km.user).length;

  return (
    <>
      <h3>Hotkeys</h3>
      <div className="settings-row">
        <label className="inline">
          Preset
          <select aria-label="Key map preset" value={km.preset} onChange={(e) => setPreset(e.target.value as PresetId)}>
            {PRESETS.map((p) => (
              <option key={p.id} value={p.id}>{p.label}</option>
            ))}
          </select>
        </label>
        <button type="button" disabled={changed === 0} title={changed === 0 ? "No action has been rebound" : "Drop every rebinding; the preset stays"} onClick={() => (resetAll(), setMessage({ ok: true, text: "All rebindings removed" }))}>
          Reset all rebindings
        </button>
      </div>
      <p className="muted hint">{PRESETS.find((p) => p.id === km.preset)?.note} Editor keys (tool digits, R, W, Delete…) belong to the editors and cannot be bound to actions there.</p>
      <div className="settings-row">
        <input type="search" aria-label="Search hotkeys by command or key" placeholder="Search by command or key" value={filter} onChange={(e) => setFilter(e.target.value)} />
        <button type="button" aria-pressed={recording === "search"} onClick={() => setRecording(recording === "search" ? null : "search")}>
          {recording === "search" ? "Press a key…" : "Find by key press"}
        </button>
        {keyFilter && (
          <button type="button" aria-label={`Clear the key filter ${formatShortcut(keyFilter)}`} onClick={() => setKeyFilter(null)}>
            Key: {formatShortcut(keyFilter)} ×
          </button>
        )}
      </div>
      {message && (
        <p role={message.ok ? "status" : "alert"} className={message.ok ? "muted" : "error"}>
          {message.text}
        </p>
      )}
      <table className="settings-table" aria-label="Hotkeys">
        <thead>
          <tr>
            <th>Command</th>
            <th>Where</th>
            <th>Keys</th>
            <th>Change</th>
          </tr>
        </thead>
        <tbody>
          {list.map((a) => {
            const ks = effectiveKeys(a);
            const own = km.user[a.id] !== undefined;
            return (
              <tr key={a.id} data-action={a.id}>
                <td>{a.label}</td>
                <td className="muted">{a.docs ? a.docs.map((c) => CTX_LABEL[c]).join(", ") : "Everywhere"}</td>
                <td>
                  <kbd>{showKeys(ks)}</kbd>
                  {own && <span className="muted" title={`Preset: ${showKeys(presetKeys(a))}`}> (changed)</span>}
                </td>
                <td className="settings-actions">
                  <button type="button" aria-label={`Change keys of ${a.label}`} aria-pressed={recording === a.id} onClick={() => (setMessage(null), setRecording(recording === a.id ? null : a.id))}>
                    {recording === a.id ? "Press keys…" : "Change"}
                  </button>
                  <button type="button" aria-label={`Remove keys of ${a.label}`} disabled={ks.length === 0} title={ks.length === 0 ? "No key is bound" : undefined} onClick={() => clearAction(a.id)}>
                    Remove
                  </button>
                  <button type="button" aria-label={`Reset keys of ${a.label}`} disabled={!own} title={!own ? "Already the preset's keys" : undefined} onClick={() => resetAction(a.id)}>
                    Reset
                  </button>
                </td>
              </tr>
            );
          })}
          {list.length === 0 && (
            <tr>
              <td colSpan={4} className="muted">No action matches{keyFilter ? ` the key ${formatShortcut(keyFilter)}` : ""}</td>
            </tr>
          )}
        </tbody>
      </table>
    </>
  );
}

/** Current value of a token on the document (the override when one is set, else the theme's). */
const tokenValue = (token: string) => (typeof document === "undefined" ? "" : getComputedStyle(document.documentElement).getPropertyValue(token).trim());
const toHex = (c: string) => (/^#[0-9a-f]{6}$/i.test(c) ? c.toLowerCase() : "#888888");

function Colours() {
  const v = useColours();
  const changed = v.preset !== "theme" || Object.keys(v.user).length > 0;
  return (
    <>
      <h3>Canvas colours</h3>
      <div className="settings-row">
        <label className="inline">
          Preset
          <select aria-label="Canvas colour preset" value={v.preset} onChange={(e) => setColourPreset(e.target.value as ColourPreset)}>
            {COLOUR_PRESETS.map((p) => (
              <option key={p.id} value={p.id}>{p.label}</option>
            ))}
          </select>
        </label>
        <button type="button" disabled={!changed} title={changed ? "Back to the theme's colours" : "Already the theme's colours"} onClick={resetColours}>
          Reset all colours
        </button>
      </div>
      <p className="muted hint">Copper layer colours are set per layer in the Layers panel.</p>
      <table className="settings-table" aria-label="Canvas colours">
        <thead>
          <tr>
            <th>Object</th>
            <th>Editor</th>
            <th>Colour</th>
            <th>Reset</th>
          </tr>
        </thead>
        <tbody>
          {CANVAS_OBJECTS.map((o) => {
            const own = v.user[o.token] !== undefined;
            return (
              <tr key={o.token} data-token={o.token}>
                <td>{o.label}</td>
                <td className="muted">{o.editor}</td>
                <td>
                  <input type="color" aria-label={`${o.label} colour`} value={toHex(overrideOf(o.token, v) ?? tokenValue(o.token))} onChange={(e) => setColour(o.token, e.target.value)} />
                </td>
                <td>
                  <button type="button" aria-label={`Reset ${o.label} colour`} disabled={!own} title={!own ? "Not changed" : undefined} onClick={() => resetColour(o.token)}>
                    Reset
                  </button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </>
  );
}

function GridPage({ kind }: { kind: GridKind }) {
  const name = kind === "pcb" ? "PCB" : "Schematic";
  return (
    <>
      <h3>{name} grid</h3>
      <p className="muted hint">Grid spacing, unit, visibility and snap of the {name} editor. The editor opens with these, and its toolbar shows the same values.</p>
      <GridControls kind={kind} />
      <div>
        <button type="button" onClick={() => updateGrid(kind, GRID_KINDS[kind].defaults)}>Restore EAK defaults</button>
      </div>
    </>
  );
}

/** Settings › Power ports: the entries of the schematic Power port split button (slice B4). */
function PowerPortsPage() {
  const list = usePowerPorts();
  const edit = (i: number, patch: Partial<{ name: string; style: PowerStyle }>) => setPowerPorts(list.map((p, k) => (k === i ? { ...p, ...patch } : p)));
  return (
    <>
      <h3>Power ports</h3>
      <p className="muted hint">
        Each entry is one item of the schematic toolbar&apos;s Power port button and of the Place menu. Placing it adds a power port
        (AddNetLabel, kind power) with this net name, drawn with the chosen symbol. Analog ground uses the ground symbol: the EAK-IR has
        ground, supply rail and chassis styles.
      </p>
      <table className="settings-table" aria-label="Power ports">
        <thead>
          <tr>
            <th>Net name</th>
            <th>Symbol</th>
            <th>Remove</th>
          </tr>
        </thead>
        <tbody>
          {list.map((p, i) => {
            const bad = powerPortProblem(p);
            return (
              <tr key={i}>
                <td>
                  <input aria-label={`Power port ${i + 1} net name`} value={p.name} aria-invalid={bad ? true : undefined} title={bad ?? undefined} onChange={(e) => edit(i, { name: e.target.value })} />
                </td>
                <td>
                  <select aria-label={`Power port ${i + 1} symbol`} value={p.style} onChange={(e) => edit(i, { style: e.target.value as PowerStyle })}>
                    {POWER_STYLES.map((s) => (
                      <option key={s.id} value={s.id}>{s.label}</option>
                    ))}
                  </select>
                </td>
                <td>
                  <button type="button" aria-label={`Remove power port ${i + 1}`} onClick={() => setPowerPorts(list.filter((_, k) => k !== i))}>
                    Remove
                  </button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <div className="settings-row">
        <button type="button" onClick={() => setPowerPorts([...list, { name: "VCC", style: "rail" }])}>Add power port</button>
        <button type="button" onClick={resetPowerPorts}>Restore EAK defaults</button>
      </div>
    </>
  );
}
