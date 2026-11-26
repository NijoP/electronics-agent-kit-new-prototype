//! # eak-engineering — deterministic engineering core
//!
//! * `ipc`     – trace ampacity, clearance, creepage, via current (provenance: DESK audit).
//! * `calc`    – circuit calculations and engineering value parsing.
//! * `netlist` – schematic connectivity → canonical nets.
//! * `board`   – the one derived board geometry (outline, copper, fills, holes, silk) that DRC,
//!   connectivity, the 3D scene and the desktop views all read (ADR-0052).
//! * `copper`  – PCB copper shapes, zone fill with thermal reliefs, physical connectivity.
//! * `stitch`  – zone stitching: escape vias for orphan SMD pads, stitching vias for pour islands (DESK).
//! * `silk`    – silkscreen legend: stroke-font reference designators, mask-opening clip.
//! * `router`  – EAK's native two-layer grid router (passes, rip-up, congestion, pairs, widths).
//! * `routing_intel` – congestion, stagnation, differential pairs, width/via selection (DESK-derived).
//! * `interactive` – route-step API for the interactive PCB route tool (walkaround, 45/90, vias).
//! * `placer`  – constructive footprint placement.
//! * `placement_intel` – placement mental models (roles, snaps, analysis) ported from DESK.
//! * `power`   – unit-aware trace width, voltage drop, rail drop, return-path continuity.
//! * `scene3d` – board/component geometry for the 3D workspace and mechanical checks.

pub mod board;
pub mod calc;
pub mod clearance;
pub mod copper;
pub mod fill_view;
pub mod interactive;
pub mod ipc;
pub mod netlist;
pub mod placement_intel;
pub mod placer;
pub mod power;
pub mod priority;
pub mod router;
pub mod routing_intel;
pub mod scene3d;
pub mod silk;
pub mod stitch;
