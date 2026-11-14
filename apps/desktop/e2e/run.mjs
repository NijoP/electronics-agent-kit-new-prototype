// Real desktop end-to-end workflow test for the EAK desktop app.
//
// Drives the actual debug build of apps/desktop (Tauri 2 + WebKitGTK) through tauri-driver
// and WebdriverIO: real window, real clicks, real Rust engine, real files on disk. No mocks.
// Where a workflow step has no UI control yet, the step is driven through the same Tauri IPC
// command the UI would use (window.__TAURI_INTERNALS__.invoke) and recorded as "IPC (no UI)".
//
// Every step records PASS / FAIL from what the app and the disk actually show. A failed step
// never stops the run; later steps run and report their own result.
//
// Usage: see README.md in this directory. Configuration is by environment variable only.
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { remote } from "webdriverio";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../../..");
const env = (k, d) => process.env[k] ?? d;
const APP = env("EAK_APP", path.join(env("CARGO_TARGET_DIR", path.join(repo, "target")), "debug", "eak-desktop"));
const TAURI_DRIVER = env("TAURI_DRIVER", "tauri-driver");
const NATIVE_DRIVER = env("NATIVE_DRIVER", ""); // WebKitWebDriver path; empty = from PATH
const OUT = env("E2E_OUT", fs.mkdtempSync(path.join(os.tmpdir(), "eak-e2e-")));
const SHOTS = env("E2E_SHOTS", path.join(OUT, "screenshots"));
const RESULTS = env("E2E_RESULTS", path.join(OUT, "E2E_RESULTS.md"));
const PORT = Number(env("E2E_PORT", "4444"));
const PROJECT_DIR = path.join(OUT, "workspace", `e2e-${Date.now()}`);
const PROJECT_NAME = "E2E Demo Board";

fs.mkdirSync(SHOTS, { recursive: true });
fs.mkdirSync(path.dirname(PROJECT_DIR), { recursive: true });
if (!fs.existsSync(APP)) {
  console.error(`App binary not found: ${APP} (set EAK_APP)`);
  process.exit(2);
}

// Isolated per-run app data (recent-project index, WebKit localStorage).
const appHome = path.join(OUT, "app-data");
const childEnv = {
  ...process.env,
  XDG_DATA_HOME: path.join(appHome, "data"),
  XDG_CONFIG_HOME: path.join(appHome, "config"),
  XDG_CACHE_HOME: path.join(appHome, "cache"),
};
const driverArgs = ["--port", String(PORT)];
if (NATIVE_DRIVER) driverArgs.push("--native-driver", NATIVE_DRIVER);
const driver = spawn(TAURI_DRIVER, driverArgs, { env: childEnv, stdio: ["ignore", "pipe", "pipe"] });
const driverLog = fs.createWriteStream(path.join(OUT, "tauri-driver.log"));
driver.stdout.pipe(driverLog);
driver.stderr.pipe(driverLog);
await new Promise((r) => setTimeout(r, 1500));

const steps = [];
let browser;
let shotN = 0;

async function shot(name) {
  const file = `${String(++shotN).padStart(2, "0")}-${name}.png`;
  try {
    await browser.saveScreenshot(path.join(SHOTS, file));
    return file;
  } catch {
    return null;
  }
}

/** Run one workflow step; never throws. `fn` returns evidence text or throws on failure. */
async function step(id, name, mode, fn) {
  const t0 = Date.now();
  let result = "PASS";
  let evidence = "";
  try {
    evidence = (await fn()) ?? "";
  } catch (e) {
    result = "FAIL";
    evidence = String(e?.message ?? e).split("\n")[0];
  }
  const screenshot = await shot(id);
  const ms = Date.now() - t0;
  steps.push({ id, name, mode, result, evidence, screenshot, ms });
  console.log(`${result.padEnd(4)} ${id} ${name} [${mode}] ${evidence}`);
  return result === "PASS";
}

const ok = (cond, msg) => {
  if (!cond) throw new Error(msg);
};

