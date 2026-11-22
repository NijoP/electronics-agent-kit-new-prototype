// Recording CanvasRenderingContext2D stand-in for the renderer tests (jsdom has no 2D context).
// Every method call is logged as `{ fn, args }`; text is measured as 0.6 em per character.
export interface Call {
  fn: string;
  args: unknown[];
}

export interface StubCtx {
  calls: Call[];
  canvas: { width: number; height: number };
  ctx: CanvasRenderingContext2D;
}

const METHODS = [
  "setTransform",
  "clearRect",
  "save",
  "restore",
  "beginPath",
  "rect",
  "clip",
  "moveTo",
  "lineTo",
  "stroke",
  "fill",
  "closePath",
  "arc",
  "arcTo",
  "fillText",
  "drawImage",
  "translate",
  "rotate",
  "setLineDash",
];

export function stubContext(width = 0, height = 0, log?: Call[]): StubCtx {
  const calls = log ?? [];
  const canvas = { width, height };
  const state: Record<string, unknown> = {
    globalAlpha: 1,
    font: "10px sans-serif",
    lineWidth: 1,
    strokeStyle: "#000",
    fillStyle: "#000",
    lineCap: "butt",
    lineJoin: "miter",
    textAlign: "start",
    textBaseline: "alphabetic",
  };
  const obj: Record<string, unknown> = { canvas };
  for (const m of METHODS) obj[m] = (...args: unknown[]) => void calls.push({ fn: m, args });
  obj.measureText = (text: string) => {
    const px = Number(/([\d.]+)px/.exec(String(state.font))?.[1] ?? 10);
    return { width: text.length * px * 0.6 };
  };
  for (const k of Object.keys(state))
    Object.defineProperty(obj, k, {
      get: () => state[k],
      set: (v: unknown) => {
        state[k] = v;
      },
    });
  return { calls, canvas, ctx: obj as unknown as CanvasRenderingContext2D };
}

/** A surface factory over stub contexts that share one call log. */
export function stubSurfaces(log: Call[]) {
  let made = 0;
  const create = (w: number, h: number) => {
    made++;
    const s = stubContext(w, h, log);
    return { canvas: s.canvas as unknown as HTMLCanvasElement, ctx: s.ctx };
  };
  return { create, count: () => made };
}
