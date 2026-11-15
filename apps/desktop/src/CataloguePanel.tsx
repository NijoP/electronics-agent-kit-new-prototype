// LCSC catalogue browser and API-credential settings (ADR-0032).
// Credentials go straight to the OS credential store through the backend; the secret is never
// kept in component state after Save and never shown again. Nothing is fetched until the user
// opens a section: the category tree, the manufacturer list and each page of products are
// lazy. Every result says whether it is live, cached or stale, and when LCSC produced it. When
// LCSC cannot be asked (no credentials, rejected, rate-limited, offline) the panel shows why
// and shows no rows; it never fills the table itself.
import { useCallback, useState } from "react";
import { api } from "./api";
import type {
  CatalogueReply,
  CatalogueResults,
  Candidate,
  CategoryNode,
  LcscView,
  LibraryLink,
  Manufacturer,
  MatchKind,
  Served,
  Standing,
} from "./types";

interface Props {
  busy: boolean;
  /** Imports one LCSC part into the project library (engineering definition), via the parent. */
  onImport: (lcsc: string) => void;
}

const KIND_LABEL: Record<MatchKind, string> = {
  exact_lcsc: "Exact LCSC #",
  exact_mpn_manufacturer: "Exact MPN + maker",
  exact_mpn: "Exact MPN",
  fuzzy: "Fuzzy suggestion",
  browse: "Category listing",
};

const STANDING_LABEL: Record<Standing, string> = {
  existing_eak: "Existing EAK component",
  imported_definition: "Imported definition",
  supplier_only: "Supplier record only",
  needs_authoring: "Needs new symbol/footprint",
};

function LinkCell({ link }: { link: LibraryLink | undefined }) {
  if (!link) return <>—</>;
  return (
    <span title={link.explanation}>
      {STANDING_LABEL[link.standing]}
      {link.import_template ? ` (template ${link.import_template})` : ""}
      {link.unverified ? " · unverified engineering definition" : ""}
    </span>
  );
}

function when(unix: number | null): string {
  return unix === null ? "never" : new Date(unix * 1000).toLocaleString();
}

export function servedLabel(served: Served | null, fetched: number | null): string {
  if (!served) return "";
  const at = fetched === null ? "" : ` · LCSC data from ${when(fetched)}`;
  switch (served.source) {
    case "live":
      return `live${at}`;
    case "cache":
      return `cached ${served.age_s ?? 0} s ago${at}`;
    case "stale_cache":
      return `STALE (LCSC unavailable) — ${served.age_s ?? 0} s old${at}`;
  }
}

function price(c: Candidate): string {
  const b = c.record.price_breaks[0];
  return b ? `${b.currency} ${b.unit_price} @ ${b.min_quantity}+` : "—";
}

function Settings() {
  const [view, setView] = useState<LcscView | null>(null);
  const [key, setKey] = useState("");
  const [secret, setSecret] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async (f: () => Promise<LcscView>, after?: () => void) => {
    setBusy(true);
    setNote(null);
    try {
      setView(await f());
      after?.();
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  };

  const status = view?.status;
  return (
    <section aria-label="LCSC API credentials">
      <h3>LCSC API credentials</h3>
      {!view && (
        <button type="button" disabled={busy} onClick={() => void run(api.lcscSettings)}>
          Show credential status
        </button>
      )}
      {view && (
        <>
          <p role="status">
            {view.has_credentials
              ? `Credentials stored in the operating-system credential store (key …${view.key_hint ?? ""}).`
              : "No credentials stored."}{" "}
            Connection: {status?.state ?? "untested"}
            {status?.message ? ` — ${status.message}` : ""}
            {status?.tested_at_unix_s ? ` (tested ${when(status.tested_at_unix_s)})` : ""}. Last success:{" "}
            {when(status?.last_success_unix_s ?? null)}. Requests this session: {view.requests_this_session}.
          </p>
          {view.store_error && (
            <p role="alert" className="error">
              {view.store_error}
            </p>
          )}
          {view.needs && (
            <p role="alert" className="error">
              {view.needs}
            </p>
          )}
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const k = key;
              const s = secret;
              // The secret leaves component state as soon as it is submitted.
              setSecret("");
              void run(
                () => api.lcscSaveCredentials(k, s),
                () => setKey(""),
              );
            }}
          >
            <label>
              API key
              <input value={key} onChange={(e) => setKey(e.target.value)} autoComplete="off" spellCheck={false} />
            </label>
            <label>
              API secret
              <input
                type="password"
                value={secret}
                onChange={(e) => setSecret(e.target.value)}
                autoComplete="new-password"
                spellCheck={false}
              />
            </label>
            <button type="submit" disabled={busy || key.trim() === "" || secret.trim() === ""}>
              Save credentials
            </button>
            <button
              type="button"
              disabled={busy || !view.has_credentials}
              onClick={() => void run(api.lcscTestConnection)}
            >
              Test connection
            </button>
            <button
              type="button"
              disabled={busy || !view.has_credentials}
              onClick={() => void run(api.lcscClearCredentials)}
            >
              Remove credentials
            </button>
          </form>
          <p className="muted">
            Approved credentials and a whitelisted IP address come from LCSC (
            <a href={view.apply_url} target="_blank" rel="noreferrer">
              apply
            </a>
            ). The secret is kept in the OS credential store only; requests go to {view.host} and are signed locally.
          </p>
        </>
      )}
      {note && (
        <p role="alert" className="error">
          {note}
        </p>
      )}
    </section>
  );
}

