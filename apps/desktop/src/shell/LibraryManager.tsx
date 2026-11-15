// Library manager (ADR-0035): the libraries behind part search, with provenance and version, and a
// real enable/disable switch handled by the backend. A disabled library is hidden from search and
// placement. Nothing here touches the filesystem.
import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import type { LibraryCatalogView } from "../types";

export function LibraryManager({ revision }: { revision: number }) {
  const [cat, setCat] = useState<LibraryCatalogView | null>(null);
  const [q, setQ] = useState("");
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .libraryCatalog()
      .then((c) => {
        if (live) setCat(c ?? null);
      })
      .catch((e) => {
        if (live) setErr(String(e));
      });
    return () => {
      live = false;
    };
  }, [revision]);

  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return (cat?.libraries ?? []).filter(
      (r) => !needle || [r.name, r.id, r.source, r.license, r.scope, r.version].some((t) => t.toLowerCase().includes(needle)),
    );
  }, [cat, q]);

  const toggle = (id: string, enabled: boolean) => {
    setErr(null);
    api
      .librarySetEnabled(id, enabled)
      .then((c) => setCat(c ?? null))
      .catch((e) => setErr(String(e)));
  };

  return (
    <details className="library-manager">
      <summary>Libraries ({cat?.libraries.length ?? 0})</summary>
      <input aria-label="Search libraries" placeholder="Search libraries: name, source, licence…" value={q} onChange={(e) => setQ(e.target.value)} />
      {err && (
        <p className="muted" role="status">
          {err}
        </p>
      )}
      <ul className="library-manager-list">
        {rows.map((r) => (
          <li key={r.id}>
            <label>
              <input type="checkbox" aria-label={`Enable ${r.name}`} checked={r.enabled} onChange={(e) => toggle(r.id, e.target.checked)} />
              <strong>{r.name}</strong> <span className="muted">{r.scope} · v{r.version} · {r.components} part(s)</span>
            </label>
            <div className="muted">
              Licence: {r.license} · Source: {r.source} · Content {r.content_hash.slice(0, 12)}
            </div>
          </li>
        ))}
      </ul>
      {rows.length === 0 && <p className="empty-state">No library matches.</p>}
      {(cat?.problems ?? []).map((p) => (
        <p key={p} className="muted" role="status">
          {p}
        </p>
      ))}
    </details>
  );
}
