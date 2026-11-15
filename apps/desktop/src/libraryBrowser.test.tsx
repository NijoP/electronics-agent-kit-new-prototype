// @vitest-environment jsdom
// Library browser against a mocked Tauri IPC: filters go to the backend, favourites/tags/personal
// saves are typed calls, imported libraries show their licence, and a part's detail shows the
// pin-to-pad mapping. The Rust side is tested headless in src-tauri/tests/library_ipc.rs.
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { LibraryBrowser } from "./LibraryBrowser";
import type { ComponentDef, LibraryBrowse, LibraryEntry2 } from "./types";

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];

const def = (id: string): ComponentDef => ({
  id,
  name: `Resistor ${id}`,
  category: "Resistor",
  refdes_prefix: "R",
  default_value: "10k",
  manufacturer: "Mine Co",
  mpn: "R-1",
  distributor: null,
  datasheet: null,
  symbol: {
    pins: [
      { number: "1", name: "~", electrical_type: "passive", position: { x: -5_000_000, y: 0 } },
      { number: "2", name: "~", electrical_type: "passive", position: { x: 5_000_000, y: 0 } },
    ],
    body_half_w: 2_000_000,
    body_half_h: 1_000_000,
  },
  footprint: {
    name: "R_0603",
    pads: [
      { number: "A", kind: "smd", shape: "rect", position: { x: -800_000, y: 0 }, size_x: 800_000, size_y: 900_000, drill: null },
      { number: "B", kind: "smd", shape: "rect", position: { x: 800_000, y: 0 }, size_x: 800_000, size_y: 900_000, drill: null },
    ],
    courtyard_half_w: 1_500_000,
    courtyard_half_h: 800_000,
    body_height: null,
  },
  pin_map: { "1": ["A"], "2": ["B"] },
  ratings: [{ name: "power", value: 0.1, unit: "W", source: "unverified", verified: false }],
  provenance: "saved to the personal library",
});

const entry = (id: string, scope: LibraryEntry2["scope"], favourite = false): LibraryEntry2 => ({
  scope,
  scope_label: scope.kind === "imported" ? `imported:${scope.name}` : scope.kind,
  id,
  def: def(id),
  favourite,
  tags: favourite ? ["bench"] : [],
  recent_rank: null,
  unverified_ratings: 1,
  has_datasheet: false,
  in_project: false,
  supplier: { distributor: "C25804", stock: 42, as_of_unix_s: 1_700_000_000 },
});

const browse = (): LibraryBrowse => ({
  entries: [entry("R_0603", { kind: "personal" }, true), entry("R_0603", { kind: "builtin" }), entry("OSS_R", { kind: "imported", name: "kicad-lite" })],
  facets: {
    sources: [
      { value: "builtin", count: 1 },
      { value: "personal", count: 1 },
    ],
    categories: [{ value: "Resistor", count: 3 }],
    manufacturers: [{ value: "Mine Co", count: 3 }],
    packages: [{ value: "R_0603", count: 3 }],
    tags: [{ value: "bench", count: 1 }],
  },
  total: 3,
  problems: [],
  imported: [{ manifest: { name: "kicad-lite", licence: "CC-BY-SA-4.0", source_url: "https://example.invalid/lib" }, components: 1 }],
});

beforeEach(() => {
  calls = [];
  mockIPC((cmd, payload) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    calls.push({ cmd, args: (payload ?? {}) as Record<string, unknown> });
    switch (cmd) {
      case "library_browse":
        return browse();
      case "library_meta":
        return { favourites: [], tags: {}, recent: [] };
      case "library_personal_save":
      case "library_personal_remove":
      case "library_imported_remove":
        return null;
      default:
        throw new Error(`unmocked ${cmd}`);
    }
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("LibraryBrowser", () => {
  it("lists every scope separately and passes filters to the backend", async () => {
    render(<LibraryBrowser />);
    await screen.findByRole("table", { name: "Library results" });
    // The same id from two scopes stays two rows.
    expect(screen.getAllByRole("button", { name: "R_0603" })).toHaveLength(2);
    expect(screen.getByText("imported:kicad-lite")).toBeTruthy();
    expect(calls[0]).toEqual({ cmd: "library_browse", args: { query: {} } });

    fireEvent.change(screen.getByLabelText("Source"), { target: { value: "personal" } });
    await waitFor(() => expect(calls.at(-1)).toEqual({ cmd: "library_browse", args: { query: { source: "personal" } } }));
    fireEvent.click(screen.getByLabelText("Favourites"));
    await waitFor(() => expect(calls.at(-1)?.args).toMatchObject({ query: { source: "personal", favourites_only: true } }));
  });

  it("toggles favourites and saves/removes personal copies through typed calls", async () => {
    render(<LibraryBrowser />);
    await screen.findByRole("table", { name: "Library results" });
    fireEvent.click(screen.getByRole("button", { name: "Favourite OSS_R" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "library_meta" && c.args.favourite === true && c.args.id === "OSS_R")).toBe(true));

    const saves = screen.getAllByRole("button", { name: "Save to personal" });
    fireEvent.click(saves[0] as HTMLElement);
    await waitFor(() => expect(calls.some((c) => c.cmd === "library_personal_save")).toBe(true));
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "library_personal_remove" && c.args.id === "R_0603")).toBe(true));
  });

  it("shows the pin to pad mapping, the unverified marking and the supplier snapshot for a part", async () => {
    render(<LibraryBrowser />);
    await screen.findByRole("table", { name: "Library results" });
    fireEvent.click(screen.getAllByRole("button", { name: "OSS_R" })[0] as HTMLElement);
    const mapping = await screen.findByRole("table", { name: "Pin to pad mapping" });
    expect(mapping.textContent).toContain("A");
    expect(screen.getByText(/a snapshot, not a suitability check/)).toBeTruthy();
    expect(screen.getByText(/power 0.1 W \(unverified\)/)).toBeTruthy();
    await waitFor(() => expect(calls.some((c) => c.cmd === "library_meta" && c.args.touch === true)).toBe(true));
  });

  it("shows the licence and source of an imported open-source library", async () => {
    render(<LibraryBrowser />);
    await screen.findByRole("table", { name: "Library results" });
    const list = screen.getByRole("list", { name: "Imported libraries" });
    expect(list.textContent).toContain("CC-BY-SA-4.0");
    fireEvent.click(screen.getByRole("button", { name: "Remove kicad-lite" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "library_imported_remove" && c.args.name === "kicad-lite")).toBe(true));
  });
});
