// @vitest-environment jsdom
// Library increment A4: "Import a library folder…" scans first, imports only with a licence and a
// source, and reports unpaired symbols and missing 3D models; export and file import go through
// their IPC commands. The Rust side is tested headless in src-tauri/tests/library_import_ipc.rs.
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { LibraryFolders } from "./shell/LibraryManager";

let calls: { cmd: string; args: Record<string, unknown> }[];
let dialog: string;
beforeEach(() => {
  calls = [];
  dialog = "/libs/demo";
  mockIPC((cmd, payload) => {
    const a = (payload ?? {}) as Record<string, unknown>;
    if (cmd === "plugin:dialog|open") return dialog;
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    calls.push({ cmd, args: a });
    switch (cmd) {
      case "library_db_search":
        return {
          libraries: [{ library: "Demo", items: 2 }],
          items: [{ kind: "symbol", library: "Demo", name: "EAK_R", description: "", maturity: "imported", licence: "CC0-1.0", source: "test" }],
        };
      case "kicad_library_scan":
        return {
          folder: a.folder,
          symbol_libs: [{ name: "Demo", kind: "symbol", path: "/libs/demo/Demo.kicad_sym", origin: "folder", exists: true, usable: true, notes: [] }],
          footprint_libs: [{ name: "Gone", kind: "footprint", path: null, origin: "table", exists: false, usable: false, notes: ["path variable(s) not set: X"] }],
          problems: [],
        };
      case "kicad_library_import":
        return {
          folder: a.folder,
          symbols: 2,
          footprints: 2,
          stored: 4,
          unpaired: ["Demo:EAK_ORPHAN"],
          models_found: 1,
          models_missing: ["DemoFP:R_B"],
          skipped_libraries: [],
          problems: [],
        };
      case "user_library_export":
        return `${a.folder}/Demo.eak-library.json`;
      case "user_library_import":
        return { library: "Demo", imported: 1, rejected: [["BAD", "content hash does not match the payload"]], maturity_not_kept: ["CLAIM"] };
    }
    throw new Error(`unmocked ${cmd}`);
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("LibraryFolders", () => {
  it("scans a folder, needs a licence and a source, and reports what was not paired or found", async () => {
    render(<LibraryFolders />);
    await screen.findByText("EAK_R");
    fireEvent.click(screen.getByRole("button", { name: "Import a library folder…" }));
    await screen.findByText(/path variable\(s\) not set: X/);
    const go = screen.getByRole("button", { name: "Import to library database" }) as HTMLButtonElement;
    expect(go.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Folder licence"), { target: { value: "CC0-1.0" } });
    expect(go.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Folder source"), { target: { value: "https://example.org/demo" } });
    expect(go.disabled).toBe(false);
    fireEvent.click(go);
    await screen.findByText(/Stored 4 item\(s\)/);
    expect(screen.getByText(/Stored 4 item\(s\).*not verified/)).toBeTruthy();
    expect(screen.getByText(/Symbols without a matching footprint: Demo:EAK_ORPHAN/)).toBeTruthy();
    expect(screen.getByText(/Missing 3D models: DemoFP:R_B/)).toBeTruthy();
    expect(calls.find((c) => c.cmd === "kicad_library_import")?.args).toEqual({ folder: "/libs/demo", licence: "CC0-1.0", source: "https://example.org/demo" });
  });

  it("exports a chosen library and imports a file without trusting its maturity", async () => {
    render(<LibraryFolders />);
    await screen.findByText("EAK_R");
    const exp = screen.getByRole("button", { name: "Export library…" }) as HTMLButtonElement;
    expect(exp.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Library to export"), { target: { value: "Demo" } });
    dialog = "/out";
    fireEvent.click(exp);
    await screen.findByText("Saved /out/Demo.eak-library.json");
    expect(calls.find((c) => c.cmd === "user_library_export")?.args).toEqual({ library: "Demo", folder: "/out" });

    dialog = "/in/Demo.eak-library.json";
    fireEvent.click(screen.getByRole("button", { name: "Import a library file…" }));
    await screen.findByText("Imported 1 item(s) into Demo.");
    expect(screen.getByText(/higher maturity is not trusted\): CLAIM/)).toBeTruthy();
    expect(screen.getByText("Rejected BAD: content hash does not match the payload")).toBeTruthy();
    await waitFor(() => expect(calls.filter((c) => c.cmd === "library_db_search").length).toBeGreaterThan(1));
  });
});
