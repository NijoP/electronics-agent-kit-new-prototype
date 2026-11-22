// Render QA for the Canvas2D PCB renderer (slice A4), run in CI under Xvfb.
//
// Starts the Vite dev server for apps/desktop, opens `render-qa.html` in a real Chrome window and
// drives it over the Chrome DevTools Protocol (Node's built-in WebSocket; no extra packages):
//   1. pixel-diff snapshots of the three reference boards against `render-baselines/*.png`;
//   2. real-input picking: a mouse click on a footprint pad in the PCB editor selects it;
//   3. screenshots of the PCB editor (Canvas2D and the SVG fallback, and the benchmark board);
//   4. the 2,000-footprint / 20,000-segment pan/zoom benchmark.
// Results go to $RENDER_QA_OUT (RENDER_QA.md, render-qa.json, snapshots/, diffs/, screens/).
// Exit code 1 when a snapshot differs or has no baseline, a picking check fails, or a benchmark
// correctness check fails (the fast pan/zoom paths must end on the same pixels as a fresh render).
// Frame rates are recorded, never a pass/fail gate: under 30 fps is reported as a shortfall.
//
// Env: RENDER_QA_OUT (default: new temp dir), CHROME (browser binary), RENDER_QA_UPDATE=1 (write
// the current snapshots as the new baselines; review them before committing).
import { execFileSync, spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const app = path.resolve(here, "..");
const OUT = process.env.RENDER_QA_OUT ?? fs.mkdtempSync(path.join(os.tmpdir(), "eak-render-qa-"));
const BASELINES = path.join(here, "render-baselines");
const UPDATE = process.env.RENDER_QA_UPDATE === "1";
const PORT = Number(process.env.RENDER_QA_PORT ?? 1431);
const DEBUG_PORT = Number(process.env.RENDER_QA_DEBUG_PORT ?? 9333);
/** A snapshot passes when at most this share of its pixels changed. */
const MAX_RATIO = 0.002;
const TARGET_FPS = 30;
for (const d of ["snapshots", "diffs", "screens"]) fs.mkdirSync(path.join(OUT, d), { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (...a) => console.log("[render-qa]", ...a);

function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  for (const c of ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]) {
    try {
      return execFileSync("which", [c], { encoding: "utf8" }).trim();
    } catch {
      /* not on PATH */
    }
  }
  throw new Error("no Chrome/Chromium found (set CHROME)");
}

async function waitFor(what, fn, ms = 60000) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    try {
      const v = await fn();
      if (v) return v;
    } catch (e) {
      last = e;
    }
    await sleep(250);
  }
  throw new Error(`timed out waiting for ${what}${last ? `: ${last.message}` : ""}`);
}

class Cdp {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.id = 0;
    this.pending = new Map();
    this.ws.onmessage = (ev) => {
      const m = JSON.parse(typeof ev.data === "string" ? ev.data : ev.data.toString());
      const p = m.id !== undefined ? this.pending.get(m.id) : undefined;
      if (!p) return;
      this.pending.delete(m.id);
      if (m.error) p.reject(new Error(`${p.method}: ${m.error.message}`));
      else p.resolve(m.result);
    };
    this.open = new Promise((res, rej) => {
      this.ws.onopen = res;
      this.ws.onerror = () => rej(new Error("CDP websocket failed"));
    });
  }
  send(method, params = {}) {
    const id = ++this.id;
    this.ws.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => this.pending.set(id, { resolve, reject, method }));
  }
  async eval(expression) {
    const r = await this.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error(`page: ${r.exceptionDetails.exception?.description ?? r.exceptionDetails.text}`);
    return r.result.value;
  }
  close() {
    this.ws.close();
  }
}

const procs = [];
const results = { environment: {}, snapshots: [], picking: [], screens: [], bench: null, errors: [] };
let failed = false;

