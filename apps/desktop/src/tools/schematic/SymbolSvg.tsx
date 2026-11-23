// SVG markup of one placed schematic symbol (slice B2): body graphics, pin lines with their
// connection dots, pin names and numbers. Geometry comes from `render/symbolDraw.ts`.
import type { CSSProperties } from "react";
import type { BodyPrim, SymbolDrawing, TextPrim } from "../../render/symbolDraw";

const strokeStyle = (b: BodyPrim): CSSProperties | undefined =>
  b.width !== undefined || b.dash ? { ...(b.width !== undefined ? { strokeWidth: b.width } : {}), ...(b.dash ? { strokeDasharray: b.dash } : {}) } : undefined;

/** One body primitive; class `body` plus its fill mode (`fill-body`, `fill-solid`, `fill-none`). */
export function BodyShape({ b }: { b: BodyPrim }) {
  const className = `body fill-${b.fill}`;
  const style = strokeStyle(b);
  switch (b.kind) {
    case "rect":
      return <rect className={className} x={b.x} y={b.y} width={b.w} height={b.h} style={style} />;
    case "circle":
      return <circle className={className} cx={b.cx} cy={b.cy} r={b.r} style={style} />;
    case "path":
      return <path className={className} d={b.d} style={style} />;
  }
}

/** Symbol text at its own size (inline: the canvas-wide text size rule must not apply). */
export function SymbolText({ t }: { t: TextPrim }) {
  return (
    <text
      className={`sym-text ${t.role}`}
      x={t.x}
      y={t.y}
      transform={t.rotate ? `rotate(${t.rotate} ${t.x} ${t.y})` : undefined}
      style={{ fontSize: t.size, textAnchor: t.anchor }}
    >
      {t.text}
    </text>
  );
}

/** Pins (behind the body, so a box covers its stubs), the body, then the texts. */
export function SymbolSvg({ drawing, activePin }: { drawing: SymbolDrawing; activePin?: string | null }) {
  return (
    <>
      {drawing.pins.map((p) => {
        const active = activePin === p.number;
        return (
          <g key={p.number} data-pin={p.number}>
            {(p.at[0] !== p.inner[0] || p.at[1] !== p.inner[1]) && <line className="pin" x1={p.inner[0]} y1={p.inner[1]} x2={p.at[0]} y2={p.at[1]} />}
            <circle className={active ? "pin-end active" : "pin-end"} cx={p.at[0]} cy={p.at[1]} r={active ? 0.8 : 0.4} />
          </g>
        );
      })}
      {drawing.body.map((b, i) => (
        <BodyShape key={i} b={b} />
      ))}
      {drawing.texts.map((t, i) => (
        <SymbolText key={i} t={t} />
      ))}
    </>
  );
}
