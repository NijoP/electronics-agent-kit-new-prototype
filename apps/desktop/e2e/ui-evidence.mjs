// Real-window UI evidence for the EAK desktop app (CI, Linux, Xvfb).
//
// Runs a data-driven scenario (scenarios/*.json) against the real debug build under an X
// server. Input is real X input through XTest (xdotool); screenshots are of the whole X
// screen (ImageMagick `import -window root`), so they show the real window, not a webview
// render. WebDriver (tauri-driver) is used only to read the DOM: to find where an element is
// and to wait until the UI shows the expected state. The one non-UI action is fixture setup,
// which creates an isolated software project through the same IPC the UI uses and is labelled
// SETUP in the results.
//
// Usage and step reference: docs/qa/UI_EVIDENCE.md. Configuration is by environment variable.
import { execFileSync, spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { remote } from "webdriverio";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../../..");
const env = (k, d) => process.env[k] ?? d;
const APP = env("EAK_APP", path.join(env("CARGO_TARGET_DIR", path.join(repo, "target")), "debug", "eak-desktop"));
const SCENARIO = path.resolve(env("UI_SCENARIO", path.join(here, "scenarios", "a1-menus.json")));
const OUT = env("UI_OUT", fs.mkdtempSync(path.join(os.tmpdir(), "eak-ui-")));
const SHOTS = path.join(OUT, "screenshots");
const PORT = Number(env("UI_PORT", "4444"));
const NATIVE_DRIVER = env("NATIVE_DRIVER", "");

fs.mkdirSync(SHOTS, { recursive: true });
if (!fs.existsSync(APP)) {
  console.error(`App binary not found: ${APP} (set EAK_APP)`);
  process.exit(2);
}
const scenario = JSON.parse(fs.readFileSync(SCENARIO, "utf8"));

// Isolated app data: no recent projects, no saved layout, nothing from a real profile.
const appHome = path.join(OUT, "app-data");
const childEnv = {
  ...process.env,
  GDK_BACKEND: "x11",
  XDG_DATA_HOME: path.join(appHome, "data"),
  XDG_CONFIG_HOME: path.join(appHome, "config"),
  XDG_CACHE_HOME: path.join(appHome, "cache"),
};
// tauri-driver starts WebKitWebDriver on --native-port (default 4445): keep the two apart.
const driverArgs = ["--port", String(PORT), "--native-port", String(PORT + 1)];
if (NATIVE_DRIVER) driverArgs.push("--native-driver", NATIVE_DRIVER);
const driver = spawn(env("TAURI_DRIVER", "tauri-driver"), driverArgs, { env: childEnv, stdio: ["ignore", "pipe", "pipe"] });
const driverLog = fs.createWriteStream(path.join(OUT, "tauri-driver.log"));
driver.stdout.pipe(driverLog);
driver.stderr.pipe(driverLog);
await new Promise((r) => setTimeout(r, 1500));

const sh = (cmd, args) => execFileSync(cmd, args, { encoding: "utf8", env: childEnv }).trim();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
let browser;
let shotN = 0;

function shot(name) {
  const file = `${String(++shotN).padStart(3, "0")}-${name.replace(/[^A-Za-z0-9._-]+/g, "_")}.png`;
  try {
    sh("import", ["-window", "root", path.join(SHOTS, file)]);
    return file;
  } catch (e) {
    console.error(`screenshot failed: ${e.message}`);
    return null;
  }
}

/** Screen position of the app's X window (there is no window manager under Xvfb, so no frame). */
function origin() {
  const id = sh("xdotool", ["search", "--sync", "--onlyvisible", "--name", "^EAK$"]).split("\n")[0];
  const g = Object.fromEntries(
    sh("xdotool", ["getwindowgeometry", "--shell", id])
      .split("\n")
      .map((l) => l.split("=")),
  );
  return { x: Number(g.X), y: Number(g.Y) };
}

/** Viewport rect of the first visible element matching `selector` whose text starts with `text`. */
const rectOf = (selector, text) =>
  browser.execute(
    (s, t) => {
      const norm = (x) => x.replace(/\s+/g, " ").trim();
      for (const el of document.querySelectorAll(s)) {
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) continue;
        if (t && !norm(el.textContent ?? "").startsWith(t)) continue;
        return { x: r.x, y: r.y, w: r.width, h: r.height };
      }
      return null;
    },
    selector,
    text ?? null,
  );

