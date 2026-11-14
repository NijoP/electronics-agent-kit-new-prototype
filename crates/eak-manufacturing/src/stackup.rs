//! Stackup description file and the JLCPCB multilayer stackup check.
//!
//! The stackup file is an informational record of the build-up the Gerbers assume; the
//! fabricator's own stackup choice is authoritative. The JLCPCB limits below are the ones
//! this project recorded from the public capability pages and are **not verified** against
//! the current published capabilities; the check says so in every result and never reports
//! an unknown as a pass.

use eak_core::{geom::to_mm, LayerKind, Project, Stackup, ViaKind};
use serde::{Deserialize, Serialize};

/// JSON stackup file written next to the Gerbers.
pub fn stackup_json(p: &Project) -> String {
    let st = &p.pcb.board.stackup;
    let layers: Vec<serde_json::Value> = st
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| {
            serde_json::json!({
                "order": i + 1,
                "layer": l.layer.name(),
                "kind": match l.kind { LayerKind::Signal => "signal", LayerKind::Plane => "plane" },
                "copper_oz": l.copper_oz,
                "dielectric_after_um": l.dielectric_after_um,
                "dielectric_er": l.er,
            })
        })
        .collect();
    let vias: Vec<serde_json::Value> = p
        .pcb
        .vias
        .values()
        .map(|v| (v.from, v.to, v.kind()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|(f, t, k)| {
            serde_json::json!({
                "from": f.name(), "to": t.name(),
                "kind": match k { ViaKind::Through => "through", ViaKind::Blind => "blind", ViaKind::Buried => "buried" },
            })
        })
        .collect();
    let doc = serde_json::json!({
        "format": "eak-stackup/1",
        "project": p.name,
        "revision": p.revision,
        "copper_layers": st.copper_layers(),
        "board_thickness_mm": to_mm(st.board_thickness),
        "dielectric": st.dielectric,
        "layers": layers,
        "via_spans_in_use": vias,
        "note": "Informational. Impedance is not modelled; the fabricator's stackup is authoritative.",
    });
    serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n"
}

/// Problems with a stackup file (empty when it parses and describes a valid stackup).
pub fn validate(text: &str) -> Vec<String> {
    let v: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return vec![format!("not valid JSON: {e}")],
    };
    let mut out = Vec::new();
    let n = v["copper_layers"].as_u64().unwrap_or(0) as usize;
    if v["layers"].as_array().map(Vec::len) != Some(n) || n < 2 {
        out.push("layer list does not match copper_layers".into());
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FabFinding {
    pub id: String,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FabStackupCheck {
    pub fab: String,
    /// Always true: the limits are not verified against the fabricator's current pages.
    pub limits_unverified: bool,
    pub copper_layers: usize,
    pub findings: Vec<FabFinding>,
    /// Always false for the checks that could not be made; never counted as a pass.
    pub not_checked: Vec<String>,
}

impl FabStackupCheck {
    pub fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }
}

/// Standard finished thicknesses (mm) offered for multilayer boards (unverified).
const JLC_THICKNESS_MM: [f64; 7] = [0.4, 0.6, 0.8, 1.0, 1.2, 1.6, 2.0];
/// Smallest plated via drill accepted (mm) and the smaller size recommended for multilayer.
const JLC_MIN_VIA_DRILL_MM: f64 = 0.15;
const JLC_RECOMMENDED_VIA_DRILL_MM: f64 = 0.2;
/// Thinnest dielectric between copper layers (µm) assumed available.
const JLC_MIN_DIELECTRIC_UM: u32 = 70;

/// Check the stackup (and the via spans in use) against JLCPCB multilayer capability.
pub fn jlcpcb_check(p: &Project) -> FabStackupCheck {
    let st: &Stackup = &p.pcb.board.stackup;
    let mut f = Vec::new();
    let mut add = |id: &str, severity, message: String| {
        f.push(FabFinding {
            id: id.into(),
            severity,
            message,
        })
    };
    let n = st.copper_layers();
    if !(2..=32).contains(&n) || !n.is_multiple_of(2) {
        add(
            "JLC-STK-001",
            Severity::Error,
            format!("{n} copper layers: JLCPCB builds even layer counts from 2 to 32"),
        );
    }
    let mm = to_mm(st.board_thickness);
    if n >= 4 && !JLC_THICKNESS_MM.iter().any(|t| (t - mm).abs() < 1e-6) {
        add(
            "JLC-STK-002",
            Severity::Warning,
            format!("board thickness {mm} mm is not one of the standard multilayer thicknesses {JLC_THICKNESS_MM:?} mm"),
        );
    }
    for (i, l) in st.layers.iter().enumerate() {
        let outer = i == 0 || i + 1 == n;
        let ok: &[f64] = if outer { &[1.0, 2.0] } else { &[0.5, 1.0] };
        if n >= 4 && !ok.iter().any(|o| (o - l.copper_oz).abs() < 1e-9) {
            add(
                "JLC-STK-003",
                Severity::Warning,
                format!(
                    "{} copper is {} oz; the standard {} weights are {ok:?} oz",
                    l.layer.name(),
                    l.copper_oz,
                    if outer { "outer" } else { "inner" }
                ),
            );
        }
        if i + 1 < n && l.dielectric_after_um < JLC_MIN_DIELECTRIC_UM {
            add(
                "JLC-STK-004",
                Severity::Error,
                format!(
                    "dielectric below {} is {} µm, thinner than the {} µm assumed available",
                    l.layer.name(),
                    l.dielectric_after_um,
                    JLC_MIN_DIELECTRIC_UM
                ),
            );
        }
    }
    let special = p
        .pcb
        .vias
        .values()
        .filter(|v| v.kind() != ViaKind::Through)
        .count();
    if special > 0 {
        add(
            "JLC-STK-005",
            Severity::Warning,
            format!("{special} blind/buried via(s): not part of the standard multilayer service; confirm availability, extra process steps and cost with the fabricator"),
        );
    }
    for v in p.pcb.vias.values() {
        let d = to_mm(v.drill);
        if d + 1e-9 < JLC_MIN_VIA_DRILL_MM {
            add(
                "JLC-STK-006",
                Severity::Error,
                format!(
                    "via {} drill {d:.3} mm is below the {JLC_MIN_VIA_DRILL_MM} mm minimum",
                    v.id
                ),
            );
        } else if n >= 4 && d + 1e-9 < JLC_RECOMMENDED_VIA_DRILL_MM {
            add(
                "JLC-STK-006",
                Severity::Warning,
                format!("via {} drill {d:.3} mm is below the {JLC_RECOMMENDED_VIA_DRILL_MM} mm usually used on multilayer boards", v.id),
            );
        }
    }
    FabStackupCheck {
        fab: "JLCPCB".into(),
        limits_unverified: true,
        copper_layers: n,
        findings: f,
        not_checked: vec![
            "limits are recorded from public capability pages and are not verified against the current pages".into(),
            "controlled impedance and the fabricator's own stackup (core/prepreg part numbers) are not modelled".into(),
            "minimum track/space, annular ring and drill-to-copper rules per layer are not part of this check".into(),
            "pricing, lead time and special-process availability".into(),
        ],
    }
}
