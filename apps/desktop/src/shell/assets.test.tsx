// @vitest-environment jsdom
// Phase 1 E9 Assets view against a mocked Tauri IPC: 3D model references, datasheets and part
// provenance come from the project library in the snapshot (IR model refs), libraries with their
// licence and source from `library_catalog`, and outputs per revision from `project_files`
// (`output/rev-<N>`, newest first, current vs older, the manifest's DRAFT status, a file opening in
// the file viewer). Values that are not recorded say so. The real window is covered by the E9
// ui-evidence scenario (`e2e/scenarios/e9-assets.json`).
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "../api";
import { fixture } from "../fixture.test-data";
import type { LibraryCatalogView, Snapshot } from "../types";
import { AssetsPanel } from "./AssetsPanel";
import { manifestStatus, modelQuality, outputRevisions, partAssets } from "./assetsModel";
import type { FileEntry } from "./files";

const file = (rel: string, is_dir: boolean, size = 0): FileEntry => ({
  name: rel.slice(rel.lastIndexOf("/") + 1),
  rel,
  is_dir,
  size,
  protected: false,
  role: is_dir ? "folder" : "file",
  modified_ms: 1,
});

/** The fixture with a project library: R_0603 with a model and a datasheet, LED_0603 with neither. */
function snap(revision = 7): Snapshot {
  const s = fixture(revision);
  const lib = {
    R_0603: {
      id: "R_0603",
      name: "Resistor 0603",
      category: "passive",
      refdes_prefix: "R",
      default_value: "10k",
      symbol: { pins: [] },
      datasheet: "https://example.org/r0603.pdf",
      provenance: "EAK built-in library",
      model3d: {
        kind: "box",
        color: "#808080",
        external_ref: null,
        model_ref: {
          source: { kind: "file", path: "/models/R_0603.wrl" },
          format: "wrl",
          quality: null,
          transform: { offset_mm: [0, 0, 0], rotation_deg: [0, 0, 0], scale: [1, 1, 1] },
          licence: "CC-BY-4.0",
          provenance: "Test model",
        },
      },
    },
    LED_0603: { id: "LED_0603", name: "LED 0603", category: "opto", refdes_prefix: "D", default_value: "Green", symbol: { pins: [] }, datasheet: null, provenance: "" },
  };
  s.project.library = lib as unknown as Snapshot["project"]["library"];
  return s;
}

const catalog: LibraryCatalogView = {
  libraries: [
    { id: "builtin", name: "EAK built-in", namespace: "eak", version: "1", source: "bundled", license: "MIT", description: "", content_hash: "abcdef0123456789", scope: "system", enabled: true, components: 40 },
    { id: "imported:e9", name: "e9-lib", namespace: "e9", version: "0.1", source: "", license: "", description: "", content_hash: "", scope: "imported", enabled: false, components: 1 },
  ],
  problems: [],
};

const manifest = JSON.stringify({ revision: 7, status: "DRAFT", files: [{ path: "a" }, { path: "b" }] });
let tree: Record<string, FileEntry[]>;