async function waitRect(selector, text, timeout = 15000) {
  let r = null;
  await browser.waitUntil(async () => (r = await rectOf(selector, text)) !== null, {
    timeout,
    interval: 200,
    timeoutMsg: `not visible after ${timeout} ms: ${selector}${text ? ` "${text}"` : ""}`,
  });
  return r;
}

/**
 * What is at viewport point (vx, vy) when it is not the element itself (or inside it): a clipped,
 * scrolled-out or covered element would take the click somewhere else, so the step fails instead
 * (except under another shape of the same SVG drawing).
 */
const coveredBy = (selector, text, vx, vy) =>
  browser.execute(
    (s, t, x, y) => {
      const norm = (v) => v.replace(/\s+/g, " ").trim();
      const el = [...document.querySelectorAll(s)].find((e) => {
        const r = e.getBoundingClientRect();
        return r.width > 0 && r.height > 0 && (!t || norm(e.textContent ?? "").startsWith(t));
      });
      const hit = document.elementFromPoint(x, y);
      if (!el || (hit && el.contains(hit))) return null;
      // Inside a drawing, another shape of the same SVG over the target (a marker on a pin) is
      // part of the canvas the click is meant for: the canvas handles it by position.
      if (hit && el.ownerSVGElement && el.ownerSVGElement.contains(hit)) return null;
      return hit ? `${hit.tagName.toLowerCase()}${typeof hit.className === "string" && hit.className ? `.${hit.className.split(" ").join(".")}` : ""}` : "nothing (outside the window)";
    },
    selector,
    text ?? null,
    vx,
    vy,
  );

/** Scroll the first visible match into view (no-op when it is already visible), as a user would. */
const reveal = (selector, text) =>
  browser.execute(
    (s, t) => {
      const norm = (x) => x.replace(/\s+/g, " ").trim();
      for (const el of document.querySelectorAll(s)) {
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) continue;
        if (t && !norm(el.textContent ?? "").startsWith(t)) continue;
        el.scrollIntoView({ block: "nearest", inline: "nearest" });
        return;
      }
    },
    selector,
    text ?? null,
  );

/** Real X pointer click (XTest) at the centre of an element, optionally offset by dx/dy px;
 *  the element is first scrolled into view if needed, as a user would. */
async function xclick(selector, text, dx = 0, dy = 0) {
  await waitRect(selector, text);
  await reveal(selector, text);
  await sleep(150);
  const r = await waitRect(selector, text);
  const cover = await coveredBy(selector, text, r.x + r.w / 2 + dx, r.y + r.h / 2 + dy);
  if (cover) throw new Error(`the click point of ${selector}${text ? ` "${text}"` : ""} is covered by or scrolled out to ${cover}`);
  const o = origin();
  const x = Math.round(o.x + r.x + r.w / 2 + dx);
  const y = Math.round(o.y + r.y + r.h / 2 + dy);
  sh("xdotool", ["mousemove", "--sync", String(x), String(y), "click", "1"]);
  await sleep(250);
  return `XTest click at ${x},${y}`;
}

/** Screen point at fractions (fx, fy) of an element (0.5, 0.5 is its centre). */
async function screenPoint(selector, text, fx = 0.5, fy = 0.5) {
  const r = await waitRect(selector, text);
  const cover = await coveredBy(selector, text, r.x + r.w * fx, r.y + r.h * fy);
  if (cover) throw new Error(`point (${fx}, ${fy}) of ${selector} is covered by or scrolled out to ${cover}`);
  const o = origin();
  return { x: Math.round(o.x + r.x + r.w * fx), y: Math.round(o.y + r.y + r.h * fy) };
}

/** Whitespace-normalised text of the first visible match, or null. */
const textOf = (selector) =>
  browser.execute((s) => {
    for (const el of document.querySelectorAll(s)) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      return (el.textContent ?? "").replace(/\s+/g, " ").trim();
    }
    return null;
  }, selector);
