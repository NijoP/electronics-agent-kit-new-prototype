// Parts search and manufacturing prep panels against a mocked Tauri IPC: command names,
// argument shapes, honest handling of a blocked LCSC search, and the "prepared for review"
// wording. The Rust side is tested headless in src-tauri/tests/parts_ipc.rs.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ManufacturingPanel } from "./ManufacturingPanel";
import { PartsPanel } from "./PartsPanel";
import type { DfmReport, ManufacturingPreview, PartsSearch, SourcingRecord } from "./types";

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];

const record: SourcingRecord = {
  provider: "LCSC",
  source_url: "https://wmsc.lcsc.com/ftps/wm/product/detail?productCode=C25804",
  retrieved_at_unix_s: 1,
  raw_sha256: "a".repeat(64),
  lcsc_part: "C25804",
  mpn: "0603WAF1002T5E",
  manufacturer: "UNI-ROYAL",
  package: "0603",
  category: "Resistors / Chip Resistor - Surface Mount",
  description: null,
  datasheet_url: "https://datasheet.lcsc.com/x.pdf",
  specs: [],
  stock: 1004300,
  price_breaks: [{ min_quantity: 100, unit_price: "0.0022", currency: "USD" }],
  missing: [],
  rejected: [],
  state: "SearchHit",
};

const search = (status: PartsSearch["live"]["status"], records: SourcingRecord[], message = ""): PartsSearch => ({
  query: "q",
  library: [{ id: "R_0603", name: "Resistor 0603", category: "Resistor", lcsc: null, mpn: null }],
  stored: [],
  live: { status, message, records },
});

const dfm: DfmReport = {
  profile: { id: "jlcpcb-fr4-2layer-1oz", version: "2026-11-14", source_url: "https://jlcpcb.com", verified_on: "2026-11-14" },
  checks: [
    { id: "JLC-010", title: "Minimum track width", status: "pass", required: "0.100 mm", measured: "minimum 0.250 mm", detail: [] },
    { id: "JLC-040", title: "Every assembled BOM line has an LCSC part number", status: "fail", required: "", measured: "2 of 3 line(s) lack one", detail: ["R1 (10k)"] },
    { id: "JLC-051", title: "CPL rotation and polarity per package", status: "not_checked", required: "", measured: "not evaluated", detail: ["x"] },
  ],
  failures: 1,
  warnings: 0,
  not_checked: 1,
  statement: "PREPARED FOR REVIEW. 1 failing, 0 warning, 1 not-checked rule(s).",
  limitations: [],
};

const preview: ManufacturingPreview = {
  revision: 4,
  dfm,
  default_out_dir: "/p/output/jlcpcb-rev-4",
  bom: [
    {
      component_id: "R_0603",
      value: "10k",
      designators: ["R1", "R2"],
      quantity: 2,
      footprint: "R_0603",
      lcsc: null,
      manufacturer: null,
      mpn: null,
      smd: true,
      placed: 2,
      stock: null,
      unit_price_usd: null,
      price_as_of_unix_s: null,
      issues: ["no LCSC part number"],
    },
  ],
};

let liveReply: PartsSearch;

