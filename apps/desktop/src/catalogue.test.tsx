// LCSC catalogue panel against a mocked Tauri IPC: credentials never come back to the UI, a
// failed LCSC call shows the reason and no rows, results show how they matched and whether they
// are live/cached/stale, paging uses the page the backend reported. The Rust side is tested
// headless in src-tauri/tests/catalogue_ipc.rs and crates/eak-manufacturing/src/lcsc_api.rs.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CataloguePanel, servedLabel } from "./CataloguePanel";
import type { CatalogueReply, CatalogueResults, Candidate, LcscView, SourcingRecord } from "./types";

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];

const rec = (lcsc: string, mpn: string, maker: string): SourcingRecord => ({
  provider: "LCSC",
  source_url: "https://api.lcsc.com/rest/api/agent/product/v1/keywordsearch",
  retrieved_at_unix_s: 1,
  raw_sha256: `sha-${lcsc}`,
  lcsc_part: lcsc,
  mpn,
  manufacturer: maker,
  package: "0603",
  category: "Resistors",
  description: null,
  datasheet_url: null,
  specs: [],
  stock: 1234,
  price_breaks: [{ min_quantity: 1, unit_price: "0.0123", currency: "USD" }],
  missing: [],
  rejected: [],
  state: "SearchHit",
});

const cand = (r: SourcingRecord, kind: Candidate["kind"], reasons: string[]): Candidate => ({
  record: r,
  kind,
  exact: kind !== "fuzzy" && kind !== "browse",
  reasons,
  manufacturer_conflict: false,
  package_match: null,
  category_match: null,
  engineering_status: "supplier_listing_only",
});

const view = (over: Partial<LcscView> = {}): LcscView => ({
  has_credentials: false,
  key_hint: null,
  store_error: null,
  status: { state: "untested", message: "", tested_at_unix_s: null, last_success_unix_s: null },
  requests_this_session: 0,
  apply_url: "https://www.lcsc.com/agent/apply",
  host: "https://api.lcsc.com",
  needs: "LCSC API credentials are not configured.",
  ...over,
});

const results = (page: number, hasMore: boolean): CatalogueReply<CatalogueResults> => ({
  status: "ok",
  message: "",
  served: { source: "cache", age_s: 12 },
  fetched_at_unix_s: 1_700_000_000,
  data: {
    candidates: [
      cand(rec("C25804", "0603WAF1002T5E", "UNI-ROYAL"), "exact_mpn", ["manufacturer part number is identical"]),
      cand(rec("C99", "0603WAF1002T5X", "OTHER"), "fuzzy", ["fuzzy suggestion: not an exact match"]),
    ],
    links: {
      C25804: { standing: "supplier_only", matches: [], unverified: true, import_template: "R_0603", explanation: "Supplier listing only." },
      C99: { standing: "needs_authoring", matches: [], unverified: true, import_template: null, explanation: "Author the symbol and footprint later." },
    },
    total: 120,
    page,
    limit: 50,
    has_more: hasMore,
    via: "keyword",
    filtered_out: 0,
  },
});

let searchReply: CatalogueReply<CatalogueResults>;

