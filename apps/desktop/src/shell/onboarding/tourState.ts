// First-run tour (slice S1): four short steps shown on Project Home at first launch. It can be
// skipped at any step and replayed from Help (Home's Help menu or the workspace Help menu). The
// only thing stored is whether it was finished or skipped, in this window's local storage.

export const TOUR_KEY = "eak.onboarding.tour";

export type TourState = "unseen" | "done" | "skipped";

export function tourState(): TourState {
  try {
    const v = localStorage.getItem(TOUR_KEY);
    return v === "done" || v === "skipped" ? v : "unseen";
  } catch {
    return "unseen";
  }
}

export function setTourState(s: Exclude<TourState, "unseen">): void {
  try {
    localStorage.setItem(TOUR_KEY, s);
  } catch {
    /* storage unavailable: the tour shows again next launch */
  }
}

export interface TourStep {
  id: string;
  title: string;
  body: string;
  /** `data-tour` name of the Home region the step points at (highlighted while it is shown). */
  target?: string;
}

/** Original copy for EAK; each step names a control that exists and works. */
export const TOUR_STEPS: readonly TourStep[] = [
  {
    id: "start",
    title: "Start from what the board must do",
    body: "Describe the board in the box and press Start. EAK creates the project and records your sentences as its design brief. No AI is used for this.",
    target: "prompt",
  },
  {
    id: "projects",
    title: "New design, sample board or import",
    body: "New design asks for a name and a folder. Open sample board builds a small reference board you can explore. Import brings KiCad symbols and footprints into your library.",
    target: "projects",
  },
  {
    id: "continue",
    title: "Continue where you stopped",
    body: "Continue shows facts read from the project folder: check errors on the current revision, unrouted nets, BOM risk and how many release gates pass. Each Needs attention item opens the place that fixes it.",
    target: "continue",
  },
  {
    id: "evidence",
    title: "Checks are evidence, not promises",
    body: "ERC, DRC and the release check report what they ran. A check not run on the current revision says so and never counts as a pass, and EAK never calls a design manufacturing-ready. Replay this tour from Help.",
  },
];
