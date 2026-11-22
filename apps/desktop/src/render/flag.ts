// PCB renderer flag (slice A4 mitigation: the SVG path stays available until the Canvas2D path
// has proven parity). The choice is per machine (localStorage `eak.pcb.renderer`).
//   "canvas": the Canvas2D scene renderer (default wherever a 2D canvas context exists);
//   "svg":    the original SVG markup, kept as the fallback.
// Environments without a usable 2D canvas (the jsdom unit tests, a context that fails to start)
// always get SVG, and the PCB header says why Canvas2D is unavailable.
import { useCallback, useState } from "react";

export type PcbRenderer = "canvas" | "svg";
export const RENDERER_KEY = "eak.pcb.renderer";

let probed: boolean | null = null;
/** Whether this environment can create a 2D canvas context (probed once). */
export function canvas2dAvailable(): boolean {
  if (probed !== null) return probed;
  probed = false;
  try {
    // jsdom defines HTMLCanvasElement but implements no context; probing it only logs noise.
    if (typeof document === "undefined" || typeof navigator === "undefined" || /jsdom/i.test(navigator.userAgent)) return probed;
    probed = !!document.createElement("canvas").getContext("2d");
  } catch {
    probed = false;
  }
  return probed;
}

/** Tests only: force the probe result (null re-probes). */
export function setCanvas2dAvailableForTests(v: boolean | null) {
  probed = v;
}

function stored(): PcbRenderer | null {
  try {
    const v = localStorage.getItem(RENDERER_KEY);
    return v === "svg" || v === "canvas" ? v : null;
  } catch {
    return null;
  }
}

/** The renderer to use now: the stored choice when it can run, else the best available. */
export function pcbRenderer(): PcbRenderer {
  if (!canvas2dAvailable()) return "svg";
  return stored() ?? "canvas";
}

/** [renderer in use, choose one (persisted), whether Canvas2D can run here]. */
export function usePcbRenderer(): [PcbRenderer, (r: PcbRenderer) => void, boolean] {
  const [r, setR] = useState<PcbRenderer>(pcbRenderer);
  const set = useCallback((next: PcbRenderer) => {
    try {
      localStorage.setItem(RENDERER_KEY, next);
    } catch {
      /* storage unavailable: the choice lasts for this view only */
    }
    setR(next === "canvas" && !canvas2dAvailable() ? "svg" : next);
  }, []);
  return [r, set, canvas2dAvailable()];
}
