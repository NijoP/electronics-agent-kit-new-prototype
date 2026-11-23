// Assets view (side panel "Assets", Phase 1 slice E9): the project's 3D models, datasheets,
// libraries with their licence and provenance, and exported outputs per revision. Everything is
// read from real state (engine snapshot, `library_catalog`, the sandboxed `project_files` listing);
// a value that is not recorded says so. Read-only: assets are IR model references, edited where
// they live (Library dock, 3D view, part editor), never here.
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { api } from "../api";
import { EmptyState, Pill, Section } from "../design/ui";
import type { LibraryCatalogView, Snapshot } from "../types";
import { formatSize, manifestStatus, modelQuality, outputRevisions, partAssets, type OutputRevision, type UsedBy } from "./assetsModel";
import type { FileEntry } from "./files";

interface Props {
  snapshot: Snapshot;
  /** Opens a project file in the file viewer. */
  onOpenFile: (entry: FileEntry) => void;
  /** Selects a placed part and shows it in the editor (the design manager's navigation). */
  onPart: (symbolId: string) => void;
  /** Changes when an export finished in this session, so the outputs are listed again. */
  outputsKey?: string | null;
}

const errText = (e: unknown) => (e && typeof e === "object" && "message" in e ? String((e as { message: unknown }).message) : String(e));

function Parts({ list, onPart }: { list: UsedBy[]; onPart: (id: string) => void }) {
  if (list.length === 0) return <span className="muted">not placed</span>;
  return (
    <span className="asset-parts">
      {list.map((u) => (
        <button key={u.symbolId} type="button" className="linkish" title={`Select ${u.refdes} and show it in the editor`} onClick={() => onPart(u.symbolId)}>
          {u.refdes}
        </button>
      ))}
    </span>
  );
}

interface FolderProps {
  projectId: string;
  entry: FileEntry;
  label: string;
  aside?: ReactNode;
  onOpenFile: (e: FileEntry) => void;
  depth: number;
}

/** One folder of an output revision: expands to its files (each opens in the file viewer). */
function Folder({ projectId, entry, label, aside, onOpenFile, depth }: FolderProps) {
  const [open, setOpen] = useState(false);
  const [items, setItems] = useState<FileEntry[] | null>(null);
  const [manifest, setManifest] = useState<ReturnType<typeof manifestStatus>>(null);
  const [error, setError] = useState<string | null>(null);
  const toggle = () => {
    const next = !open;
    setOpen(next);
    if (!next) return;
    setError(null);
    api
      .projectFiles(projectId, entry.rel)
      .then(async (list) => {
        setItems(list);
        const m = list.find((f) => !f.is_dir && f.name === "manifest.json");
        setManifest(m ? manifestStatus((await api.projectFileRead(projectId, m.rel)).text) : null);
      })
      .catch((e) => setError(errText(e)));
  };
  return (
    <li className="asset-folder" data-rel={entry.rel} style={{ paddingLeft: depth * 12 }}>
      <div className="asset-row">
        <button type="button" className="linkish" aria-expanded={open} aria-label={`${open ? "Collapse" : "Expand"} ${label}`} onClick={toggle}>
          {open ? "▾" : "▸"} {label}
        </button>
        {aside}
        {open && manifest && (
          <Pill tone={manifest.status === "DRAFT" ? "warn" : "info"} title="Status written in this folder's manifest.json; EAK never calls an export manufacturing-ready">
            {manifest.status} · {manifest.files} file(s)
          </Pill>
        )}
      </div>
      {open && error && <p className="empty-state">Cannot list {entry.rel}: {error}</p>}
      {open && items && items.length === 0 && <p className="empty-state">Empty folder.</p>}
      {open && items && items.length > 0 && (
        <ul className="asset-list">
          {items.map((f) =>
            f.is_dir ? (
              <Folder key={f.rel} projectId={projectId} entry={f} label={`${f.name}/`} onOpenFile={onOpenFile} depth={1} />
            ) : (
              <li key={f.rel} className="asset-row" style={{ paddingLeft: 12 }}>
                <button type="button" className="linkish mono" title={`Open ${f.rel} in the file viewer`} onClick={() => onOpenFile(f)}>
                  {f.name}
                </button>
                <span className="muted">{formatSize(f.size)}</span>
              </li>
            ),
          )}
        </ul>
      )}
    </li>
  );
}

