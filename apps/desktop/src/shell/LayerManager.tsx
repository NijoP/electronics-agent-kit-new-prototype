// PCB Layer Manager: lists the copper layers of the engine's stackup with active-layer, show/
// hide and colour controls (display state), and the stackup itself (kind, copper weight,
// dielectric) which is edited only through the SetLayerCount / SetStackup engine commands.
import { layerCountOptions, layerKey, layerName, stackLayers } from "../layers";
import type { LayerView } from "../layerView";
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

export function LayerManager({ snapshot, view, unrouted, edit, busy }: Props) {
  const stack = stackLayers(snapshot.project.pcb.board.stackup);
  const st = snapshot.project.pcb.board.stackup;
  const entries = st?.layers ?? [];
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
        {stack.map((s) => {
          const k = layerKey(s.layer);
          const active = view.active === k;
          return (
            <li key={k} className={active ? "layer-row active" : "layer-row"}>
              <input type="radio" name="active-layer" aria-label={`Active layer ${layerName(s.layer)}`} checked={active} onChange={() => view.setActive(s.layer)} title="Active routing layer" />
              <input type="checkbox" aria-label={`Show ${layerName(s.layer)}`} checked={!view.hidden.has(k)} onChange={() => view.toggle(s.layer)} title="Show / hide" />
              <input type="color" aria-label={`Colour ${layerName(s.layer)}`} value={view.colourOf(s.layer)} onChange={(e) => view.setColour(s.layer, e.target.value)} title="Layer colour (display only)" />
              <span className="grow">{layerName(s.layer)}</span>
              <span className="tag plain">{s.kind}</span>
            </li>
          );
        })}
      </ul>
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
