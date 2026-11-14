// Read-only viewer for a project file opened from the explorer. The design itself is edited in the
// Schematic and PCB editors, never here; this shows what is on disk, with the size cap the backend
// enforces and an honest message for binary or oversized files.
import { useEffect, useState } from "react";
import { api } from "../api";
import { formatSize, type FileContent } from "./files";

export function FileViewer({ projectId, rel, revision }: { projectId: string; rel: string; revision: number }) {
  const [content, setContent] = useState<FileContent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    let live = true;
    api
      .projectFileRead(projectId, rel)
      .then((c) => live && (setContent(c), setError(null)))
      .catch((e) => live && (setContent(null), setError(String(e).replace(/^Error: /, ""))));
    return () => {
      live = false;
    };
  }, [projectId, rel, revision, nonce]);

  return (
    <section className="file-viewer" aria-label={`File ${rel}`}>
      <header className="fv-head">
        <strong>{rel}</strong>
        {content && <span className="muted">{formatSize(content.size)}</span>}
        <span className="muted">Read-only view — edit the design in the Schematic and PCB editors</span>
        <span className="spacer" />
        <button type="button" onClick={() => setNonce((n) => n + 1)} title="Read the file from disk again">Reload</button>
      </header>
      {error && <p role="alert" className="explorer-error">{error}</p>}
      {content?.text != null && <pre className="fv-body" tabIndex={0}>{content.text}</pre>}
      {content?.binary && <p className="empty-state">Binary file ({formatSize(content.size)}): it cannot be shown as text.</p>}
      {content?.truncated && <p className="empty-state">This file is {formatSize(content.size)}, larger than the {formatSize(512 * 1024)} the viewer loads.</p>}
    </section>
  );
}