const attrOf = (selector, attr) => browser.execute((s, a) => document.querySelector(s)?.getAttribute(a) ?? null, selector, attr);

/** Waits until no visible element matches (a hidden dock, a closed menu). */
async function waitGone(selector, timeout = 15000) {
  await browser.waitUntil(async () => (await rectOf(selector)) === null, { timeout, interval: 200, timeoutMsg: `still visible after ${timeout} ms: ${selector}` });
}
/** Values remembered by a step (`save`) for a later `check-attr` (`equalsVar`). */
const vars = {};

const xkey = (keys) => sh("xdotool", ["key", "--clearmodifiers", ...keys.split(" ")]);

const MENU_BAR = 'nav[aria-label="Application menu"]';
const menuButtons = () =>
  browser.execute((s) => [...document.querySelectorAll(`${s} button.menu-btn`)].map((b) => b.textContent.trim()), MENU_BAR);
const openMenuItems = (label) =>
  browser.execute(
    (l) => [...document.querySelectorAll(`ul[role="menu"][aria-label="${l}"] [role="menuitem"]`)].map((b) => b.textContent.replace(/\s+/g, " ").trim()),
    label,
  );

async function openMenu(label) {
  await xclick(`${MENU_BAR} button.menu-btn`, label);
  await waitRect(`ul[role="menu"][aria-label="${label}"]`, null, 5000);
}
/** Escape closes the menu; a second click on its button (a toggle) also does. Throws if neither does. */
async function closeMenu(label) {
  const list = `ul[role="menu"][aria-label="${label}"]`;
  const closed = async () => (await rectOf(list)) === null;
  xkey("Escape");
  try {
    await browser.waitUntil(closed, { timeout: 3000 });
    return "closed with Escape";
  } catch {}
  await xclick(`${MENU_BAR} button.menu-btn`, label);
  await browser.waitUntil(closed, { timeout: 3000, timeoutMsg: `menu ${label} closed neither on Escape nor on a second click` });
  return "Escape did not close it; closed with a second click on the menu button";
}

// The same IPC entry point the UI's api.ts uses (async callback: WebKitWebDriver cannot return promises).
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

/** One result row: runs `fn`, which returns evidence text (or { evidence, file }) or throws. */
async function run(st, fn) {
  const t0 = Date.now();
  let result = "PASS";
  let evidence = "";
  let file = null;
  try {
    const r = await fn();
    if (r && typeof r === "object") ({ evidence, file } = r);
    else evidence = r ?? "";
  } catch (e) {
    result = "FAIL";
    evidence = String(e?.message ?? e).split("\n")[0];
  }
  if (!file && st.shot !== false) file = shot(st.id);
  // Evidence that was expected but not taken is a failure, not a pass without a picture.
  if (!file && st.shot !== false) {
    result = "FAIL";
    evidence = `no screenshot taken${evidence ? `; ${evidence}` : ""}`;
  }
  const row = { id: st.id, title: st.title ?? st.do, result, evidence, file, ms: Date.now() - t0 };
  results.push(row);
  console.log(`${result.padEnd(4)} ${row.id} ${row.title} — ${evidence}`);
}

const savedRects = new Map();

