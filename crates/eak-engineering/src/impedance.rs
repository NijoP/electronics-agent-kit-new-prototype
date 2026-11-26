//! Characteristic impedance of a track from the board stack-up (closed-form estimates).
//!
//! Models (public closed-form approximations, written from the published equations):
//!
//! * surface microstrip (outer layer over the adjacent plane), IPC-2141A eq. 4-1:
//!   `Z0 = 87 / sqrt(er + 1.41) * ln(5.98 h / (0.8 w + t))`, valid for `0.1 <= w/h <= 2.0`, `1 <= er <= 15`;
//! * symmetric stripline (inner layer centred between two planes), IPC-2141A eq. 4-5:
//!   `Z0 = 60 / sqrt(er) * ln(4 b / (0.67 pi (0.8 w + t)))`, valid for `w/b < 0.35`, `t/b < 0.25`;
//! * edge-coupled differential, IPC-2141A eq. 4-9 / 4-10:
//!   microstrip `Zdiff = 2 Z0 (1 - 0.48 exp(-0.96 s/h))`, stripline `Zdiff = 2 Z0 (1 - 0.347 exp(-2.9 s/b))`.
//!
//! `h` is the dielectric to the reference plane, `t` the copper thickness (oz x 34.8 µm), `b` the
//! plane-to-plane spacing, `s` the edge-to-edge gap. All come from the EAK-IR stack-up; a missing
//! or unusable input (no adjacent plane, zero thickness, no permittivity, geometry outside the
//! formula's validity) makes the result `Unknown` with that input named. A width is never
//! guessed. These are estimates (typically within ±10 % of a field solver inside their validity
//! range); the fabricator's controlled-impedance stack-up is authoritative.

use eak_core::{CopperLayer, LayerKind, Nm, Stackup, COPPER_UM_PER_OZ};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

pub const MICROSTRIP: &str =
    "IPC-2141A eq. 4-1 surface microstrip: Z0 = 87/sqrt(er+1.41) ln(5.98h/(0.8w+t))";
pub const STRIPLINE: &str =
    "IPC-2141A eq. 4-5 symmetric stripline: Z0 = 60/sqrt(er) ln(4b/(0.67 pi (0.8w+t)))";
pub const DIFF_MICROSTRIP: &str =
    "IPC-2141A eq. 4-9 edge-coupled microstrip: Zdiff = 2 Z0 (1 - 0.48 exp(-0.96 s/h))";
pub const DIFF_STRIPLINE: &str =
    "IPC-2141A eq. 4-10 edge-coupled stripline: Zdiff = 2 Z0 (1 - 0.347 exp(-2.9 s/b))";

/// Cross-section of a track on one layer, in mm.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Geometry {
    /// Outer layer over one plane at `h`.
    Microstrip { h_mm: f64, t_mm: f64, er: f64 },
    /// Inner layer centred between two planes `b` apart.
    Stripline { b_mm: f64, t_mm: f64, er: f64 },
}

/// An impedance figure or the reason it cannot be given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Estimate {
    Known {
        ohm: f64,
        formula: String,
        geometry: Geometry,
    },
    /// Not computed: `missing` names the input that is absent or outside the model.
    Unknown { missing: String },
}

impl Estimate {
    pub fn ohm(&self) -> Option<f64> {
        match self {
            Estimate::Known { ohm, .. } => Some(*ohm),
            Estimate::Unknown { .. } => None,
        }
    }
}

/// A width (or gap) sized for a target impedance, or why it cannot be sized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Sizing {
    Known {
        value: Nm,
        /// Impedance the rounded value gives.
        achieved_ohm: f64,
        formula: String,
        geometry: Geometry,
    },
    Unknown {
        missing: String,
    },
}

fn unknown(why: impl Into<String>) -> Estimate {
    Estimate::Unknown {
        missing: why.into(),
    }
}

