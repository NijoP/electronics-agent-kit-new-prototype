# IPC / IEC rule values and their verification status

Every numeric value EAK takes from, or associates with, an IPC or IEC standard is listed below,
together with the numbers EAK uses as its own defaults in the same rules (so they are not
mistaken for standard values).

**Status vocabulary**
- **UNVERIFIED**: transcribed from a secondary source. Nobody has checked it against the
  purchased standard text. Results that depend on it say so in their DRC/ERC message.
- **EAK DEFAULT**: chosen by EAK or taken from common fabricator practice. It is not claimed to
  come from any standard.
- **EXACT**: a unit definition, correct by definition. It is not a standard-table value.

No IPC/IEC value in EAK is marked verified. The standard texts (IPC-2221B, IPC-7351,
IEC 60664-1) are not in this repository; they are copyrighted. To promote a value to VERIFIED, a
named engineer must check it against the purchased edition, and this table must record that
engineer, the edition, the clause/table and the date. `eak_engineering::ipc::VERIFIED_AGAINST_STANDARD`
stays `false` until every value in sections 1–3 has been verified that way.

## 1. Conductor current capacity (IPC-2221)

Implementation: `crates/eak-engineering/src/ipc.rs`. Rules: DRC-008 (tracks) and DRC-014 (thermal-relief spokes).
Secondary source: DESK `desk/ipc.py` @ 5ad8492.

| Value | EAK uses | Status |
|---|---|---|
| Formula I = k · ΔT^0.44 · A^0.725 (A in mil², ΔT in °C) | `trace_area_mil2` | UNVERIFIED |
| k, external conductors | 0.048 | UNVERIFIED |
| k, internal conductors (also used for via barrels) | 0.024 | UNVERIFIED |
| ΔT exponent | 0.44 | UNVERIFIED |
| Area exponent | 0.725 | UNVERIFIED |
| Copper thickness per oz/ft² | 1.37 mil (≈ 34.8 µm) | UNVERIFIED (physical conversion as transcribed; not an IPC table) |
| mil per mm | 39.37007874 | EXACT (25.4 mm per inch) |
| Default allowed temperature rise | 10 °C (`Constraints::temp_rise_c`) | EAK DEFAULT |
| IPC-2152 (newer charts) | not implemented | n/a |

The code reproduces these values as computed by EAK. This checks the code against the formula
above, not the formula against the standard:

| Case | EAK result |
|---|---|
| 1 A, external, 1 oz, ΔT 10 °C | ≈ 11.9 mil (0.30 mm) |
| 3 A, external, 1 oz, ΔT 10 °C (3 A buck output path) | ≈ 54.1 mil (≈ 1.37 mm) |
| 3 A, external, 2 oz, ΔT 10 °C | ≈ 27.1 mil (≈ 0.69 mm) |
| 10 A, external, 1 oz, ΔT 10 °C | ≈ 284.9 mil |
| 1 A, internal, 1 oz, ΔT 10 °C | ≈ 30.9 mil |

**Thermal-relief spokes (DRC-014).** The same formula is applied to the summed raster width of a
pad's connected spokes. It assumes the zone net's full declared current flows through that one
pad. That is an upper bound, so a shortfall is reported as a **warning**, never as a pass.

## 2. Vias (IPC-2221 conductor formula applied to the barrel)

| Value | EAK uses | Status |
|---|---|---|
| Barrel cross-section | π · (d + t) · t (mil²) | UNVERIFIED (geometric approximation) |
| k applied to the barrel | 0.024 (internal) | UNVERIFIED. DESK's header says "external", but its code uses internal; EAK keeps the more conservative internal value |
| Parallel-via current-sharing derating | 0.8 | EAK DEFAULT (heuristic) |
| Reference: 12 mil drill, 1 mil plating, ΔT 10 °C | ≈ 1.0–1.3 A per via; 3 A needs 4 vias | UNVERIFIED |

## 3. Electrical spacing

### IPC-2221B Table 6-1: minimum conductor spacing (mm) by peak voltage

`ipc.rs::clearance_mm`. DRC-002 applies it for nets with declared voltages, using condition B2
for outer layers.