beforeEach(() => {
  calls = [];
  liveReply = search("ok", [record]);
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    switch (cmd) {
      case "parts_search":
        return liveReply;
      case "parts_import":
        return {
          component_id: "LCSC_C25804",
          response: { accepted: true, revision_after: 5, command: "UpsertComponentDefinition", error: null },
          notes: ["symbol and footprint reused from R_0603"],
          record,
        };
      case "manufacturing_preview":
        return preview;
      case "jlcpcb_prepare":
        return {
          prep: {
            out_dir: String(args.out_dir) || "/p/output/jlcpcb-rev-4",
            manifest: {
              statement: "PREPARED FOR REVIEW — BLOCKED: 1 DFM failure(s)",
              files: [{ name: "x-JLCPCB-gerbers.zip", bytes: 99, sha256: "b", kind: "gerber_zip" }],
              not_verified: ["JLCPCB's own DFM / engineering review of these files"],
            },
            dfm,
          },
          reports: [],
        };
      default:
        throw new Error(`unexpected ${cmd}`);
    }
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("PartsPanel", () => {
  it("searches, shows stock/price/package/datasheet and imports with the rendered revision", async () => {
    const onImported = vi.fn(async () => {});
    render(<PartsPanel revision={4} onImported={onImported} />);
    fireEvent.change(screen.getByLabelText(/LCSC part number or keyword/i), { target: { value: " C25804 " } });
    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    await screen.findByRole("table", { name: "LCSC results" });
    expect(calls[0]).toEqual({ cmd: "parts_search", args: { query: " C25804 " } });
    expect(screen.getByText("0603WAF1002T5E")).toBeTruthy();
    expect(screen.getByText("1,004,300")).toBeTruthy();
    expect(screen.getByText("$0.0022 @ 100+")).toBeTruthy();
    expect(screen.getByRole("link", { name: "datasheet" }).getAttribute("href")).toContain("datasheet.lcsc.com");

    fireEvent.click(screen.getByRole("button", { name: "Import C25804" }));
    await waitFor(() => expect(onImported).toHaveBeenCalled());
    expect(calls[1]).toEqual({ cmd: "parts_import", args: { expected_revision: 4, lcsc: "C25804" } });
    expect(screen.getByText(/Imported C25804 as LCSC_C25804/)).toBeTruthy();
  });

  it("says plainly when LCSC refuses automated access and shows no invented rows", async () => {
    liveReply = search("blocked", [], "LCSC refused automated access (HTTP 403: Access Denied). No results are shown rather than inventing any.");
    render(<PartsPanel revision={1} onImported={async () => {}} />);
    fireEvent.change(screen.getByLabelText(/LCSC part number or keyword/i), { target: { value: "10k resistor" } });
    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    expect((await screen.findByRole("alert")).textContent).toContain("refused automated access");
    expect(screen.queryByRole("table", { name: "LCSC results" })).toBeNull();
    // Offline library matches are still offered.
    expect(screen.getByRole("list", { name: "Library matches" }).textContent).toContain("R_0603");
  });

  it("does not search an empty query", () => {
    render(<PartsPanel revision={1} onImported={async () => {}} />);
    expect((screen.getByRole("button", { name: "Search" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("ManufacturingPanel", () => {
  it("shows the BOM and DFM results, flags missing LCSC numbers and never calls unchecked rules passing", async () => {
    render(<ManufacturingPanel revision={4} onPrepared={() => {}} />);
    await screen.findByRole("table", { name: "Bill of materials" });
    expect(screen.getByText("missing")).toBeTruthy();
    const dfmTable = screen.getByRole("table", { name: "DFM checks" });
    expect(dfmTable.textContent).toContain("not checked");
    expect(dfmTable.textContent).toContain("FAIL");
    expect(screen.getByText(/Not manufacturing-ready/)).toBeTruthy();
  });

  it("generates the bundle into the typed folder and shows the prepared-for-review status", async () => {
    const onPrepared = vi.fn();
    render(<ManufacturingPanel revision={4} onPrepared={onPrepared} />);
    await screen.findByRole("table", { name: "Bill of materials" });
    fireEvent.change(screen.getByLabelText(/Output folder/), { target: { value: "/home/me/jlc" } });
    fireEvent.click(screen.getByRole("button", { name: "Generate JLCPCB bundle" }));
    await screen.findByLabelText("Prepared bundle");
    expect(calls.find((c) => c.cmd === "jlcpcb_prepare")?.args).toEqual({ out_dir: "/home/me/jlc" });
    expect(screen.getByText(/PREPARED FOR REVIEW — BLOCKED/)).toBeTruthy();
    expect(onPrepared).toHaveBeenCalledWith(expect.stringContaining("/home/me/jlc"));
  });

  it("shows a refused folder as an error", async () => {
    mockIPC((cmd) => {
      if (cmd === "manufacturing_preview") return preview;
      throw "choose an absolute folder path";
    });
    render(<ManufacturingPanel revision={4} onPrepared={() => {}} />);
    await screen.findByRole("table", { name: "Bill of materials" });
    fireEvent.click(screen.getByRole("button", { name: "Generate JLCPCB bundle" }));
    expect((await screen.findByRole("alert")).textContent).toContain("absolute folder path");
  });
});
