// Library browser across every scope (ADR-0033): built-in, project, personal and imported
// open-source libraries, with search, source/category/manufacturer/package/tag filters,
// favourites, tags, recents, symbol/footprint previews, pin-to-pad inspection, datasheet, 3D
// model info and the supplier snapshot stored with the project. The UI keeps no library model:
// every list comes from the backend. A supplier snapshot is never shown as suitability evidence.
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { setDraggedComponent, EAK_COMPONENT_MIME } from "./dragState";
import { KicadImport } from "./KicadImport";
import { FootprintPreview, SymbolPreview } from "./shell/LibraryPanel";
import type { Facet, LibraryBrowse, LibraryBrowseQuery, LibraryEntry2 } from "./types";

const keyOf = (e: LibraryEntry2) => `${e.scope_label}/${e.id}`;

function FacetSelect({
  label,
  value,
  facets,
  onChange,
}: {
  label: string;
  value: string;
  facets: Facet[];
  onChange: (v: string) => void;
}) {
  return (
    <label>
      {label}
      <select value={value} onChange={(e) => onChange(e.target.value)}>
        <option value="">any</option>
        {facets.map((f) => (
          <option key={f.value} value={f.value}>
            {f.value} ({f.count})
          </option>
        ))}
      </select>
    </label>
  );
}