function Tree({ nodes, onPick, depth }: { nodes: CategoryNode[]; onPick: (n: CategoryNode) => void; depth: number }) {
  const [open, setOpen] = useState<Record<number, boolean>>({});
  return (
    <ul style={{ listStyle: "none", paddingLeft: depth === 0 ? 0 : 14 }}>
      {nodes.map((n) => (
        <li key={n.id}>
          {n.children.length > 0 && (
            <button
              type="button"
              aria-label={`${open[n.id] ? "Collapse" : "Expand"} ${n.name}`}
              aria-expanded={!!open[n.id]}
              onClick={() => setOpen({ ...open, [n.id]: !open[n.id] })}
            >
              {open[n.id] ? "▾" : "▸"}
            </button>
          )}{" "}
          <button type="button" onClick={() => onPick(n)}>
            {n.name}
            {n.product_count !== null ? ` (${n.product_count.toLocaleString("en-US")})` : ""}
          </button>
          {open[n.id] && n.children.length > 0 && <Tree nodes={n.children} onPick={onPick} depth={depth + 1} />}
        </li>
      ))}
    </ul>
  );
}

export function CataloguePanel({ busy, onImport }: Props) {
  const [tree, setTree] = useState<CatalogueReply<CategoryNode[]> | null>(null);
  const [makers, setMakers] = useState<CatalogueReply<Manufacturer[]> | null>(null);
  const [keyword, setKeyword] = useState("");
  const [maker, setMaker] = useState("");
  const [pkg, setPkg] = useState("");
  const [inStock, setInStock] = useState(false);
  const [limit, setLimit] = useState(50);
  const [category, setCategory] = useState<CategoryNode | null>(null);
  const [reply, setReply] = useState<CatalogueReply<CatalogueResults> | null>(null);
  const [wait, setWait] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  const guard = useCallback(async (f: () => Promise<void>) => {
    setWait(true);
    setNote(null);
    try {
      await f();
    } catch (e) {
      setNote(String(e));
    } finally {
      setWait(false);
    }
  }, []);

  const query = (page: number, cat: CategoryNode | null, refresh: boolean, kw = keyword) =>
    guard(async () => {
      const byKeyword = kw.trim() !== "";
      setReply(
        await api.catalogueSearch({
          keyword: byKeyword ? kw.trim() : undefined,
          category_id: byKeyword ? undefined : cat?.id,
          category_name: byKeyword ? undefined : cat?.name,
          manufacturer: maker || undefined,
          package: pkg.trim() || undefined,
          in_stock_only: inStock,
          limit,
          page,
          refresh,
        }),
      );
    });

  const saveRecord = (lcsc: string) =>
    guard(async () => {
      const r = await api.partsSaveRecord(lcsc);
      setSaved(`Saved supplier record ${lcsc} to ${r.path}. ${r.link.explanation}`);
    });

  const data = reply?.data ?? null;
  return (
    <section aria-label="LCSC catalogue">
      <h3>LCSC catalogue</h3>
      <Settings />

      <details>
        <summary>Categories and manufacturers</summary>
        <button
          type="button"
          disabled={wait}
          onClick={() => void guard(async () => setTree(await api.catalogueCategories(false)))}
        >
          Load categories
        </button>
        <button
          type="button"
          disabled={wait}
          onClick={() => void guard(async () => setMakers(await api.catalogueManufacturers(false)))}
        >
          Load manufacturers
        </button>
        {tree && tree.status !== "ok" && tree.status !== "stale" && (
          <p role="alert" className="error">
            LCSC: {tree.message}
          </p>
        )}
        {tree?.data && (
          <>
            <p className="muted">Categories — {servedLabel(tree.served, tree.fetched_at_unix_s)}</p>
            <Tree
              nodes={tree.data}
              depth={0}
              onPick={(n) => {
                setCategory(n);
                setKeyword("");
                void query(1, n, false, "");
              }}
            />
          </>
        )}
        {makers && makers.status !== "ok" && makers.status !== "stale" && (
          <p role="alert" className="error">
            LCSC: {makers.message}
          </p>
        )}
        {makers?.data && (
          <label>
            Manufacturer
            <select value={maker} onChange={(e) => setMaker(e.target.value)}>
              <option value="">any</option>
              {makers.data.map((m) => (
                <option key={`${m.id ?? ""}-${m.name}`} value={m.name}>
                  {m.name}
                </option>
              ))}
            </select>
          </label>
        )}
      </details>

      <form
        aria-label="Catalogue search"
        onSubmit={(e) => {
          e.preventDefault();
          setCategory(null);
          void query(1, null, false);
        }}
      >
        <label>
          LCSC #, MPN or keyword
          <input value={keyword} onChange={(e) => setKeyword(e.target.value)} placeholder="C25804 or 0603WAF1002T5E" />
        </label>
        <label>
          Package
          <input value={pkg} onChange={(e) => setPkg(e.target.value)} placeholder="0603" />
        </label>
        <label>
          <input type="checkbox" checked={inStock} onChange={(e) => setInStock(e.target.checked)} /> In stock only
        </label>
        <label>
          Page size
          <select value={limit} onChange={(e) => setLimit(Number(e.target.value))}>
            {[25, 50, 100, 200, 500].map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </label>
        <button type="submit" disabled={wait || keyword.trim() === ""}>
          Search catalogue
        </button>
      </form>
      <p className="muted">
        Package and manufacturer filters apply to the loaded page. A supplier listing is not proof that a part is
        electrically suitable; import creates a project definition from an existing symbol/footprint only.
      </p>

      {note && (
        <p role="alert" className="error">
          {note}
        </p>
      )}
      {saved && <p role="status">{saved}</p>}
      {reply && reply.status !== "ok" && reply.status !== "stale" && (
        <p role="alert" className="error">
          LCSC ({reply.status.replace("_", " ")}): {reply.message}
        </p>
      )}
      {reply?.status === "stale" && (
        <p role="alert" className="error">
          {reply.message}
        </p>
      )}
      {data && (
        <>
          <p role="status">
            {servedLabel(reply?.served ?? null, reply?.fetched_at_unix_s ?? null)} · page {data.page}
            {data.total !== null
              ? ` of ${Math.max(1, Math.ceil(data.total / data.limit))} (${data.total.toLocaleString("en-US")} results)`
              : ""}
            {data.filtered_out > 0 ? ` · ${data.filtered_out} hidden by filters` : ""}
            {data.via !== "keyword" ? ` · via ${data.via}` : ""}
          </p>
          {data.candidates.length === 0 ? (
            <p>LCSC returned no matching parts.</p>
          ) : (
            <table aria-label="Catalogue results">
              <thead>
                <tr>
                  <th>Match</th>
                  <th>LCSC #</th>
                  <th>MPN</th>
                  <th>Maker</th>
                  <th>Package</th>
                  <th>Stock</th>
                  <th>Price</th>
                  <th>Datasheet</th>
                  <th>Library</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {data.candidates.map((c) => {
                  const lcsc = c.record.lcsc_part;
                  const link = lcsc ? data.links?.[lcsc] : undefined;
                  return (
                  <tr key={c.record.raw_sha256}>
                    <td>
                      <details>
                        <summary>{KIND_LABEL[c.kind]}</summary>
                        <ul>
                          {c.reasons.map((r) => (
                            <li key={r}>{r}</li>
                          ))}
                        </ul>
                      </details>
                      {c.manufacturer_conflict && <strong> maker differs</strong>}
                    </td>
                    <td>{c.record.lcsc_part ?? "—"}</td>
                    <td>{c.record.mpn ?? "—"}</td>
                    <td>{c.record.manufacturer ?? "—"}</td>
                    <td>{c.record.package ?? "—"}</td>
                    <td>{c.record.stock === null ? "—" : c.record.stock.toLocaleString("en-US")}</td>
                    <td>{price(c)}</td>
                    <td>
                      {c.record.datasheet_url ? (
                        <a href={c.record.datasheet_url} target="_blank" rel="noreferrer">
                          datasheet
                        </a>
                      ) : (
                        "—"
                      )}
                    </td>
                    <td>
                      <LinkCell link={link} />
                    </td>
                    <td>
                      {lcsc && (
                        <button type="button" disabled={wait} onClick={() => void saveRecord(lcsc)} aria-label={`Save record ${lcsc}`}>
                          Save record
                        </button>
                      )}
                      {/* Import only where an existing symbol/footprint template can carry the part. */}
                      {lcsc && (link === undefined || link.standing === "supplier_only") && (
                        <button type="button" disabled={busy} onClick={() => onImport(lcsc)} aria-label={`Import ${lcsc}`}>
                          Import
                        </button>
                      )}
                    </td>
                  </tr>
                  );
                })}
              </tbody>
            </table>
          )}
          <div>
            <button type="button" disabled={wait || data.page <= 1} onClick={() => void query(data.page - 1, category, false)}>
              Previous page
            </button>
            <button type="button" disabled={wait || !data.has_more} onClick={() => void query(data.page + 1, category, false)}>
              Next page
            </button>
            <button type="button" disabled={wait} onClick={() => void query(data.page, category, true)}>
              Refresh from LCSC
            </button>
          </div>
        </>
      )}
    </section>
  );
}