// The same IPC entry point the UI's api.ts uses.
// WebKitWebDriver cannot serialize promise results of `execute`, so use the async callback
// and pass the value as JSON; IPC errors come back as { err } and are rethrown here.
async function ipc(cmd, args = {}) {
  const raw = await browser.executeAsync(
    (c, a, done) =>
      window.__TAURI_INTERNALS__
        .invoke(c, a)
        .then((v) => done(JSON.stringify({ ok: v ?? null })))
        .catch((e) => done(JSON.stringify({ err: String(e) }))),
    cmd,
    args,
  );
  const r = JSON.parse(raw);
  if ("err" in r) throw new Error(`${cmd}: ${r.err}`);
  return r.ok;
}
const snap = () => ipc("current_project");
const count = (o) => Object.keys(o ?? {}).length;

async function waitFor(pred, msg, timeout = 15000) {
  await browser.waitUntil(pred, { timeout, interval: 200, timeoutMsg: msg });
}
async function waitRevisionAbove(rev, msg) {
  let s;
  try {
    await waitFor(async () => (s = await snap()).project.revision > rev, msg);
  } catch (e) {
    // Carry the app's own status line (it names the engine's rejection reason).
    throw new Error(`${msg}; status bar: ${(await statusMessage()).replace(/\s+/g, " ").slice(-200)}`);
  }
  return s;
}
/** Text of the status bar (the workspace's message line). */
const statusMessage = () =>
  browser.execute(() => [...document.querySelectorAll(".statusbar, .status-bar, footer")].map((e) => e.textContent).join(" | "));
async function clickButton(scope, text) {
  const b = await browser.$(scope).$(`button=${text}`);
  await b.waitForClickable({ timeout: 10000 });
  await b.click();
}
async function menu(top, item) {
  await clickButton('nav[aria-label="Application menu"]', top);
  const list = await browser.$(`ul[role="menu"][aria-label="${top}"]`);
  await list.waitForDisplayed({ timeout: 5000 });
  const b = await list.$(`button*=${item}`);
  await b.waitForClickable({ timeout: 5000 });
  await b.click();
}
/** Real pointer click at viewport coordinates (canvas tools act on pointerdown/up). */
async function clickAt(x, y) {
  await browser
    .action("pointer", { parameters: { pointerType: "mouse" } })
    .move({ x: Math.round(x), y: Math.round(y), origin: "viewport" })
    .down({ button: 0 })
    .up({ button: 0 })
    .perform();
}
const rectOf = (sel) =>
  browser.execute((s) => {
    const el = document.querySelector(s);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, w: r.width, h: r.height };
  }, sel);
/** Client point of a view-space (mm, y-down) point, mapped through the SVG's screen CTM. */
const viewToClient = (svgSel, vx, vy) =>
  browser.execute(
    (s, x, y) => {
      const svg = document.querySelector(s);
      if (!svg) return null;
      const p = svg.createSVGPoint();
      p.x = x;
      p.y = y;
      const q = p.matrixTransform(svg.getScreenCTM());
      return { x: q.x, y: q.y };
    },
    svgSel,
    vx,
    vy,
  );
const NM_PER_MM = 1_000_000;
/** Client point of an EAK-IR point ({x, y} nm, Y up). */
const irToClient = (svgSel, p) => viewToClient(svgSel, p.x / NM_PER_MM, -p.y / NM_PER_MM);
const inside = (r, p) => p && p.x > r.x + 2 && p.x < r.x + r.w - 2 && p.y > r.y + 2 && p.y < r.y + r.h - 2;
/**
 * Zoom a canvas with real mouse-wheel input (the Canvas zooms about the cursor) until `done`
 * holds. deltaY < 0 zooms in, > 0 zooms out. Returns whether `done` was reached.
 */
