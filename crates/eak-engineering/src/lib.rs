//! # eak-engineering — deterministic engineering core
//!
//! * `ipc`     – trace ampacity, clearance, creepage, via current (provenance: DESK audit).
//! * `calc`    – circuit calculations and engineering value parsing.
//! * `netlist` – schematic connectivity → canonical nets.
//! * `board`   – the one derived board geometry (outline, copper, fills, holes, silk) that DRC,
//!   connectivity, the 3D scene and the desktop views all read (ADR-0052).
//! * `copper`  – PCB copper shapes, zone fill with thermal reliefs, physical connectivity.
//! * `domain`  – domain classification (archetypes and overlays) from the structured requirement
//!   record, with the evidence for and against each class (ADR-0060).
//! * `stitch`  – zone stitching: escape vias for orphan SMD pads, stitching vias for pour islands (DESK).
//! * `silk`    – silkscreen legend: stroke-font reference designators, mask-opening clip.
//! * `router`  – EAK's native multilayer grid router (signal layers, through vias, passes, rip-up,
//!   congestion, pairs, per-layer widths, failure explanations; declared differential pairs
//!   routed coupled in `router::diffpair`).
//! * `routing_intel` – congestion, stagnation, differential pairs, width/via selection (DESK-derived).
//! * `interactive` – route-step API for the interactive PCB route tool (walkaround, 45/90, vias).
//! * `placer`  – constructive footprint placement.
//! * `placement_intel` – placement mental models (roles, snaps, analysis) ported from DESK.
//! * `impedance` – microstrip/stripline and edge-coupled differential Z0 from the stack-up (IPC-2141A).
//! * `power`   – unit-aware trace width, voltage drop, rail drop, return-path continuity.
//! * `pathres` – DC path resistance of a net from its routed copper (tracks, via barrels).
//! * `rating`  – part-rating names and derating facts (ERC-022..024, harness component checks).
//! * `scene3d` – board/component geometry for the 3D workspace and mechanical checks.
//! * `verify`  – verification engines: typed inputs to PASS / FAIL / UNKNOWN / BLOCKED with sources.

pub mod board;
pub mod calc;
pub mod clearance;
pub mod copper;
pub mod domain;
pub mod fill_view;
pub mod impedance;
pub mod interactive;
pub mod ipc;
pub mod netlist;
pub mod pathres;
pub mod placement_intel;
pub mod placer;
pub mod power;
pub mod priority;
pub mod rating;
pub mod router;
pub mod routing_intel;
pub mod scene3d;
pub mod silk;
pub mod stitch;
pub mod verify;
