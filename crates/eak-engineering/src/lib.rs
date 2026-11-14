//! # eak-engineering — deterministic engineering core
//!
//! * `ipc`     – trace ampacity, clearance, creepage, via current (provenance: DESK audit).
//! * `calc`    – circuit calculations and engineering value parsing.
//! * `netlist` – schematic connectivity → canonical nets.
//! * `copper`  – PCB copper shapes, zone fill with thermal reliefs, physical connectivity.
//! * `silk`    – silkscreen legend: stroke-font reference designators, mask-opening clip.
//! * `router`  – EAK's native two-layer grid router.
//! * `interactive` – route-step API for the interactive PCB route tool (walkaround, 45/90, vias).
//! * `placer`  – constructive footprint placement.
//! * `placement_intel` – placement mental models (roles, snaps, analysis) ported from DESK.
//! * `power`   – unit-aware trace width, voltage drop, rail drop, return-path continuity.
//! * `scene3d` – board/component geometry for the 3D workspace and mechanical checks.

pub mod calc;
pub mod copper;
pub mod interactive;
pub mod ipc;
pub mod netlist;
pub mod placement_intel;
pub mod placer;
pub mod priority;
pub mod power;
pub mod router;
pub mod scene3d;
pub mod silk;