/// The cross-section of `layer`: microstrip on an outer layer whose neighbour is a plane,
/// stripline on an inner layer between two planes at (within 10 %) equal distance.
pub fn geometry(st: &Stackup, layer: CopperLayer) -> Result<Geometry, String> {
    let Some(i) = st.index_of(layer) else {
        return Err(format!("layer {} in the stack-up", layer.name()));
    };
    let n = st.layers.len();
    if n < 2 {
        return Err("a stack-up with at least two copper layers".into());
    }
    let l = &st.layers[i];
    if !(l.copper_oz.is_finite() && l.copper_oz > 0.0) {
        return Err(format!("copper weight of {}", layer.name()));
    }
    let t_mm = l.copper_oz * COPPER_UM_PER_OZ / 1000.0;
    // Reference layer `j`; the dielectric between it and `layer` is recorded on `owner`
    // (the upper of the two).
    let side = |j: usize, owner: usize| -> Result<(f64, f64), String> {
        if st.layers[j].kind != LayerKind::Plane {
            return Err(format!(
                "reference plane next to {}: {} is a signal layer (declare it a plane if it is solid copper)",
                layer.name(),
                st.layers[j].layer.name()
            ));
        }
        let d = &st.layers[owner];
        if d.dielectric_after_um == 0 {
            return Err(format!("dielectric thickness below {}", d.layer.name()));
        }
        if !(d.er.is_finite() && d.er > 1.0) {
            return Err(format!(
                "dielectric permittivity (er) below {}",
                d.layer.name()
            ));
        }
        Ok((f64::from(d.dielectric_after_um) / 1000.0, d.er))
    };
    if i == 0 || i + 1 == n {
        let (h_mm, er) = if i == 0 {
            side(1, 0)?
        } else {
            side(n - 2, n - 2)?
        };
        return Ok(Geometry::Microstrip { h_mm, t_mm, er });
    }
    let (h1, er1) = side(i - 1, i - 1)?;
    let (h2, er2) = side(i + 1, i)?;
    if (h1 - h2).abs() > 0.1 * h1.max(h2) {
        return Err(format!(
            "symmetric stripline on {}: planes are {h1:.3} and {h2:.3} mm away (offset stripline is not modelled)",
            layer.name()
        ));
    }
    if (er1 - er2).abs() > 1e-9 {
        return Err(format!(
            "one permittivity around {}: {er1} above, {er2} below (mixed dielectric is not modelled)",
            layer.name()
        ));
    }
    Ok(Geometry::Stripline {
        b_mm: h1 + h2 + t_mm,
        t_mm,
        er: er1,
    })
}

/// Single-ended impedance of a `width_nm` track in `g`, inside the formula's validity range.
pub fn z0(g: Geometry, width_nm: Nm) -> Estimate {
    if width_nm <= 0 {
        return unknown("a positive track width");
    }
    let w = width_nm as f64 / 1e6;
    match g {
        Geometry::Microstrip {
            h_mm: h,
            t_mm: t,
            er,
        } => {
            if !(0.1..=2.0).contains(&(w / h)) || !(1.0..=15.0).contains(&er) {
                return unknown(format!(
                    "geometry inside the microstrip model's validity (0.1 <= w/h <= 2, 1 <= er <= 15): w/h = {:.3}, er = {er}",
                    w / h
                ));
            }
            Estimate::Known {
                ohm: 87.0 / (er + 1.41).sqrt() * (5.98 * h / (0.8 * w + t)).ln(),
                formula: MICROSTRIP.into(),
                geometry: g,
            }
        }
        Geometry::Stripline {
            b_mm: b,
            t_mm: t,
            er,
        } => {
            if w / b >= 0.35 || t / b >= 0.25 {
                return unknown(format!(
                    "geometry inside the stripline model's validity (w/b < 0.35, t/b < 0.25): w/b = {:.3}, t/b = {:.3}",
                    w / b,
                    t / b
                ));
            }
            Estimate::Known {
                ohm: 60.0 / er.sqrt() * (4.0 * b / (0.67 * PI * (0.8 * w + t))).ln(),
                formula: STRIPLINE.into(),
                geometry: g,
            }
        }
    }
}

