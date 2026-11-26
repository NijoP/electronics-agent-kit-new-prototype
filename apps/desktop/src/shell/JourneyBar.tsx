// Journey bar (report §10 S2): one row above the documents with the stages of the design journey,
// each coloured from the facts of the revision on screen (journey.ts), and "Next" naming the first
// stage that is not done. A click on a stage goes to the view that acts on it.
import { nextStage, STATE_TEXT, type Stage, type StageId } from "./journey";

interface Props {
  stages: Stage[];
  onStage: (id: StageId) => void;
  /** Why a stage cannot be opened in this window (e.g. no 3D view registered); null when it can. */
  blocked?: (id: StageId) => string | null;
}

export function JourneyBar({ stages, onStage, blocked }: Props) {
  const next = nextStage(stages);
  return (
    <nav className="journey" aria-label="Design journey">
      <ol>
        {stages.map((s, i) => {
          const why = blocked?.(s.id) ?? null;
          return (
            <li key={s.id}>
              {i > 0 && <span className="journey-link" aria-hidden="true" />}
              <button
                type="button"
                className={next?.id === s.id ? "journey-stage next" : "journey-stage"}
                data-stage={s.id}
                data-state={s.state}
                aria-label={`${s.label}: ${STATE_TEXT[s.state]}`}
                aria-current={next?.id === s.id ? "step" : undefined}
                disabled={why !== null}
                title={why ?? s.reason}
                onClick={() => onStage(s.id)}
              >
                <span className="journey-mark" aria-hidden="true" />
                {s.label}
              </button>
            </li>
          );
        })}
      </ol>
      <span className="journey-next" title={next?.reason}>
        {next ? `Next · ${next.label}: ${next.reason}` : "Every stage is done for this revision"}
      </span>
    </nav>
  );
}