function Detail({ e }: { e: LibraryEntry2 }) {
  const pins = e.def.symbol.pins;
  const model = e.def.model3d;
  return (
    <section aria-label={`Details of ${e.id}`}>
      <h4>
        {e.def.name} <span className="muted">({e.scope_label})</span>
      </h4>
      <p className="muted">{e.def.provenance}</p>
      <div style={{ display: "flex", gap: 12 }}>
        <SymbolPreview def={e.def} />
        <FootprintPreview fp={e.def.footprint} />
      </div>
      <table aria-label="Pin to pad mapping">
        <thead>
          <tr>
            <th>Pin</th>
            <th>Name</th>
            <th>Type</th>
            <th>Pads</th>
          </tr>
        </thead>
        <tbody>
          {pins.map((p) => (
            <tr key={p.number}>
              <td>{p.number}</td>
              <td>{p.name}</td>
              <td>{p.electrical_type}</td>
              <td>{(e.def.pin_map[p.number] ?? []).join(", ") || "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <ul>
        <li>
          Datasheet:{" "}
          {e.def.datasheet ? (
            <a href={e.def.datasheet} target="_blank" rel="noreferrer">
              open
            </a>
          ) : (
            "none recorded"
          )}
        </li>
        <li>
          3D model:{" "}
          {model?.model_ref
            ? `${model.model_ref.format.toUpperCase()} (${model.model_ref.quality ?? "quality not declared"})`
            : `generated ${model?.kind ?? "box"} body (no model file)`}
        </li>
        <li>
          Ratings:{" "}
          {e.def.ratings.length === 0
            ? "none recorded"
            : e.def.ratings.map((r) => `${r.name} ${r.value} ${r.unit}${r.verified ? "" : " (unverified)"}`).join("; ")}
        </li>
        <li>
          Supplier snapshot:{" "}
          {e.supplier
            ? `${e.supplier.distributor}, stock ${e.supplier.stock ?? "not stated"}, retrieved ${new Date(e.supplier.as_of_unix_s * 1000).toLocaleString()} — a snapshot, not a suitability check`
            : "none stored with this project"}
        </li>
      </ul>
    </section>
  );
}

export function LibraryBrowser() {
  const [q, setQ] = useState<LibraryBrowseQuery>({});
  const [text, setText] = useState("");
  const [data, setData] = useState<LibraryBrowse | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [tagText, setTagText] = useState("");
  const [folder, setFolder] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async (query: LibraryBrowseQuery) => {
    setBusy(true);
    try {
      setData(await api.libraryBrowse(query));
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void load(q);
  }, [q, load]);

  const act = async (f: () => Promise<unknown>, done?: string) => {
    setNote(null);
    setBusy(true);
    try {
      await f();
      if (done) setNote(done);
      await load(q);
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  };

  const set = (patch: Partial<LibraryBrowseQuery>) => setQ({ ...q, ...patch });
  const sel = data?.entries.find((e) => keyOf(e) === selected) ?? null;
  const f = data?.facets;

  return (
    <section aria-label="Library browser">
      <h3>Library</h3>
      <KicadImport />
      <form
        onSubmit={(e) => {
          e.preventDefault();
          set({ query: text });
        }}
      >
        <label>
          Search library
          <input value={text} onChange={(e) => setText(e.target.value)} placeholder="id, name, MPN, Distributor #, package" />
        </label>
        <button type="submit" disabled={busy}>
          Search library
        </button>
        {f && (
          <>
            <FacetSelect label="Source" value={q.source ?? ""} facets={f.sources} onChange={(v) => set({ source: v })} />
            <FacetSelect label="Category" value={q.category ?? ""} facets={f.categories} onChange={(v) => set({ category: v })} />
            <FacetSelect label="Manufacturer" value={q.manufacturer ?? ""} facets={f.manufacturers} onChange={(v) => set({ manufacturer: v })} />
            <FacetSelect label="Package" value={q.package ?? ""} facets={f.packages} onChange={(v) => set({ package: v })} />
            <FacetSelect label="Tag" value={q.tag ?? ""} facets={f.tags} onChange={(v) => set({ tag: v })} />
          </>
        )}
        <label>
          <input type="checkbox" checked={!!q.favourites_only} onChange={(e) => set({ favourites_only: e.target.checked })} /> Favourites
        </label>
        <label>
          <input type="checkbox" checked={!!q.recent_only} onChange={(e) => set({ recent_only: e.target.checked })} /> Recently used
        </label>
      </form>
      {note && <p role="status">{note}</p>}
      {data && data.problems.length > 0 && (
        <p role="alert" className="error">
          {data.problems.length} library file(s) could not be read: {data.problems[0]}
        </p>
      )}
      {data && (
        <p className="muted">
          {data.total} part(s). Sources: {data.facets.sources.map((s) => `${s.value} ${s.count}`).join(", ")}.
        </p>
      )}
      {data && (
        <table aria-label="Library results">
          <thead>
            <tr>
              <th />
              <th>Part</th>
              <th>Source</th>
              <th>Category</th>
              <th>Maker / MPN</th>
              <th>Package</th>
              <th>Engineering</th>
              <th>Supplier</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {data.entries.map((e) => (
              <tr key={keyOf(e)}>
                <td>
                  <button
                    type="button"
                    aria-pressed={e.favourite}
                    aria-label={`${e.favourite ? "Unfavourite" : "Favourite"} ${e.id}`}
                    onClick={() => void act(() => api.libraryMeta(e.id, { favourite: !e.favourite }))}
                  >
                    {e.favourite ? "★" : "☆"}
                  </button>
                </td>
                <td>
                  <button
                    type="button"
                    onClick={() => {
                      setSelected(keyOf(e));
                      setTagText(e.tags.join(", "));
                      void api.libraryMeta(e.id, { touch: true }).catch(() => undefined);
                    }}
                    draggable={true}
                    onDragStart={(ev) => {
                      setDraggedComponent({ id: e.id, name: e.def.name, category: e.def.category, footprint: e.def.footprint?.name });
                      ev.dataTransfer.setData(EAK_COMPONENT_MIME, JSON.stringify({ id: e.id, name: e.def.name, category: e.def.category, footprint: e.def.footprint?.name }));
                      ev.dataTransfer.setData("text/plain", e.id);
                      ev.dataTransfer.effectAllowed = "copy";
                    }}
                    onDragEnd={() => {
                      setDraggedComponent(null);
                    }}
                    title="Click to inspect · Drag onto schematic to place"
                  >
                    {e.id}
                  </button>{" "}
                  <span className="muted">{e.def.name}</span>
                  {e.tags.length > 0 && <span className="muted"> · {e.tags.join(", ")}</span>}
                </td>
                <td>{e.scope_label}</td>
                <td>{e.def.category}</td>
                <td>
                  {e.def.manufacturer ?? "—"} / {e.def.mpn ?? "—"}
                </td>
                <td>{e.def.footprint.name}</td>
                <td>
                  {e.unverified_ratings > 0 ? `${e.unverified_ratings} rating(s) unverified` : "ratings verified or none"}
                  {e.has_datasheet ? "" : " · no datasheet"}
                </td>
                <td>{e.supplier ? `${e.supplier.distributor} · stock ${e.supplier.stock ?? "?"}` : "—"}</td>
                <td>
                  {e.scope.kind !== "personal" && (
                    <button type="button" disabled={busy} onClick={() => void act(() => api.libraryPersonalSave(e.id), `Saved ${e.id} to the personal library`)}>
                      Save to personal
                    </button>
                  )}
                  {e.scope.kind === "personal" && (
                    <button type="button" disabled={busy} onClick={() => void act(() => api.libraryPersonalRemove(e.id), `Removed ${e.id} from the personal library`)}>
                      Remove
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {sel && (
        <>
          <form
            onSubmit={(ev) => {
              ev.preventDefault();
              void act(() =>
                api.libraryMeta(sel.id, {
                  tags: tagText
                    .split(",")
                    .map((t) => t.trim())
                    .filter((t) => t !== ""),
                }),
              );
            }}
          >
            <label>
              Tags for {sel.id}
              <input value={tagText} onChange={(e) => setTagText(e.target.value)} placeholder="comma separated" />
            </label>
            <button type="submit" disabled={busy}>
              Save tags
            </button>
          </form>
          <Detail e={sel} />
        </>
      )}
      <details>
        <summary>Open-source libraries</summary>
        {data && data.imported.length > 0 && (
          <ul aria-label="Imported libraries">
            {data.imported.map((l) => (
              <li key={l.manifest.name}>
                {l.manifest.name} — {l.components} part(s), licence {l.manifest.licence},{" "}
                <a href={l.manifest.source_url} target="_blank" rel="noreferrer">
                  source
                </a>{" "}
                <button type="button" disabled={busy} onClick={() => void act(() => api.libraryImportedRemove(l.manifest.name), `Removed ${l.manifest.name}`)}>
                  Remove {l.manifest.name}
                </button>
              </li>
            ))}
          </ul>
        )}
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void act(async () => {
              const r = await api.libraryImportOpenSource(folder);
              setNote(
                `Imported ${r.accepted.length} part(s) from ${r.library.manifest.name}` +
                  (r.rejected.length > 0 ? `; ${r.rejected.length} file(s) rejected: ${r.rejected[0]}` : ""),
              );
            });
          }}
        >
          <label>
            Library folder (absolute path)
            <input value={folder} onChange={(e) => setFolder(e.target.value)} placeholder="/path/to/library" />
          </label>
          <button type="submit" disabled={busy || folder.trim() === ""}>
            Import open-source library
          </button>
        </form>
        <p className="muted">
          The folder needs a manifest.json with the library&apos;s name, licence and source URL, plus one component-definition
          JSON file per part. Without a licence nothing is imported; imported parts are marked as not verified by EAK.
        </p>
      </details>
    </section>
  );
}
