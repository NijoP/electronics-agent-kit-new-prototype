//! EAK-IR: the single canonical engineering representation of a project.
//!
//! Schematic, PCB, 3D, AI and manufacturing all read this one structure. It is only
//! mutated by `eak-runtime`'s command engine.

use crate::geom::{Nm, Point, Rotation};
use crate::library::ComponentDef;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Current on-disk schema version. Bump together with a migration in `migrate.rs`.
pub const SCHEMA_VERSION: u32 = 1;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CopperLayer {
    Top,
    Bottom,
}

impl CopperLayer {
    pub const ALL: [CopperLayer; 2] = [CopperLayer::Top, CopperLayer::Bottom];
    pub fn other(self) -> CopperLayer {
        match self {
            CopperLayer::Top => CopperLayer::Bottom,
            CopperLayer::Bottom => CopperLayer::Top,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stackup {
    pub copper_layers: u8,
    pub board_thickness: Nm,
    /// Outer copper weight in ounces per square foot.
    pub outer_copper_oz: f64,
    pub dielectric: String,
}

impl Default for Stackup {
    fn default() -> Self {
        Stackup {
            copper_layers: 2,
            board_thickness: 1_600_000,
            outer_copper_oz: 1.0,
            dielectric: "FR-4".into(),
        }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Via {
    pub id: Id,
    pub position: Point,
    pub drill: Nm,
    pub diameter: Nm,
    pub net: String,
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