function Outputs({ snapshot, onOpenFile, outputsKey }: Pick<Props, "snapshot" | "onOpenFile" | "outputsKey">) {
  const projectId = snapshot.project.id;
  const revision = snapshot.project.revision;
  const saved = !!snapshot.dir;
  const [revs, setRevs] = useState<OutputRevision[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    if (!saved) return;
    setError(null);
    api
      .projectFiles(projectId, "")
      .then(async (root) => (root.some((e) => e.is_dir && e.name === "output") ? outputRevisions(await api.projectFiles(projectId, "output"), revision) : []))
      .then(setRevs)
      .catch((e) => setError(errText(e)));
  }, [projectId, revision, saved]);
  useEffect(load, [load, outputsKey]);
  return (
    <Section
      title="Outputs per revision"
      label="Outputs"
      aside={
        <button type="button" className="linkish" disabled={!saved} title={saved ? "List output/ again (files written outside EAK appear after this)" : "The project has no folder on disk yet"} onClick={load}>
          Refresh
        </button>
      }
    >
      {!saved ? (
        <p className="empty-state">Exports need a saved project folder.</p>
      ) : error ? (
        <p className="empty-state">Cannot list outputs: {error}</p>
      ) : revs === null ? (
        <p className="empty-state">Listing output/…</p>
      ) : revs.length === 0 ? (
        <p className="empty-state">Nothing exported yet. Exports (manufacturing, BOM, documents) go to output/rev-&lt;N&gt;.</p>
      ) : (
        <ul className="asset-list" aria-label="Output revisions">
          {revs.map((r) => (
            <Folder
              key={r.entry.rel}
              projectId={projectId}
              entry={r.entry}
              label={`Revision ${r.revision}`}
              depth={0}
              onOpenFile={onOpenFile}
              aside={
                r.current ? (
                  <Pill tone="ok" title="Exported from the revision the project is at now">current</Pill>
                ) : (
                  <Pill tone="neutral" title={`The project is at revision ${revision}; these files describe an older design`}>older</Pill>
                )
              }
            />
          ))}
        </ul>
      )}
    </Section>
  );
}