| Peak V ≤ | B1 | B2 | B3 | B4 | A5 | A6 | A7 |
|---|---|---|---|---|---|---|---|
| 15 | 0.05 | 0.10 | 0.10 | 0.05 | 0.13 | 0.13 | 0.13 |
| 30 | 0.05 | 0.10 | 0.10 | 0.05 | 0.13 | 0.25 | 0.13 |
| 50 | 0.10 | 0.60 | 0.60 | 0.13 | 0.13 | 0.40 | 0.13 |
| 100 | 0.10 | 0.60 | 1.50 | 0.13 | 0.13 | 0.50 | 0.13 |
| 150 | 0.20 | 0.60 | 3.20 | 0.40 | 0.40 | 0.80 | 0.40 |
| 170 | 0.20 | 1.25 | 3.20 | 0.40 | 0.40 | 0.80 | 0.40 |
| 250 | 0.20 | 1.25 | 6.40 | 0.40 | 0.40 | 0.80 | 0.40 |
| 300 | 0.20 | 1.25 | 12.5 | 0.80 | 0.80 | 0.80 | 0.80 |
| 500 | 0.25 | 2.50 | 12.5 | 0.80 | 0.80 | 1.50 | 0.80 |
| > 500, per volt | 0.0025 | 0.0050 | 0.0250 | 0.00305 | 0.00305 | 0.00305 | 0.00305 |

Status: **UNVERIFIED**, every cell. Two items need a reviewer's attention:
- The A7 column is identical to A5 in the transcription. Confirm this against the standard.
- At 12–24 V (the buck input), B2 gives 0.10 mm. The project minimum clearance (0.15 mm) and
  net-class clearance usually dominate.

### IEC 60664-1 Table F.4: creepage (mm)

`ipc.rs::creepage_mm`, for pollution degree 1 and pollution degree 2 (material groups I, II and
III). Rows run from 10 V to 1000 V; the values are in `ipc.rs` and match DESK. Above 1000 V and
pollution degrees 3–4 are refused, not guessed.

Status: **UNVERIFIED and FLAGGED**. The PD2 columns jump from 0.04 mm (≤ 40 V) to 0.60, 0.85 and
1.20 mm at 50 V, which suggests that printed-wiring-material and other-material columns were
mixed. No DRC rule uses this table yet.

## 4. Land patterns (IPC-7351)

The built-in footprints (`crates/eak-core/src/builtin.rs`) are described as approximating
IPC-7351 nominal density. Pad sizes, courtyards and pitches are **UNVERIFIED** against IPC-7351
and against the manufacturers' recommended land patterns.

## 5. Values EAK defines itself (no standard source claimed)

| Rule | Value | Where | Status |
|---|---|---|---|
| Thermal-relief gap | = zone clearance | `copper::relief_rule` | EAK DEFAULT. No IPC thermal-relief value is transcribed or used. |
| Thermal-relief spoke width | zone net-class track width (≥ minimum track width). The raster spoke is never narrower and is less than 2 × 0.1 mm wider. | `copper::relief_rule` | EAK DEFAULT |
| Spokes per relief / minimum landed | 4 axis spokes / 2 | `copper::SPOKE_DIRS`, `drc::MIN_SPOKES` | EAK DEFAULT |
| Zone fill raster pitch | 0.1 mm, plus a 0.071 mm halo on every keep-out | `copper::ZONE_FILL_PITCH` | EAK DEFAULT |
| Silkscreen line width | 0.15 mm | `silk::LINE_WIDTH` | EAK DEFAULT (common fabricator minimum; check the fabricator profile) |
| Reference-designator cap height | 1.0 mm | `silk::TEXT_HEIGHT` | EAK DEFAULT |
| Silkscreen-to-soldermask-opening clearance | 0.1 mm | `silk::MASK_CLEARANCE`, DRC-015 | EAK DEFAULT |
| Soldermask expansion | 0.05 mm | `ManufacturingConfig::mask_expansion` | EAK DEFAULT |
| Minimum track / clearance / drill / annular ring / edge clearance | 0.15 / 0.15 / 0.30 / 0.13 / 0.30 mm | `Constraints::default` | EAK DEFAULT (generic low-cost 2-layer profile; not an IPC class value) |
| Capacitor voltage margin | < rail: error; < 1.5 × rail: warning | ERC-011 | EAK DEFAULT (heuristic from DESK) |

## 6. Stroke font provenance

The silkscreen stroke font (`crates/eak-engineering/src/silk.rs::glyph`) is an original
single-stroke glyph set, drawn on a 4 × 6 grid and written for EAK in this repository. No
third-party font data (Hershey, KiCad, GPL/AGPL or unlicensed sources) was copied or consulted
for coordinates. It is covered by the repository's own license (see `LICENSING.md`).

It covers A–Z (case-insensitive), 0–9 (with a slashed zero) and `- + _ . /` plus space. Any
other character renders as a hollow box, so a missing glyph is visible on the board rather than
silently dropped.
