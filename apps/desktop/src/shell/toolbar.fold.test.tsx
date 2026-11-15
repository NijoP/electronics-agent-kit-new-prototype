// Toolbar grouping and narrow-window overflow (written during the testing freeze; not yet run).
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Toolbar, type ToolbarGroup } from "./Toolbar";

const groups: ToolbarGroup[] = [
  [{ id: "save", group: "file", label: "Save", run: vi.fn() }],
  [{ id: "undo", group: "edit", label: "Undo", disabled: "Nothing to undo" }],
  [{ id: "export", group: "mfg", label: "Export", run: vi.fn() }],
];

afterEach(cleanup);

describe("Toolbar", () => {
  it("shows every group inline with captions when the width is unknown", () => {
    render(<Toolbar groups={groups} />);
    expect(screen.getByRole("group", { name: "Manufacturing" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /More/ })).toBeNull();
  });

  it("folds low-priority groups into More on a narrow window and keeps disabled reasons", () => {
    const orig = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, value: 700 });
    class RO { constructor(private cb: () => void) {} observe() { this.cb(); } disconnect() {} unobserve() {} }
    vi.stubGlobal("ResizeObserver", RO);
    try {
      render(<Toolbar groups={groups} />);
      expect(screen.queryByRole("group", { name: "Manufacturing" })).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: /More/ }));
      expect(screen.getByRole("button", { name: "Export" })).toBeTruthy();
      expect((screen.getByRole("button", { name: "Undo" }) as HTMLButtonElement).title).toContain("Nothing to undo");
    } finally {
      vi.unstubAllGlobals();
      if (orig) Object.defineProperty(HTMLElement.prototype, "clientWidth", orig);
    }
  });
});