/// Differential impedance of two `width_nm` tracks `gap_nm` apart (edge to edge) in `g`.
pub fn zdiff(g: Geometry, width_nm: Nm, gap_nm: Nm) -> Estimate {
    if gap_nm <= 0 {
        return unknown("a positive pair gap");
    }
    let s = gap_nm as f64 / 1e6;
    let (ohm, formula, single) = match (z0(g, width_nm), g) {
        (Estimate::Known { ohm, .. }, Geometry::Microstrip { h_mm, .. }) => (
            2.0 * ohm * (1.0 - 0.48 * (-0.96 * s / h_mm).exp()),
            DIFF_MICROSTRIP,
            MICROSTRIP,
        ),
        (Estimate::Known { ohm, .. }, Geometry::Stripline { b_mm, .. }) => (
            2.0 * ohm * (1.0 - 0.347 * (-2.9 * s / b_mm).exp()),
            DIFF_STRIPLINE,
            STRIPLINE,
        ),
        (u, _) => return u,
    };
    Estimate::Known {
        ohm,
        formula: format!("{formula}; single-ended {single}"),
        geometry: g,
    }
}

/// Bisect `f(x)` (decreasing in `x`) for `target` over `lo..hi` nm to 1 µm.
fn solve(
    lo: Nm,
    hi: Nm,
    target: f64,
    f: impl Fn(Nm) -> Estimate,
) -> Result<(Nm, Estimate), String> {
    let z = |x: Nm| f(x).ohm();
    let (Some(zlo), Some(zhi)) = (z(lo), z(hi)) else {
        return Err("geometry inside the model's validity range at the search bounds".into());
    };
    if !(zhi..=zlo).contains(&target) {
        return Err(format!(
            "a target inside what this layer can reach ({zhi:.1}..{zlo:.1} ohm); {target} ohm is outside it"
        ));
    }
    let (mut a, mut b) = (lo, hi);
    while b - a > 1_000 {
        let m = a + (b - a) / 2;
        match z(m) {
            Some(v) if v > target => a = m,
            Some(_) => b = m,
            None => return Err("geometry inside the model's validity range".into()),
        }
    }
    // Of the two bracketing values, the one closer to the target.
    let off = |x: Nm| z(x).map_or(f64::MAX, |v| (v - target).abs());
    let pick = if off(a) <= off(b) { a } else { b };
    Ok((pick, f(pick)))
}

fn sized(r: Result<(Nm, Estimate), String>) -> Sizing {
    match r {
        Ok((
            value,
            Estimate::Known {
                ohm,
                formula,
                geometry,
            },
        )) => Sizing::Known {
            value,
            achieved_ohm: ohm,
            formula,
            geometry,
        },
        Ok((_, Estimate::Unknown { missing })) | Err(missing) => Sizing::Unknown { missing },
    }
}

/// Track width on `layer` for a single-ended `target_ohm`.
pub fn width_for(st: &Stackup, layer: CopperLayer, target_ohm: f64) -> Sizing {
    let g = match geometry(st, layer) {
        Ok(g) => g,
        Err(missing) => return Sizing::Unknown { missing },
    };
    let mm = |x: f64| (x * 1e6) as Nm;
    // Search bounds that keep the model valid.
    let (lo, hi) = match g {
        Geometry::Microstrip { h_mm, .. } => (mm(0.1 * h_mm) + 1_000, mm(2.0 * h_mm)),
        Geometry::Stripline { b_mm, .. } => (10_000, mm(0.35 * b_mm) - 1_000),
    };
    sized(solve(lo, hi, target_ohm, |w| z0(g, w)))
}

