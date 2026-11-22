// @vitest-environment jsdom
// F2 regression: a KiCad commit needs a licence and a source the user names (no "Unspecified"
// default), and the success message says how the parts can actually be placed.
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { KicadImport } from "./KicadImport";

let commits: Record<string, unknown>[];
beforeEach(() => {
  commits = [];
  mockIPC((cmd, payload) => {
    const a = (payload ?? {}) as Record<string, unknown>;
    if (cmd === "plugin:dialog|open") return "/libs/passives.kicad_sym";
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    const report = { format: "kicad_sym", items: [{ name: "RES_0805", status: "imported", notes: [] }], unsupported: {} };
    if (cmd === "kicad_import_report") return { file: "passives.kicad_sym", report, imported: 1, rejected: 0 };
    if (cmd === "kicad_import_commit") {
      commits.push(a);
      return {
        library_name: "kicad_passives",
        licence: a.licence,
        source_url: a.source,
        placeholder_footprints: true,
        components_created: 1,
        component_ids: ["RES_0805"],
        rejected: [],
        report,
      };
    }
    throw new Error(`unmocked ${cmd}`);
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("KicadImport", () => {
  it("refuses to commit until a licence and an http(s) source are named, then sends them", async () => {
    render(<KicadImport />);
    fireEvent.click(screen.getByRole("button", { name: "Import from KiCad…" }));
    const commit = (await screen.findByRole("button", { name: "Commit to Library" })) as HTMLButtonElement;
    expect(commit.disabled).toBe(true);
    expect(screen.getByText(/Without a licence nothing is imported/)).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Library licence"), { target: { value: "CC-BY-SA-4.0" } });
    expect(commit.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Library source URL"), { target: { value: "kicad.org" } });
    expect(commit.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Library source URL"), { target: { value: "https://example.org/passives" } });
    expect(commit.disabled).toBe(false);
    fireEvent.click(commit);

    const ok = await screen.findByRole("status");
    expect(commits).toEqual([{ path: "/libs/passives.kicad_sym", licence: "CC-BY-SA-4.0", source: "https://example.org/passives" }]);
    expect(ok.textContent).toContain("licence CC-BY-SA-4.0");
    expect(ok.textContent).toContain("drag one onto a schematic sheet");
    expect(ok.textContent).toContain("placeholder");
    expect(ok.textContent).not.toContain("Ready for placement");
    await waitFor(() => expect(screen.queryByRole("button", { name: "Commit to Library" })).toBeNull());
  });
});