/** Step types (docs/qa/UI_EVIDENCE.md). Each returns evidence text or throws. */
const handlers = {
  async wait(s) {
    await waitRect(s.selector, s.text, s.timeout ?? 30000);
    return `visible: ${s.selector}${s.text ? ` "${s.text}"` : ""}`;
  },
  async "fixture-project"(s) {
    const dir = path.join(OUT, "fixture", s.folder ?? "ui-evidence-fixture");
    fs.mkdirSync(path.dirname(dir), { recursive: true });
    await ipc("create_project", { path: dir, name: s.name });
    // Reload to Project Home so the project is then opened through the UI (Recent list).
    await browser.execute(() => window.location.reload());
    await waitRect("button.home-row", s.name, 30000);
    return `SETUP (IPC create_project, isolated software fixture "${s.name}"; not a UI action)`;
  },
  // SETUP: one typed engine command on the active project through the same IPC the UI's
  // api.execute uses (for example a fixture board outline). Run it before the project is opened
  // from Recent projects, so the UI loads the result.
  async "fixture-command"(s) {
    const snap = await ipc("current_project");
    const r = await ipc("execute_command", { expected_revision: snap.project.revision, command: s.command });
    if (!r.accepted) throw new Error(`${s.command.type} rejected: ${JSON.stringify(r.error)}`);
    return `SETUP (IPC execute_command ${s.command.type} → revision ${r.revision_after}; not a UI action)`;
  },
  // SETUP: an open-source library folder (manifest with licence and source, plus one definition
  // copied from a built-in part under a new id), imported through the IPC the Library browser
  // uses. Needs an open project (the built-in part is read through library_search).
  async "fixture-library"(s) {
    const dir = path.join(OUT, "fixture", `library-${s.name}`);
    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(dir, { recursive: true });
    const hits = await ipc("library_search", { query: s.from, category: "", source: "builtin" });
    const base = (hits ?? []).find((p) => p.id === s.from);
    if (!base) throw new Error(`no built-in part ${s.from}`);
    const def = { ...base.def, id: s.part, name: s.partName, provenance: `UI evidence fixture: copy of built-in ${s.from}` };
    fs.writeFileSync(path.join(dir, `${s.part}.json`), JSON.stringify(def, null, 2));
    fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ name: s.name, licence: s.licence, source_url: s.source }, null, 2));
    const r = await ipc("library_import_open_source", { dir });
    return `SETUP (IPC library_import_open_source: "${s.name}", licence ${s.licence}, ${r.accepted.length} part(s); not a UI action)`;
  },
  // SETUP: writes the scenario's `.kicad_sym` text (`lines`, an original test symbol) and imports
  // it through `kicad_import_commit`, the IPC the KiCad import dialog commits with (licence and
  // source recorded). The dialog itself needs a native file chooser.
  async "fixture-kicad-symbol"(s) {
    const dir = path.join(OUT, "fixture");
    fs.mkdirSync(dir, { recursive: true });
    const file = path.join(dir, `${s.file}.kicad_sym`);
    fs.writeFileSync(file, s.lines.join("\n") + "\n");
    const r = await ipc("kicad_import_commit", { path: file, licence: s.licence, source: s.source });
    if (r.components_created !== 1) throw new Error(`expected 1 imported symbol, got ${r.components_created} (rejected: ${r.rejected.join("; ")})`);
    return `SETUP (IPC kicad_import_commit: ${s.file}.kicad_sym, licence ${s.licence}, ${r.component_ids.join(", ")}; not a UI action)`;
  },
  // XTest click into a field, then XTest typing.
  // XTest typing of `value`: into `selector` after clicking it, or without one into the focused field.
  async type(s) {
    const how = s.selector ? await xclick(s.selector, s.text) : "focused field";
    sh("xdotool", ["type", "--delay", "40", "--", s.value]);
    await sleep(300);
    if (s.expect) await waitRect(s.expect, s.expectText, s.timeout ?? 20000);
    return `${how}; XTest typed "${s.value}"${s.expect ? `; then visible: ${s.expect}` : ""}`;
  },
  async click(s) {
    const how = await xclick(s.selector, s.text, s.dx ?? 0, s.dy ?? 0);
    if (s.expect) await waitRect(s.expect, s.expectText, s.timeout ?? 20000);
    if (s.gone) await waitGone(s.gone, s.timeout ?? 20000);
    return `${how}${s.expect ? `; then visible: ${s.expect}` : ""}${s.gone ? `; then gone: ${s.gone}` : ""}`;
  },
  // Passes once no visible element matches (a removed control, a closed dialog); fails if one
  // is still visible after `timeout` ms.
  async absent(s) {
    const timeout = s.timeout ?? 3000;
    const t0 = Date.now();
    while (Date.now() - t0 < timeout) {
      if ((await rectOf(s.selector, s.text)) === null) return `absent: ${s.selector}${s.text ? ` "${s.text}"` : ""}`;
      await sleep(200);
    }
    throw new Error(`still visible after ${timeout} ms: ${s.selector}${s.text ? ` "${s.text}"` : ""}`);
  },
  async key(s) {
    xkey(s.keys);
    await sleep(300);
    if (s.expect) await waitRect(s.expect, s.expectText, s.timeout ?? 20000);
    if (s.gone) await waitGone(s.gone, s.timeout ?? 20000);
    return `XTest key ${s.keys}${s.expect ? `; then visible: ${s.expect}` : ""}${s.gone ? `; then gone: ${s.gone}` : ""}`;
  },
  // XTest pointer move to fractions `at` of an element; `report` is read after it. With
  // `changed` the report must differ from what it showed before the move.
  async move(s) {
    const before = s.report ? await textOf(s.report) : null;
    const [fx, fy] = s.at ?? [0.5, 0.5];
    const p = await screenPoint(s.selector, s.text, fx, fy);
    // Two motion events: the first one crossing into the element may arrive as an enter only.
    sh("xdotool", ["mousemove", "--sync", String(p.x - 2), String(p.y - 2)]);
    await sleep(100);
    sh("xdotool", ["mousemove", "--sync", String(p.x), String(p.y)]);
    await sleep(300);
    if (!s.report) return `XTest move to ${p.x},${p.y}`;
    let now = null;
    await browser.waitUntil(async () => {
      now = await textOf(s.report);
      return now !== null && now !== "—" && (!s.changed || now !== before);
    }, { timeout: s.timeout ?? 5000, interval: 100, timeoutMsg: `${s.report} did not update (before "${before}")` });
    return `XTest move to ${p.x},${p.y}; ${s.report}: "${before}" → "${now}"`;
  },
  // XTest drag: press at the element's centre, move by dx/dy in steps, release. `attr` (default
  // aria-valuenow) of the element must change; `save` remembers its new value.
  async drag(s) {
    const attr = s.attr ?? "aria-valuenow";
    const before = await attrOf(s.selector, attr);
    const p = await screenPoint(s.selector, s.text);
    const steps = 6;
    sh("xdotool", ["mousemove", "--sync", String(p.x), String(p.y), "mousedown", "1"]);
    for (let i = 1; i <= steps; i++) {
      await sleep(40);
      sh("xdotool", ["mousemove", "--sync", String(Math.round(p.x + ((s.dx ?? 0) * i) / steps)), String(Math.round(p.y + ((s.dy ?? 0) * i) / steps))]);
    }
    sh("xdotool", ["mouseup", "1"]);
    await sleep(300);
    const after = await attrOf(s.selector, attr);
    if (after === before) throw new Error(`${attr} stayed ${before} after an XTest drag of ${s.dx ?? 0},${s.dy ?? 0}`);
    if (s.save) vars[s.save] = after;
    return `XTest drag ${s.dx ?? 0},${s.dy ?? 0} from ${p.x},${p.y}; ${attr} ${before} → ${after}`;
  },
  // XTest typing into whatever has the keyboard focus, then waits for `expect`.
  async gone(s) {
    await waitGone(s.selector, s.timeout ?? 15000);
    return `not visible: ${s.selector}`;
  },
  // Reads an attribute; with `equalsVar` it must equal the value a previous step saved.
  async "check-attr"(s) {
    await waitRect(s.selector, s.text, s.timeout ?? 15000);
    const v = await attrOf(s.selector, s.attr);
    if (s.equalsVar && v !== vars[s.equalsVar]) throw new Error(`${s.attr} is ${v}, expected ${vars[s.equalsVar]} (saved as ${s.equalsVar})`);
    return `${s.selector} ${s.attr}=${v}${s.equalsVar ? ` (equals saved ${s.equalsVar})` : ""}`;
  },
  // Reloads the webview (a new UI session with the same app data), then waits for `expect`.
  async reload(s) {
    await browser.execute(() => window.location.reload());
    if (s.expect) await waitRect(s.expect, s.expectText, s.timeout ?? 30000);
    return `webview reloaded${s.expect ? `; then visible: ${s.expect}` : ""}`;
  },
  async menu(s) {
    await openMenu(s.menu);
    await xclick(`ul[role="menu"][aria-label="${s.menu}"] [role="menuitem"]`, s.item);
    if (s.expect) await waitRect(s.expect, s.expectText, s.timeout ?? 20000);
    if (s.gone) await waitGone(s.gone, s.timeout ?? 20000);
    return `${s.menu} › ${s.item}${s.expect ? `; then visible: ${s.expect}` : ""}`;
  },
  async shot() {
    return "screenshot";
  },
  // DOM read: the viewport rect of the first visible match. `save` stores it under a name;
  // `same` fails unless it equals (within 0.5 px) the rect saved under that name.
  async rect(s) {
    const r = await waitRect(s.selector, s.text, s.timeout ?? 10000);
    const fmt = (q) => `x=${q.x.toFixed(1)} y=${q.y.toFixed(1)} w=${q.w.toFixed(1)} h=${q.h.toFixed(1)}`;
    if (s.save) {
      savedRects.set(s.save, r);
      return `saved "${s.save}": ${fmt(r)}`;
    }
    const before = savedRects.get(s.same);
    if (!before) throw new Error(`no rect saved as "${s.same}"`);
    if (["x", "y", "w", "h"].some((k) => Math.abs(r[k] - before[k]) > 0.5)) throw new Error(`moved: was ${fmt(before)}, now ${fmt(r)}`);
    return `unchanged since "${s.same}": ${fmt(r)}`;
  },
  // DOM read: waits until exactly `equals` elements match (visible or not), e.g. markers drawn.
  async count(s) {
    let n = -1;
    const read = () => browser.execute((q) => document.querySelectorAll(q).length, s.selector);
    try {
      await browser.waitUntil(async () => (n = await read()) === s.equals, { timeout: s.timeout ?? 10000, interval: 200 });
    } catch {
      throw new Error(`${n} element(s) match ${s.selector}, expected ${s.equals}`);
    }
    return `${n} element(s) match ${s.selector}`;
  },
  // Opens every top-level menu in turn: one row and one screenshot per menu, listing its items.
  async "each-menu"(s) {
    const labels = await menuButtons();
    if (labels.length === 0) throw new Error("no menus in the application menu bar");
    for (const label of labels) {
      await run({ id: `${s.id}-${label}`, title: `${s.title ?? "Menu"}: ${label}` }, async () => {
        await openMenu(label);
        const items = await openMenuItems(label);
        await sleep(200);
        const file = shot(`${s.id}-${label}`);
        const how = await closeMenu(label);
        return { evidence: `${items.length} items: ${items.join(" | ")} (${how})`, file };
      });
    }
    return `opened ${labels.length} menus: ${labels.join(", ")}`;
  },
};