/// Edge-to-edge gap on `layer` that gives `target_ohm` differential with `width_nm` tracks.
pub fn gap_for(st: &Stackup, layer: CopperLayer, width_nm: Nm, target_ohm: f64) -> Sizing {
    let g = match geometry(st, layer) {
        Ok(g) => g,
        Err(missing) => return Sizing::Unknown { missing },
    };
    // A width outside the model fails for every gap: say so, not "search bounds".
    if let Estimate::Unknown { missing } = z0(g, width_nm) {
        return Sizing::Unknown { missing };
    }
    // Zdiff rises with the gap: solve on the negated gap so the function decreases.
    let r = solve(-5_000_000, -10_000, target_ohm, |neg| {
        zdiff(g, width_nm, -neg)
    });
    sized(r.map(|(neg, e)| (-neg, e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::FR4_ER;

    fn four_layer() -> Stackup {
        Stackup::multilayer(4, 1_600_000, 1.0, 0.5, "FR-4", FR4_ER)
    }

    #[test]
    fn microstrip_matches_a_hand_calculation() {
        // h = 0.2 mm, t = 0.035 mm, er = 4.5, w = 0.35 mm:
        // 87/sqrt(5.91) * ln(1.196/0.315) = 35.787 * 1.3342 = 47.75 ohm.
        let g = Geometry::Microstrip {
            h_mm: 0.2,
            t_mm: 0.035,
            er: 4.5,
        };
        let z = z0(g, 350_000).ohm().unwrap();
        assert!((z - 47.75).abs() < 0.05, "{z}");
        // 2 * 47.75 * (1 - 0.48 e^-0.96) = 95.5 * 0.8162 = 77.95 ohm.
        let d = zdiff(g, 350_000, 200_000).ohm().unwrap();
        assert!((d - 77.95).abs() < 0.1, "{d}");
    }

    #[test]
    fn outer_layer_over_a_plane_is_microstrip_and_two_layer_is_unknown() {
        let st = four_layer();
        assert!(matches!(
            geometry(&st, CopperLayer::Top),
            Ok(Geometry::Microstrip { .. })
        ));
        // The default two-layer board declares no plane: no reference, no impedance.
        let two = Stackup::two_layer(1_600_000, 1.0, "FR-4", FR4_ER);
        let e = geometry(&two, CopperLayer::Top).unwrap_err();
        assert!(e.contains("reference plane"), "{e}");
        assert!(matches!(
            width_for(&two, CopperLayer::Top, 50.0),
            Sizing::Unknown { .. }
        ));
    }

    #[test]
    fn missing_permittivity_or_thickness_is_named() {
        let mut st = four_layer();
        st.layers[0].er = 0.0;
        assert!(geometry(&st, CopperLayer::Top)
            .unwrap_err()
            .contains("permittivity"));
        let mut st = four_layer();
        st.layers[0].dielectric_after_um = 0;
        assert!(geometry(&st, CopperLayer::Top)
            .unwrap_err()
            .contains("thickness"));
    }

    #[test]
    fn solved_width_and_gap_hit_the_target() {
        let st = four_layer();
        let Sizing::Known {
            value: w,
            achieved_ohm,
            ..
        } = width_for(&st, CopperLayer::Top, 50.0)
        else {
            panic!("50 ohm on a 4-layer top layer is reachable");
        };
        assert!((achieved_ohm - 50.0).abs() < 0.1, "{achieved_ohm}");
        let g = geometry(&st, CopperLayer::Top).unwrap();
        assert_eq!(z0(g, w).ohm(), Some(achieved_ohm));
        let target = zdiff(g, w, 300_000).ohm().unwrap();
        let Sizing::Known { value: s, .. } = gap_for(&st, CopperLayer::Top, w, target) else {
            panic!("the gap that gave the target is reachable");
        };
        assert!((s - 300_000).abs() <= 1_000, "{s}");
        assert!(matches!(
            width_for(&st, CopperLayer::Top, 500.0),
            Sizing::Unknown { .. }
        ));
        let Sizing::Unknown { missing } = gap_for(&st, CopperLayer::Top, 5_000_000, 90.0) else {
            panic!("a 5 mm track is outside the microstrip model");
        };
        assert!(missing.contains("w/h"), "{missing}");
    }

    #[test]
    fn stripline_needs_symmetric_planes() {
        let mut st = Stackup::multilayer(6, 1_600_000, 1.0, 0.5, "FR-4", FR4_ER);
        for l in &mut st.layers {
            l.kind = LayerKind::Plane;
        }
        st.layers[2].kind = LayerKind::Signal;
        let g = geometry(&st, CopperLayer::Inner(2)).unwrap();
        assert!(matches!(g, Geometry::Stripline { .. }));
        assert!(z0(g, 150_000).ohm().is_some());
        st.layers[2].dielectric_after_um *= 3;
        assert!(geometry(&st, CopperLayer::Inner(2))
            .unwrap_err()
            .contains("offset"));
    }
}
