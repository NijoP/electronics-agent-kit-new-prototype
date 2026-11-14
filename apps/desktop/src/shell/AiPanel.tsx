// Right-hand assistant. With a configured model provider, requests go to the eak-intelligence
// agent loop (ai_propose); every typed command it proposes is dry-run by the engine and shown
// here with Accept / Reject. Accept applies it through Engine::execute as the agent actor;
// Rollback undoes that step. Without a provider the panel says so and only offers the
// clearly-labelled offline command interpreter (no model, no replies invented).
import { useEffect, useRef, useState } from "react";
import { diffLines, type Proposal } from "./ai";
import { describeError } from "../api";
import type { AiStep, LibraryEntry, Snapshot } from "../types";
import { Icons } from "./icons";

export type MsgStatus = "pending" | "accepted" | "rejected" | "rolled_back";

export interface ChatMsg {
  id: number;
  role: "user" | "assistant";
  text: string;
  /** "ai" = from the configured model; "offline" = local keyword interpreter (no model). */
  source?: "ai" | "offline";
  /** Offline interpreter proposal. */
  proposal?: Proposal;
  /** Agent-proposed engine command awaiting the user's decision. */
  step?: AiStep;
  /** Journal op of an accepted agent step (for rollback). */
  opId?: string;
  status?: MsgStatus;
  error?: boolean;
}

interface Props {
  messages: ChatMsg[];
  snapshot: Snapshot;
  library: LibraryEntry[];
  contextLabel: string | null;
  busy: boolean;
  /** Label of the configured, ready model provider; null = none configured. */
  modelLabel: string | null;
  focusNonce: number;
  onSend: (text: string) => void;
  onAccept: (m: ChatMsg) => void;
  onReject: (m: ChatMsg) => void;
  onRollback: (m: ChatMsg) => void;
  onOpenSettings: () => void;
  onClose: () => void;
}

const SUGGESTIONS = ["rotate", "annotate", "sync", "auto-place", "auto-route"];

const CLASS_LABEL: Record<AiStep["class"], string> = {
  read_only: "read-only",
  reversible_local: "reversible",
  engineering_impacting: "engineering-impacting",
  destructive: "destructive",
};

export function stepSummary(s: AiStep): string[] {
  const p = s.preview;
  const lines = [`~ ${s.command.type} (${CLASS_LABEL[s.class]})`];
  if (p.accepted) {
    if (p.created.length) lines.push(`+ creates ${p.created.length} object(s)`);
    if (p.affected.length) lines.push(`~ affects ${p.affected.length} object(s)`);
    for (const w of p.warnings) lines.push(`! ${w}`);
  } else lines.push(`- dry run refused: ${p.error ? describeError(p.error) : "unknown error"}`);
  if (s.checks_to_run.length) lines.push(`  checks to re-run after applying: ${s.checks_to_run.join(", ")}`);
  return lines;
}

function StepCard({ m, busy, onAccept, onReject, onRollback }: { m: ChatMsg & { step: AiStep }; busy: boolean } & Pick<Props, "onAccept" | "onReject" | "onRollback">) {
  return (
    <div className={`proposal ${m.status ?? ""}`} role="group" aria-label={`AI step: ${m.step.command.type}`}>
      <strong>{m.step.command.type}</strong>
      <pre className="diff">
        {stepSummary(m.step).map((l, i) => (
          <span key={i} className={l[0] === "+" ? "add" : l[0] === "-" ? "del" : "chg"}>{l + "\n"}</span>
        ))}
      </pre>
      {m.status === "pending" && (
        <div className="row">
          <button type="button" className="primary" disabled={busy} onClick={() => onAccept(m)}>Accept</button>
          <button type="button" disabled={busy} onClick={() => onReject(m)}>Reject</button>
          <span className="muted hint">Dry-run by the engine; nothing applied yet</span>
        </div>
      )}
      {m.status === "accepted" && (
        <div className="row">
          <span className="muted hint">Applied through the engine as agent:assistant</span>
          <button type="button" disabled={busy} onClick={() => onRollback(m)}>Rollback</button>
        </div>
      )}
      {m.status === "rejected" && <span className="muted hint">Rejected — nothing changed</span>}
      {m.status === "rolled_back" && <span className="muted hint">Rolled back (undone)</span>}
    </div>
  );
}

export function AiPanel({ messages, snapshot, library, contextLabel, busy, modelLabel, focusNonce, onSend, onAccept, onReject, onRollback, onOpenSettings, onClose }: Props) {
  const [text, setText] = useState("");
  const input = useRef<HTMLTextAreaElement>(null);
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    input.current?.focus();
  }, [focusNonce]);
  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [messages]);
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
      {modelLabel ? (
        <p className="ai-note muted" data-testid="ai-mode">
          AI: {modelLabel}. The model proposes typed engine commands; each is dry-run and applied only when you Accept. It never
          certifies engineering numbers — run ERC/DRC.
        </p>
      ) : (
        <div className="ai-note" role="note" data-testid="ai-mode">
          <p>
            <strong>No AI model is configured</strong> (or its API key is missing). Requests run as <strong>offline commands</strong>: a fixed
            local keyword interpreter, not a model.
          </p>
          <button type="button" onClick={onOpenSettings}>Open model settings</button>
        </div>
      )}
      <div className="chat" role="log" aria-label="Assistant conversation">
        {messages.length === 0 && (
          <p className="empty-state">Ask for a change, or select a part, net or finding and choose “Ask AI”. Every change appears as a preview you accept or reject.</p>
        )}
        {messages.map((m) => (
          <div key={m.id} className={`msg ${m.role}${m.error ? " error" : ""}`}>
            {m.role === "assistant" && m.source === "offline" && <span className="muted hint">Offline command (no AI)</span>}
            {m.text && <p>{m.text}</p>}
            {m.step && <StepCard m={m as ChatMsg & { step: AiStep }} busy={busy} onAccept={onAccept} onReject={onReject} onRollback={onRollback} />}
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
        {!modelLabel && (
          <div className="row chips">
            {SUGGESTIONS.map((s) => (
              <button key={s} type="button" className="chip" onClick={() => send(s)} disabled={busy}>{s}</button>
            ))}
          </div>
        )}
        <textarea
          ref={input}
          aria-label="Ask the assistant"
          placeholder={modelLabel ? "Ask the AI for a change…  (Enter to send)" : "Offline command, e.g. rotate…  (Enter to send)"}
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
