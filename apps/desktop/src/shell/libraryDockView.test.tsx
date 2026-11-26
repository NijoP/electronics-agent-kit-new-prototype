// @vitest-environment jsdom
// Phase 1 D1 Library dock against a mocked Tauri IPC: scopes go to `library_browse`, the category
// tree and kind tabs change the grid, the grid pages, supplier columns come from the offer, Place
// arms the Place tool with the part, drag carries it, offline keeps built-in and project only, the
// common-parts palette places a variant, and standardisation shows the SOURCING report with what it
// does not check. The engine side of placing (PlaceSymbol copies the part into the project) is
// covered by flow.test.tsx and the D1 ui-evidence scenario.
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "../api";
import { getDraggedComponent } from "../dragState";
import type { ComponentDef, LibraryBrowse, LibraryEntry2, Report } from "../types";
import { LibraryDock } from "./LibraryDock";

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];

const def = (id: string, category: string, footprint: string): ComponentDef => ({
  id,
  name: `Part ${id}`,
  category,
  refdes_prefix: "R",
  default_value: "10k",
  manufacturer: "Maker",
  mpn: `MPN-${id}`,
  distributor: null,
  datasheet: null,
  symbol: {
    pins: [
      { number: "1", name: "A", electrical_type: "passive", position: { x: -5_000_000, y: 0 } },
      { number: "2", name: "B", electrical_type: "passive", position: { x: 5_000_000, y: 0 } },
    ],
    body_half_w: 2_000_000,
    body_half_h: 1_000_000,
  },
  footprint: {
    name: footprint,
    pads: [
      { number: "1", kind: "smd", shape: "rect", position: { x: -800_000, y: 0 }, size_x: 800_000, size_y: 900_000, drill: null },
      { number: "2", kind: "smd", shape: "rect", position: { x: 800_000, y: 0 }, size_x: 800_000, size_y: 900_000, drill: null },
    ],
    courtyard_half_w: 1_500_000,
    courtyard_half_h: 800_000,
    body_height: 500_000,
  },
  pin_map: { "1": ["1"], "2": ["2"] },
  ratings: [],
  model3d: { kind: "box", color: "#202020", external_ref: null, model_ref: null },
  provenance: "generic",
});

const entry = (d: ComponentDef, kind: "builtin" | "project" | "personal" = "builtin"): LibraryEntry2 => ({
  scope: { kind },
  scope_label: kind,
  id: d.id,
  def: d,
  favourite: false,
  tags: [],
  recent_rank: null,
  unverified_ratings: 0,
  has_datasheet: false,
  in_project: false,
  supplier: null,
});

// 23 resistors (two pages), one capacitor, the palette ids R_0603 / C_0805, one offer with price.
const builtin = (): LibraryEntry2[] => {
  const rs = Array.from({ length: 23 }, (_, i) => entry(def(`R_${String(i).padStart(2, "0")}`, "Resistor", "R_0603")));
  const offer = def("R_0603", "Resistor", "R_0603");
  offer.supplier_offers = [{ supplier: "distributor", supplier_part: "C25804", stock: 1200, price_breaks: [{ min_quantity: 100, unit_price: "0.0021", currency: "USD" }], fetched_at_unix_s: 1_700_000_000 }];
  return [entry(offer), ...rs, entry(def("C_0805", "Capacitor", "C_0805"))];
};

const browse = (source: string): LibraryBrowse => {
  const entries = source === "builtin" ? builtin() : source === "personal" ? [entry(def("MY_R", "Resistor", "R_0603"), "personal")] : [];
  return {
    entries,
    facets: { sources: [{ value: "builtin", count: 25 }, { value: "personal", count: 1 }], categories: [], manufacturers: [], packages: [], tags: [] },
    total: entries.length,
    problems: [],
    imported: [],
  };
};

const report: Report = {
  check: "SOURCING",
  project_id: "prj_1",
  revision: 7,
  input_sha256: "x",
  tool: "eak-validation",
  findings: [{ id: "SRC-001-a", rule_id: "SRC-001", severity: "error", message: "R_0603 (R1): MPN is MISSING", objects: ["sym_r1"], location: null }],
  outcome: "ERRORS",
  errors: 1,
  warnings: 0,
  not_implemented: ["Stock, price and lifecycle (NRND / EOL) checks"],
  limitations: ["Package comparison is textual"],
};

