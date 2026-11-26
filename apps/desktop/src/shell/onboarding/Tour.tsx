// First-run tour panel. Non-modal on purpose: it sits in Home's page flow (or, when replayed from
// the workspace Help menu, in a corner) and never blocks the controls it describes.
import { useEffect, useState } from "react";
import { TOUR_STEPS, type TourStep } from "./tour";

interface Props {
  /** The tour ended: finished on the last step, or skipped. */
  onClose: (how: "done" | "skipped") => void;
  /** The step shown changed (Home highlights `step.target`). */
  onStep?: (step: TourStep) => void;
  floating?: boolean;
}

export function Tour({ onClose, onStep, floating = false }: Props) {
  const [i, setI] = useState(0);
  const step = TOUR_STEPS[i];
  const last = i === TOUR_STEPS.length - 1;
  useEffect(() => onStep?.(step), [step, onStep]);
  return (
    <section role="region" aria-label="First-run tour" className={floating ? "tour floating" : "tour"} data-step={step.id}>
      <div className="tour-head">
        <span className="tour-count">
          Step {i + 1} of {TOUR_STEPS.length}
        </span>
        <span className="spacer" />
        {!last && (
          <button type="button" className="ghost" onClick={() => onClose("skipped")}>
            Skip tour
          </button>
        )}
      </div>
      <h3 className="tour-title">{step.title}</h3>
      <p className="tour-body">{step.body}</p>
      <div className="tour-actions">
        <ol className="tour-dots" aria-hidden="true">
          {TOUR_STEPS.map((s, k) => (
            <li key={s.id} className={k === i ? "on" : k < i ? "past" : undefined} />
          ))}
        </ol>
        <span className="spacer" />
        {i > 0 && (
          <button type="button" className="ghost" onClick={() => setI(i - 1)}>
            Back
          </button>
        )}
        {last ? (
          <button type="button" className="primary" onClick={() => onClose("done")}>
            Done
          </button>
        ) : (
          <button type="button" className="primary" onClick={() => setI(i + 1)}>
            Next
          </button>
        )}
      </div>
    </section>
  );
}
