# EAK product vision

`eak-product-vision.html` is the long-term product vision for EAK (Electronics Agent Kit), by TUSK Innovations Global: an electronics engineering
workspace where AI agents draft the design with you, and EAK checks every change before you
accept it. Open it in any browser. It is one self-contained file: no network, no fonts or
libraries fetched, no build step. Three.js (MIT, licence text alongside it) is inlined for the 3D board.

It is an interactive simulation. Integrations, AI agents, LCSC data and prices are simulated
with sample data and labelled as such. Nothing in it is a live service. It does not change
the EAK application.

## 1. Design system

### Identity
- **Idea:** an engineering instrument, not a website: calm, precise and bench-grade. Near-black and white carry the identity, as in the TUSK (Tusk Innovations Global) mark.
- **Wordmark and credit:** EAK stands alone as a wordmark, with its full name, Electronics Agent Kit, where it is introduced. TUSK is credited as "by TUSK Innovations Global" with the exact TUSK mark, embedded from the supplied artwork (never redrawn). Orange is a restrained accent: focus, the primary action and current progress. It never fills large areas.
- **Materials stay physical:** board copper, ENIG gold and soldermask keep their real colours. Brand orange is never used for copper.

### CMF (colour, material, finish)
| Role | Material | Value |
|---|---|---|
| Housing, deepest | Anodised graphite | `#0C0D0F` |
| App surface | Graphite | `#121316` |
| Panels | Graphite, lifted | `#17191C` |
| Raised, hover | Satin graphite | `#1E2024`, `#25282D` |
| Text | White, grey, ash | `#F2F2F0`, `#A9A9A6`, `#6E6E6B` |
| Brand accent / focus | TUSK orange | `#EE5F1B` (soft `rgba(238,95,27,.13)`) |
| Board copper | Copper (physical) | `#C98543` tracks, `#D4B36A` ENIG pads |
| AI agent activity | Signal cyan | `#86C5E0` |
| Pass, warning, fail, unknown | Phosphor, amber, coral, slate | `#7FCB9B`, `#E2B35C`, `#EC7C6B`, `#8A929E` |
| Board | Matte black soldermask, ENIG gold pads, white silk | `#111614`, `#D4B36A`, `#E9E6DE` |

- **Finish:** matte. A faint film grain (an SVG noise layer at 3% opacity) gives surfaces a material feel.
- **Lighting:** a single soft key light from top left, made from layered solid tones and shadow falloff. No gradients for decoration.
- **Status colour:** used only for status. Orange appears only where the user's focus is.

### Type
- **Faces:** the system UI stack (`Inter`, SF Pro, Segoe UI, Roboto) for interface text; the system monospace stack for designators, nets and values. Tabular numbers everywhere.
- **Scale:** 11 / 12 / 13 (body) / 15 / 20 / 28 / 44 / 64 (film titles), at a ratio of about 1.33.
- **Weights:** 400 and 560 for the interface; 300 for film titles. Uppercase only for 11 px labels, with 0.08 em tracking.
- **Hierarchy:** carried by type, space and tone. There are no borders around groups.

### Space
- **Grid:** 4 px base, with steps of 4 / 8 / 12 / 16 / 24 / 32 / 48 / 64.
- **Layout:** the rail is 56 px; panels are 264 px (explorer) and 320 px (inspector, agent); the canvas is fluid.
- **Viewports:** designed for 1440×900 and verified from 1280 to 1920.

### Elevation and depth
| Level | Treatment | Used for |
|---|---|---|
| 0 | Canvas | Schematic, board, 3D |
| 1 | Surface | Rail, panels |
| 2 | Raised | Inputs, selected rows |
| 3 | Overlay: tone `#1E2024` + shadow `0 24px 64px rgba(0,0,0,.55)` | Command palette, review sheet, dialogs |

Separation between levels comes from tone steps. A hairline (`rgba(255,255,255,.05)`) is used only where two equal tones meet.

### Motion
| Kind | Curve | Duration |
|---|---|---|
| Interface | `cubic-bezier(.2,.7,.1,1)` | 140 ms for state, 240 ms for panels |
| Camera (film and canvas moves; the film runs on a scrubbable timeline) | `cubic-bezier(.65,0,.15,1)` | 900–1400 ms |
| Trace drawing | linear stroke-dash | – |

- **One thing moves at a time**, and motion always means something: a change of state or a change of place.
- **`prefers-reduced-motion`:** every transition becomes an instant cut and the 3D view stops auto-rotating.

### Iconography
- **Style:** 20 px inline SVG with a 1.5 px stroke, round caps and joins, and geometric forms drawn from electronics: pin, trace, via, layer stack, probe.
- **Rules:** no fills and no colour, except the active icon, which turns orange. Model providers appear as text, never as logos.