async function zoomUntil(svgSel, at, deltaY, done, maxSteps = 40) {
  for (let i = 0; i < maxSteps; i++) {
    if (await done()) return true;
    const c = at ? await irToClient(svgSel, at) : null;
    const r = await rectOf(svgSel);
    const x = Math.round(c && inside(r, c) ? c.x : r.x + r.w / 2);
    const y = Math.round(c && inside(r, c) ? c.y : r.y + r.h / 2);
    await browser.action("wheel").scroll({ x, y, deltaX: 0, deltaY, origin: "viewport" }).perform();
    await browser.pause(60);
  }
  return done();
}
const dist = (a, b) => Math.hypot(a.x - b.x, a.y - b.y);

const pre = {};
try {
  browser = await remote({
    hostname: "127.0.0.1",
    port: PORT,
    logLevel: "error",
    capabilities: { "tauri:options": { application: APP } },
  });

  await step("S01", "Launch app, Project Home visible", "UI", async () => {
    const h = await browser.$("main.home h1");
    await h.waitForDisplayed({ timeout: 30000 });
    const recent = await browser.$('section[aria-label="Recent projects"]').getText();
    return `home shown; ${recent.includes("No recent projects") ? "no recent projects (clean profile)" : "recent list present"}`;
  });

  await step("S02", "Create project (New project form)", "UI", async () => {
    const form = 'form[aria-label="Create project"]';
    const inputs = await browser.$$(`${form} input`);
    await inputs[0].setValue(PROJECT_DIR);
    await inputs[1].setValue(PROJECT_NAME);
    await clickButton(form, "Create");
    await browser.$(".workspace").waitForDisplayed({ timeout: 20000 });
    const s = await snap();
    ok(s.project.name === PROJECT_NAME, `project name ${s.project.name}`);
    ok(fs.existsSync(PROJECT_DIR), "project folder not on disk");
    return `project "${s.project.name}" at r${s.project.revision}; folder has ${fs.readdirSync(PROJECT_DIR).join(", ")}`;
  });

  await step("S03", "Open schematic editor", "UI", async () => {
    const sel = 'section[aria-label="Schematic view"]';
    if (!(await browser.$(sel).isExisting())) await menu("Project", "Open sheet");
    await browser.$(sel).waitForDisplayed({ timeout: 10000 });
    const s = await snap();
    return `schematic view shown; ${count(s.project.schematic.sheets)} sheet(s)`;
  });

  const schSvg = 'section[aria-label="Schematic view"] svg[aria-label="Schematic canvas"]';
  await step("S04", "Place two components (Place tool + canvas clicks)", "UI", async () => {
    const lib = await ipc("library_components");
    ok(lib.length > 0, "library is empty");
    const part = lib.find((c) => /resistor/i.test(`${c.name} ${c.id}`)) ?? lib[0];
    await clickButton('[aria-label="Schematic tools"]', "Place");
    await browser.$('select[aria-label="Part"]').selectByAttribute("value", part.id);
    const sheet = Object.values((await snap()).project.schematic.sheets)[0];
    // View space is mm with y pointing down; the sheet spans x 0..w, y -h..0.
    const w = sheet.width / NM_PER_MM;
    const h = sheet.height / NM_PER_MM;
    // Close together so both parts stay in view once zoomed in far enough to hit pins.
    for (const fx of [0.45, 0.55]) {
      const before = (await snap()).project.revision;
      const p = await viewToClient(schSvg, fx * w, -0.6 * h);
      ok(p, "schematic canvas not found");
      await clickAt(p.x, p.y);
      await waitRevisionAbove(before, "PlaceSymbol was not accepted");
    }
    const s = await snap();
    const syms = Object.values(s.project.schematic.symbols);
    ok(syms.length === 2, `expected 2 symbols, engine has ${syms.length}`);
    const drawn = (await browser.$$(`${schSvg} g[data-symbol-id]`)).length;
    ok(drawn === 2, `canvas draws ${drawn} symbols`);
    return `placed ${syms.map((x) => `${x.refdes}=${x.component_id}`).join(", ")} → r${s.project.revision}; canvas draws ${drawn}`;
  });

  await step("S05", "Connect parts (Connect pins tool + Wire tool)", "UI", async () => {
    const s0 = await snap();
    const syms = s0.geometry.symbols;
    ok(syms.length >= 2, "need two placed symbols");
    await clickButton('[aria-label="Schematic tools"]', "Connect pins");
    // Left/right part by x; inner pins face each other, outer pins face away.
    const [a, b] = [...syms].sort((p, q) => p.position.x - q.position.x);
    const byX = (s) => [...s.pins].sort((p, q) => p.position.x - q.position.x);
    const [aOuter, aInner] = [byX(a)[0], byX(a)[a.pins.length - 1]];
    const [bInner, bOuter] = [byX(b)[0], byX(b)[b.pins.length - 1]];
    // At fit-to-sheet zoom a part's pins are ~2 px apart; zoom in (real wheel input) about the
    // midpoint until 1 mm ≥ 6 px, so a click lands well inside the 1.5 mm pin-snap radius.
    const mid = { x: (a.position.x + b.position.x) / 2, y: (a.position.y + b.position.y) / 2 };
    const pxPerMm = async () =>
      dist(await irToClient(schSvg, mid), await irToClient(schSvg, { x: mid.x + 10 * NM_PER_MM, y: mid.y })) / 10;
    ok(await zoomUntil(schSvg, mid, -120, async () => (await pxPerMm()) >= 6), "could not zoom the schematic in");
    const r = await rectOf(schSvg);
    const click = async (p, what) => {
      const at = await irToClient(schSvg, p);
      ok(inside(r, at), `${what} at ${JSON.stringify(at)} is outside the zoomed canvas`);
      await clickAt(at.x, at.y);
    };
    let rev = s0.project.revision;
    // Inner pins: Connect pins tool (engine draws the wire).
    await click(aInner.position, `${a.refdes}.${aInner.number}`);
    await click(bInner.position, `${b.refdes}.${bInner.number}`);
    rev = (await waitRevisionAbove(rev, "ConnectPins was not accepted")).project.revision;
    // Outer pins: Wire tool, three segments routed 7.62 mm above both parts.
    await clickButton('[aria-label="Schematic tools"]', "Wire");
    const up = 7.62 * NM_PER_MM;
    const path = [aOuter.position, { x: aOuter.position.x, y: aOuter.position.y + up }, { x: bOuter.position.x, y: bOuter.position.y + up }, bOuter.position];
    await click(path[0], "wire start");
    for (const [i, p] of path.slice(1).entries()) {
      await click(p, `wire point ${i + 1}`);
      rev = (await waitRevisionAbove(rev, `AddWire segment ${i + 1} not accepted`)).project.revision;
    }
    await browser.keys(["Escape"]);
    const s = await snap();
    return `Connect pins ${a.refdes}.${aInner.number}–${b.refdes}.${bInner.number} + 3-segment Wire ${a.refdes}.${aOuter.number}–${b.refdes}.${bOuter.number} → r${s.project.revision}; ${count(s.project.schematic.wires)} wire(s) in IR`;
  });

  await step("S06", "Sync schematic to PCB (Project menu)", "UI", async () => {
    const before = (await snap()).project.revision;
    await menu("Project", "Sync schematic to PCB");
    const s = await waitRevisionAbove(before, "SyncSchematicToPcb not accepted");
    const fps = count(s.project.pcb.footprints);
    ok(fps === 2, `expected 2 footprints, got ${fps}`);
    if (!(await browser.$('section[aria-label="PCB view"]').isExisting())) await menu("Project", "Open PCB");
    await browser.$('section[aria-label="PCB view"]').waitForDisplayed({ timeout: 10000 });
    return `${fps} footprints on PCB → r${s.project.revision}`;
  });

  // The engine's connectivity analysis is PCB copper connectivity (the one DRC-004 uses); the
  // UI shows it only as airwires. There is no netlist / connectivity view, so the numbers are
  // read through the same IPC the PCB view calls.
  await step("S07", "Connectivity: nets + airwires after sync", "IPC (no UI) + UI airwires", async () => {
    const s = await snap();
    const c = await ipc("connectivity");
    ok(c.revision === s.project.revision, `connectivity is for r${c.revision}, project is r${s.project.revision}`);
    const nets = [...new Set(c.ratsnest.map(([n]) => n))];
    ok(nets.length >= 2, `expected unrouted connections on ≥2 nets, got ${nets.length}`);
    let drawn = 0;
    await waitFor(async () => (drawn = (await browser.$$('section[aria-label="PCB view"] line.airwire')).length) >= c.ratsnest.length, "airwires not drawn");
    return `${nets.length} nets (${nets.join(", ")}), ${c.ratsnest.length} unrouted, ${c.shorts.length} shorts; PCB draws ${drawn} airwires. No netlist/connectivity view in the UI`;
  });

  const pcbSvg = 'section[aria-label="PCB view"] svg[aria-label="PCB canvas"]';
  await step("S08", "Board outline (Board outline tool, 2 corners, Finish)", "UI", async () => {
    await clickButton('[aria-label="PCB tools"]', "Board outline");
    const s0 = await snap();
    // A 50 × 36 mm board centred on the synced footprints, drawn in board millimetres.
    const fps = Object.values(s0.project.pcb.footprints).map((f) => f.position);
    const c = { x: fps.reduce((t, p) => t + p.x, 0) / fps.length, y: fps.reduce((t, p) => t + p.y, 0) / fps.length };
    const c1 = { x: c.x - 25 * NM_PER_MM, y: c.y + 18 * NM_PER_MM };
    const c2 = { x: c.x + 25 * NM_PER_MM, y: c.y - 18 * NM_PER_MM };
    const bothIn = async () => {
      const r = await rectOf(pcbSvg);
      return inside(r, await irToClient(pcbSvg, c1)) && inside(r, await irToClient(pcbSvg, c2));
    };
    ok(await zoomUntil(pcbSvg, c, 120, bothIn), "could not zoom the PCB out to fit a 50 mm board");
    const before = s0.project.revision;
    for (const p of [c1, c2]) {
      const at = await irToClient(pcbSvg, p);
      await clickAt(at.x, at.y);
    }
    await clickButton('section[aria-label="PCB view"]', "Finish outline (2 pts)");
    const s = await waitRevisionAbove(before, "SetBoardOutline not accepted");
    const o = s.project.pcb.board.outline;
    ok(o.length >= 3, "outline not stored");
    const xs = o.map((p) => p.x / NM_PER_MM);
    const ys = o.map((p) => p.y / NM_PER_MM);
    await clickButton('[aria-label="PCB tools"]', "Select");
    return `outline ${o.length} points, ${(Math.max(...xs) - Math.min(...xs)).toFixed(1)} × ${(Math.max(...ys) - Math.min(...ys)).toFixed(1)} mm → r${s.project.revision}`;
  });

  await step("S09", "Footprint placement (Design → Auto-place footprints)", "UI", async () => {
    const before = (await snap()).project.revision;
    await menu("Design", "Auto-place footprints");
    const s = await waitRevisionAbove(before, "AutoPlace not accepted");
    const pos = Object.values(s.project.pcb.footprints).map(
      (f) => `${(f.position.x / NM_PER_MM).toFixed(1)},${(f.position.y / NM_PER_MM).toFixed(1)}`,
    );
    return `footprints at ${pos.join(" / ")} mm → r${s.project.revision}`;
  });

  await step("S10", "Layers: set 4 copper layers (PCB Copper layers select)", "UI", async () => {
    const before = (await snap()).project.revision;
    await browser.$('select[aria-label="Copper layers"]').selectByAttribute("value", "4");
    const s = await waitRevisionAbove(before, "SetLayerCount not accepted");
    const toggles = (await browser.$$('section[aria-label="PCB view"] [aria-label="Layers"] input[type="checkbox"]')).length;
    ok(toggles >= 4, `layer panel shows ${toggles} copper layer toggles`);
    return `PCB layer panel shows ${toggles} copper layers → r${s.project.revision}`;
  });

  await step("S11", "Route (Route → Auto-route all nets)", "UI", async () => {
    const before = (await snap()).project.revision;
    await menu("Route", "Auto-route all nets");
    const s = await waitRevisionAbove(before, "AutoRoute not accepted");
    const tracks = count(s.project.pcb.tracks);
    const c = await ipc("connectivity");
    ok(tracks > 0, "no tracks after auto-route");
    return `${tracks} track segment(s), ${count(s.project.pcb.vias)} via(s), ${c.ratsnest.length} unrouted connection(s) → r${s.project.revision}`;
  });

  await step("S12", "Copper zone (Zone tool, layer + net, 2 corners, Finish)", "UI", async () => {
    await clickButton('[aria-label="PCB tools"]', "Zone");
    const opts = (s) =>
      browser.execute((q) => [...document.querySelectorAll(q + " option")].map((o) => o.value).filter(Boolean), s);
    const netSel = 'section[aria-label="PCB view"] select[aria-label="Net"]';
    const layerSel = 'section[aria-label="PCB view"] select[aria-label="Layer"]';
    const nets = await opts(netSel);
    ok(nets.length > 0, "no nets offered for the zone");
    await browser.$(netSel).selectByAttribute("value", nets[0]);
    const layers = await opts(layerSel);
    const zl = layers[layers.length - 1];
    await browser.$(layerSel).selectByAttribute("value", zl);
    const s0 = await snap();
    // Zone inset 3 mm inside the board outline's bounding box, in board millimetres.
    const o = s0.project.pcb.board.outline;
    const z1 = { x: Math.min(...o.map((p) => p.x)) + 3 * NM_PER_MM, y: Math.max(...o.map((p) => p.y)) - 3 * NM_PER_MM };
    const z2 = { x: Math.max(...o.map((p) => p.x)) - 3 * NM_PER_MM, y: Math.min(...o.map((p) => p.y)) + 3 * NM_PER_MM };
    const zc = { x: (z1.x + z2.x) / 2, y: (z1.y + z2.y) / 2 };
    const bothIn = async () => {
      const r = await rectOf(pcbSvg);
      return inside(r, await irToClient(pcbSvg, z1)) && inside(r, await irToClient(pcbSvg, z2));
    };
    ok(await zoomUntil(pcbSvg, zc, 120, bothIn), "could not fit the zone corners in the PCB canvas");
    const before = s0.project.revision;
    for (const p of [z1, z2]) {
      const at = await irToClient(pcbSvg, p);
      await clickAt(at.x, at.y);
    }
    await clickButton('section[aria-label="PCB view"]', "Finish zone (2 pts)");
    const s = await waitRevisionAbove(before, "AddZone not accepted");
    ok(count(s.project.pcb.zones) === 1, "zone not stored");
    return `zone on layer ${zl}, net ${nets[0]} → r${s.project.revision}`;
  });

  for (const [id, chk] of [
    ["S13", "erc"],
    ["S14", "drc"],
  ]) {
    const label = chk.toUpperCase();
    await step(id, `${label} (toolbar Run ${label})`, "UI", async () => {
      await clickButton("header.toolbar", `Run ${label}`);
      await waitFor(async () => (await statusMessage()).includes(`${chk} ran on revision`) || (await statusMessage()).includes("ran on revision"), `${chk} did not report in the status bar`);
      // The same check through IPC, to read the full report the findings panel renders.
      const r = (await ipc("run_check", { check: chk })).report;
      ok(Array.isArray(r.not_implemented) && Array.isArray(r.limitations), "report lacks not_implemented/limitations");
      ok(!/manufacturing.ready/i.test(JSON.stringify(r)), "report claims manufacturing-ready");
      return `outcome ${r.outcome}, ${r.errors} error(s), ${r.warnings} warning(s); lists ${r.not_implemented.length} not_implemented and ${r.limitations.length} limitation(s)`;
    });
  }

  await step("S15", "Generate outputs + manufacturing checks (toolbar Export Gerbers)", "UI", async () => {
    await clickButton("header.toolbar", "Export Gerbers");
    let msg = "";
    await waitFor(async () => /Exported \d+ files to/.test((msg = await statusMessage())), "export did not report", 60000);
    const m = msg.match(/Exported (\d+) files to (.+?) \((DRAFT|INTERNALLY_VERIFIED_RELEASE_CANDIDATE)\)/);
    ok(m, `unexpected message: ${msg}`);
    const dir = m[2];
    ok(fs.existsSync(dir), `export folder ${dir} not on disk`);
    const files = fs.readdirSync(dir, { recursive: true }).filter((f) => fs.statSync(path.join(dir, f)).isFile());
    ok(files.length >= Number(m[1]), `disk has ${files.length} files, UI said ${m[1]}`);
    return `${files.length} files on disk, status ${m[3]}: ${files.slice(0, 8).join(", ")}${files.length > 8 ? ", …" : ""}; ERC+DRC+mech reports run inside export (no separate manufacturing-check control)`;
  });

  await step("S16", "Save (Ctrl+S); engine persists every command", "UI", async () => {
    const s = await snap();
    pre.rev = s.project.revision;
    pre.counts = [count(s.project.schematic.symbols), count(s.project.pcb.footprints), count(s.project.pcb.tracks), count(s.project.pcb.zones)];
    await browser.$("header.toolbar strong").click();
    await browser.keys(["Control", "s"]);
    await waitFor(async () => (await statusMessage()).includes("Saved:"), "no save confirmation in the status bar");
    return `status: "${(await statusMessage()).match(/Saved:[^|]*/)?.[0]?.trim()}"; project folder: ${fs.readdirSync(PROJECT_DIR).join(", ")}`;
  });

  await step("S17", "Close project (toolbar Close project)", "UI", async () => {
    await clickButton("header.toolbar", "Close project");
    await browser.$("main.home").waitForDisplayed({ timeout: 10000 });
    let err = "";
    try {
      await snap();
    } catch (e) {
      err = String(e);
    }
    ok(err, "engine still reports an open project after close");
    return "back on Project Home; current_project now errors (engine closed)";
  });

  await step("S18", "Reopen from Recent projects and compare", "UI", async () => {
    const b = await browser.$('section[aria-label="Recent projects"]').$(`button*=${PROJECT_NAME}`);
    await b.waitForClickable({ timeout: 10000 });
    await b.click();
    await browser.$(".workspace").waitForDisplayed({ timeout: 20000 });
    const s = await snap();
    const counts = [count(s.project.schematic.symbols), count(s.project.pcb.footprints), count(s.project.pcb.tracks), count(s.project.pcb.zones)];
    ok(s.project.revision === pre.rev, `revision ${s.project.revision} != ${pre.rev}`);
    ok(JSON.stringify(counts) === JSON.stringify(pre.counts), `counts ${counts} != ${pre.counts}`);
    return `reopened at r${s.project.revision}; symbols/footprints/tracks/zones = ${counts.join("/")} (identical to before close)`;
  });
} catch (e) {
  steps.push({ id: "SETUP", name: "WebDriver session", mode: "-", result: "FAIL", evidence: String(e?.message ?? e).split("\n")[0], screenshot: null, ms: 0 });
  console.error(e);
} finally {
  try {
    await browser?.deleteSession();
  } catch {}
  driver.kill();
}

const passed = steps.filter((s) => s.result === "PASS").length;
const esc = (t) => String(t).replace(/\|/g, "\\|");
const md = [
  `| Step | Workflow step | Driven via | Result | Evidence | Screenshot |`,
  `|---|---|---|---|---|---|`,
  ...steps.map((s) => `| ${s.id} | ${esc(s.name)} | ${s.mode} | **${s.result}** | ${esc(s.evidence)} | ${s.screenshot ?? "-"} |`),
  ``,
  `${passed}/${steps.length} steps passed.`,
].join("\n");
fs.writeFileSync(RESULTS, md + "\n");
fs.writeFileSync(path.join(OUT, "results.json"), JSON.stringify(steps, null, 2));
console.log(`\n${passed}/${steps.length} passed. Table: ${RESULTS}. Screenshots: ${SHOTS}`);
process.exit(passed === steps.length ? 0 : 1);
