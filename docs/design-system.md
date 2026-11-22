# EAK design system

EAK is an electronics engineering workstation: precise, calm, technical. Every screen draws from
one scale (`apps/desktop/src/design/tokens.css`) and one set of primitives
(`apps/desktop/src/design/ui.tsx` plus the shell components listed below). A new size, gap or
colour is a change to the scale, not a literal in a component: `src/design/tokens.test.ts` fails
when a stylesheet contains a raw colour, reads an undefined token, or uses a `var()` fallback
(written during the testing freeze; not yet run).

Status: code-reviewed only. Nothing in this document has been checked visually in a running app.

## Principles

- **Engineering first.** The canvas, the terminal output and engineering numbers get the space and
  the contrast; chrome is quiet (panel surfaces, 1 px lines, no gradients, one small shadow for
  floating layers only).
- **One accent.** EAK amber marks the selected thing, the focused control and the primary action.
  Red, amber, green and blue are *status* colours only: never decoration.
- **Real state only.** A dot, pill or count shows what the engine, validation or terminal runtime
  reported. Unknown is shown as unknown ("not run"), never as passing.
- **Dense where engineers work, spacious where they navigate.** Editors, panels and terminal
  rows use the small control height; Project Home and the overview use the 8 px grid generously.
- **Keyboard everywhere.** Every focusable control shows the same focus ring; shortcuts are named
  in tooltips (`design/shortcuts.ts`).

## Tokens (`tokens.css`)

| Group | Names | Notes |
| --- | --- | --- |
| Spacing | `--s-quarter` 2, `--s-half` 4, `--s-1` 8, `--s-1h` 12, `--s-2` 16, `--s-3` 24, `--s-4` 32 | 8 px grid; quarter/half steps for dense rows |
| Corners | `--r-1` 4 (controls, rows), `--r-2` 8 (popovers, dialogs, cards), `--r-pill` | |
| Type | `--fs-2xs` 10, `--fs-xs` 11, `--fs-sm` 12, `--fs-md` 13 (body), `--fs-lg` 15, `--fs-xl` 18, `--fs-2xl` 22; `--lh`, `--lh-tight`; `--fw-medium`, `--fw-strong`; `--tracking-caps` | `--font` system UI, `--mono` for paths, numbers, nets, terminal |
| Control heights | `--h-row` 22, `--h-sm` 24 (toolbars, rows), `--h-md` 28 (buttons, fields), `--h-lg` 32 (tabs, main action), `--h-bar` 34 (app bar) | |
| Surfaces | `--bg-canvas` < `--bg-app` < `--bg-panel` < `--bg-raised`; `--bg-hover`, `--bg-active` | canvas is the drawing/terminal area; raised is fields and popovers |
| Lines | `--line` (quiet), `--line-strong` (controls, popovers) | `--border` is the 1 px width |
| Text | `--fg`, `--fg-dim`, `--fg-faint` | |
| Accent | `--accent`, `--accent-fg`, `--accent-soft`, `--on-accent-edge` | |
| Focus | `--focus-ring` | accent hairline + soft halo (box-shadow) |
| Status | `--err`, `--warn`, `--ok`, `--info`, `--neutral` and `*-soft` fills | |
| Runtime states | `--st-running`, `--st-idle`, `--st-waiting`, `--st-failed`, `--st-ended` | terminal sessions, tasks |
| Version control | `--vcs-modified`, `--vcs-added`, `--vcs-deleted` | file explorer |
| Layers | `--z-raised`, `--z-fab`, `--z-menubar`, `--z-popover`, `--z-modal` | |
| Terminal | `--term-bg` | darker than the canvas in both themes |
| Schematic | `--c-sheet`, `--c-sheet-line`, `--c-wire`, `--c-label`, `--c-power`, `--c-text`, `--c-sym-*`, `--c-grid-minor`, `--c-grid-major` | |
| PCB | `--c-board`, `--c-copper-top`, `--c-copper-bottom`, `--c-pad`, `--c-pad-bottom`, `--c-courtyard`, `--c-via`, `--c-airwire`, `--c-preview`, `--c-route-blocked`, `--c-violation` | inner-layer copper defaults live in `src/layers.ts` (the colour inputs and the 3D viewer need concrete hex values; users can override them per project view) |
| Plots | `--c-plot-1` … `--c-plot-6` | simulation traces, in order |

Light theme (`:root[data-theme="light"]`, toggled by `design/theme.ts`) overrides the same names;
it never introduces new ones. Reduced motion zeroes the `--t-*` durations.

## Primitives

| Need | Use |
| --- | --- |
| A status mark (session, task, project row) | `StatusDot` (`design/ui.tsx`) → `.dot` with a runtime state or tone |
| A status word or count | `Pill` (`tone`: ok / warn / bad / info / neutral) |
| Nothing to show yet | `EmptyState` (title, why, the one action that fixes it) |
| A titled block in a panel or overview | `Section` |
| Name / value rows (properties, facts) | `<dl className="facts">` with `Fact` |
| Main toolbar | `shell/Toolbar.tsx` (data-driven from the action registry; icon buttons, split buttons for tool families, grid combos; disabled items carry their reason) |
| Icon | `Icon` (`design/icons/icons.tsx`): EAK-authored 20 px-grid stroke icons in `currentColor`, shown at 16 or 20 px; every icon is listed in `design/icons/README.md` (icon audit, checked by `icons.test.ts`). Never trace or copy another product's icons |
| Tabs | `.tabs` + `.tab` / `shell/DocTabs.tsx` for closable documents |
| Resizable divider | `shell/Splitter.tsx` (keyboard operable) |
| Dialog | `.modal-backdrop` + `.modal` (`ProjectDialogs`, `RulesDialog`, `ExportDialog`, `ModelSettings`) |
| Context menu | `.ctx-menu` (one popover style shared by file and terminal menus) |
| Menus | `shell/MenuBar.tsx` |
| Command palette | `CommandPalette.tsx` |
| Finding row | `.finding` in `EngineeringMonitor.tsx` (severity, location, jump, ask) |
| Approval / AI proposal | `.proposal` (diff, accept / reject — the user approves, the engine applies) |
| Confirmation (destructive terminal actions) | `.term-confirm` alert dialog |

Buttons: default (raised), `.primary` (the one main action of a screen), `.icon`, `.linkish`
(inline navigation). Disabled controls stay visible and explain why in their tooltip.

## Reference notes

Layout and design-system architecture were studied from the Craft Anything project (one scale in
one file, control sizes, surface hierarchy, a guard test that rejects literals, quiet sidebars,
status pills) and the general visual language of premium developer tools (restrained palette,
compact navigation, quiet borders, careful focus states). No code, assets or screens were copied;
EAK keeps its own amber identity and electronics-specific workflows.
