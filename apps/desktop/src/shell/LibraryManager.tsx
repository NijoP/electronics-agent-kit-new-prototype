// Library manager (ADR-0035): the libraries behind part search, with provenance and version, and a
// real enable/disable switch handled by the backend. A disabled library is hidden from search and
// placement. `LibraryFolders` (below) reads only the folders and files the user picks.
import { useEffect, useMemo, useState } from "react";
import { api, pickFolder, pickUserLibraryFile } from "../api";
import type { KicadFolderScan, LibraryDbSearch } from "../api";
import { KicadImport } from "../KicadImport";
import type { LibraryCatalogView } from "../types";

/** `onChanged` runs after a library was enabled, disabled or committed, so searches refresh. */
export function LibraryManager({ revision, onChanged }: { revision: number; onChanged?: () => void }) {
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
      .then((c) => {
        setCat(c ?? null);
        onChanged?.();
      })
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
      <LibraryFolders onChanged={onChanged} />
      <KicadImport
        onCommitted={() => {
          api.libraryCatalog().then((c) => setCat(c ?? null));
          onChanged?.();
        }}
      />
      {rows.length === 0 && <p className="empty-state">No library matches.</p>}
      {(cat?.problems ?? []).map((p) => (
        <p key={p} className="muted" role="status">
          {p}
        </p>
      ))}
    </details>
  );
}

/** KiCad library folders into the local library database, its search, and user library files.
 * Imported items are stored at maturity "imported" with the licence and source the user names;
 * nothing here verifies them or adds them to a design. */
export function LibraryFolders({ onChanged }: { onChanged?: () => void }) {
  const [scan, setScan] = useState<KicadFolderScan | null>(null);
  const [licence, setLicence] = useState("");
  const [source, setSource] = useState("");
  const [q, setQ] = useState("");
  const [found, setFound] = useState<LibraryDbSearch | null>(null);
  const [exportLib, setExportLib] = useState("");
  const [msg, setMsg] = useState<string[]>([]);

  const refresh = (query: string) => {
    api
      .libraryDbSearch(query)
      .then((r) => setFound(r ?? null))
      .catch((e) => setMsg([String(e)]));
  };
  useEffect(() => refresh(""), []);

  const chooseFolder = async () => {
    setMsg([]);
    const folder = await pickFolder("Choose a KiCad library folder");
    if (!folder) return;
    try {
      setScan(await api.kicadLibraryScan(folder));
    } catch (e) {
      setMsg([String(e)]);
    }
  };

  const importFolder = async () => {
    if (!scan) return;
    try {
      const r = await api.kicadLibraryImport(scan.folder, licence.trim(), source.trim());
      const lines = [
        `Stored ${r.stored} item(s): ${r.symbols} symbol(s), ${r.footprints} footprint(s), maturity "imported" (not verified).`,
        `3D models found on disk: ${r.models_found}; not found: ${r.models_missing.length}.`,
      ];
      if (r.unpaired.length) lines.push(`Symbols without a matching footprint: ${r.unpaired.join(", ")}`);
      if (r.models_missing.length) lines.push(`Missing 3D models: ${r.models_missing.join(", ")}`);
      lines.push(...r.skipped_libraries.map((s) => `Not imported: ${s}`), ...r.problems);
      setMsg(lines);
      setScan(null);
      refresh(q);
      onChanged?.();
    } catch (e) {
      setMsg([String(e)]);
    }
  };

  const exportLibrary = async () => {
    const folder = await pickFolder("Choose where to save the library file");
    if (!folder) return;
    try {
      setMsg([`Saved ${await api.userLibraryExport(exportLib, folder)}`]);
    } catch (e) {
      setMsg([String(e)]);
    }
  };

  const importFile = async () => {
    setMsg([]);
    const path = await pickUserLibraryFile("Choose a user library file");
    if (!path) return;
    try {
      const r = await api.userLibraryImport(path);
      const lines = [`Imported ${r.imported} item(s) into ${r.library}.`];
      if (r.maturity_not_kept.length) lines.push(`Stored as "imported" (the file's higher maturity is not trusted): ${r.maturity_not_kept.join(", ")}`);
      lines.push(...r.rejected.map(([name, why]) => `Rejected ${name}: ${why}`));
      setMsg(lines);
      refresh(q);
      onChanged?.();
    } catch (e) {
      setMsg([String(e)]);
    }
  };

  const libs = [...(scan?.symbol_libs ?? []), ...(scan?.footprint_libs ?? [])];
  const canImport = !!scan && libs.some((l) => l.usable) && licence.trim() !== "" && source.trim() !== "";
  return (
    <section className="library-folders" aria-label="KiCad library folders and user libraries">
      <button type="button" onClick={chooseFolder}>
        Import a library folder…
      </button>
      {scan && (
        <div>
          <p className="muted">{scan.folder}</p>
          <ul>
            {libs.map((l) => (
              <li key={`${l.kind}:${l.name}`}>
                <strong>{l.name}</strong>{" "}
                <span className="muted">
                  {l.kind} · {l.origin === "table" ? "lib table" : "folder"} · {l.usable ? "will be imported" : "not imported"}
                  {l.notes.length ? ` · ${l.notes.join("; ")}` : ""}
                </span>
              </li>
            ))}
          </ul>
          {libs.length === 0 && <p className="empty-state">No KiCad symbol or footprint library in this folder.</p>}
          {scan.problems.map((p) => (
            <p key={p} className="muted">
              {p}
            </p>
          ))}
          <input aria-label="Folder licence" placeholder="Licence (required), e.g. CC-BY-SA-4.0" value={licence} onChange={(e) => setLicence(e.target.value)} />
          <input aria-label="Folder source" placeholder="Source (required), e.g. a URL" value={source} onChange={(e) => setSource(e.target.value)} />
          <button type="button" disabled={!canImport} onClick={importFolder}>
            Import to library database
          </button>
        </div>
      )}
      <input
        aria-label="Search library database"
        placeholder="Search imported symbols and footprints…"
        value={q}
        onChange={(e) => {
          setQ(e.target.value);
          refresh(e.target.value);
        }}
      />
      <ul className="library-manager-list">
        {(found?.items ?? []).map((i) => (
          <li key={`${i.kind}:${i.library}:${i.name}`}>
            <strong>{i.name}</strong>{" "}
            <span className="muted">
              {i.kind} · {i.library} · {i.maturity} (not verified) · Licence: {i.licence} · Source: {i.source}
            </span>
          </li>
        ))}
      </ul>
      <select aria-label="Library to export" value={exportLib} onChange={(e) => setExportLib(e.target.value)}>
        <option value="">Library to export…</option>
        {(found?.libraries ?? []).map((l) => (
          <option key={l.library} value={l.library}>
            {l.library} ({l.items})
          </option>
        ))}
      </select>
      <button type="button" disabled={!exportLib} onClick={exportLibrary}>
        Export library…
      </button>
      <button type="button" onClick={importFile}>
        Import a library file…
      </button>
      {msg.map((m) => (
        <p key={m} className="muted" role="status">
          {m}
        </p>
      ))}
    </section>
  );
}
