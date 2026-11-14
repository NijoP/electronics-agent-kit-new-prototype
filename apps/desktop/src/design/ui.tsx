// Shared UI primitives (docs/design-system.md, "Components"). Each one is the single version of a
// pattern that used to be hand-written per screen; styles come from design/tokens.css via
// styles.css. They render real state only: callers pass what the engine or runtime reported.
import type { ReactNode } from "react";

/** Status colours of the design system. */
export type Tone = "ok" | "warn" | "bad" | "info" | "neutral";

/** A small round status mark. `state` is a runtime state name (terminal `SessionState`) or a tone. */
export function StatusDot({ state, label }: { state: string; label?: string }) {
  const cls = ["ok", "warn", "bad", "info"].includes(state) ? state : `st-${state}`;
  return label ? (
    <span className={`dot ${cls}`} role="img" aria-label={label} title={label} />
  ) : (
    <span className={`dot ${cls}`} aria-hidden="true" />
  );
}

/** A status word in a pill (counts, "No ERC run", "Writes blocked"). */
export function Pill({ tone = "neutral", children, title }: { tone?: Tone; children: ReactNode; title?: string }) {
  return (
    <span className={`pill ${tone}`} title={title}>
      {children}
    </span>
  );
}

/** Nothing to show yet: what is missing and, optionally, the one action that fixes it. */
export function EmptyState({ title, children, action }: { title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="empty-block" role="note">
      <strong>{title}</strong>
      {children && <p className="muted">{children}</p>}
      {action}
    </div>
  );
}

/** A section with an uppercase caption and an optional right-aligned action (panels, overview). */
export function Section({ title, aside, children, label }: { title: string; aside?: ReactNode; children: ReactNode; label?: string }) {
  return (
    <section className="ui-section" aria-label={label ?? title}>
      <header className="ui-section-head">
        <h3>{title}</h3>
        {aside}
      </header>
      {children}
    </section>
  );
}

/** A name and its value on one row (property panels, overview facts). Use inside `<dl className="facts">`. */
export function Fact({ name, children }: { name: string; children: ReactNode }) {
  return (
    <div className="fact">
      <dt>{name}</dt>
      <dd>{children}</dd>
    </div>
  );
}