async function main() {
  // 1. Vite dev server (serves render-qa.html; it is not part of the packaged build).
  const vite = spawn(process.execPath, [path.join(app, "node_modules", "vite", "bin", "vite.js"), "--port", String(PORT), "--strictPort", "--host", "127.0.0.1"], {
    cwd: app,
    stdio: ["ignore", "pipe", "pipe"],
  });
  procs.push(vite);
  const viteLog = fs.createWriteStream(path.join(OUT, "vite.log"));
  vite.stdout.pipe(viteLog);
  vite.stderr.pipe(viteLog);
  const base = `http://127.0.0.1:${PORT}`;
  await waitFor("vite", async () => (await fetch(`${base}/render-qa.html`)).ok);

  // 2. Chrome: a real window on the Xvfb display (headless only when there is no display).
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), "eak-render-qa-chrome-"));
  const args = [
    `--remote-debugging-port=${DEBUG_PORT}`,
    `--user-data-dir=${profile}`,
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    "--window-size=1680,1120",
    "--force-device-scale-factor=1",
    "--disable-background-timer-throttling",
    "--disable-renderer-backgrounding",
    "--disable-backgrounding-occluded-windows",
  ];
  if (!process.env.DISPLAY) args.push("--headless=new");
  const chrome = spawn(findChrome(), [...args, "about:blank"], { stdio: ["ignore", "pipe", "pipe"] });
  procs.push(chrome);
  const chromeLog = fs.createWriteStream(path.join(OUT, "chrome.log"));
  chrome.stdout.pipe(chromeLog);
  chrome.stderr.pipe(chromeLog);
  const version = await waitFor("chrome devtools", async () => (await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/version`)).json());
  const target = await waitFor("a page target", async () => {
    const list = await (await fetch(`http://127.0.0.1:${DEBUG_PORT}/json/list`)).json();
    return list.find((t) => t.type === "page");
  });
  const cdp = new Cdp(target.webSocketDebuggerUrl);
  await cdp.open;
  await cdp.send("Page.enable");
  await cdp.send("Runtime.enable");
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1600, height: 1000, deviceScaleFactor: 1, mobile: false });
  results.environment = {
    browser: version.Browser,
    userAgent: version["User-Agent"],
    display: process.env.DISPLAY ? `Xvfb ${process.env.DISPLAY}` : "headless",
    cpus: os.cpus().length,
    cpuModel: os.cpus()[0]?.model ?? "unknown",
    memoryGb: Math.round((os.totalmem() / 2 ** 30) * 10) / 10,
    platform: `${os.type()} ${os.release()}`,
    runner: process.env.RUNNER_NAME ? `GitHub Actions ${process.env.ImageOS ?? ""} ${process.env.ImageVersion ?? ""}`.trim() : "local",
  };

  const open = async (query) => {
    await cdp.send("Page.navigate", { url: `${base}/render-qa.html${query}` });
    await sleep(300);
    await waitFor("QA page ready", () => cdp.eval("!!(window.__qa && window.__qa.ready)"));
    await sleep(400); // let the first canvas frames land
  };
  const shot = async (file) => {
    const { data } = await cdp.send("Page.captureScreenshot", { format: "png" });
    fs.writeFileSync(path.join(OUT, "screens", file), Buffer.from(data, "base64"));
    results.screens.push(file);
  };
  const click = async (x, y) => {
    await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1 });
    await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 1 });
    await sleep(300);
  };

  // 3. Pixel-diff snapshots of the reference boards.
  await open("");
  const boards = await cdp.eval("window.__qa.boards");
  for (const name of boards) {
    const url = await cdp.eval(`window.__qa.snapshot(${JSON.stringify(name)})`);
    const png = Buffer.from(url.split(",")[1], "base64");
    fs.writeFileSync(path.join(OUT, "snapshots", `${name}.png`), png);
    const basePath = path.join(BASELINES, `${name}.png`);
    if (UPDATE) {
      fs.mkdirSync(BASELINES, { recursive: true });
      fs.writeFileSync(basePath, png);
    }
    if (!fs.existsSync(basePath)) {
      results.snapshots.push({ name, status: "NO BASELINE", detail: `missing ${path.relative(app, basePath)}` });
      failed = true;
      continue;
    }
    const b64 = fs.readFileSync(basePath).toString("base64");
    const d = await cdp.eval(`window.__qa.diff(${JSON.stringify(name)}, "data:image/png;base64,${b64}")`);
    if (d.diffPng) fs.writeFileSync(path.join(OUT, "diffs", `${name}.png`), Buffer.from(d.diffPng.split(",")[1], "base64"));
    const pass = d.sizeMatch && d.ratio <= MAX_RATIO;
    if (!pass) failed = true;
    results.snapshots.push({
      name,
      status: pass ? "PASS" : "FAIL",
      detail: d.sizeMatch ? `${d.mismatched} of ${d.width * d.height} px changed (${(d.ratio * 100).toFixed(3)} %, limit ${MAX_RATIO * 100} %)` : "size differs from baseline",
    });
  }

  // 4. The PCB editor itself: renderer in use, real-input picking, screenshots.
  for (const [board, renderer] of [
    ["ref-four-layer", "canvas"],
    ["ref-four-layer", "svg"],
    ["ref-dense", "canvas"],
    ["benchmark", "canvas"],
  ]) {
    await open(`?view=pcb&board=${board}&renderer=${renderer}`);
    const used = await cdp.eval("window.__qa.renderer()");
    const check = { board, renderer, used, status: used === renderer ? "PASS" : "FAIL", detail: `renderer in use: ${used}` };
    if (board === "ref-four-layer") {
      // R2 (fp_1, at 34, 12 mm) has pad 1 at (33.2, 12): a real mouse click there must select R2.
      const at = await cdp.eval("window.__qa.clientOf(33.2, 12)");
      if (at) await click(at[0], at[1]);
      const sel = await cdp.eval("window.__qa.selectionText()");
      const picked = typeof sel === "string" && sel.includes("R2");
      check.status = check.status === "PASS" && picked ? "PASS" : "FAIL";
      check.detail += `; click on R2 pad 1 → selection bar: ${sel === null ? "none" : JSON.stringify(sel.slice(0, 40))}`;
    }
    if (check.status !== "PASS") failed = true;
    results.picking.push(check);
    await shot(`pcb-${board}-${renderer}.png`);
  }

  // 5. Benchmark (recorded; not a gate).
  await open("");
  log("benchmark running (about 20 s)");
  results.bench = await cdp.eval("window.__qa.bench()");
  for (const c of results.bench?.checks ?? []) {
    c.status = c.ratio <= MAX_RATIO ? "PASS" : "FAIL";
    if (c.status !== "PASS") failed = true;
  }
  cdp.close();
}