beforeEach(() => {
  calls = [];
  searchReply = results(1, true);
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    switch (cmd) {
      case "lcsc_settings_get":
        return view();
      case "lcsc_settings_save":
        return view({ has_credentials: true, key_hint: "1234", needs: null });
      case "catalogue_search":
        return searchReply;
      case "parts_save_record":
        return {
          record: rec(String(args.lcsc), "X", "Y"),
          path: "/p/sourcing/C99.json",
          link: { standing: "needs_authoring", matches: [], unverified: true, import_template: null, explanation: "Author the symbol and footprint later." },
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

describe("CataloguePanel", () => {
  it("fetches nothing until asked", () => {
    render(<CataloguePanel busy={false} onImport={vi.fn()} />);
    expect(calls).toEqual([]);
  });

  it("says what credentials are needed, and sends the secret once without keeping it", async () => {
    render(<CataloguePanel busy={false} onImport={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Show credential status" }));
    await screen.findByText(/No credentials stored/);
    expect(screen.getByText(/LCSC API credentials are not configured/)).toBeTruthy();

    const secret = screen.getByLabelText("API secret") as HTMLInputElement;
    expect(secret.type).toBe("password");
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: "KEY-ABCD1234" } });
    fireEvent.change(secret, { target: { value: "s3cret-value" } });
    fireEvent.click(screen.getByRole("button", { name: "Save credentials" }));
    await screen.findByText(/key …1234/);
    expect(calls.at(-1)).toEqual({ cmd: "lcsc_settings_save", args: { key: "KEY-ABCD1234", secret: "s3cret-value" } });
    expect((screen.getByLabelText("API secret") as HTMLInputElement).value).toBe("");
    expect(document.body.textContent).not.toContain("s3cret-value");
  });

  it("shows ranked results with why they matched, cache age and the LCSC data time, and pages", async () => {
    const onImport = vi.fn();
    render(<CataloguePanel busy={false} onImport={onImport} />);
    fireEvent.change(screen.getByLabelText(/LCSC #, MPN or keyword/), { target: { value: "0603WAF1002T5E" } });
    fireEvent.change(screen.getByLabelText("Package"), { target: { value: "0603" } });
    fireEvent.click(screen.getByRole("button", { name: "Search catalogue" }));
    await screen.findByRole("table", { name: "Catalogue results" });
    expect(calls[0]).toMatchObject({
      cmd: "catalogue_search",
      args: {
        query: {
          keyword: "0603WAF1002T5E",
          package: "0603",
          in_stock_only: false,
          limit: 50,
          page: 1,
          refresh: false,
        },
      },
    });
    expect(screen.getByText("Exact MPN")).toBeTruthy();
    expect(screen.getByText("Fuzzy suggestion")).toBeTruthy();
    expect(screen.getByText("manufacturer part number is identical")).toBeTruthy();
    expect(screen.getByText(/cached 12 s ago/)).toBeTruthy();
    expect(screen.getByText(/page 1 of 3 \(120 results\)/)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Import C25804" }));
    expect(onImport).toHaveBeenCalledWith("C25804");

    // A listing that needs a new symbol/footprint is kept, not rejected, and cannot be imported yet.
    expect(screen.getByText(/Needs new symbol\/footprint/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Import C99" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Save record C99" }));
    await screen.findByText(/Saved supplier record C99 to \/p\/sourcing\/C99.json/);
    expect(calls.at(-1)).toEqual({ cmd: "parts_save_record", args: { lcsc: "C99" } });

    searchReply = results(2, false);
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(calls.at(-1)?.args).toMatchObject({ query: { page: 2 } }));
  });

  it("shows the reason and no rows when LCSC cannot be asked", async () => {
    searchReply = {
      status: "needs_credentials",
      message: "LCSC API credentials are not configured.",
      served: null,
      fetched_at_unix_s: null,
      data: null,
    };
    render(<CataloguePanel busy={false} onImport={vi.fn()} />);
    fireEvent.change(screen.getByLabelText(/LCSC #, MPN or keyword/), { target: { value: "C25804" } });
    fireEvent.click(screen.getByRole("button", { name: "Search catalogue" }));
    await screen.findByText(/needs credentials/);
    expect(screen.getByText(/LCSC API credentials are not configured/)).toBeTruthy();
    expect(screen.queryByRole("table", { name: "Catalogue results" })).toBeNull();
  });

  it("labels a stale answer as stale", () => {
    expect(servedLabel({ source: "stale_cache", age_s: 90, because: "Offline" }, 1_700_000_000)).toMatch(/^STALE/);
    expect(servedLabel({ source: "live" }, null)).toBe("live");
  });
});
