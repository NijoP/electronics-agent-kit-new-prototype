// @vitest-environment jsdom
// D3 output dialogs against a mocked Tauri IPC: the BOM configuration (column picker, order,
// headers, sort, grouping, filter rules) goes to the engine for the preview and the export, is
// remembered across dialog openings, and round-trips through JSON; the documents dialog sends
// the chosen outputs; results show the engine's manifest status (DRAFT). The Rust side is tested
// in crates/eak-manufacturing (bom_config, outputs) and src-tauri/tests/ipc.rs.
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "../api";
import type { BomConfig, ExportResult } from "../types";
import { defaultBomConfig, loadBomConfig, moveColumn, parseBomConfig, saveBomConfig, toggleColumn } from "./bomConfig";
import { BomExportDialog, DocumentExportDialog } from "./OutputDialogs";

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];

const result = (kinds: string[]): ExportResult => ({
  out_dir: "/p/output/rev-3/bom",
  manifest: {
    revision: 3,
    status: "DRAFT",
    statement: "DRAFT — blocking issues listed; NOT for fabrication.",
    blocking_reasons: ["the board has no outline yet"],
    files: kinds.map((k, i) => ({ name: `f${i}.${k}`, bytes: 10, sha256: "ab", kind: k })),
    not_verified: ["Independent senior electronics engineering review"],
  },
  reports: [],
});