function Libraries({ revision }: { revision: number }) {
  const [view, setView] = useState<LibraryCatalogView | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    api
      .libraryCatalog()
      .then((v) => {
        if (!live) return;
        setView(v);
        setError(null);
      })
      .catch((e) => {
        if (live) setError(errText(e));
      });
    return () => {
      live = false;
    };
  }, [revision]);
  return (
    <Section title="Libraries" label="Libraries">
      {error ? (
        <p className="empty-state">Cannot read the library catalog: {error}</p>
      ) : !view ? (
        <p className="empty-state">Reading the library catalog…</p>
      ) : view.libraries.length === 0 ? (
        <p className="empty-state">No libraries are registered.</p>
      ) : (
        <ul className="asset-list" aria-label="Library list">
          {view.libraries.map((l) => (
            <li key={l.id} className="asset-item" data-library={l.name}>
              <div className="asset-row">
                <strong>{l.name}</strong>
                <span className="muted">{l.version}</span>
                <Pill tone="neutral">{l.scope}</Pill>
                {!l.enabled && <Pill tone="warn" title="Disabled in Library manager: its parts are not listed">disabled</Pill>}
              </div>
              <div className="asset-meta">
                Licence: {l.license.trim() ? l.license : <Pill tone="warn">not recorded</Pill>} · {l.components} part(s)
              </div>
              <div className="asset-meta">
                Source: <span className="mono">{l.source.trim() || "not recorded"}</span>
              </div>
              {l.content_hash && (
                <div className="asset-meta" title={l.content_hash}>
                  Content hash: <span className="mono">{l.content_hash.slice(0, 12)}</span>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
      {view && view.problems.length > 0 && (
        <ul className="asset-problems">
          {view.problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}
    </Section>
  );
}

export function AssetsPanel({ snapshot, onOpenFile, onPart, outputsKey }: Props) {
  const parts = partAssets(snapshot);
  const withModel = parts.filter((p) => p.model);
  const withSheet = parts.filter((p) => p.datasheet);
  return (
    <div className="assets" role="region" aria-label="Assets">
      <Section title="3D models" label="3D models">
        {parts.length === 0 ? (
          <EmptyState title="No parts in the project library">Place a part: its definition, with any 3D model and datasheet, is copied into the project.</EmptyState>
        ) : (
          <>
            {withModel.length === 0 && <p className="empty-state">No part has a 3D model reference; the 3D view draws box bodies.</p>}
            {withModel.length > 0 && (
              <ul className="asset-list" aria-label="Model list">
                {withModel.map((p) => {
                  const m = p.model!;
                  return (
                    <li key={p.componentId} className="asset-item" data-component={p.componentId}>
                      <div className="asset-row">
                        <strong>{p.name}</strong>
                        <Pill tone="neutral">{m.format.toUpperCase()}</Pill>
                        <Pill tone={m.quality ? "info" : "warn"}>{modelQuality(m)}</Pill>
                      </div>
                      <div className="asset-meta mono" title="Path recorded in the model reference (the 3D view reads the file when it loads)">
                        {m.source.path}
                      </div>
                      <div className="asset-meta">Licence: {m.licence?.trim() ? m.licence : <Pill tone="warn">not recorded</Pill>}</div>
                      <div className="asset-meta">Provenance: {m.provenance}</div>
                      <div className="asset-meta">
                        Used by: <Parts list={p.usedBy} onPart={onPart} />
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
            {withModel.length > 0 && withModel.length < parts.length && (
              <p className="empty-state">
                Box body only (no model):{" "}
                {parts
                  .filter((p) => !p.model)
                  .map((p) => p.componentId)
                  .join(", ")}
              </p>
            )}
          </>
        )}
      </Section>
      <Section title="Datasheets" label="Datasheets">
        {parts.length === 0 ? (
          <p className="empty-state">No parts in the project library.</p>
        ) : (
          <>
            {withSheet.length > 0 && (
              <ul className="asset-list" aria-label="Datasheet list">
                {withSheet.map((p) => (
                  <li key={p.componentId} className="asset-item" data-component={p.componentId}>
                    <div className="asset-row">
                      <strong>{p.name}</strong>
                    </div>
                    <div className="asset-meta mono" title="Recorded reference; EAK does not fetch or check it">
                      {p.datasheet}
                    </div>
                    <div className="asset-meta">
                      Used by: <Parts list={p.usedBy} onPart={onPart} />
                    </div>
                  </li>
                ))}
              </ul>
            )}
            {withSheet.length < parts.length && (
              <p className="empty-state">
                No datasheet recorded:{" "}
                {parts
                  .filter((p) => !p.datasheet)
                  .map((p) => p.componentId)
                  .join(", ")}
              </p>
            )}
          </>
        )}
      </Section>
      <Section title="Part provenance" label="Part provenance">
        {parts.length === 0 ? (
          <p className="empty-state">No parts in the project library.</p>
        ) : (
          <ul className="asset-list" aria-label="Provenance list">
            {parts.map((p) => (
              <li key={p.componentId} className="asset-item" data-component={p.componentId}>
                <div className="asset-row">
                  <strong>{p.componentId}</strong>
                  <span className="muted">{p.name}</span>
                </div>
                <div className="asset-meta">{p.provenance ?? <Pill tone="warn">not recorded</Pill>}</div>
              </li>
            ))}
          </ul>
        )}
      </Section>
      <Libraries revision={snapshot.project.revision} />
      <Outputs snapshot={snapshot} onOpenFile={onOpenFile} outputsKey={outputsKey} />
    </div>
  );
}
