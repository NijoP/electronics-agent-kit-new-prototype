// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditCtx } from "../../EditContext";
import { fixture } from "../../fixture.test-data";
import { SchematicView } from "../../SchematicView";
import type { Command, Snapshot } from "../../types";
import { boundsOf, fit, type ViewBox } from "../../viewport";
import { setDraggedComponent, EAK_COMPONENT_MIME } from "../../dragState";

const SIZE = 1000;
const mm = (v: number) => v * 1_000_000;

let sent: Command[];
let notify: ReturnType<typeof vi.fn<(m: string) => void>>;
const edit = (): EditCtx => ({
  execute: async (c) => {
    sent.push(...(Array.isArray(c) ? c : [c]));
  },
  busy: false,
  library: [
    {
      id: "C_0603",
      name: "Capacitor 0603",
      category: "Capacitor",
      refdes_prefix: "C",
      default_value: "100n",
      pins: [
        ["1", "~"],
        ["2", "~"],
      ],
    },
  ],
  notify,
});

beforeEach(() => {
  sent = [];
  notify = vi.fn<(m: string) => void>();
  Element.prototype.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: SIZE, bottom: SIZE, width: SIZE, height: SIZE, toJSON: () => ({}) }) as DOMRect;
});
afterEach(() => {
  cleanup();
  setDraggedComponent(null);
});

const vb: ViewBox = fit(
  boundsOf([
    [0, 0],
    [297, -210],
  ])!,
);

/** Client coordinates of an EAK-IR point given in mm (Y up). */
function client(x: number, y: number) {
  const s = Math.min(SIZE / vb.w, SIZE / vb.h);
  return { clientX: (x - vb.x) * s + (SIZE - vb.w * s) / 2, clientY: (-y - vb.y) * s + (SIZE - vb.h * s) / 2 };
}

describe("Schematic component drag-and-drop & click-to-place", () => {
  it("drops a component from library onto schematic canvas and executes PlaceSymbol", () => {
    const snap: Snapshot = fixture();
    render(<SchematicView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} />);

    const canvas = screen.getByRole("img", { name: "Schematic canvas" });

    // Set dragged component (as when dragging from LibraryPanel)
    setDraggedComponent({ id: "C_0603", name: "Capacitor 0603", category: "Capacitor" });

    const c = client(50, 50);
    const dataTransfer = {
      types: [EAK_COMPONENT_MIME, "text/plain"],
      getData: (mime: string) => (mime === EAK_COMPONENT_MIME ? JSON.stringify({ id: "C_0603", name: "Capacitor 0603" }) : "C_0603"),
      setData: vi.fn(),
      dropEffect: "none",
    };

    // Drag over canvas
    fireEvent.dragOver(canvas, { ...c, dataTransfer });
    expect(screen.getByText(/Drop component to place/)).toBeDefined();

    // Drop on canvas
    fireEvent.drop(canvas, { ...c, dataTransfer });

    expect(sent.length).toBe(1);
    const cmd = sent[0];
    expect(cmd.type).toBe("PlaceSymbol");
    if (cmd.type === "PlaceSymbol") {
      expect(cmd.component_id).toBe("C_0603");
      expect(cmd.rotation).toBe("R0");
      // Snapped to grid: coordinate close to 50mm
      expect(Math.abs(cmd.position.x - mm(50))).toBeLessThan(mm(2));
      expect(Math.abs(cmd.position.y - mm(50))).toBeLessThan(mm(2));
    }
  });

  it("rotates during placement with R key and commits with rotated orientation", () => {
    const snap: Snapshot = fixture();
    render(<SchematicView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} armPart={{ id: "C_0603", n: Date.now() }} />);

    // Placement mode armed
    expect(screen.getByText(/Click sheet to place/)).toBeDefined();

    // Press 'r' to rotate
    fireEvent.keyDown(window, { key: "r" });
    expect(screen.getByText(/R90/)).toBeDefined();

    fireEvent.keyDown(window, { key: "r" });
    expect(screen.getByText(/R180/)).toBeDefined();

    // Click to place
    const canvas = screen.getByRole("img", { name: "Schematic canvas" });
    const c = client(60, 40);
    fireEvent.pointerDown(canvas, { ...c, button: 0 });
    fireEvent.pointerUp(canvas, { ...c, button: 0 });

    expect(sent.length).toBe(1);
    const cmd = sent[0];
    expect(cmd.type).toBe("PlaceSymbol");
    if (cmd.type === "PlaceSymbol") {
      expect(cmd.component_id).toBe("C_0603");
      expect(cmd.rotation).toBe("R180");
    }
  });

  it("exits placement mode on Escape", () => {
    const snap: Snapshot = fixture();
    render(<SchematicView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} armPart={{ id: "C_0603", n: Date.now() }} />);

    expect(screen.getByText(/Click sheet to place/)).toBeDefined();

    // Press Escape
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByText(/Click sheet to place/)).toBeNull();
  });
});
