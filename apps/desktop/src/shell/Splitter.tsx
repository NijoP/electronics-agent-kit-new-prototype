// Draggable (and keyboard-operable) divider between two panels. `onResize` receives the
// pointer delta in px along the axis; `sign` flips it for panels that grow the other way.
import { useRef } from "react";

interface Props {
  orientation: "vertical" | "horizontal";
  label: string;
  value: number;
  onResize: (delta: number) => void;
  onReset: () => void;
}

export function Splitter({ orientation, label, value, onResize, onReset }: Props) {
  const last = useRef<number | null>(null);
  const pos = (e: { clientX: number; clientY: number }) => (orientation === "vertical" ? e.clientX : e.clientY);
  return (
    <div
      className={`splitter ${orientation}`}
      role="separator"
      aria-orientation={orientation}
      aria-label={label}
      aria-valuenow={value}
      tabIndex={0}
      title={`${label} — drag, arrow keys resize, double-click resets`}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture?.(e.pointerId);
        last.current = pos(e);
      }}
      onPointerMove={(e) => {
        if (last.current === null) return;
        const p = pos(e);
        onResize(p - last.current);
        last.current = p;
      }}
      onPointerUp={() => (last.current = null)}
      onDoubleClick={onReset}
      onKeyDown={(e) => {
        const step = e.shiftKey ? 48 : 16;
        const fwd = orientation === "vertical" ? "ArrowRight" : "ArrowDown";
        const back = orientation === "vertical" ? "ArrowLeft" : "ArrowUp";
        if (e.key === fwd) onResize(step);
        else if (e.key === back) onResize(-step);
        else return;
        e.preventDefault();
      }}
    />
  );
}
