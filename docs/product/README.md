# EAK product demo

`eak-product-demo.html` is a clickable storyboard of how EAK should feel to an electronics
engineer who has never used a terminal or an AI agent. It is a product demonstration, not
the application. Open it in any browser: it is one self-contained file with no network
access, no build step and no dependencies.

## What it shows

It follows one sample project, an ESP32-S3 environmental sensor board: 12 V in through a
TPS62933 buck, a USB-C port, and SHT40 and BME280 sensors on I2C. The project uses real part
numbers. The journey covers:

- Home: describe the board, start a project, continue one.
- The workspace: a journey bar with 12 stages, file tree, schematic, PCB and 3D.
- The library, the BOM and a Problems list.
- An AI co-pilot that only proposes changes; you review them like the Update PCB preview.
- Settings › AI tools: connect Claude Code, Codex and others without a terminal.
- The Terminal workspace as it is today (kept unchanged).
- Release check.

The storyboard panel on the right walks through 15 frames. Each frame says what the
engineer sees, thinks and feels, how EAK guides them, and which feature backs it.

## Badges

- **EXISTS**: in EAK on `main` today. Hover the badge for the file, ADR or ui-evidence
  scenario behind it.
- **PROPOSED**: not built. Proposed areas also have a dashed outline.

## What is real and what is simulated

- All checks, stock figures and AI answers are simulated in the page.
- Stock and price are stored sample records. EAK's live distributor search is not configured
  on `main`.
- The page never claims a backend capability EAK lacks. Where EAK has a limit today (for
  example the autorouter, 3D viewer and fabricator profile), the demo states it.
- Running a check or generating outputs does not change the design revision. Every design
  change is one revision and one undo step, as with `Engine::execute`.

## Background

The full review is in the Firstmate task report `eak-traycer-review/report.md`. It explains
which ideas EAK can adapt from Traycer, and the proposed architecture with its phased slices.
Nothing here changes the app: no code under `apps/` or `crates/` was touched.
