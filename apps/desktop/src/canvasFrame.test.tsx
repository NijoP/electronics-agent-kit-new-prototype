// Per-frame pointer coalescing in Canvas (PERFORMANCE_LEDGER.md L4). Written, not run
// (no-build directive): the first move of a frame is handled at once, later moves in the same
// frame are folded into one, and release flushes it so the drag ends where the pointer was.
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Canvas } from "./Canvas";

beforeEach(() => {
  Element.prototype.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: 100, bottom: 100, width: 100, height: 100, toJSON: () => ({}) }) as DOMRect;
});
afterEach(cleanup);

it("folds pointer moves within a frame and flushes the last one on release", () => {
  const move = vi.fn();
  const end = vi.fn();
  const { container } = render(
    <Canvas label="test" bounds={{ minX: 0, minY: 0, maxX: 100, maxY: 100 }} onDrag={{ start: () => true, move, end, cancel: vi.fn() }}>
      <g />
    </Canvas>,
  );
  const svg = container.querySelector("svg")!;
  fireEvent.pointerDown(svg, { clientX: 0, clientY: 0, button: 0, buttons: 1 });
  fireEvent.pointerMove(svg, { clientX: 10, clientY: 0, buttons: 1 });
  fireEvent.pointerMove(svg, { clientX: 20, clientY: 0, buttons: 1 });
  fireEvent.pointerMove(svg, { clientX: 30, clientY: 0, buttons: 1 });
  expect(move).toHaveBeenCalledTimes(1);
  fireEvent.pointerUp(svg, { clientX: 30, clientY: 0, button: 0 });
  expect(move).toHaveBeenCalledTimes(2);
  expect(end).toHaveBeenCalledTimes(1);
  // The folded move is the last position, not an intermediate one.
  expect(move.mock.calls[1][0]).toBeCloseTo(end.mock.calls[0][0]);
});
