// Right-hand assistant. Requests become typed engine commands shown as a diff; nothing is
// applied until Accept, and Accept goes through the same engine path as the UI and CLI.
import { useEffect, useRef, useState } from "react";
import { diffLines, type Proposal } from "./ai";
import type { LibraryEntry, Snapshot } from "../types";
import { Icons } from "./icons";

export interface ChatMsg {
  id: number;
  role: "user" | "assistant";
  text: string;
  proposal?: Proposal;
  status?: "pending" | "accepted" | "rejected";
}

interface Props {
  messages: ChatMsg[];
  snapshot: Snapshot;
  library: LibraryEntry[];
  contextLabel: string | null;
  busy: boolean;
  modelLabel: string | null;
  focusNonce: number;
  onSend: (text: string) => void;
  onAccept: (m: ChatMsg) => void;
  onReject: (m: ChatMsg) => void;
  onClose: () => void;
}

const SUGGESTIONS = ["rotate", "annotate", "sync", "auto-place", "auto-route"];

export function AiPanel({ messages, snapshot, library, contextLabel, busy, modelLabel, focusNonce, onSend, onAccept, onReject, onClose }: Props) {
  const [text, setText] = useState("");
  const input = useRef<HTMLTextAreaElement>(null);
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => input.current?.focus(), [focusNonce]);
  useEffect(() => end.current?.scrollIntoView?.({ block: "end" }), [messages]);
  const send = (t: string) => {
    if (!t.trim()) return;
    onSend(t);
    setText("");
  };
  return (
    <aside className="ai" aria-label="AI assistant">
      <header className="panel-head">
        <span className="title">{Icons.sparkle} Assistant</span>
        <button type="button" className="icon" aria-label="Hide assistant" title="Hide assistant (Ctrl+L)" onClick={onClose}>×</button>
      </header>
      <p className="ai-note muted">
        Local command interpreter: it builds typed engine commands for you to review. It makes no model request
        {modelLabel ? ` (model ${modelLabel} is configured but not used for chat yet)` : ""} and never certifies engineering numbers.
      </p>
      <div className="chat" role="log" aria-label="Assistant conversation">
        {messages.length === 0 && (
          <p className="empty-state">Ask for a change, or select a part, net or finding and choose “Ask AI”. Every change appears as a preview you accept or reject.</p>
        )}
        {messages.map((m) => (
          <div key={m.id} className={`msg ${m.role}`}>
            <p>{m.text}</p>
            {m.proposal && (
              <div className={`proposal ${m.status ?? ""}`} role="group" aria-label={`Proposal: ${m.proposal.title}`}>
                <strong>{m.proposal.title}</strong>
                <pre className="diff">
                  {diffLines(m.proposal, snapshot, library).map((l, i) => (
                    <span key={i} className={l[0] === "+" ? "add" : l[0] === "-" ? "del" : "chg"}>{l + "\n"}</span>
                  ))}
                </pre>
                {m.status === "pending" ? (
                  <div className="row">
                    <button type="button" className="primary" disabled={busy} onClick={() => onAccept(m)}>Accept</button>
                    <button type="button" disabled={busy} onClick={() => onReject(m)}>Reject</button>
                    <span className="muted hint">Previewed on the canvas</span>
                  </div>
                ) : (
                  <span className="muted hint">{m.status === "accepted" ? "Applied through the engine" : "Rejected — nothing changed"}</span>
                )}
              </div>
            )}
          </div>
        ))}
        <div ref={end} />
      </div>
      <div className="composer">
        {contextLabel && <span className="ctx-chip">Context: {contextLabel}</span>}
        <div className="row chips">
          {SUGGESTIONS.map((s) => (
            <button key={s} type="button" className="chip" onClick={() => send(s)} disabled={busy}>{s}</button>
          ))}
        </div>
        <textarea
          ref={input}
          aria-label="Ask the assistant"
          placeholder="Ask for a change…  (Enter to send)"
          rows={2}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send(text);
            }
          }}
        />
      </div>
    </aside>
  );
}
