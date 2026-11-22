// Toolbar grouping and narrow-window overflow (written during the testing freeze; not yet run).
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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

// Slice A2: icon buttons, split buttons that remember the last pick, and the labels option.
describe("Toolbar icons and split buttons", () => {
  beforeEach(() => localStorage.clear());
  const zone = vi.fn();
  const outline = vi.fn();
  const fam: ToolbarGroup[] = [
    [
      { id: "save", group: "file", label: "Save", icon: "save", shortcut: "Mod+S", title: "Save the project", run: vi.fn() },
      { id: "t-outline", group: "edit", label: "Board outline", icon: "outline", split: "pcb-regions", run: outline },
      { id: "t-zone", group: "edit", label: "Copper zone", icon: "zone", split: "pcb-regions", run: zone },
      { id: "t-keep", group: "edit", label: "Keep-out", icon: "keepout", split: "pcb-regions", disabled: "Not implemented: no keep-out command" },
    ],
  ];

  it("icon buttons keep the label as accessible name and show tooltip plus shortcut; text appears only with labels on", () => {
    const { container, unmount } = render(<Toolbar groups={[fam[0].slice(0, 1)]} />);
    const save = screen.getByRole("button", { name: "Save" });
    expect(save.querySelector('svg[data-icon="save"]')).toBeTruthy();
    expect(save.textContent).toBe("");
    expect(save.title).toMatch(/^Save the project \((Ctrl\+S|⌘S)\)$/);
    expect(container.querySelector(".toolbar.labels")).toBeNull();
    unmount();
    render(<Toolbar groups={[fam[0].slice(0, 1)]} labels />);
    expect(screen.getByRole("button", { name: "Save" }).textContent).toBe("Save");
  });

  it("a family is one split button: the main part runs the first enabled member until another is picked, then remembers it", () => {
    const { unmount } = render(<Toolbar groups={fam} />);
    expect(screen.queryByRole("button", { name: "Copper zone" })).toBeNull(); // folded into the split button
    fireEvent.click(screen.getByRole("button", { name: "Board outline" }));
    expect(outline).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Board regions: choose" }));
    const items = screen.getAllByRole("menuitemradio");
    expect(items.map((i) => i.textContent)).toEqual(["Board outline", "Copper zone", "Keep-outNot implemented: no keep-out command"]);
    expect((items[2] as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(items[1]);
    expect(zone).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu", { name: "Board regions" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Copper zone" }));
    expect(zone).toHaveBeenCalledTimes(2);
    expect(localStorage.getItem("eak.toolbar.split.pcb-regions")).toBe("t-zone");
    unmount();
    // a new session starts on the remembered pick
    render(<Toolbar groups={fam} />);
    expect(screen.getByRole("button", { name: "Copper zone" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Board outline" })).toBeNull();
  });

  it("extras render inside their group (the grid combos in View)", () => {
    render(<Toolbar groups={[[{ id: "fit", group: "view", label: "Fit to view", icon: "fit", run: vi.fn() }]]} extras={{ view: <select aria-label="Grid spacing" /> }} />);
    expect(screen.getByRole("group", { name: "View" }).querySelector('select[aria-label="Grid spacing"]')).toBeTruthy();
  });
});
