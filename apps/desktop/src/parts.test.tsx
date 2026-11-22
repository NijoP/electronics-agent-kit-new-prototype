// Parts search and manufacturing prep panels against a mocked Tauri IPC: command names,
// argument shapes, honest handling of a blocked Distributor search, and the "prepared for review"
// wording. The Rust side is tested headless in src-tauri/tests/parts_ipc.rs.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ManufacturingPanel } from "./ManufacturingPanel";
import { distributorUnconfigured, PartsPanel } from "./PartsPanel";
import type { DfmReport, ManufacturingPreview, PartsSearch, SourcingRecord } from "./types";

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];

const record: SourcingRecord = {
  provider: "Distributor",
  source_url: "https://distributor.invalid/ftps/wm/product/detail?productCode=C25804",
  retrieved_at_unix_s: 1,
  raw_sha256: "a".repeat(64),
  distributor_part: "C25804",
  mpn: "0603WAF1002T5E",
  manufacturer: "UNI-ROYAL",
  package: "0603",
  category: "Resistors / Chip Resistor - Surface Mount",
  description: null,
  datasheet_url: "https://distributor.invalid/x.pdf",
  specs: [],
  stock: 1004300,
  price_breaks: [{ min_quantity: 100, unit_price: "0.0022", currency: "USD" }],
  missing: [],
  rejected: [],
  state: "SearchHit",
};

const search = (status: PartsSearch["live"]["status"], records: SourcingRecord[], message = ""): PartsSearch => ({
  query: "q",
  library: [{ id: "R_0603", name: "Resistor 0603", category: "Resistor", distributor: null, mpn: null }],
  stored: [],
  live: { status, message, records },
});

const dfm: DfmReport = {
  profile: { id: "fabricator-fr4-2layer-1oz", version: "2026-11-14", source_url: "https://fabricator.invalid", verified_on: "2026-11-14" },
  checks: [
    { id: "Fab-010", title: "Minimum track width", status: "pass", required: "0.100 mm", measured: "minimum 0.250 mm", detail: [] },
    { id: "Fab-040", title: "Every assembled BOM line has an Distributor part number", status: "fail", required: "", measured: "2 of 3 line(s) lack one", detail: ["R1 (10k)"] },
    { id: "Fab-051", title: "CPL rotation and polarity per package", status: "not_checked", required: "", measured: "not evaluated", detail: ["x"] },
  ],
  failures: 1,
  warnings: 0,
  not_checked: 1,
  statement: "PREPARED FOR REVIEW. 1 failing, 0 warning, 1 not-checked rule(s).",
  limitations: [],
};

const preview: ManufacturingPreview = {
  assembly: { rows: [], warnings: [] },
  revision: 4,
  dfm,
  default_out_dir: "/p/output/fabricator-rev-4",
  bom: [
    {
      component_id: "R_0603",
      value: "10k",
      designators: ["R1", "R2"],
      quantity: 2,
      footprint: "R_0603",
      distributor: null,
      manufacturer: null,
      mpn: null,
      smd: true,
      placed: 2,
      stock: null,
      unit_price_usd: null,
      price_as_of_unix_s: null,
      issues: ["no Distributor part number"],
    },
  ],
};

let liveReply: PartsSearch;
// The distributor host the backend reports (a real-looking host unless a test says otherwise).
let host: string;
const ran = (cmd: string) => calls.find((c) => c.cmd === cmd);

