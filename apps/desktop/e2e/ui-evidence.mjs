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

/** Real X pointer click (XTest) at the centre of an element, optionally offset by dx/dy px. */
async function xclick(selector, text, dx = 0, dy = 0) {
  const r = await waitRect(selector, text);
  const o = origin();
  const x = Math.round(o.x + r.x + r.w / 2 + dx);
  const y = Math.round(o.y + r.y + r.h / 2 + dy);
  sh("xdotool", ["mousemove", "--sync", String(x), String(y), "click", "1"]);
  await sleep(250);
  return `XTest click at ${x},${y}`;
}

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
  const row = { id: st.id, title: st.title ?? st.do, result, evidence, file, ms: Date.now() - t0 };
  results.push(row);
  console.log(`${result.padEnd(4)} ${row.id} ${row.title} — ${evidence}`);
}

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
  // XTest click into a field, then XTest typing.
  async type(s) {
    const how = await xclick(s.selector, s.text);
    sh("xdotool", ["type", "--delay", "40", s.value]);
    if (s.expect) await waitRect(s.expect, s.expectText, s.timeout ?? 20000);
    return `${how}; XTest typed "${s.value}"${s.expect ? `; then visible: ${s.expect}` : ""}`;
  },
  async click(s) {
    const how = await xclick(s.selector, s.text, s.dx ?? 0, s.dy ?? 0);
    if (s.expect) await waitRect(s.expect, s.expectText, s.timeout ?? 20000);
    return `${how}${s.expect ? `; then visible: ${s.expect}` : ""}`;
  },
  async key(s) {
    xkey(s.keys);
    await sleep(300);
    return `XTest key ${s.keys}`;
  },
  async menu(s) {
    await openMenu(s.menu);
    await xclick(`ul[role="menu"][aria-label="${s.menu}"] [role="menuitem"]`, s.item);
    if (s.expect) await waitRect(s.expect, s.expectText, s.timeout ?? 20000);
    return `${s.menu} › ${s.item}${s.expect ? `; then visible: ${s.expect}` : ""}`;
  },
  async shot() {
    return "screenshot";
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
process.exit(failed === 0 ? 0 : 1);