beforeEach(() => {
  calls = [];
  localStorage.clear();
  mockIPC((cmd, payload) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    switch (cmd) {
      case "bom_table": {
        const c = args.config as BomConfig;
        return { fields: c.columns.map((x) => x.field), headers: c.columns.map((x) => x.header ?? x.field), rows: [c.columns.map(() => "R1")], removed: c.exclude.length, parts: 1 };
      }
      case "export_bom":
        return result(["bom", "bom-config"]);
      case "export_documents":
        return result(["pdf", "ipc-d-356"]);
      default:
        throw new Error(`unmocked ${cmd}`);
    }
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
});

/** The configuration of the last `bom_table` (preview) or `export_bom` call. */
const lastConfig = (cmd: "bom_table" | "export_bom"): BomConfig => {
  const c = [...calls].reverse().find((x) => x.cmd === cmd);
  if (!c) throw new Error(`no ${cmd} call`);
  return (cmd === "export_bom" ? (c.args.bom as { config: BomConfig }).config : c.args.config) as BomConfig;
};
const lastField = (c: BomConfig, fromEnd: number) => c.columns[c.columns.length - fromEnd]?.field;

describe("BOM configuration helpers", () => {
  it("round-trips through JSON and rejects what the engine would reject", () => {
    let c = defaultBomConfig();
    c = toggleColumn(c, "issues");
    c = moveColumn(c, c.columns.length - 1, -100);
    c = { ...c, grouping: "designator", exclude: [{ field: "comment", op: "contains", value: "DNP" }] };
    c.columns[0] = { ...c.columns[0], header: "Problems", sort: "desc" };
    expect(c.columns[0].field).toBe("issues");
    expect(parseBomConfig(JSON.stringify(c))).toEqual(c);
    expect(parseBomConfig("{}")).toBeNull();
    expect(parseBomConfig('{"columns":[{"field":"mpn"},{"field":"mpn"}]}')).toBeNull();
    expect(parseBomConfig('{"columns":[{"field":"bogus"}]}')).toBeNull();
    expect(parseBomConfig("not json")).toBeNull();
    // The last column cannot be removed.
    const one = { ...c, columns: [c.columns[0]] };
    expect(toggleColumn(one, "issues")).toBe(one);
  });

  it("is remembered on this computer and falls back to the default", () => {
    expect(loadBomConfig()).toEqual(defaultBomConfig());
    const c = toggleColumn(defaultBomConfig(), "stock");
    saveBomConfig(c);
    expect(loadBomConfig()).toEqual(c);
    localStorage.setItem("eak.bomConfig.v1", "{broken");
    expect(loadBomConfig()).toEqual(defaultBomConfig());
  });
});

describe("Export BOM dialog", () => {
  it("previews through the engine, edits columns, sort, grouping and filters, and exports that configuration", async () => {
    let exported: ExportResult | null = null;
    render(<BomExportDialog revision={3} busy={false} onExported={(r) => (exported = r)} onClose={() => {}} />);
    const dlg = screen.getByRole("dialog", { name: "Export BOM" });
    await waitFor(() => expect(within(dlg).getByRole("table", { name: "BOM preview table" })).toBeTruthy());
    expect(lastConfig("bom_table")).toEqual(defaultBomConfig());

    fireEvent.click(within(dlg).getByRole("checkbox", { name: "Column Stock (stored record)" }));
    await waitFor(() => expect(lastField(lastConfig("bom_table"), 1)).toBe("stock"));
    fireEvent.click(within(dlg).getByRole("button", { name: "Move Stock (stored record) up" }));
    await waitFor(() => expect(lastField(lastConfig("bom_table"), 2)).toBe("stock"));
    fireEvent.change(within(dlg).getByRole("textbox", { name: "Header for Comment" }), { target: { value: "Value" } });
    fireEvent.change(within(dlg).getByRole("combobox", { name: "Sort Quantity" }), { target: { value: "desc" } });
    fireEvent.change(within(dlg).getByRole("combobox", { name: "Group rows" }), { target: { value: "designator" } });
    fireEvent.click(within(dlg).getByRole("button", { name: "Add filter rule" }));
    fireEvent.change(within(dlg).getByRole("combobox", { name: "Filter 1 condition" }), { target: { value: "is_empty" } });
    expect(within(dlg).queryByRole("textbox", { name: "Filter 1 value" })).toBeNull();
    fireEvent.change(within(dlg).getByRole("combobox", { name: "File format" }), { target: { value: "xlsx" } });
    await waitFor(() => expect(within(dlg).getByText(/1 removed by filters/)).toBeTruthy());

    const preview = lastConfig("bom_table");
    expect(preview.columns[0]).toEqual({ field: "comment", header: "Value", sort: "none" });
    expect(preview.columns.find((c) => c.field === "quantity")?.sort).toBe("desc");
    expect(preview.grouping).toBe("designator");
    expect(preview.exclude).toEqual([{ field: "comment", op: "is_empty", value: "DNP" }]);

    fireEvent.click(within(dlg).getByRole("button", { name: "Export XLSX" }));
    await waitFor(() => expect(exported).not.toBeNull());
    const call = calls.find((c) => c.cmd === "export_bom");
    expect((call?.args.bom as { format: string }).format).toBe("xlsx");
    expect(lastConfig("export_bom")).toEqual(preview);
    // The engine's manifest status is shown as is: DRAFT, never upgraded.
    expect(within(dlg).getByText("DRAFT — not for fabrication")).toBeTruthy();
    expect(within(dlg).getByRole("list", { name: "Exported files" }).textContent).toContain("bom-config");

    // The configuration is remembered for the next opening.
    cleanup();
    calls = [];
    render(<BomExportDialog revision={3} busy={false} onExported={() => {}} onClose={() => {}} />);
    await waitFor(() => expect(lastConfig("bom_table")).toEqual(preview));
  });

  it("restores the default and blocks export while another command runs", async () => {
    saveBomConfig(toggleColumn(defaultBomConfig(), "issues"));
    render(<BomExportDialog revision={1} busy onExported={() => {}} onClose={() => {}} />);
    const dlg = screen.getByRole("dialog", { name: "Export BOM" });
    const run = within(dlg).getByRole("button", { name: "Export CSV" }) as HTMLButtonElement;
    expect(run.disabled).toBe(true);
    expect(run.title).toBe("An engine command is running");
    await waitFor(() => expect(lastField(lastConfig("bom_table"), 1)).toBe("issues"));
    fireEvent.click(within(dlg).getByRole("button", { name: "Restore default" }));
    await waitFor(() => expect(lastConfig("bom_table")).toEqual(defaultBomConfig()));
  });
});

describe("Export schematic and netlist dialog", () => {
  it("sends the chosen outputs and shows the DRAFT manifest", async () => {
    let exported: ExportResult | null = null;
    render(<DocumentExportDialog revision={3} busy={false} onExported={(r) => (exported = r)} onClose={() => {}} />);
    const dlg = screen.getByRole("dialog", { name: "Export schematic and netlist" });
    fireEvent.click(within(dlg).getByRole("checkbox", { name: "Schematic SVG" }));
    fireEvent.click(within(dlg).getByRole("checkbox", { name: "IPC-D-356 netlist" }));
    fireEvent.click(within(dlg).getByRole("button", { name: "Export" }));
    await waitFor(() => expect(exported).not.toBeNull());
    expect(calls.find((c) => c.cmd === "export_documents")?.args.request).toEqual({ schematic_pdf: true, schematic_svg: true, ipc_d_356: false, netlist_json: false });
    expect(within(dlg).getByText("DRAFT — not for fabrication")).toBeTruthy();
    expect(within(dlg).getByRole("list", { name: "Blocking reasons" }).textContent).toContain("no outline");
  });

  it("cannot export with nothing chosen", () => {
    render(<DocumentExportDialog revision={3} busy={false} onExported={() => {}} onClose={() => {}} />);
    const dlg = screen.getByRole("dialog", { name: "Export schematic and netlist" });
    fireEvent.click(within(dlg).getByRole("checkbox", { name: "Schematic PDF" }));
    fireEvent.click(within(dlg).getByRole("checkbox", { name: "IPC-D-356 netlist" }));
    const run = within(dlg).getByRole("button", { name: "Export" }) as HTMLButtonElement;
    expect(run.disabled).toBe(true);
    expect(run.title).toBe("Choose at least one output");
  });
});