beforeEach(() => {
  calls = [];
  liveReply = search("ok", [record]);
  host = "https://api.distributor.example";
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    switch (cmd) {
      case "distributor_settings_get":
        return { has_credentials: false, key_hint: null, store_error: null, status: { state: "untested", message: "", tested_at_unix_s: null, last_success_unix_s: null }, requests_this_session: 0, apply_url: `${host}/apply`, host, needs: null };
      case "parts_search":
        return liveReply;
      case "parts_import":
        return {
          component_id: "DISTRIBUTOR_C25804",
          response: { accepted: true, revision_after: 5, command: "UpsertComponentDefinition", error: null },
          notes: ["symbol and footprint reused from R_0603"],
          record,
        };
      case "manufacturing_preview":
        return preview;
      case "fabricator_prepare":
        return {
          prep: {
            out_dir: String(args.out_dir) || "/p/output/fabricator-rev-4",
            manifest: {
              statement: "PREPARED FOR REVIEW — BLOCKED: 1 DFM failure(s)",
              files: [{ name: "x-Fabricator-gerbers.zip", bytes: 99, sha256: "b", kind: "gerber_zip" }],
              not_verified: ["Fabricator's own DFM / engineering review of these files"],
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
    fireEvent.change(screen.getByLabelText(/Distributor part number or keyword/i), { target: { value: " C25804 " } });
    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    await screen.findByRole("table", { name: "Distributor results" });
    expect(ran("parts_search")).toEqual({ cmd: "parts_search", args: { query: " C25804 " } });
    expect(screen.getByText("0603WAF1002T5E")).toBeTruthy();
    expect(screen.getByText("1,004,300")).toBeTruthy();
    expect(screen.getByText("$0.0022 @ 100+")).toBeTruthy();
    expect(screen.getByRole("link", { name: "datasheet" }).getAttribute("href")).toContain("distributor.invalid");

    fireEvent.click(screen.getByRole("button", { name: "Import C25804" }));
    await waitFor(() => expect(onImported).toHaveBeenCalled());
    expect(ran("parts_import")).toEqual({ cmd: "parts_import", args: { expected_revision: 4, distributor: "C25804" } });
    expect(screen.getByText(/Imported C25804 as DISTRIBUTOR_C25804/)).toBeTruthy();
  });

  it("says plainly when Distributor refuses automated access and shows no invented rows", async () => {
    liveReply = search("blocked", [], "Distributor refused automated access (HTTP 403: Access Denied). No results are shown rather than inventing any.");
    render(<PartsPanel revision={1} onImported={async () => {}} />);
    fireEvent.change(screen.getByLabelText(/Distributor part number or keyword/i), { target: { value: "10k resistor" } });
    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    expect((await screen.findByRole("alert")).textContent).toContain("refused automated access");
    expect(screen.queryByRole("table", { name: "Distributor results" })).toBeNull();
    // Offline library matches are still offered.
    expect(screen.getByRole("list", { name: "Library matches" }).textContent).toContain("R_0603");
  });

  it("slice E1: with the reserved .invalid distributor host the catalogue and live import are replaced by 'Distributor not configured'", async () => {
    host = "https://distributor.invalid";
    render(<PartsPanel revision={1} onImported={async () => {}} />);
    const note = await screen.findByRole("note", { name: "Distributor not configured" });
    expect(note.textContent).toContain("distributor.invalid");
    expect(screen.queryByText(/Distributor catalogue: credentials/)).toBeNull();
    // a search still lists library matches but never offers a live import
    fireEvent.change(screen.getByLabelText(/Part number or keyword/i), { target: { value: "C25804" } });
    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    await screen.findByRole("list", { name: "Library matches" });
    expect(screen.queryByRole("table", { name: "Distributor results" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Import C25804" })).toBeNull();
    expect(ran("parts_import")).toBeUndefined();
  });

  it("classifies distributor hosts: only reserved or unparsable hosts are 'not configured'", () => {
    expect(distributorUnconfigured("https://distributor.invalid")).toBe(true);
    expect(distributorUnconfigured("https://a.b.INVALID/x")).toBe(true);
    expect(distributorUnconfigured("not a url")).toBe(true);
    expect(distributorUnconfigured("https://api.distributor.example")).toBe(false);
  });

  it("does not search an empty query", () => {
    render(<PartsPanel revision={1} onImported={async () => {}} />);
    expect((screen.getByRole("button", { name: "Search" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("ManufacturingPanel", () => {
  it("shows the BOM and DFM results, flags missing Distributor numbers and never calls unchecked rules passing", async () => {
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
    fireEvent.change(screen.getByLabelText(/Output folder/), { target: { value: "/home/me/fab" } });
    fireEvent.click(screen.getByRole("button", { name: "Generate Fabricator bundle" }));
    await screen.findByLabelText("Prepared bundle");
    expect(calls.find((c) => c.cmd === "fabricator_prepare")?.args).toEqual({ out_dir: "/home/me/fab" });
    expect(screen.getByText(/PREPARED FOR REVIEW — BLOCKED/)).toBeTruthy();
    expect(onPrepared).toHaveBeenCalledWith(expect.stringContaining("/home/me/fab"));
  });

  it("shows a refused folder as an error", async () => {
    mockIPC((cmd) => {
      if (cmd === "manufacturing_preview") return preview;
      throw "choose an absolute folder path";
    });
    render(<ManufacturingPanel revision={4} onPrepared={() => {}} />);
    await screen.findByRole("table", { name: "Bill of materials" });
    fireEvent.click(screen.getByRole("button", { name: "Generate Fabricator bundle" }));
    expect((await screen.findByRole("alert")).textContent).toContain("absolute folder path");
  });
});