function report() {
  const env = results.environment;
  const lines = [
    "# Render QA (Canvas2D PCB renderer, slice A4)",
    "",
    `Browser: ${env.browser ?? "?"} on ${env.display ?? "?"} · ${env.cpus ?? "?"} CPUs (${env.cpuModel ?? "?"}) · ${env.memoryGb ?? "?"} GB · ${env.runner ?? ""}`,
    "",
    "## Pixel-diff snapshots",
    "",
    "| Board | Result | Detail |",
    "|---|---|---|",
    ...results.snapshots.map((s) => `| ${s.name} | ${s.status} | ${s.detail} |`),
    "",
    "## PCB editor in a real browser window",
    "",
    "| Board | Renderer asked | Result | Detail |",
    "|---|---|---|---|",
    ...results.picking.map((p) => `| ${p.board} | ${p.renderer} | ${p.status} | ${p.detail} |`),
    "",
  ];
  const b = results.bench;
  if (b) {
    lines.push(
      `## Benchmark: ${b.board.footprints} footprints, ${b.board.segments} segments (${b.board.renderObjects} render objects), ${b.viewport}`,
      "",
      `Scene build ${b.ms.buildScene.toFixed(0)} ms · setScene ${b.ms.setScene.toFixed(0)} ms · first full render ${b.ms.firstFullRender.toFixed(0)} ms (JS time) · pick ${b.pick.meanUs.toFixed(1)} µs/call · one-track edit: rebuild ${b.edit.rebuildSceneMs.toFixed(0)} ms, redraw ${b.edit.setSceneAndRenderMs.toFixed(0)} ms (${b.edit.stats.dirtyRects} dirty rects, ${b.edit.stats.fullLayers} full layers)`,
      "",
      "| Scenario | fps | p95 frame ms | mean render ms | max render ms | full layer redraws/frame | scaled-cache frames | ≥ 30 fps |",
      "|---|---:|---:|---:|---:|---:|---:|---|",
      ...b.runs.map(
        (r) =>
          `| ${r.scenario} | ${r.fps.toFixed(1)} | ${r.p95FrameMs.toFixed(1)} | ${r.meanRenderMs.toFixed(1)} | ${r.maxRenderMs.toFixed(1)} | ${r.fullLayersPerFrame.toFixed(1)} | ${((r.staleShare ?? 0) * 100).toFixed(0)} % | ${r.fps >= TARGET_FPS ? "yes" : `no (shortfall ${(TARGET_FPS - r.fps).toFixed(1)} fps)`} |`,
      ),
      "",
      "| Benchmark check | Result | Pixels changed vs a fresh render | Detail |",
      "|---|---|---:|---|",
      ...(b.checks ?? []).map((c) => `| ${c.name} | ${c.status} | ${(c.ratio * 100).toFixed(3)} % (limit ${MAX_RATIO * 100} %) | ${c.detail} |`),
      "",
    );
  } else lines.push("## Benchmark", "", "Not run (an earlier step failed).", "");
  if (results.errors.length) lines.push("## Errors", "", ...results.errors.map((e) => `- ${e}`), "");
  const md = lines.join("\n");
  fs.writeFileSync(path.join(OUT, "RENDER_QA.md"), md);
  fs.writeFileSync(path.join(OUT, "render-qa.json"), JSON.stringify(results, null, 2));
  if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${md}\n`);
  console.log(md);
}

try {
  await main();
} catch (e) {
  failed = true;
  results.errors.push(String(e?.stack ?? e));
  console.error(e);
} finally {
  report();
  for (const p of procs) p.kill("SIGTERM");
}
process.exit(failed ? 1 : 0);