beforeEach(() => {
  calls = [];
  localStorage.clear();
  mockIPC((cmd, payload) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    switch (cmd) {
      case "library_browse":
        return browse(String((args.query as { source?: string }).source ?? ""));
      case "library_meta":
        return { favourites: [], tags: {}, recent: [] };
      case "part_standardisation":
        return report;
      case "footprint_verify":
        return {
          check: "footprint",
          definition_id: String(args.id),
          footprint: "R_0603",
          spec_source: null,
          tolerance_nm: 50_000,
          rule_results: [
            { rule_id: "FP-001", state: "PASS", reason: "" },
            { rule_id: "FP-002", state: "UNKNOWN", reason: "no land-pattern table" },
          ],
          overall: "UNKNOWN",
          not_implemented: ["FP-010 3D model bounding box against the footprint body"],
          limitations: [],
        };
      case "pin_pad_verify":
        return {
          check: "pin_pad",
          definition_id: String(args.id),
          footprint: "R_0603",
          exposed_pads: [],
          rule_results: [{ rule_id: "PP-001", state: "FAIL", reason: "pin 1 maps to missing pad 9" }],
          overall: "FAIL",
          not_implemented: [],
          limitations: [],
        };
      default:
        throw new Error(`unmocked ${cmd}`);
    }
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
  localStorage.clear();
});

function dock() {
  const onPlace = vi.fn();
  render(<LibraryDock revision={7} busy={false} execute={vi.fn()} onPlace={onPlace} onImported={vi.fn()} />);
  return onPlace;
}
const grid = () => screen.findByRole("table", { name: "Library grid" });
const browsed = () => calls.filter((c) => c.cmd === "library_browse").map((c) => c.args.query);
const pageText = () => screen.getByTestId("lib-page").textContent ?? "";

describe("Library dock (D1)", () => {
  it("browses the built-in scope by default, pages the grid and shows supplier, stock and price", async () => {
    dock();
    const g = await grid();
    expect(browsed()[0]).toEqual({ query: "", source: "builtin" });
    expect(within(g).getAllByRole("row")).toHaveLength(21); // header + 20
    expect(pageText()).toContain("1–20 of 25 part(s)");
    const first = within(g).getAllByRole("row")[1];
    expect(first.textContent).toContain("distributor C25804");
    expect(first.textContent).toContain("1,200");
    expect(first.textContent).toContain("0.0021 USD @ 100+");
    expect((screen.getByRole("button", { name: "Previous page" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(pageText()).toContain("21–25 of 25 part(s)");
    expect((screen.getByRole("button", { name: "Next page" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("filters by the category tree and sends the scope and the search to the backend", async () => {
    dock();
    await grid();
    const cats = screen.getByRole("list", { name: "Library categories" });
    fireEvent.click(within(cats).getByRole("button", { name: /^Capacitor/ }));
    expect(pageText()).toContain("1–1 of 1 part(s)");
    fireEvent.click(within(cats).getByRole("button", { name: /^Passives/ }));
    expect(pageText()).toContain("of 25 part(s)");
    fireEvent.click(screen.getByRole("button", { name: "Personal" }));
    await waitFor(() => expect(browsed().at(-1)).toEqual({ query: "", source: "personal" }));
    fireEvent.change(screen.getByLabelText("Search library"), { target: { value: "MY" } });
    await waitFor(() => expect(browsed().at(-1)).toEqual({ query: "MY", source: "personal" }));
  });

  it("Place arms the Place tool with the selected part, and the row drags it", async () => {
    const onPlace = dock();
    const g = await grid();
    const row = within(g).getByRole("button", { name: "Part R_0603" });
    fireEvent.dragStart(row, { dataTransfer: { setData: () => undefined, effectAllowed: "" } });
    expect(getDraggedComponent()?.id).toBe("R_0603");
    fireEvent.dragEnd(row);
    expect(getDraggedComponent()).toBeNull();
    fireEvent.click(row);
    const preview = screen.getByRole("region", { name: "Preview of Part R_0603" });
    expect(within(preview).getByRole("img", { name: "Symbol preview of Part R_0603" })).toBeTruthy();
    expect(within(preview).getByRole("img", { name: "Footprint preview of R_0603" })).toBeTruthy();
    fireEvent.click(within(preview).getByRole("button", { name: "Place on schematic" }));
    expect(onPlace).toHaveBeenCalledWith(expect.objectContaining({ id: "R_0603", name: "Part R_0603", refdes_prefix: "R" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "library_meta" && c.args.touch === true && c.args.id === "R_0603")).toBe(true));
  });

  it("the selected part shows its verification report: an unknown footprint rule reads as footprint unverified", async () => {
    dock();
    const g = await grid();
    fireEvent.click(within(g).getByRole("button", { name: "Part R_0603" }));
    const report = await screen.findByRole("region", { name: "Verification of R_0603" });
    const rules = await within(report).findByRole("table", { name: "Footprint rules" });
    expect(rules.textContent).toContain("no land-pattern table");
    const status = within(report).getByLabelText("Footprint status");
    expect(status.textContent).toBe("Footprint unverified");
    expect(status.getAttribute("data-verified")).toBe("false");
    expect(within(report).getByLabelText("Pin to pad status").textContent).toBe("Pin-to-pad unverified");
    expect(within(report).getByRole("table", { name: "Pin to pad rules" }).textContent).toContain("missing pad 9");
    expect(within(report).getByText(/Not implemented: FP-010/)).toBeTruthy();
  });

  it("Footprint and 3D kinds show footprints with their parts and the generated body", async () => {
    const onPlace = dock();
    await grid();
    fireEvent.click(screen.getByRole("tab", { name: "Footprint" }));
    const g = await grid();
    expect(within(g).getAllByRole("row")).toHaveLength(3); // header + C_0805 + R_0603
    fireEvent.click(within(g).getByRole("button", { name: "R_0603" }));
    const parts = screen.getByRole("list", { name: "Parts using R_0603" });
    expect(within(parts).getAllByRole("button")).toHaveLength(24);
    fireEvent.click(within(parts).getAllByRole("button")[0]);
    expect(onPlace).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("tab", { name: "3D" }));
    fireEvent.click(within(await grid()).getByRole("button", { name: "Part R_0603" }));
    expect(screen.getByRole("img", { name: "3D body preview of Part R_0603" })).toBeTruthy();
    expect(screen.getByText(/No model file: the 3D view draws this generated box/)).toBeTruthy();
  });

  it("offline mode keeps the built-in and project scopes only, and is remembered", async () => {
    dock();
    await grid();
    fireEvent.click(screen.getByRole("button", { name: "Personal" }));
    await waitFor(() => expect(browsed().at(-1)).toMatchObject({ source: "personal" }));
    fireEvent.click(screen.getByLabelText("Offline mode"));
    await waitFor(() => expect(browsed().at(-1)).toMatchObject({ source: "builtin" }));
    for (const s of ["Personal", "Imported", "Distributor"]) {
      const b = screen.getByRole("button", { name: s }) as HTMLButtonElement;
      expect(b.disabled, s).toBe(true);
      expect(b.title).toMatch(/^Offline mode/);
    }
    expect((screen.getByRole("button", { name: "Project" }) as HTMLButtonElement).disabled).toBe(false);
    expect(localStorage.getItem("eak.library.offline")).toBe("1");
    cleanup();
    dock();
    expect(((await screen.findByLabelText("Offline mode")) as HTMLInputElement).checked).toBe(true);
  });

  it("the common-parts palette places a package variant of a built-in part", async () => {
    const onPlace = dock();
    fireEvent.click(screen.getByRole("tab", { name: "Common parts" }));
    const r = await screen.findByRole("group", { name: "Resistor" });
    fireEvent.click(within(r).getByRole("button", { name: "Place Part R_0603" }));
    expect(onPlace).toHaveBeenCalledWith(expect.objectContaining({ id: "R_0603" }));
    // Only variants the built-in library lists are offered.
    expect(within(screen.getByRole("group", { name: "Capacitor (MLCC)" })).getAllByRole("button")).toHaveLength(1);
    expect(screen.queryByRole("group", { name: "Transistor" })).toBeNull();
  });

  it("standardisation shows the SOURCING findings and what is not checked", async () => {
    dock();
    fireEvent.click(screen.getByRole("tab", { name: "Standardisation" }));
    const t = await screen.findByRole("table", { name: "Standardisation findings" });
    expect(t.textContent).toContain("MPN is MISSING");
    expect(screen.getByRole("status").textContent).toContain("1 error(s)");
    expect(screen.getByRole("list", { name: "Not implemented" }).textContent).toContain("lifecycle");
    expect(screen.getByRole("list", { name: "Limitations" }).textContent).toContain("textual");
    fireEvent.click(screen.getByRole("button", { name: "Run again" }));
    await waitFor(() => expect(calls.filter((c) => c.cmd === "part_standardisation")).toHaveLength(2));
  });
});