## 2. Storyline: storyboard frame → product screen

The storyline is the existing 15-frame storyboard: the engineer's view, the 12-stage workflow and its loops, the product architecture, and the AI tool integration (Traycer review report §3, §8, §10 and §11). A launch film introduces it. Scenes: brand reveal, intent, agents and parts, the real schematic in macro, the routed PCB in macro, the physical board in 3D with the real four-layer explode (grab to rotate), BOM, Gerber and fab prep, and the final reveal. It autoplays on the same data as the product, with play, pause, replay, skip, a scrubbable timeline with scene markers, and a seamless hand-off into the workspace. The frames then play in order as a guided walkthrough through the product: the presenter advances with → / ← or **Next**, and every screen stays freely explorable.

1 First launch → Home · 2 Describe the design goal → Home, draft brief with question cards · 3 Continue next day → Home, Continue and Needs attention · 4 Workspace and journey → Schematic, journey in the top bar · 5 Choose components → Parts: symbol, footprint, datasheet values, LCSC (sample) · 6 Schematic capture → Schematic, Power sheet · 7 ERC with guided fixes → Checks · 8 Review a proposed change → Agents, the schematic agent's proposal, ERC re-run on a copy · 9 Update PCB → Board, update dialog · 10 Placement, routing, DRC → Agents, two orchestrated agent lanes (layout and power) with live tool calls on the canvas · 11 3D check → 3D, the same board with real packages and the four-layer explode · 12 Outputs and release check → Fabricate: BOM, Gerber, order estimate, release check · 13 Checkpoint, come back tomorrow → Project history and checkpoints · 14 Connect an AI tool, no terminal → Agents, model providers and model selection · 15 Terminal → Terminal, a detected agent session.

## 3. The product

Ten areas of one shell around one flagship board, **Aurora S3**: a four-layer ESP32-S3 environmental sensor node with a 12–24 V input, a TPS62933 buck, USB-C (TPS2051 switch, USBLC6 ESD), BME280 and SHT40. The board (70 × 45 × 1.6 mm, 27 parts, 57 tracks, 40 vias) is one model, `window.EAKBoard`. The Board, 3D and Fabricate areas are one PCB module built on it, and the film draws its PCB, 3D and fab scenes from the same model; the film's 3D is the same Three.js scene as the workspace.

1. **Home:** projects, new design, KiCad import, continue, and attention items (ERC errors, stock risk, proposals to review).
2. **Project:** file explorer, brief and rail currents, design history, checkpoints.
3. **Schematic:** three sheets with symbols, pins, wires, junctions, a bus, power flags and net labels; hover traces a net.
4. **Board:** layers and stack-up, routed tracks, vias, GND pours, antenna keep-out, silkscreen, nets, the 90 Ω USB pair (computed 90.4 Ω, lengths matched at 41.763 mm), editable design rules and a DRC that measures the board geometry (a demo checker, not eak-validation; unimplemented rules are listed as not implemented).
5. **Parts:** search, symbol and footprint previews, datasheet viewer, parameters, LCSC number, stock and price breaks (sample data).
6. **Agents:** model providers (Anthropic, OpenAI, Google, xAI) with their connection state, and model selection per provider and per agent role (Claude Opus 5.5, Fable 5.2 and Sonnet 5.5; GPT-6.2 Astra, GPT-6.2 Sol and GPT-6.1 Terra; Gemini 4 Carbon, Neon and Argon; Grok 4.7 and 4.6, as listed on each provider's model page in November 2026). Orchestrated tasks split across agents, the context each sees, live tool calls on a copy, proposals with a check delta, then apply or undo. Providers are simulated.
7. **Checks:** ERC and DRC findings that locate on the canvas; UNKNOWN is never a pass; editable rules.
8. **3D:** Three.js render of the same board: FR-4, matte black mask, ENIG pads, real packages (ESP32-S3 module, USB-C shell, barrel jack, inductor, switches, chip parts), camera presets, layer visibility and an explode that separates the full stack-up.
9. **Fabricate:** BOM with sample pricing, a Gerber and drill viewer generated from the board, an order estimate, and the release check (outputs per revision, engineer sign-off).
10. **Terminal:** sessions with agent detection, sharing the same tasks and checks.

Keyboard:
- **→ / ←** move through the walkthrough; **G** shows or hides it.
- **1–0** switch areas.
- **Ctrl+K / ⌘K** opens the command palette.
- **Ctrl+Z / Ctrl+Shift+Z** undo and redo.
- **Esc** closes overlays.
