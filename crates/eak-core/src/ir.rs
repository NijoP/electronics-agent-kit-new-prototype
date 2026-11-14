//! EAK-IR: the single canonical engineering representation of a project.
//!
//! Schematic, PCB, 3D, AI and manufacturing all read this one structure. It is only
//! mutated by `eak-runtime`'s command engine.

use crate::geom::{Nm, Point, Rotation};
use crate::library::ComponentDef;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Current on-disk schema version. Bump together with a migration in `migrate.rs`.
pub const SCHEMA_VERSION: u32 = 2;

pub type Id = String;

/// Allocate a fresh stable object identifier with a type prefix (`sym_`, `wir_`, ...).
pub fn new_id(prefix: &str) -> Id {
    let u = uuid::Uuid::new_v4().simple().to_string();
    format!("{prefix}_{}", &u[..16])
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: u32,
    pub id: Id,
    pub name: String,
    /// Monotonic revision; every committed command increments it by one.
    pub revision: u64,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
    /// Project-local copy of every component definition used (self-contained project).
    #[serde(default)]
    pub library: BTreeMap<String, ComponentDef>,
    pub schematic: Schematic,
    pub pcb: Pcb,
    pub constraints: Constraints,
    #[serde(default)]
    pub manufacturing: ManufacturingConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Requirement {
    pub id: Id,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Schematic {
    pub sheets: BTreeMap<Id, Sheet>,
    pub symbols: BTreeMap<Id, SymbolInstance>,
    pub wires: BTreeMap<Id, Wire>,
    pub labels: BTreeMap<Id, NetLabel>,
    /// Explicit no-connect markers on pins: (symbol id, pin number).
    #[serde(default)]
    pub no_connects: BTreeMap<Id, NoConnect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    pub id: Id,
    pub name: String,
    pub width: Nm,
    pub height: Nm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolInstance {
    pub id: Id,
    pub sheet_id: Id,
    pub component_id: String,
    pub refdes: String,
    pub value: String,
    pub position: Point,
    pub rotation: Rotation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wire {
    pub id: Id,
    pub sheet_id: Id,
    pub a: Point,
    pub b: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LabelKind {
    /// Ordinary net label.
    #[default]
    Net,
    /// Power port: declares the net as a driven supply (satisfies power-input pins).
    Power,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetLabel {
    pub id: Id,
    pub sheet_id: Id,
    pub name: String,
    pub position: Point,
    #[serde(default)]
    pub kind: LabelKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoConnect {
    pub id: Id,
    pub symbol_id: Id,
    pub pin: String,
}

/// One copper layer. `Inner(n)` is the n-th inner layer counted from the top (1-based), so a
/// 4-layer board has `Top, Inner(1), Inner(2), Bottom`. Stack order is defined by the position
/// in [`Stackup::layers`], not by the derived `Ord`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CopperLayer {
    Top,
    Inner(u8),
    Bottom,
}

impl CopperLayer {
    /// The two outer layers (a 2-layer board's whole stack).
    pub const OUTER: [CopperLayer; 2] = [CopperLayer::Top, CopperLayer::Bottom];
    pub fn is_outer(self) -> bool {
        !matches!(self, CopperLayer::Inner(_))
    }
    /// KiCad-style display name: `F.Cu`, `In1.Cu`, `B.Cu`.
    pub fn name(self) -> String {
        match self {
            CopperLayer::Top => "F.Cu".into(),
            CopperLayer::Inner(n) => format!("In{n}.Cu"),
            CopperLayer::Bottom => "B.Cu".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    #[default]
    Top,
    Bottom,
}

impl Side {
    pub fn copper(self) -> CopperLayer {
        match self {
            Side::Top => CopperLayer::Top,
            Side::Bottom => CopperLayer::Bottom,
        }
    }
}

/// Copper foil thickness per ounce of weight (1 oz/ft² ≈ 34.8 µm).
pub const COPPER_UM_PER_OZ: f64 = 34.8;
/// Nominal relative permittivity assumed for FR-4 when none is given (v1 files, default).
pub const FR4_ER: f64 = 4.5;
/// Allowed mismatch between the per-layer build-up and the declared finished thickness
/// (the usual ±10 % fabrication tolerance).
pub const STACKUP_THICKNESS_TOLERANCE: f64 = 0.10;

/// What a copper layer is used for. A plane layer is a (mostly) solid power or ground pour;
/// a signal layer carries routing. Both can hold tracks, vias and zones, but DRC treats a
/// plane without a zone as a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    #[default]
    Signal,
    Plane,
}

/// One copper layer of the build-up and the dielectric beneath it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackupLayer {
    pub layer: CopperLayer,
    /// Signal or plane (absent in older files = signal).
    #[serde(default)]
    pub kind: LayerKind,
    /// Copper weight in ounces per square foot (drives IPC trace-width sizing).
    pub copper_oz: f64,
    /// Dielectric thickness between this copper layer and the next one, in µm.
    /// Zero for the last (bottom) layer.
    pub dielectric_after_um: u32,
    /// Relative permittivity of that dielectric.
    pub er: f64,
}

/// Most copper layers the editor and the Gerber writer support.
pub const MAX_COPPER_LAYERS: usize = 16;

/// Board build-up, ordered top to bottom: one entry per copper layer. The layer count and
/// every copper weight come from `layers`; there is no separate summary field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stackup {
    pub layers: Vec<StackupLayer>,
    /// Declared finished board thickness (what is ordered from the fab).
    pub board_thickness: Nm,
    /// Dielectric material name.
    pub dielectric: String,
}

impl Stackup {
    /// A two-layer board of `board_thickness` with `copper_oz` on both sides; the core
    /// thickness is whatever the copper leaves of the finished thickness.
    pub fn two_layer(board_thickness: Nm, copper_oz: f64, dielectric: &str, er: f64) -> Stackup {
        Stackup::multilayer(2, board_thickness, copper_oz, copper_oz, dielectric, er)
    }

    /// An `n`-layer board (`n` even, 2..=[`MAX_COPPER_LAYERS`]). Copper is `outer_oz` on the two
    /// outer layers and `inner_oz` inside; the dielectric between adjacent copper layers is the
    /// same everywhere (whatever the copper leaves of `board_thickness`, split evenly). The
    /// first and last inner layers are planes (ground and power), any others are signal; edit
    /// the result for another arrangement. Callers pass it to `SetStackup`, which validates it.
    pub fn multilayer(
        n: usize,
        board_thickness: Nm,
        outer_oz: f64,
        inner_oz: f64,
        dielectric: &str,
        er: f64,
    ) -> Stackup {
        let n = n.max(2);
        let copper_um: f64 = (0..n)
            .map(|i| {
                let oz = if i == 0 || i == n - 1 {
                    outer_oz
                } else {
                    inner_oz
                };
                oz * COPPER_UM_PER_OZ
            })
            .sum();
        let gaps = (n - 1) as f64;
        let gap_um = ((board_thickness as f64 / 1000.0 - copper_um) / gaps)
            .round()
            .max(0.0) as u32;
        let layers = (0..n)
            .map(|i| {
                let (layer, oz, kind) = if i == 0 {
                    (CopperLayer::Top, outer_oz, LayerKind::Signal)
                } else if i == n - 1 {
                    (CopperLayer::Bottom, outer_oz, LayerKind::Signal)
                } else {
                    let kind = if i == 1 || i == n - 2 {
                        LayerKind::Plane
                    } else {
                        LayerKind::Signal
                    };
                    (CopperLayer::Inner(i as u8), inner_oz, kind)
                };
                StackupLayer {
                    layer,
                    kind,
                    copper_oz: oz,
                    dielectric_after_um: if i == n - 1 { 0 } else { gap_um },
                    er,
                }
            })
            .collect();
        Stackup {
            layers,
            board_thickness,
            dielectric: dielectric.into(),
        }
    }

    pub fn copper_layers(&self) -> usize {
        self.layers.len()
    }

    /// The copper layers, top to bottom.
    pub fn layer_ids(&self) -> Vec<CopperLayer> {
        self.layers.iter().map(|l| l.layer).collect()
    }

    /// Position of `layer` in the stack (0 = top), or `None` if the stackup lacks it.
    pub fn index_of(&self, layer: CopperLayer) -> Option<usize> {
        self.layers.iter().position(|l| l.layer == layer)
    }

    pub fn contains(&self, layer: CopperLayer) -> bool {
        self.index_of(layer).is_some()
    }

    /// Signal or plane, or `None` if the stackup lacks the layer.
    pub fn kind_of(&self, layer: CopperLayer) -> Option<LayerKind> {
        self.layers
            .iter()
            .find(|l| l.layer == layer)
            .map(|l| l.kind)
    }

    /// Every copper layer from `a` to `b` inclusive, in stack order (`a`/`b` in either order);
    /// empty if either is not in the stackup.
    pub fn layers_between(&self, a: CopperLayer, b: CopperLayer) -> Vec<CopperLayer> {
        match (self.index_of(a), self.index_of(b)) {
            (Some(i), Some(j)) => self.layers[i.min(j)..=i.max(j)]
                .iter()
                .map(|l| l.layer)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Copper weight of `layer`, or `None` if the stackup does not contain it.
    pub fn copper_oz(&self, layer: CopperLayer) -> Option<f64> {
        self.layers
            .iter()
            .find(|l| l.layer == layer)
            .map(|l| l.copper_oz)
    }

    /// Problems that make this stackup unusable; empty when it is valid.
    pub fn validate(&self) -> Vec<String> {
        let mut e = Vec::new();
        let n = self.layers.len();
        if !(2..=MAX_COPPER_LAYERS).contains(&n) || !n.is_multiple_of(2) {
            e.push(format!(
                "copper layer count must be even and between 2 and {MAX_COPPER_LAYERS}, got {n}"
            ));
        } else {
            let want: Vec<CopperLayer> = (0..n)
                .map(|i| match i {
                    0 => CopperLayer::Top,
                    i if i == n - 1 => CopperLayer::Bottom,
                    i => CopperLayer::Inner(i as u8),
                })
                .collect();
            if self.layer_ids() != want {
                e.push(format!(
                    "layers must be listed top to bottom as F.Cu, In1.Cu … In{}.Cu, B.Cu, got {:?}",
                    n.saturating_sub(2),
                    self.layer_ids()
                ));
            }
        }
        if self
            .layers
            .first()
            .is_some_and(|l| l.kind == LayerKind::Plane)
            || self
                .layers
                .last()
                .is_some_and(|l| l.kind == LayerKind::Plane)
        {
            e.push("the outer layers carry components and cannot be plane layers".into());
        }
        if self.board_thickness <= 0 {
            e.push("board thickness must be positive".into());
        }
        let last = self.layers.len().saturating_sub(1);
        let mut total_um = 0.0;
        for (i, l) in self.layers.iter().enumerate() {
            if !(l.copper_oz.is_finite() && l.copper_oz > 0.0) {
                e.push(format!("{} copper weight must be positive", l.layer.name()));
            }
            if !(l.er.is_finite() && l.er >= 1.0) {
                e.push(format!("{} dielectric er must be ≥ 1", l.layer.name()));
            }
            if i == last && l.dielectric_after_um != 0 {
                e.push("the last copper layer has no dielectric after it".into());
            }
            if i < last && l.dielectric_after_um == 0 {
                e.push(format!(
                    "{} dielectric thickness must be positive",
                    l.layer.name()
                ));
            }
            total_um += l.copper_oz * COPPER_UM_PER_OZ + f64::from(l.dielectric_after_um);
        }
        let declared_um = self.board_thickness as f64 / 1000.0;
        if self.board_thickness > 0
            && ((total_um - declared_um).abs() > declared_um * STACKUP_THICKNESS_TOLERANCE)
        {
            e.push(format!(
                "layer build-up totals {total_um:.0} µm but the declared board thickness is {declared_um:.0} µm (more than ±{:.0} % apart)",
                STACKUP_THICKNESS_TOLERANCE * 100.0
            ));
        }
        e
    }
}

impl Default for Stackup {
    /// 2-layer, 1.6 mm FR-4, 1 oz copper on both sides.
    fn default() -> Self {
        Stackup::two_layer(1_600_000, 1.0, "FR-4", FR4_ER)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Board {
    /// Closed outline polygon (counter-clockwise preferred). Empty = no outline yet.
    pub outline: Vec<Point>,
    pub stackup: Stackup,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootprintInstance {
    pub id: Id,
    /// Schematic symbol this footprint realises (cross-probe identity).
    pub symbol_id: Id,
    pub component_id: String,
    pub refdes: String,
    pub position: Point,
    pub rotation: Rotation,
    pub side: Side,
    /// pad number -> net name, set by schematic-to-PCB synchronisation.
    #[serde(default)]
    pub pad_nets: BTreeMap<String, String>,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: Id,
    pub layer: CopperLayer,
    pub width: Nm,
    pub a: Point,
    pub b: Point,
    pub net: String,
}

fn via_top() -> CopperLayer {
    CopperLayer::Top
}
fn via_bottom() -> CopperLayer {
    CopperLayer::Bottom
}

/// How far a via reaches through the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViaKind {
    /// Top to bottom.
    Through,
    /// From one outer layer to an inner layer.
    Blind,
    /// Between two inner layers; not visible from outside the board.
    Buried,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Via {
    pub id: Id,
    pub position: Point,
    pub drill: Nm,
    pub diameter: Nm,
    pub net: String,
    /// First copper layer of the barrel (absent in older files = top).
    #[serde(default = "via_top")]
    pub from: CopperLayer,
    /// Last copper layer of the barrel (absent in older files = bottom).
    #[serde(default = "via_bottom")]
    pub to: CopperLayer,
}

impl Via {
    pub fn kind(&self) -> ViaKind {
        match (self.from.is_outer(), self.to.is_outer()) {
            (true, true) => ViaKind::Through,
            (false, false) => ViaKind::Buried,
            _ => ViaKind::Blind,
        }
    }

    /// Copper layers the barrel touches in `stackup` (pads and connections exist on each).
    pub fn layers(&self, stackup: &Stackup) -> Vec<CopperLayer> {
        stackup.layers_between(self.from, self.to)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub id: Id,
    pub layer: CopperLayer,
    pub net: String,
    pub outline: Vec<Point>,
    /// Clearance kept from copper of other nets.
    pub clearance: Nm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Pcb {
    pub board: Board,
    pub footprints: BTreeMap<Id, FootprintInstance>,
    pub tracks: BTreeMap<Id, Track>,
    pub vias: BTreeMap<Id, Via>,
    pub zones: BTreeMap<Id, Zone>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetClass {
    pub name: String,
    pub track_width: Nm,
    pub clearance: Nm,
    pub via_drill: Nm,
    pub via_diameter: Nm,
    #[serde(default)]
    pub nets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraints {
    pub min_track_width: Nm,
    pub min_clearance: Nm,
    pub min_drill: Nm,
    pub min_annular_ring: Nm,
    pub board_edge_clearance: Nm,
    pub default_class: NetClass,
    #[serde(default)]
    pub net_classes: Vec<NetClass>,
    /// Declared continuous current per net (A) – drives IPC-2221 width checks.
    #[serde(default)]
    pub net_currents_a: BTreeMap<String, f64>,
    /// Declared peak working voltage per net (V) – drives voltage clearance checks.
    #[serde(default)]
    pub net_voltages_v: BTreeMap<String, f64>,
    /// Allowed conductor temperature rise for current sizing (°C).
    pub temp_rise_c: f64,
    /// Maximum component height above board (enclosure constraint), if any.
    #[serde(default)]
    pub max_component_height: Option<Nm>,
}

impl Default for Constraints {
    fn default() -> Self {
        // Defaults sized for common low-cost 2-layer fabrication (0.15 mm / 0.15 mm, 0.3 mm drill).
        Constraints {
            min_track_width: 150_000,
            min_clearance: 150_000,
            min_drill: 300_000,
            min_annular_ring: 130_000,
            board_edge_clearance: 300_000,
            default_class: NetClass {
                name: "Default".into(),
                track_width: 250_000,
                clearance: 200_000,
                via_drill: 300_000,
                via_diameter: 600_000,
                nets: vec![],
            },
            net_classes: vec![],
            net_currents_a: BTreeMap::new(),
            net_voltages_v: BTreeMap::new(),
            temp_rise_c: 10.0,
            max_component_height: None,
        }
    }
}

impl Constraints {
    pub fn class_for(&self, net: &str) -> &NetClass {
        self.net_classes
            .iter()
            .find(|c| c.nets.iter().any(|n| n == net))
            .unwrap_or(&self.default_class)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManufacturingConfig {
    pub fabricator_profile: String,
    pub surface_finish: String,
    pub solder_mask_color: String,
    /// Solder mask expansion around pads.
    pub mask_expansion: Nm,
}

impl Default for ManufacturingConfig {
    fn default() -> Self {
        ManufacturingConfig {
            fabricator_profile: "generic-2layer".into(),
            surface_finish: "HASL".into(),
            solder_mask_color: "Green".into(),
            mask_expansion: 50_000,
        }
    }
}

impl Project {
    pub fn new(name: &str) -> Project {
        let sheet_id = new_id("sht");
        let mut sheets = BTreeMap::new();
        sheets.insert(
            sheet_id.clone(),
            Sheet {
                id: sheet_id,
                name: "Sheet 1".into(),
                width: 297_000_000,
                height: 210_000_000,
            },
        );
        Project {
            schema_version: SCHEMA_VERSION,
            id: new_id("prj"),
            name: name.to_string(),
            revision: 0,
            requirements: vec![],
            library: BTreeMap::new(),
            schematic: Schematic {
                sheets,
                ..Default::default()
            },
            pcb: Pcb::default(),
            constraints: Constraints::default(),
            manufacturing: ManufacturingConfig::default(),
        }
    }

    pub fn first_sheet_id(&self) -> Option<&Id> {
        self.schematic.sheets.keys().next()
    }

    /// Absolute position of a symbol pin.
    pub fn pin_position(&self, sym: &SymbolInstance, pin: &str) -> Option<Point> {
        let def = self.library.get(&sym.component_id)?;
        let p = def.pin(pin)?;
        Some(sym.position.plus(sym.rotation.apply(p.position)))
    }

    /// Absolute position of a footprint pad (bottom side mirrors X).
    pub fn pad_position(&self, fp: &FootprintInstance, pad: &crate::library::PadDef) -> Point {
        let mut local = pad.position;
        if fp.side == Side::Bottom {
            local.x = -local.x;
        }
        fp.position.plus(fp.rotation.apply(local))
    }
}

#[cfg(test)]
mod multilayer_tests {
    use super::*;

    #[test]
    fn copper_layer_json_shape_is_stable() {
        assert_eq!(serde_json::to_string(&CopperLayer::Top).unwrap(), "\"top\"");
        assert_eq!(
            serde_json::to_string(&CopperLayer::Inner(2)).unwrap(),
            "{\"inner\":2}"
        );
        let l: CopperLayer = serde_json::from_str("{\"inner\":3}").unwrap();
        assert_eq!(l, CopperLayer::Inner(3));
    }

    #[test]
    fn multilayer_builder_makes_valid_stackups() {
        for n in [2usize, 4, 6, 8, 16] {
            let s = Stackup::multilayer(n, 1_600_000, 1.0, 0.5, "FR-4", FR4_ER);
            assert_eq!(s.copper_layers(), n);
            assert!(s.validate().is_empty(), "{n}: {:?}", s.validate());
        }
        let s = Stackup::multilayer(4, 1_600_000, 1.0, 0.5, "FR-4", FR4_ER);
        assert_eq!(s.kind_of(CopperLayer::Inner(1)), Some(LayerKind::Plane));
        assert_eq!(s.kind_of(CopperLayer::Top), Some(LayerKind::Signal));
        assert_eq!(
            s.layers_between(CopperLayer::Inner(2), CopperLayer::Top)
                .len(),
            3
        );
    }

    #[test]
    fn invalid_stackups_are_rejected() {
        let ok = Stackup::multilayer(4, 1_600_000, 1.0, 0.5, "FR-4", FR4_ER);
        let mut odd = ok.clone();
        odd.layers.remove(1);
        assert!(!odd.validate().is_empty(), "3 layers");
        let mut swapped = ok.clone();
        swapped.layers.swap(1, 2);
        assert!(!swapped.validate().is_empty(), "inner order");
        let mut plane_outer = ok.clone();
        plane_outer.layers[0].kind = LayerKind::Plane;
        assert!(!plane_outer.validate().is_empty(), "outer plane");
        assert!(Stackup::multilayer(18, 1_600_000, 1.0, 0.5, "FR-4", FR4_ER)
            .validate()
            .iter()
            .any(|m| m.contains("count")));
    }

    #[test]
    fn older_via_and_layer_json_defaults_to_through_signal() {
        let v: Via = serde_json::from_str(
            r#"{"id":"v","position":{"x":0,"y":0},"drill":300000,"diameter":600000,"net":"N"}"#,
        )
        .unwrap();
        assert_eq!(v.kind(), ViaKind::Through);
        let l: StackupLayer = serde_json::from_str(
            r#"{"layer":"top","copper_oz":1.0,"dielectric_after_um":100,"er":4.5}"#,
        )
        .unwrap();
        assert_eq!(l.kind, LayerKind::Signal);
    }

    #[test]
    fn via_kinds_follow_the_span() {
        let mk = |from, to| Via {
            id: "v".into(),
            position: Point::default(),
            drill: 1,
            diameter: 2,
            net: String::new(),
            from,
            to,
        };
        assert_eq!(
            mk(CopperLayer::Top, CopperLayer::Bottom).kind(),
            ViaKind::Through
        );
        assert_eq!(
            mk(CopperLayer::Top, CopperLayer::Inner(1)).kind(),
            ViaKind::Blind
        );
        assert_eq!(
            mk(CopperLayer::Inner(2), CopperLayer::Bottom).kind(),
            ViaKind::Blind
        );
        assert_eq!(
            mk(CopperLayer::Inner(1), CopperLayer::Inner(2)).kind(),
            ViaKind::Buried
        );
    }
}
