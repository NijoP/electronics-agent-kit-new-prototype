// PCB Layer Manager: the copper layers of the engine's stackup grouped top / inner / bottom, and
// the mechanical layer (board outline), each with active-layer, show / hide, lock and colour
// controls and colour presets (all display state); the stackup itself (kind, copper weight,
// dielectric gaps, εr, finished thickness, material) and each plane layer's net, edited only through
// the SetLayerCount / SetStackup / AddZone / EditZone engine commands; and the zones with the
// engine's fill of each and StitchZones per net (slice E5).
import { useEffect, useState } from "react";
import { layerCountOptions, layerKey, layerName, stackLayers } from "../layers";
import { COLOUR_PRESETS, type LayerView } from "../layerView";
import type { Command, CopperLayer, FillView, Snapshot, Stackup, StackupLayer } from "../types";
import { netNames } from "./panels";
import { assignPlaneNet, buildUpUm, planeNet, stitchBlocked, withThickness, zoneNets } from "./planes";

interface Props {
  snapshot: Snapshot;
  view: LayerView;
  unrouted: number | null;
  edit: (c: Command | Command[]) => Promise<void>;
  busy: boolean;
  /** Engine zone fills of this revision; null while they are computed, absent where unknown. */
  fills?: FillView[] | null;
}

/** A full engine Stackup with one layer replaced; fields the engine did not report keep defaults. */
export function withLayer(s: Snapshot, layer: CopperLayer, patch: Partial<StackupLayer>): Stackup | null {
  const st = s.project.pcb.board.stackup;
  if (!st) return null;
  return { ...st, layers: st.layers.map((l) => (layerKey(l.layer) === layerKey(layer) ? { ...l, ...patch } : l)) };
}

/** Copper group of a layer for the panel's headings. */
export const layerGroup = (l: CopperLayer): "Top" | "Inner" | "Bottom" => (l === "top" ? "Top" : l === "bottom" ? "Bottom" : "Inner");

