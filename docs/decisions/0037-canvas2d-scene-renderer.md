# ADR-0037: Canvas2D scene renderer for the PCB editor

Status: accepted (2026-11-23), Phase 1 slice A4; architecture decision AD-4 of the Phase 1 plan.

## Context
The PCB view drew every footprint, pad, track, via and zone as SVG nodes. Large boards in SVG are
the main frontend performance risk (archived `EDA_DESKTOP_TARGET_ARCHITECTURE.md` §5: target
2,000 footprints and 20,000 segments). Hit-testing relied on the DOM (`closest("[data-…-id]")`),
so it could not be shared with a non-DOM backend, a future object tree or a property panel.
The reference organisation (`github.com/easyeda`) ships a Canvas2D/WebGPU editor; its open
`easyeda-viewer` separates parsing, a format-neutral model, a render output contract and the UI.

## Options considered
1. Keep SVG and optimise it (fewer nodes, CSS containment). This does not remove the per-node
   cost or the DOM-bound picking.
2. Adopt LeaferJS (the viewer's scene library). The viewer itself switches off Leafer's
   hit-testing and does its own picking, so Leafer adds a dependency without adding editing value.
3. **Our own Canvas2D renderer behind a `SceneRenderer` interface**, built to the viewer's render
   contract, re-implemented (no code taken).
4. WebGL/WebGPU now. This is premature: Canvas2D first; the interface leaves room for another backend.

## Decision
Option 3, in `apps/desktop/src/render/`:
* **Render output contract** (`types.ts`): a flat render-object table. Each object has a `key`,
  the IR `ref` it stands for, `layerKey`, `bbox`, a precise `hit(x, y, tol)`, `pathPts`
  (path-hugging outline), `selPoly` (selection box), `pickLayers` (through-hole objects list every
  layer they span), its owning footprint and draw ops. A layer stack orders and dims layers.
* **PCB draw order** (`pcbScene.ts`), back to front:
  - bottom side (zone, copper, net names, pad numbers, silk);
  - inner layers, deepest first;
  - top side;
  - multi-layer (through-hole pads, vias);
  - outline, ratsnest, drill holes.

  Layers on the far side of the active layer are drawn at 55 % opacity.
* **Constant-pixel annotations:** pad numbers and net names are drawn at a fixed pixel size,
  only where they fit the pad or the track run.
* **Canvas2D backend** (`canvas2d.ts`):
  - one offscreen surface per layer, composited with the layer's opacity;
  - a pan by whole device pixels shifts the cached pixels and redraws only the exposed strips;
  - an edit diffs each layer by object signature and redraws only the old and new boxes of the
    changed objects;
  - a zoom or a resize redraws fully;
  - surfaces are capped by a memory budget (96 MiB), and layers over it draw directly;
  - segments of one style share one path;
  - selection, highlights and drag previews are an uncached overlay drawn from the table's
    outlines.
* **Picking** (`pick.ts`): grid index on `bbox`, then `hit`. It returns what the SVG view's
  topmost element would have been (vias > footprints and their pads > tracks > zones).
  Active-layer-exclusive picking (a miss is a miss) is available through `PickOptions.activeLayer`.
  It is not yet used by the editor, to keep parity with the SVG behaviour.
* **Integration:** `Canvas.tsx` keeps pan, zoom and the pointer contract (`onPoint`, `onHover`,
  `onDrag`). With a `scene` it stacks the underlay SVG (board fill, grid), the scene canvas and a
  transparent interactive SVG that holds the live tool previews and the rulers. `PcbView.tsx`
  asks the pick table instead of the DOM in canvas mode.
* **Flag:** Canvas2D is the default wherever a 2D context exists. The SVG markup stays as the
  fallback behind the PCB renderer selector (localStorage `eak.pcb.renderer`). Environments with
  no 2D context (jsdom, a failed context) use SVG, and the selector shows Canvas2D as unavailable.
  SVG remains the export backend.

## Consequences
* Existing PCB editor tests run unchanged on the SVG path; `render/parity.test.tsx` drives the
  same scenarios on the canvas path by coordinates. `render/render.test.ts` covers the contract,
  the stack, picking and the cache behaviour against a recording context.
* CI job `render-qa` (Chrome under Xvfb) runs pixel-diff snapshots of three reference boards,
  real-input picking, editor screenshots and the benchmark. Baselines are in
  `apps/desktop/e2e/render-baselines/`.
* The 3D viewer is unchanged. The schematic stays on SVG until the AD-5 benchmark gate decides.
* No shared IR, `Command` or report contract changed; geometry stays derived from EAK-IR per
  revision (AD-6).

## Source requirement
Phase 1 plan (EDA study) §6.2 `easyeda-viewer`, §6.4 AD-4/AD-5, §7.2 slice A4; product spec
desktop performance requirements via the archived EDA desktop target architecture §5.