try {
  browser = await remote({
    hostname: "127.0.0.1",
    port: PORT,
    logLevel: "error",
    capabilities: { "tauri:options": { application: APP } },
  });
  for (const st of scenario.steps) {
    const h = handlers[st.do];
    if (!h) {
      await run({ ...st, shot: false }, () => {
        throw new Error(`unknown step type "${st.do}"`);
      });
      continue;
    }
    // each-menu adds one row per menu; its summary row needs no extra screenshot.
    await run(st.do === "each-menu" ? { ...st, shot: false } : st, () => h(st));
  }
} catch (e) {
  await run({ id: "SESSION", title: "WebDriver session" }, () => {
    throw e;
  });
} finally {
  try {
    await browser?.deleteSession();
  } catch {}
  driver.kill();
}

const failed = results.filter((r) => r.result !== "PASS").length;
const md = [
  `# UI evidence: ${scenario.name}`,
  "",
  scenario.description ?? "",
  "",
  `Commit \`${env("GITHUB_SHA", "local")}\`; ${results.length} rows, ${failed} failed.`,
  "Input: real X input (XTest via xdotool). Screenshots: whole X screen (ImageMagick import). WebDriver only reads the DOM.",
  "",
  "| Step | What | Result | Evidence | Screenshot |",
  "|---|---|---|---|---|",
  ...results.map((r) => `| ${r.id} | ${r.title} | ${r.result} | ${r.evidence.replace(/\|/g, "\\|")} | ${r.file ?? "—"} |`),
  "",
].join("\n");
fs.writeFileSync(path.join(OUT, "UI_EVIDENCE.md"), md);
console.log(`\n${results.length} rows, ${failed} failed; results in ${OUT}`);
// A scenario that recorded no rows proved nothing: that fails too.
process.exit(failed === 0 && results.length > 0 ? 0 : 1);
