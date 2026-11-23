// PCB Layer Manager: the copper layers of the engine's stackup grouped top / inner / bottom, and
// the mechanical layer (board outline), each with active-layer, show / hide, lock and colour
// controls and colour presets (all display state), and the
// stackup itself (kind, copper weight, dielectric), edited only through the SetLayerCount /
// SetStackup engine commands.
import { layerCountOptions, layerKey, layerName, stackLayers } from "../layers";
import { COLOUR_PRESETS, type LayerView } from "../layerView";
import type { Command, CopperLayer, Snapshot, Stackup, StackupLayer } from "../types";

interface Props {
  snapshot: Snapshot;
  view: LayerView;
  unrouted: number | null;
  edit: (c: Command | Command[]) => Promise<void>;
  busy: boolean;
}

/** A full engine Stackup with one layer replaced; fields the engine did not report keep defaults. */
export function withLayer(s: Snapshot, layer: CopperLayer, patch: Partial<StackupLayer>): Stackup | null {
  const st = s.project.pcb.board.stackup;
  if (!st) return null;
  return { ...st, layers: st.layers.map((l) => (layerKey(l.layer) === layerKey(layer) ? { ...l, ...patch } : l)) };
}

/** Copper group of a layer for the panel's headings. */
export const layerGroup = (l: CopperLayer): "Top" | "Inner" | "Bottom" => (l === "top" ? "Top" : l === "bottom" ? "Bottom" : "Inner");

export function LayerManager({ snapshot, view, unrouted, edit, busy }: Props) {
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
              <tr><th>Layer</th><th>Type</th><th title="Read-only: from the engine stackup rules">Cu oz</th><th title="Read-only: from the engine stackup rules">Gap µm</th><th title="Read-only: from the engine stackup rules">εr</th></tr>
            </thead>
            <tbody>
              {entries.map((l, i) => (
                <tr key={layerKey(l.layer)}>
                  <td>{layerName(l.layer)}</td>
                  <td>
                    <select
                      aria-label={`Type ${layerName(l.layer)}`}
                      value={l.kind ?? "signal"}
                      disabled={busy}
                      onChange={(e) => {
                        const next = withLayer(snapshot, l.layer, { kind: e.target.value as "signal" | "plane" });
                        if (next) void edit({ type: "SetStackup", stackup: next });
                      }}
                    >
                      <option value="signal">signal</option>
                      <option value="plane">plane</option>
                    </select>
                  </td>
                  <td>{l.copper_oz}</td>
                  <td>{i === entries.length - 1 ? "—" : l.dielectric_after_um}</td>
                  <td>{i === entries.length - 1 ? "—" : l.er}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="muted hint">
            Board thickness {(st.board_thickness / 1e6).toFixed(2)} mm · dielectric {st.dielectric}. Edit counts and layer types here; copper weight, gaps, εr and
            thickness are read-only: they come from the engine's stackup rules. Solder mask, silkscreen and paste layers are not in the engine stackup yet, so they are not listed.
          </p>
        </>
      )}
    </div>
  );
}
