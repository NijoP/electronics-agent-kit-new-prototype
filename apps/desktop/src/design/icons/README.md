# EAK icon set

Original stroke icons drawn for EAK (Phase 1 slice A2) in `icons.tsx`: 20 px grid, 1.5 px round
stroke, `currentColor`, shown at 16 or 20 px. Colour comes from the surrounding control, so the
icons follow `design/tokens.css` in both themes and carry no colour of their own.

**Icon audit.** Every icon below was authored for EAK from basic geometric primitives (lines,
arcs, rectangles, circles) in this repository. None is traced from, copied from or derived from
another product's icons, artwork, logos or fonts (EasyEDA included; see the Phase 1 study, section
4.2). `icons.test.ts` fails when an icon in `icons.tsx` is missing from this table, or the table
names an icon that does not exist.

The older activity-bar icons in `shell/icons.tsx` (24 px grid) are also EAK-authored and stay
there until the activity bar is redrawn.

| Icon | Drawing | Source |
| --- | --- | --- |
| `new` | page with folded corner and a plus | EAK-authored for slice A2, 2026-11-23 |
| `open` | open folder | EAK-authored for slice A2, 2026-11-23 |
| `save` | disk outline with shutter and label | EAK-authored for slice A2, 2026-11-23 |
| `save-as` | disk outline with a pencil | EAK-authored for slice A2, 2026-11-23 |
| `undo` | hooked arrow pointing left | EAK-authored for slice A2, 2026-11-23 |
| `redo` | hooked arrow pointing right | EAK-authored for slice A2, 2026-11-23 |
| `close` | diagonal cross | EAK-authored for slice A2, 2026-11-23 |
| `cut` | two rings with crossed blades | EAK-authored for slice A2, 2026-11-23 |
| `copy` | two offset rounded squares | EAK-authored for slice A2, 2026-11-23 |
| `paste` | board with clip and text lines | EAK-authored for slice A2, 2026-11-23 |
| `select` | pointer arrow | EAK-authored for slice A2, 2026-11-23 |
| `move` | four-way arrow cross | EAK-authored for slice A2, 2026-11-23 |
| `rotate` | open circle with corner arrow | EAK-authored for slice A2, 2026-11-23 |
| `mirror` | two triangles about a dashed axis | EAK-authored for slice A2, 2026-11-23 |
| `flip-side` | two layer lines with opposite arrows | EAK-authored for slice A2, 2026-11-23 |
| `delete` | bin with lid and two slots | EAK-authored for slice A2, 2026-11-23 |
| `align-left` | two bars against a left edge | EAK-authored for slice A2, 2026-11-23 |
| `align-right` | two bars against a right edge | EAK-authored for slice A2, 2026-11-23 |
| `align-top` | two bars hanging from a top edge | EAK-authored for slice A2, 2026-11-23 |
| `align-bottom` | two bars standing on a bottom edge | EAK-authored for slice A2, 2026-11-23 |
| `distribute-h` | bar centred between two vertical edges | EAK-authored for slice A2, 2026-11-23 |
| `distribute-v` | bar centred between two horizontal edges | EAK-authored for slice A2, 2026-11-23 |
| `part` | chip body with four leads and a pin-1 dot | EAK-authored for slice A2, 2026-11-23 |
| `wire` | right-angle wire between two end rings | EAK-authored for slice A2, 2026-11-23 |
| `bus` | two parallel rails with taps | EAK-authored for slice A2, 2026-11-23 |
| `net-label` | tag with a text stroke | EAK-authored for slice A2, 2026-11-23 |
| `net-port` | lead into a pointed port | EAK-authored for slice A2, 2026-11-23 |
| `off-page` | pointed tag with an outgoing arrow | EAK-authored for slice A2, 2026-11-23 |
| `power` | stem with a bar and a plus | EAK-authored for slice A2, 2026-11-23 |
| `ground` | stem with three shrinking bars | EAK-authored for slice A2, 2026-11-23 |
| `no-connect` | pin line ending in a cross | EAK-authored for slice A2, 2026-11-23 |
| `pin-label` | pin line with a ring and text lines | EAK-authored for slice A2, 2026-11-23 |
| `junction` | tee of two wires with a filled dot | EAK-authored for slice A2, 2026-11-23 |
| `text` | serif T | EAK-authored for slice A2, 2026-11-23 |
| `line` | single diagonal stroke | EAK-authored for slice A2, 2026-11-23 |
| `arc` | quarter arc with end dots | EAK-authored for slice A2, 2026-11-23 |
| `circle` | circle | EAK-authored for slice A2, 2026-11-23 |
| `rect` | rounded rectangle | EAK-authored for slice A2, 2026-11-23 |
| `annotate` | tag with a numeral and a plus | EAK-authored for slice A2, 2026-11-23 |
| `via` | two concentric circles | EAK-authored for slice A2, 2026-11-23 |
| `pad` | rounded square with a hole | EAK-authored for slice A2, 2026-11-23 |
| `outline` | polygon with vertex dots | EAK-authored for slice A2, 2026-11-23 |
| `zone` | rectangle with diagonal hatching | EAK-authored for slice A2, 2026-11-23 |
| `keepout` | square struck through | EAK-authored for slice A2, 2026-11-23 |
| `route` | pad-to-pad track with a 45 degree bend | EAK-authored for slice A2, 2026-11-23 |
| `autoroute` | track with a four-point spark | EAK-authored for slice A2, 2026-11-23 |
| `clear-routing` | cut track with a cross | EAK-authored for slice A2, 2026-11-23 |
| `refill` | hatched corner with a circular arrow | EAK-authored for slice A2, 2026-11-23 |
| `autoplace` | three blocks on a grid with a spark | EAK-authored for slice A2, 2026-11-23 |
| `dimension` | dimension line between two extension lines | EAK-authored for slice A2, 2026-11-23 |
| `layers` | three stacked sheets | EAK-authored for slice A2, 2026-11-23 |
| `3d` | isometric cube | EAK-authored for slice A2, 2026-11-23 |
| `zoom-in` | lens with a plus | EAK-authored for slice A2, 2026-11-23 |
| `zoom-out` | lens with a minus | EAK-authored for slice A2, 2026-11-23 |
| `fit` | four corner brackets around a square | EAK-authored for slice A2, 2026-11-23 |
| `grid` | two-by-two line grid | EAK-authored for slice A2, 2026-11-23 |
| `snap` | horseshoe magnet | EAK-authored for slice A2, 2026-11-23 |
| `unit` | ruler with ticks | EAK-authored for slice A2, 2026-11-23 |
| `crosshair` | circle with four ticks | EAK-authored for slice A2, 2026-11-23 |
| `eye` | eye outline with pupil | EAK-authored for slice A2, 2026-11-23 |
| `eye-off` | eye outline struck through | EAK-authored for slice A2, 2026-11-23 |
| `lock` | closed padlock | EAK-authored for slice A2, 2026-11-23 |
| `unlock` | open padlock | EAK-authored for slice A2, 2026-11-23 |
| `labels` | letter A beside text lines | EAK-authored for slice A2, 2026-11-23 |
| `search` | lens | EAK-authored for slice A2, 2026-11-23 |
| `sync` | two boxes joined by a bent arrow | EAK-authored for slice A2, 2026-11-23 |
| `rules` | sheet with rule lines | EAK-authored for slice A2, 2026-11-23 |
| `erc` | waveform with a check mark | EAK-authored for slice A2, 2026-11-23 |
| `drc` | board square with a trace and a check mark | EAK-authored for slice A2, 2026-11-23 |
| `dfm` | factory roof with a check mark | EAK-authored for slice A2, 2026-11-23 |
| `simulate` | scope screen with a waveform | EAK-authored for slice A2, 2026-11-23 |
| `bom` | table with header row and column | EAK-authored for slice A2, 2026-11-23 |
| `export` | arrow rising out of a tray | EAK-authored for slice A2, 2026-11-23 |
| `release` | flag on a pole | EAK-authored for slice A2, 2026-11-23 |
| `palette` | window with a prompt chevron | EAK-authored for slice A2, 2026-11-23 |
| `settings` | hub with eight spokes | EAK-authored for slice A2, 2026-11-23 |
| `help` | circle with a question mark | EAK-authored for slice A2, 2026-11-23 |
| `keyboard` | keyboard with keys and a space bar | EAK-authored for slice A2, 2026-11-23 |
| `info` | circle with an i | EAK-authored for slice A2, 2026-11-23 |
| `warning` | triangle with an exclamation mark | EAK-authored for slice A2, 2026-11-23 |
| `error` | circle with a cross | EAK-authored for slice A2, 2026-11-23 |
| `check` | check mark | EAK-authored for slice A2, 2026-11-23 |
| `chevron-down` | downward chevron | EAK-authored for slice A2, 2026-11-23 |
