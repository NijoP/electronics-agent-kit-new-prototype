// Parts search: the EAK library, records stored with the project, and the Distributor OpenAPI
// catalogue (credentials, browsing and paging are in CataloguePanel). Import is one engine
// command carrying the revision this panel last saw. When Distributor cannot be asked the panel says
// why; it never fills the list itself. When the build's distributor host is a reserved
// placeholder name, no request can reach a distributor: the live results, live import and the
// whole catalogue are then replaced by one "Distributor not configured" state (slice E1).
import { useEffect, useState } from "react";
import { api, describeError } from "./api";
import { CataloguePanel } from "./CataloguePanel";
import { setDraggedComponent, EAK_COMPONENT_MIME } from "./dragState";
import { LibraryBrowser } from "./LibraryBrowser";
import type { DistributorView, PartsSearch, SourcingRecord } from "./types";

/**
 * True when the distributor host is a reserved `.invalid` name (RFC 6761): it never resolves, so
 * the catalogue, live search results and live import cannot work in this build.
 */
export function distributorUnconfigured(host: string): boolean {
  try {
    const h = new URL(host).hostname.toLowerCase();
    return h === "invalid" || h.endsWith(".invalid");
  } catch {
    return true;
  }
}

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
  // The backend reports the distributor host it would call; null until known (or unreadable, when
  // the catalogue stays available and reports its own errors).
  const [dist, setDist] = useState<DistributorView | null>(null);
  useEffect(() => {
    let live = true;
    api
      .distributorSettings()
      .then((v) => live && setDist(v))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);
  const unconfigured = dist !== null && distributorUnconfigured(dist.host);
  const live = unconfigured ? undefined : result?.live;
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
          {unconfigured ? "Part number or keyword" : "Distributor part number or keyword"}
          <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="C25804" />
        </label>
        <button type="submit" disabled={busy || query.trim() === ""}>
          Search
        </button>
      </form>
      {unconfigured ? (
        <section className="empty-state" role="note" aria-label="Distributor not configured">
          <strong>Distributor not configured</strong>
          <p className="muted">
            This build has no distributor host (its placeholder {dist?.host} is a reserved name that never resolves), so the supplier
            catalogue, live stock and prices and live import are unavailable. Search still finds parts in the EAK library and records stored with this
            project.
          </p>
        </section>
      ) : (
        <p className="muted">
          Only the text you type is sent to Distributor, signed with your own API credentials. Stock and prices are a snapshot, not a quote.
        </p>
      )}
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
      {!unconfigured && (
        <details open={catalogueOpen} onToggle={(e) => setCatalogueOpen(e.currentTarget.open)}>
          <summary>Distributor catalogue: credentials, categories, manufacturers, paged search</summary>
          {catalogueOpen && <CataloguePanel busy={busy} onImport={(distributor) => void importPart(distributor)} />}
        </details>
      )}
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