beforeEach(() => {
  tree = {
    "": [file("output", true), file("project.eak.json", false, 10)],
    output: [file("output/rev-3", true), file("output/rev-7", true), file("output/notes.txt", false, 4)],
    "output/rev-7": [file("output/rev-7/documents", true), file("output/rev-7/manifest.json", false, 2048)],
    "output/rev-7/documents": [file("output/rev-7/documents/netlist.json", false, 12)],
    "output/rev-3": [],
  };
  mockIPC((cmd, payload) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    const args = (payload ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case "library_catalog":
        return catalog;
      case "project_files": {
        const list = tree[String(args.rel)];
        if (!list) throw new Error(`cannot list ${String(args.rel)}`);
        return list;
      }
      case "project_file_read":
        return { rel: args.rel, size: manifest.length, text: manifest, binary: false, truncated: false };
      default:
        throw new Error(`unexpected ${cmd}`);
    }
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("assets model (pure)", () => {
  it("lists each project-library definition with its model, datasheet, provenance and the parts using it", () => {
    const rows = partAssets(snap());
    expect(rows.map((r) => r.componentId)).toEqual(["LED_0603", "R_0603"]);
    const r = rows.find((x) => x.componentId === "R_0603")!;
    expect(r.usedBy).toEqual([{ symbolId: "sym_r1", refdes: "R1" }]);
    expect(r.model?.source.path).toBe("/models/R_0603.wrl");
    expect(r.datasheet).toBe("https://example.org/r0603.pdf");
    const led = rows.find((x) => x.componentId === "LED_0603")!;
    expect([led.model, led.datasheet, led.provenance]).toEqual([null, null, null]);
  });

  it("an undeclared model quality is never shown as exact", () => {
    const m = partAssets(snap()).find((x) => x.model)!.model!;
    expect(modelQuality(m)).toBe("Quality not declared");
    expect(modelQuality({ ...m, quality: "VENDOR_EXACT" })).toBe("Vendor exact");
  });

  it("output revisions: rev-<N> folders only, newest first, the project's revision marked current", () => {
    const revs = outputRevisions(tree.output, 7);
    expect(revs.map((r) => [r.revision, r.current])).toEqual([
      [7, true],
      [3, false],
    ]);
  });

  it("reads an export manifest's status and file count; anything else is not a manifest", () => {
    expect(manifestStatus(manifest)).toEqual({ status: "DRAFT", revision: 7, files: 2 });
    expect(manifestStatus("not json")).toBeNull();
    expect(manifestStatus(JSON.stringify({ files: [] }))).toBeNull();
    expect(manifestStatus(null)).toBeNull();
  });
});

describe("Assets view", () => {
  function panel(s = snap()) {
    const onOpenFile = vi.fn();
    const onPart = vi.fn();
    render(<AssetsPanel snapshot={s} onOpenFile={onOpenFile} onPart={onPart} />);
    return { onOpenFile, onPart };
  }

  it("shows the model reference with format, quality, licence and provenance; a refdes selects the part", () => {
    const { onPart } = panel();
    const models = within(screen.getByRole("region", { name: "3D models" }));
    const row = within(models.getByRole("list", { name: "Model list" }).querySelector('[data-component="R_0603"]') as HTMLElement);
    expect(row.getByText("/models/R_0603.wrl")).toBeTruthy();
    expect(row.getByText("WRL")).toBeTruthy();
    expect(row.getByText("Quality not declared")).toBeTruthy();
    expect(row.getByText(/CC-BY-4\.0/)).toBeTruthy();
    fireEvent.click(row.getByRole("button", { name: "R1" }));
    expect(onPart).toHaveBeenCalledWith("sym_r1");
    expect(models.getByText(/Box body only \(no model\): LED_0603/)).toBeTruthy();
  });

  it("lists recorded datasheets and names the parts without one; missing provenance is flagged", () => {
    panel();
    const ds = within(screen.getByRole("region", { name: "Datasheets" }));
    expect(ds.getByText("https://example.org/r0603.pdf")).toBeTruthy();
    expect(ds.getByText(/No datasheet recorded: LED_0603/)).toBeTruthy();
    const prov = within(screen.getByRole("region", { name: "Part provenance" }));
    expect(within(prov.getByRole("list").querySelector('[data-component="LED_0603"]') as HTMLElement).getByText("not recorded")).toBeTruthy();
    expect(prov.getByText("EAK built-in library")).toBeTruthy();
  });

  it("lists libraries with licence, source and state from the catalog; missing values say not recorded", async () => {
    panel();
    const libs = within(screen.getByRole("region", { name: "Libraries" }));
    await waitFor(() => expect(libs.getByText("EAK built-in")).toBeTruthy());
    const imported = within(libs.getByRole("list", { name: "Library list" }).querySelector('[data-library="e9-lib"]') as HTMLElement);
    expect(imported.getByText("disabled")).toBeTruthy();
    expect(imported.getByText("not recorded", { selector: ".pill" })).toBeTruthy();
    expect(libs.getByText(/Licence: MIT/)).toBeTruthy();
  });

  it("lists outputs per revision; expanding shows the manifest status and a file opens in the viewer", async () => {
    const { onOpenFile } = panel();
    const out = within(screen.getByRole("region", { name: "Outputs" }));
    const revs = await out.findByRole("list", { name: "Output revisions" });
    expect(within(revs).getAllByRole("button", { name: /^Expand Revision/ }).map((b) => b.textContent)).toEqual(["▸ Revision 7", "▸ Revision 3"]);
    expect(within(revs).getByText("current")).toBeTruthy();
    expect(within(revs).getByText("older")).toBeTruthy();
    fireEvent.click(out.getByRole("button", { name: "Expand Revision 7" }));
    await waitFor(() => expect(out.getByText("DRAFT · 2 file(s)")).toBeTruthy());
    fireEvent.click(await out.findByRole("button", { name: "Expand documents/" }));
    fireEvent.click(await out.findByRole("button", { name: "netlist.json" }));
    expect(onOpenFile).toHaveBeenCalledWith(expect.objectContaining({ rel: "output/rev-7/documents/netlist.json" }));
    fireEvent.click(out.getByRole("button", { name: "Expand Revision 3" }));
    await waitFor(() => expect(out.getByText("Empty folder.")).toBeTruthy());
  });

  it("says nothing was exported when output/ does not exist, and lists again on Refresh", async () => {
    tree[""] = [file("project.eak.json", false, 10)];
    panel();
    const out = within(screen.getByRole("region", { name: "Outputs" }));
    await waitFor(() => expect(out.getByText(/Nothing exported yet/)).toBeTruthy());
    tree[""] = [file("output", true)];
    fireEvent.click(out.getByRole("button", { name: "Refresh" }));
    await out.findByRole("list", { name: "Output revisions" });
  });

  it("an unsaved project has no outputs folder (Refresh disabled with the reason)", () => {
    const s = snap();
    s.dir = null;
    panel(s);
    const out = within(screen.getByRole("region", { name: "Outputs" }));
    expect(out.getByText("Exports need a saved project folder.")).toBeTruthy();
    const refresh = out.getByRole("button", { name: "Refresh" }) as HTMLButtonElement;
    expect(refresh.disabled).toBe(true);
    expect(refresh.title).toMatch(/no folder/);
  });

  it("an empty project library says how parts get there", () => {
    const s = fixture(7);
    s.project.library = {};
    panel(s);
    expect(screen.getByText("No parts in the project library")).toBeTruthy();
  });
});