export function LayerManager({ snapshot, view, unrouted, edit, busy, fills = null }: Props) {
  const outline = snapshot.project.pcb.board.outline;
  const zones = Object.values(snapshot.project.pcb.zones);
  // Plane-net choices: every net of the design, and the nets the zones already carry.
  const nets = [...new Set([...netNames(snapshot), ...zones.map((z) => z.net)])].sort();
  const stack = stackLayers(snapshot.project.pcb.board.stackup);
  const st = snapshot.project.pcb.board.stackup;
  const entries = st?.layers ?? [];
  const order = stack.map((s) => s.layer);
  return (
    <div className="panel-body layer-manager">
      <h3>Copper layers</h3>
      <label className="row">
        <span className="muted">Layer count</span>
        <select
          aria-label="Layer count"
          value={stack.length}
          disabled={busy}
          onChange={(e) => void edit({ type: "SetLayerCount", count: Number(e.target.value), board_thickness: null })}
        >
          {layerCountOptions(stack.length).map((n) => (
            <option key={n} value={n}>{n} layers</option>
          ))}
        </select>
      </label>
      <ul className="layer-list" aria-label="Layers">
        {stack.map((s, i) => {
          const k = layerKey(s.layer);
          const name = layerName(s.layer);
          const active = view.active === k;
          const locked = view.locked.has(k);
          const group = layerGroup(s.layer);
          const head = i === 0 || layerGroup(stack[i - 1].layer) !== group;
          return [
            head && (
              <li key={`group-${group}`} role="presentation" className="layer-group">
                {group}
              </li>
            ),
            <li key={k} className={`layer-row${active ? " active" : ""}${locked ? " locked" : ""}`} data-layer={k} aria-current={active ? "true" : undefined}>
              <span className="layer-marker" aria-hidden="true" title={active ? "Active layer" : undefined}>{active ? "✎" : ""}</span>
              <input type="radio" name="active-layer" aria-label={`Active layer ${name}`} checked={active} onChange={() => view.setActive(s.layer)} title="Active layer: new tracks, zones and picks use it" />
              <input type="checkbox" aria-label={`Show ${name}`} checked={!view.hidden.has(k)} onChange={() => view.toggle(s.layer)} title="Show / hide" />
              <input type="checkbox" className="lock" aria-label={`Lock ${name}`} checked={locked} onChange={() => view.toggleLock(s.layer)} title="Lock: objects on this layer cannot be selected or edited" />
              <input type="color" aria-label={`Colour ${name}`} value={view.colourOf(s.layer)} onChange={(e) => view.setColour(s.layer, e.target.value)} title="Layer colour (display only)" />
              <span className="grow">{name}</span>
              {locked && <span className="tag plain" title="Locked layer">locked</span>}
              <span className="tag plain">{s.kind}</span>
            </li>,
          ];
        })}
      </ul>
      <h3>Mechanical</h3>
      <ul className="layer-list" aria-label="Mechanical layers">
        <li className={`layer-row${view.locked.has("outline") ? " locked" : ""}`} data-layer="outline">
          <span className="layer-marker" aria-hidden="true" />
          <input type="checkbox" aria-label="Show Board outline" checked={!view.hidden.has("outline")} onChange={() => view.toggle("outline")} title="Show / hide" />
          <input type="checkbox" className="lock" aria-label="Lock Board outline" checked={view.locked.has("outline")} onChange={() => view.toggleLock("outline")} title="Lock: the outline cannot be edited" />
          <span className="grow">Board outline (Edge.Cuts)</span>
        </li>
      </ul>
      <label className="row">
        <span className="muted">Colour preset</span>
        <select aria-label="Colour preset" value="" onChange={(e) => e.target.value && view.applyPreset(e.target.value, order)}>
          <option value="">Apply a preset…</option>
          {COLOUR_PRESETS.map((p) => (
            <option key={p.id} value={p.id}>{p.label}</option>
          ))}
        </select>
      </label>
      <label className="row">
        <input type="checkbox" checked={view.airwires} onChange={(e) => view.setAirwires(e.target.checked)} />
        <span>Show airwires</span>
        <span className="muted">{unrouted === null ? "connectivity unavailable" : `${unrouted} unrouted`}</span>
      </label>

      <h3>Stackup</h3>
      {!st ? (
        <p className="empty-state">This project has no explicit stackup: it is treated as a 2-layer board (top and bottom copper).</p>
      ) : (
        <>
          <table className="bom stackup" aria-label="Stackup">
            <thead>
              <tr>
                <th>Layer</th>
                <th>Type</th>
                <th title="Copper weight, oz/ft²">Cu oz</th>
                <th title="Dielectric thickness below this layer, µm">Gap µm</th>
                <th title="Relative permittivity of the dielectric below this layer">εr</th>
                <th title="Net of the plane layer's copper (its zones)">Plane net</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((l, i) => {
                const name = layerName(l.layer);
                const set = (patch: Partial<StackupLayer>) => {
                  const next = withLayer(snapshot, l.layer, patch);
                  if (next) void edit({ type: "SetStackup", stackup: next });
                };
                const last = i === entries.length - 1;
                const pn = l.kind === "plane" ? planeNet(snapshot, l.layer) : "";
                return (
                  <tr key={layerKey(l.layer)} data-layer={layerKey(l.layer)}>
                    <td>{name}</td>
                    <td>
                      <select aria-label={`Type ${name}`} value={l.kind ?? "signal"} disabled={busy} onChange={(e) => set({ kind: e.target.value as "signal" | "plane" })}>
                        <option value="signal">signal</option>
                        <option value="plane">plane</option>
                      </select>
                    </td>
                    <td>
                      <NumField label={`Copper oz ${name}`} value={l.copper_oz} step={0.5} disabled={busy} onCommit={(v) => set({ copper_oz: v })} />
                    </td>
                    <td>{last ? "—" : <NumField label={`Gap µm ${name}`} value={l.dielectric_after_um} step={10} disabled={busy} onCommit={(v) => set({ dielectric_after_um: Math.round(v) })} />}</td>
                    <td>{last ? "—" : <NumField label={`εr ${name}`} value={l.er} step={0.1} disabled={busy} onCommit={(v) => set({ er: v })} />}</td>
                    <td>
                      {l.kind === "plane" ? (
                        <select
                          aria-label={`Plane net ${name}`}
                          value={pn ?? "__mixed"}
                          disabled={busy || (pn === "" && outline.length < 3)}
                          title={pn === "" && outline.length < 3 ? "Draw a board outline first: a new plane zone covers the outline" : "Zones on this layer move to the net (EditZone); with none, one zone over the board outline is added (AddZone)"}
                          onChange={(e) => {
                            const cmd = e.target.value ? assignPlaneNet(snapshot, l.layer, e.target.value, outline) : null;
                            if (cmd) void edit(cmd);
                          }}
                        >
                          {pn === "" && <option value="">— none —</option>}
                          {pn === null && <option value="__mixed">mixed</option>}
                          {nets.map((n) => (
                            <option key={n} value={n}>{n}</option>
                          ))}
                        </select>
                      ) : (
                        <span className="muted">—</span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <label className="row">
            <span className="muted">Board thickness (mm)</span>
            <NumField
              label="Board thickness mm"
              value={Number((st.board_thickness / 1e6).toFixed(3))}
              step={0.1}
              disabled={busy}
              title="Finished thickness: the dielectric gaps are scaled so the build-up totals it (copper kept); sent as SetStackup"
              onCommit={(v) => void edit({ type: "SetStackup", stackup: withThickness(st, Math.round(v * 1e6)) })}
            />
          </label>
          <label className="row">
            <span className="muted">Dielectric</span>
            <TextField label="Dielectric material" value={st.dielectric} disabled={busy} onCommit={(v) => void edit({ type: "SetStackup", stackup: { ...st, dielectric: v } })} />
          </label>
          <p className="muted hint" data-build-up-um={Math.round(buildUpUm(st))}>
            Build-up {Math.round(buildUpUm(st))} µm against the declared {Math.round(st.board_thickness / 1000)} µm (the engine refuses more than ±10 % apart). Every edit is a SetStackup
            command the engine validates. Solder mask, silkscreen and paste layers are not in the engine stackup yet, so they are not listed.
          </p>
        </>
      )}

      <h3>Zones and fills</h3>
      {zones.length === 0 ? (
        <p className="empty-state">No copper zones. Draw one with the Zone tool, or give a plane layer a net above.</p>
      ) : (
        <>
          <ul className="layer-list zone-list" aria-label="Zones">
            {zones.map((z) => {
              const f = fills?.find((x) => x.zone_id === z.id);
              return (
                <li
                  key={z.id}
                  className="zone-row"
                  data-zone-row={z.id}
                  data-layer={layerKey(z.layer)}
                  data-fill-area={f ? f.filled_area_mm2.toFixed(1) : undefined}
                  data-islands={f ? f.islands : undefined}
                  data-removed={f ? f.removed_islands : undefined}
                >
                  <span className="grow">{`${z.net} · ${layerName(z.layer)}`}</span>
                  <span className="muted">
                    {fills === null
                      ? "fill not computed yet"
                      : f
                        ? `${f.filled_area_mm2.toFixed(1)} mm² · ${f.islands} island(s)${f.removed_islands ? ` · ${f.removed_islands} dead removed` : ""}`
                        : "no fill (zone outline unusable)"}
                  </span>
                </li>
              );
            })}
          </ul>
          <div className="row" role="group" aria-label="Stitch zones">
            {zoneNets(snapshot).map(({ net }) => {
              const why = stitchBlocked(snapshot, net);
              return (
                <button key={net} type="button" disabled={busy || why !== null} title={why ?? `Join the ${net} pours on every layer with stitching vias (StitchZones)`} onClick={() => void edit({ type: "StitchZones", net })}>
                  {`Stitch ${net}`}
                </button>
              );
            })}
          </div>
          <p className="muted hint">
            Fills are computed by the engine (eak-engineering, 0.1 mm raster with thermal reliefs and dead islands removed), the same copper DRC checks; they are derived
            for each revision and never stored.
          </p>
        </>
      )}
    </div>
  );
}

/** A number cell that sends on Enter or blur, and only a changed, finite value. */
function NumField({ label, value, step, disabled, title, onCommit }: { label: string; value: number; step: number; disabled: boolean; title?: string; onCommit: (v: number) => void }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const v = Number(text);
    setText(String(value));
    if (text.trim() !== "" && Number.isFinite(v) && v !== value) onCommit(v);
  };
  return (
    <input
      type="number"
      aria-label={label}
      value={text}
      step={step}
      disabled={disabled}
      title={title}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        else if (e.key === "Escape") setText(String(value));
      }}
    />
  );
}

/** A text field that sends a changed, non-empty value on Enter or blur. */
function TextField({ label, value, disabled, onCommit }: { label: string; value: string; disabled: boolean; onCommit: (v: string) => void }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  const commit = () => {
    const v = text.trim();
    setText(value);
    if (v && v !== value) onCommit(v);
  };
  return (
    <input
      type="text"
      aria-label={label}
      value={text}
      disabled={disabled}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        else if (e.key === "Escape") setText(value);
      }}
    />
  );
}
