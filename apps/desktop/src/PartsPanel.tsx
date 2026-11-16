// Parts search: the EAK library, records stored with the project, and the Distributor OpenAPI
// catalogue (credentials, browsing and paging are in CataloguePanel). Import is one engine
// command carrying the revision this panel last saw. When Distributor cannot be asked the panel says
// why; it never fills the list itself.
import { useState } from "react";
import { api, describeError } from "./api";
import { CataloguePanel } from "./CataloguePanel";
import { setDraggedComponent, EAK_COMPONENT_MIME } from "./dragState";
import { LibraryBrowser } from "./LibraryBrowser";
import type { PartsSearch, SourcingRecord } from "./types";

interface Props {
  revision: number;
  /** Called after an accepted import so the workspace re-reads the engine snapshot. */
  onImported: (message: string) => Promise<void>;
}

function price(r: SourcingRecord): string {
  const b = r.price_breaks[0];
  return b ? `$${b.unit_price} @ ${b.min_quantity}+` : "—";
}

function Row({ r, onImport, busy }: { r: SourcingRecord; onImport?: (distributor: string) => void; busy: boolean }) {
  const distributor = r.distributor_part;
  return (
    <tr>
      <td>{distributor ?? "—"}</td>
      <td>{r.mpn ?? "—"}</td>
      <td>{r.manufacturer ?? "—"}</td>
      <td>{r.package ?? "—"}</td>
      <td>{r.stock === null ? "—" : r.stock.toLocaleString("en-US")}</td>
      <td>{price(r)}</td>
      <td>
        {r.datasheet_url ? (
          <a href={r.datasheet_url} target="_blank" rel="noreferrer">
            datasheet
          </a>
        ) : (
          "—"
        )}
      </td>
      <td>
        {onImport && distributor && (
          <button type="button" disabled={busy} onClick={() => onImport(distributor)} aria-label={`Import ${distributor}`}>
            Import
          </button>
        )}
      </td>
    </tr>
  );
}

export function PartsPanel({ revision, onImported }: Props) {
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<PartsSearch | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    try {
      await f();
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  };

  const search = () =>
    run(async () => {
      setNote(null);
      setResult(await api.partsSearch(query));
    });

  const importPart = (distributor: string) =>
    run(async () => {
      const r = await api.partsImport(revision, distributor);
      if (!r.response.accepted) {
        setNote(`Import rejected: ${r.response.error ? describeError(r.response.error) : "unknown error"}`);
        return;
      }
      setNote(`Imported ${distributor} as ${r.component_id}. ${r.notes.join(" ")}`);
      await onImported(`Imported ${distributor} into the project library (revision ${r.response.revision_after})`);
    });

  const [catalogueOpen, setCatalogueOpen] = useState(false);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const live = result?.live;
  return (
    <section className="parts" aria-label="Parts search">
      <h2>Parts</h2>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void search();
        }}
      >
        <label>
          Distributor part number or keyword
          <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="C25804" />
        </label>
        <button type="submit" disabled={busy || query.trim() === ""}>
          Search
        </button>
      </form>
      <p className="muted">
        Only the text you type is sent to Distributor, signed with your own API credentials. Stock and prices are a snapshot, not a quote.
      </p>
      {note && <p role="status">{note}</p>}
      {live && live.status !== "ok" && (
        <p role="alert" className="error">
          Distributor: {live.message}
        </p>
      )}
      {live && live.records.length > 0 && (
        <table aria-label="Distributor results">
          <thead>
            <tr>
              <th>Distributor #</th>
              <th>MPN</th>
              <th>Maker</th>
              <th>Package</th>
              <th>Stock</th>
              <th>Price</th>
              <th>Datasheet</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {live.records.map((r) => (
              <Row key={r.raw_sha256} r={r} onImport={importPart} busy={busy} />
            ))}
          </tbody>
        </table>
      )}
      {result && result.stored.length > 0 && (
        <>
          <h3>Stored with this project</h3>
          <table aria-label="Stored records">
            <tbody>
              {result.stored.map((r) => (
                <Row key={r.raw_sha256} r={r} busy={busy} />
              ))}
            </tbody>
          </table>
        </>
      )}
      <details open={catalogueOpen} onToggle={(e) => setCatalogueOpen(e.currentTarget.open)}>
        <summary>Distributor catalogue: credentials, categories, manufacturers, paged search</summary>
        {catalogueOpen && <CataloguePanel busy={busy} onImport={(distributor) => void importPart(distributor)} />}
      </details>
      <details open={libraryOpen} onToggle={(e) => setLibraryOpen(e.currentTarget.open)}>
        <summary>Component library: built-in, project, personal and open-source scopes</summary>
        {libraryOpen && <LibraryBrowser />}
      </details>
      {result && result.library.length > 0 && (
        <>
          <h3>EAK library</h3>
          <ul aria-label="Library matches">
            {result.library.map((h) => (
              <li
                key={h.id}
                draggable={true}
                onDragStart={(e) => {
                  setDraggedComponent({ id: h.id, name: h.name });
                  e.dataTransfer.setData(EAK_COMPONENT_MIME, JSON.stringify({ id: h.id, name: h.name }));
                  e.dataTransfer.setData("text/plain", h.id);
                  e.dataTransfer.effectAllowed = "copy";
                }}
                onDragEnd={() => {
                  setDraggedComponent(null);
                }}
                style={{ cursor: "grab" }}
                title="Drag onto schematic to place"
              >
                {h.id}{" "}
                <span className="muted">
                  {h.name}
                  {h.distributor ? ` · ${h.distributor}` : ""}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
