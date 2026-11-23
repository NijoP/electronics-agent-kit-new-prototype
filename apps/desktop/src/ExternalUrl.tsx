// An external address (datasheet, library source, supplier sign-up) shown as plain, selectable
// text with a Copy button (finding S23). The desktop app has no opener plugin, so a
// `target="_blank"` link in the Tauri webview would do nothing; the address is never presented as
// a link that pretends to open.
import { useState } from "react";

export const NO_OPENER = "EAK cannot open links in your browser yet: copy the address instead";

export function ExternalUrl({ url, label }: { url: string; label: string }) {
  const [note, setNote] = useState<string | null>(null);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(url);
      setNote("copied");
    } catch (e) {
      setNote(`copy failed: ${String(e).replace(/^Error: /, "")}; select the address instead`);
    }
  };
  return (
    <span className="external-url" title={NO_OPENER}>
      <span className="muted">{label}: </span>
      <code style={{ userSelect: "all", wordBreak: "break-all" }}>{url}</code>{" "}
      <button type="button" onClick={() => void copy()} aria-label={`Copy ${label} address`} title={NO_OPENER}>
        Copy
      </button>
      {note && <span role="status" className="muted"> {note}</span>}
    </span>
  );
}
