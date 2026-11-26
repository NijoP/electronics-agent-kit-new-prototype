//! EAK-IR: the single canonical engineering representation of a project.
//!
//! Schematic, PCB, 3D, AI and manufacturing all read this one structure. It is only
//! mutated by `eak-runtime`'s command engine.

use crate::geom::{Nm, Point, Rotation};
use crate::library::ComponentDef;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Current on-disk schema version. Bump together with a migration in `migrate.rs`.
pub const SCHEMA_VERSION: u32 = 8;

pub type Id = String;

/// Allocate a fresh stable object identifier with a type prefix (`sym_`, `wir_`, ...).
///
/// Inside [`with_id_script`] the identifier comes from the script's replay queue for that prefix
/// when it holds one, and every identifier handed out is recorded.
pub fn new_id(prefix: &str) -> Id {
    let scripted = ID_SCRIPT.with(|s| {
        let mut s = s.borrow_mut();
        let script = s.as_mut()?;
        let id = script
            .replay
            .get_mut(prefix)
            .and_then(std::collections::VecDeque::pop_front)
            .unwrap_or_else(|| random_id(prefix));
        script.issued.push(id.clone());
        Some(id)
    });
    scripted.unwrap_or_else(|| random_id(prefix))
}

fn random_id(prefix: &str) -> Id {
    let u = uuid::Uuid::new_v4().simple().to_string();
    format!("{prefix}_{}", &u[..16])
}

struct IdScript {
    replay: BTreeMap<String, std::collections::VecDeque<Id>>,
    issued: Vec<Id>,
}

thread_local! {
    static ID_SCRIPT: std::cell::RefCell<Option<IdScript>> = const { std::cell::RefCell::new(None) };
}

/// Run `f` with [`new_id`] replaying `replay` and recording what it hands out (ADR-0051).
///
/// A proposal is computed on a copy of the project and applied later through
/// `Engine::execute`. Re-running its commands would allocate new random ids, so a later step
/// that names an object an earlier step created (by the id the agent was shown) would miss
/// it. Replaying the ids recorded on the copy makes the re-run allocate the same ones. Ids
/// are taken in order per prefix (the part before the last `_`); a prefix with no recorded id
/// left gets a random one. Returns `f`'s result and every id handed out, in order. Scripts do
/// not nest: an inner call replaces the outer script until it returns.
pub fn with_id_script<T>(replay: &[Id], f: impl FnOnce() -> T) -> (T, Vec<Id>) {
    let mut queues: BTreeMap<String, std::collections::VecDeque<Id>> = BTreeMap::new();
    for id in replay {
        let prefix = id.rsplit_once('_').map_or("", |(p, _)| p);
        queues
            .entry(prefix.into())
            .or_default()
            .push_back(id.clone());
    }
    struct Restore(Option<IdScript>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let outer = self.0.take();
            ID_SCRIPT.with(|s| *s.borrow_mut() = outer);
        }
    }
    let outer = ID_SCRIPT.with(|s| {
        s.borrow_mut().replace(IdScript {
            replay: queues,
            issued: vec![],
        })
    });
    let restore = Restore(outer);
    let out = f();
    let issued = ID_SCRIPT
        .with(|s| {
            s.borrow_mut()
                .as_mut()
                .map(|x| std::mem::take(&mut x.issued))
        })
        .unwrap_or_default();
    drop(restore);
    (out, issued)
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
    /// Free project properties (company, drawn by, ...) that title blocks can show (schema v3).
    /// Omitted from the file when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
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
    /// Sheet drawings and text (schema v3). Never part of connectivity.
    #[serde(default)]
    pub graphics: BTreeMap<Id, SheetGraphic>,
    /// Buses (schema v3). A bus is not a conductor; members connect by name (ADR-0040).
    #[serde(default)]
    pub buses: BTreeMap<Id, Bus>,
    /// Bus entries joining a member wire to a bus (schema v3).
    #[serde(default)]
    pub bus_entries: BTreeMap<Id, BusEntry>,
    /// Object groups (schema v6, ADR-0045): sets of symbols, wires and net labels on one sheet
    /// that select and move together. Never part of connectivity.
    #[serde(default)]
    pub groups: BTreeMap<Id, SchematicGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    pub id: Id,
    pub name: String,
    pub width: Nm,
    pub height: Nm,
    /// Border, zones and title block (schema v3). `None` = no drawing format (every v2 sheet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<DrawingFormat>,
    /// Place in the sheet order (ADR-0043): sheets list by `(order, id)`. Absent (0) in every
    /// file written before sheets could be reordered, so those keep their id order.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub order: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Where a title-block field takes its text from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum FieldSource {
    ProjectName,
    /// The project's committed revision number.
    ProjectRevision,
    SheetName,
    /// 1-based position of the sheet in the project's sheet order.
    SheetNumber,
    SheetCount,
    /// A value of [`Project::properties`]; empty when the property is not set.
    Property {
        key: String,
    },
    /// Fixed text.
    Text {
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleBlockField {
    pub label: String,
    pub value: FieldSource,
}

/// A sheet's drawing format: border, reference zones and title block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawingFormat {
    pub border: bool,
    /// Reference zones along the border: columns are numbered 1.., rows lettered A.. (0 = none).
    pub zone_columns: u8,
    pub zone_rows: u8,
    pub title_block: Vec<TitleBlockField>,
}

/// Most reference-zone columns or rows a border may have.
pub const MAX_ZONES: u8 = 26;

impl DrawingFormat {
    /// Border with 6 x 4 zones and a title block of project name, sheet name, sheet n of m and
    /// revision.
    pub fn standard() -> DrawingFormat {
        let f = |label: &str, value| TitleBlockField {
            label: label.into(),
            value,
        };
        DrawingFormat {
            border: true,
            zone_columns: 6,
            zone_rows: 4,
            title_block: vec![
                f("Title", FieldSource::ProjectName),
                f("Sheet", FieldSource::SheetName),
                f("Sheet no.", FieldSource::SheetNumber),
                f("Sheets", FieldSource::SheetCount),
                f("Revision", FieldSource::ProjectRevision),
            ],
        }
    }

    /// Problems that make this format unusable; empty when it is valid.
    pub fn validate(&self) -> Vec<String> {
        let mut e = Vec::new();
        if self.zone_columns > MAX_ZONES || self.zone_rows > MAX_ZONES {
            e.push(format!("at most {MAX_ZONES} zone columns and rows"));
        }
        if !self.border && (self.zone_columns > 0 || self.zone_rows > 0) {
            e.push("reference zones need a border".into());
        }
        for f in &self.title_block {
            if f.label.trim().is_empty() {
                e.push("title-block field label is empty".into());
            }
            if let FieldSource::Property { key } = &f.value {
                if key.trim().is_empty() {
                    e.push(format!(
                        "title-block field {} binds an empty property key",
                        f.label
                    ));
                }
            }
        }
        e
    }
}

/// A drawing or text placed on a sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetGraphic {
    pub id: Id,
    pub sheet_id: Id,
    pub graphic: crate::graphic::Graphic,
}

/// A bus: a named polyline that groups member nets (`D[0..7]`, `{SDA,SCL}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bus {
    pub id: Id,
    pub sheet_id: Id,
    pub name: String,
    pub points: Vec<Point>,
}

impl Bus {
    /// Whether `p` lies on the bus polyline (within `tol` nm).
    pub fn touches(&self, p: Point, tol: f64) -> bool {
        self.points
            .windows(2)
            .any(|w| crate::geom::point_segment_distance(p, w[0], w[1]) <= tol)
    }
}

/// A named set of schematic objects on one sheet that select and move together (schema v6,
/// ADR-0045). Members are symbol, wire and net-label ids; an object belongs to at most one
/// group. Grouping is an editing aid only: it never changes connectivity or any check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchematicGroup {
    pub id: Id,
    pub sheet_id: Id,
    pub name: String,
    /// Member ids, sorted, at least [`MIN_GROUP_MEMBERS`].
    pub members: Vec<Id>,
}

/// Fewest members a group has; a group that would drop below this is dissolved.
pub const MIN_GROUP_MEMBERS: usize = 2;

impl Schematic {
    /// Sheet of a groupable object (symbol, wire or net label), if `id` names one.
    pub fn groupable_sheet(&self, id: &str) -> Option<&Id> {
        self.symbols
            .get(id)
            .map(|s| &s.sheet_id)
            .or_else(|| self.wires.get(id).map(|w| &w.sheet_id))
            .or_else(|| self.labels.get(id).map(|l| &l.sheet_id))
    }

    /// The group `id` belongs to, if any.
    pub fn group_of(&self, id: &str) -> Option<&SchematicGroup> {
        self.groups
            .values()
            .find(|g| g.members.iter().any(|m| m == id))
    }

    /// Drop group members that no longer exist (deleted objects, deleted sheets) and dissolve
    /// groups left with fewer than [`MIN_GROUP_MEMBERS`]. Returns the dissolved groups' names.
    pub fn prune_groups(&mut self) -> Vec<String> {
        let mut dissolved = vec![];
        let ids: Vec<Id> = self.groups.keys().cloned().collect();
        for gid in ids {
            let mut g = self.groups.remove(&gid).expect("key listed above");
            g.members
                .retain(|m| self.groupable_sheet(m).is_some_and(|s| *s == g.sheet_id));
            if g.members.len() < MIN_GROUP_MEMBERS {
                dissolved.push(g.name);
            } else {
                self.groups.insert(gid, g);
            }
        }
        dissolved
    }
}

/// Tolerance for "point lies on a wire or bus" (schematic coordinates are grid-aligned).
pub const ON_SEGMENT_TOL_NM: f64 = 1_000.0;

/// Joins one member wire to a bus: `bus_point` lies on the bus, a wire attaches at `wire_point`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BusEntry {
    pub id: Id,
    pub sheet_id: Id,
    pub bus_id: Id,
    pub bus_point: Point,
    pub wire_point: Point,
}

/// Most members a bus name may expand to.
pub const MAX_BUS_MEMBERS: usize = 1024;

/// Member net names of a bus name: `D[0..7]` (or `D[7..0]`) gives `D0`..`D7`; `{SDA,SCL}` or
/// `{SDA SCL}` lists them. `None` when the name is neither form, a range is larger than
/// [`MAX_BUS_MEMBERS`], or a member is empty or repeated.
pub fn bus_members(name: &str) -> Option<Vec<String>> {
    let name = name.trim();
    let members: Vec<String> =
        if let Some(inner) = name.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            inner
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        } else {
            let (prefix, rest) = name.split_once('[')?;
            let range = rest.strip_suffix(']')?;
            let (a, b) = range.split_once("..")?;
            let (a, b): (u32, u32) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
            let (lo, hi) = (a.min(b), a.max(b));
            if prefix.trim().is_empty() || (hi - lo) as usize >= MAX_BUS_MEMBERS {
                return None;
            }
            (lo..=hi).map(|i| format!("{}{i}", prefix.trim())).collect()
        };
    let unique: std::collections::BTreeSet<&String> = members.iter().collect();
    if members.is_empty()
        || members.len() > MAX_BUS_MEMBERS
        || unique.len() != members.len()
        || members.iter().any(|m| m.contains(['[', ']', '{', '}']))
    {
        return None;
    }
    Some(members)
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
    /// Mirrored about the symbol's local Y axis (local X negated) before `rotation` is applied
    /// (ADR-0016). Absent in files written before mirroring existed, and omitted when false so
    /// unmirrored projects keep their exact on-disk form.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror: bool,
    /// Per-field visibility that differs from [`SymbolField::visible_by_default`] (schema v3).
    /// Omitted from the file when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub field_visibility: BTreeMap<SymbolField, bool>,
}

/// A text field a placed symbol can show next to its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolField {
    Refdes,
    Value,
    /// The component's display name.
    Name,
    Footprint,
    Manufacturer,
    Mpn,
}

impl SymbolField {
    /// Reference designator and value are shown unless hidden; the others are hidden unless shown.
    pub fn visible_by_default(self) -> bool {
        matches!(self, SymbolField::Refdes | SymbolField::Value)
    }
}

impl SymbolInstance {
    /// Whether `field` is shown on the sheet.
    pub fn field_visible(&self, field: SymbolField) -> bool {
        self.field_visibility
            .get(&field)
            .copied()
            .unwrap_or_else(|| field.visible_by_default())
    }

    /// Sheet position of a point given in the symbol's local (library) coordinates.
    pub fn place(&self, local: Point) -> Point {
        let l = if self.mirror {
            Point::new(-local.x, local.y)
        } else {
            local
        };
        self.position.plus(self.rotation.apply(l))
    }
}

/// Axis of a symbol flip, in sheet coordinates through the symbol origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlipAxis {
    /// Left and right swap (sheet X mirrored).
    Horizontal,
    /// Top and bottom swap (sheet Y mirrored).
    Vertical,
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

/// How far a net label (net port) reaches (ADR-0040).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetScope {
    /// Joins same-name labels on its own sheet only.
    Local,
    /// Joins same-name global, off-page and power labels on every sheet (the v2 behaviour).
    #[default]
    Global,
    /// Off-page connector: connects like a global label and is drawn as a page connector.
    OffPage,
}

impl NetScope {
    pub fn is_global(&self) -> bool {
        *self == NetScope::Global
    }
}

/// Symbol drawn for a power port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerStyle {
    Ground,
    /// Supply rail (bar or arrow).
    Rail,
    Chassis,
}

/// A net label, which is also the net port of schema v3 (scope and power style; ADR-0040).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetLabel {
    pub id: Id,
    pub sheet_id: Id,
    pub name: String,
    pub position: Point,
    #[serde(default)]
    pub kind: LabelKind,
    /// Omitted from the file when global (the v2 behaviour).
    #[serde(default, skip_serializing_if = "NetScope::is_global")]
    pub scope: NetScope,
    /// Drawing style of a power port; `None` lets the renderer choose. Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_style: Option<PowerStyle>,
}

/// Net ports are net labels: one collection names nets (ADR-0040).
pub type NetPort = NetLabel;

impl NetLabel {
    /// Whether this label joins same-name labels on other sheets: global and off-page labels,
    /// and every power port (power flags are global).
    pub fn connects_globally(&self) -> bool {
        self.kind == LabelKind::Power || self.scope != NetScope::Local
    }
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
    /// Arc edges of the outline (schema v4, ADR-0042); every other edge is straight. Omitted
    /// when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outline_arcs: Vec<crate::pcb_objects::OutlineArc>,
}

impl Board {
    /// The outline with its arc edges flattened (chord error ≤ 10 µm): the board shape that DRC,
    /// the zone fill and the edge checks use. Equal to `outline` when no edge is an arc.
    pub fn outline_path(&self) -> Vec<Point> {
        crate::pcb_objects::flatten_outline(&self.outline, &self.outline_arcs)
    }

    /// Sweep of outline edge `edge`, if it is an arc.
    pub fn edge_sweep(&self, edge: usize) -> Option<f64> {
        self.outline_arcs
            .iter()
            .find(|a| a.edge == edge)
            .map(|a| a.sweep_deg)
    }
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
    /// Arc track (schema v4, ADR-0042): the track runs from `a` to `b` along a circular arc of
    /// this sweep in degrees, positive counter-clockwise. `None` = straight; omitted when straight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc_deg: Option<f64>,
}

impl Track {
    /// Centre line: `[a, b]` for a straight track, the flattened arc (chord error ≤ 10 µm) for
    /// an arc track.
    pub fn path(&self) -> Vec<Point> {
        match self.arc_deg {
            Some(s) => crate::pcb_objects::arc_points(
                self.a,
                self.b,
                s,
                crate::pcb_objects::ARC_FLATTEN_ERR_NM,
            ),
            None => vec![self.a, self.b],
        }
    }

    /// Centre-line length in nanometres (the true arc length for an arc track).
    pub fn length(&self) -> f64 {
        match self.arc_deg {
            Some(s) => crate::pcb_objects::arc_length(self.a, self.b, s),
            None => self.a.dist(self.b),
        }
    }
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
    /// Keep-out and constraint regions (schema v4, ADR-0042).
    #[serde(default)]
    pub regions: BTreeMap<Id, crate::pcb_objects::Region>,
    /// Text on silk or documentation layers (schema v4).
    #[serde(default)]
    pub texts: BTreeMap<Id, crate::pcb_objects::PcbText>,
    /// Linear dimensions on silk or documentation layers (schema v4).
    #[serde(default)]
    pub dimensions: BTreeMap<Id, crate::pcb_objects::PcbDimension>,
    /// Holes milled through the board (schema v4).
    #[serde(default)]
    pub cutouts: BTreeMap<Id, crate::pcb_objects::Cutout>,
    /// Interactive-router options saved with the board (schema v5, ADR-0044).
    #[serde(default)]
    pub routing: RoutingOptions,
}

/// Corner style of an interactively routed path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BendMode {
    /// Horizontal, vertical and 45-degree segments.
    #[default]
    Diagonal45,
    /// Horizontal and vertical segments only.
    Orthogonal90,
}

/// How the interactive router treats other-net copper in the way of the direct path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictMode {
    /// Go around obstacles (local A* walkaround), else stop before them.
    #[default]
    Walkaround,
    /// Stop before the first obstacle (no walkaround).
    Block,
    /// Push (shove) other-net tracks out of the direct path, keeping their ends and every
    /// clearance; when that is not possible, walk around as in `Walkaround`.
    Push,
}

/// A via size chosen for a layer switch (a preset or the net class).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViaSize {
    pub drill: Nm,
    pub diameter: Nm,
}

/// The route tool's options as the user last set them (schema v5, ADR-0044). They change
/// only through `SetRoutingOptions`; the router itself takes them per request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingOptions {
    #[serde(default)]
    pub bend: BendMode,
    #[serde(default)]
    pub conflict: ConflictMode,
    /// Track width for new routes; `None` uses the net-class width.
    #[serde(default)]
    pub width: Option<Nm>,
    /// Via for layer switches; `None` uses the net-class via.
    #[serde(default)]
    pub via: Option<ViaSize>,
    /// Delete the redundant track chain a finished route makes into a loop.
    #[serde(default = "yes")]
    pub remove_loops: bool,
}

fn yes() -> bool {
    true
}

impl Default for RoutingOptions {
    fn default() -> Self {
        RoutingOptions {
            bend: BendMode::default(),
            conflict: ConflictMode::default(),
            width: None,
            via: None,
            remove_loops: true,
        }
    }
}

impl RoutingOptions {
    /// Why these options cannot be saved, if they are invalid.
    pub fn problem(&self) -> Option<String> {
        if self.width.is_some_and(|w| w <= 0) {
            return Some("route width must be positive".into());
        }
        if let Some(v) = self.via {
            if v.drill <= 0 || v.diameter <= v.drill {
                return Some("the via diameter must be larger than its positive drill".into());
            }
        }
        None
    }
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
    /// Single-ended target impedance (ohm) of the class (schema v8, ADR-0055); `None` = no
    /// impedance requirement declared, so impedance checks are UNKNOWN, never a pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_z0_ohm: Option<f64>,
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
    /// Per-rule enable flag and severity override, by rule id (schema v7, ADR-0046 / AD-8).
    /// A rule without an entry runs with its default severity.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rules: BTreeMap<String, RuleConfig>,
    /// Object-class x object-class minimum clearances (schema v7, Phase 1 C5). One entry per
    /// unordered pair; DRC applies the larger of a cell and the other clearance rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clearance_matrix: Vec<ClearanceCell>,
    /// Minimum clearance between two named nets (schema v7, Phase 1 C5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub net_pair_rules: Vec<NetPairRule>,
    /// Declared differential pairs (schema v8, ADR-0055).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diff_pairs: Vec<DiffPairRule>,
    /// Via layer spans the router may use besides through vias (schema v8, ADR-0055); empty =
    /// through vias only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub via_spans: Vec<ViaSpanRule>,
    /// Engineering-check inputs (schema v8, ADR-0055), set together by `SetEngineeringInputs`.
    /// Absent = not declared: a check that needs one reports UNKNOWN or BLOCKED, never a pass.
    /// Ambient temperature (°C) for thermal and junction estimates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ambient_temp_c: Option<f64>,
    /// Via barrel plating thickness (µm) for via current capacity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_plating_um: Option<f64>,
    /// Largest allowed voltage drop per net (V).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub net_max_drop_v: BTreeMap<String, f64>,
    /// Declared power dissipation per part (W), by reference designator.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub part_dissipation_w: BTreeMap<String, f64>,
    /// Insulation coordination basis for creepage and clearance checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insulation: Option<Insulation>,
    /// Enclosure limits for mechanical checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosure: Option<Enclosure>,
}

/// Report severity a rule override may set (EasyEDA's ladder maps onto it: Fatal and Error
/// -> `Error`, Note -> `Info`; Ignore is `enabled: false`). Mirrors `eak_validation::Severity`,
/// which eak-core cannot depend on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSeverity {
    Info,
    Warning,
    Error,
}

/// Rule families and how many rules each declares (`ERC-001` .. `ERC-021`, ...). Kept equal to
/// the checks' rule tables by an eak-validation test, so the engine can refuse a configuration for
/// a rule that does not exist.
pub const RULE_FAMILIES: [(&str, u32); 5] = [
    ("ERC", 24),
    ("DRC", 26),
    ("DFM", 10),
    ("MECH", 4),
    ("SRC", 12),
];

/// Whether some check declares `id` (see [`RULE_FAMILIES`]).
pub fn known_rule_id(id: &str) -> bool {
    let Some((family, n)) = id.split_once('-') else {
        return false;
    };
    let Some(&(_, count)) = RULE_FAMILIES.iter().find(|(f, _)| *f == family) else {
        return false;
    };
    n.len() == 3
        && n.bytes().all(|b| b.is_ascii_digit())
        && n.parse::<u32>().is_ok_and(|n| (1..=count).contains(&n))
}

fn enabled_default() -> bool {
    true
}

/// One rule's project configuration. A disabled rule is reported as disabled, never as passing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleConfig {
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<RuleSeverity>,
}

/// Object classes of the clearance matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectClass {
    Track,
    SmdPad,
    ThPad,
    Via,
    Zone,
    Hole,
    Outline,
    Silk,
}

impl ObjectClass {
    pub const ALL: [ObjectClass; 8] = [
        ObjectClass::Track,
        ObjectClass::SmdPad,
        ObjectClass::ThPad,
        ObjectClass::Via,
        ObjectClass::Zone,
        ObjectClass::Hole,
        ObjectClass::Outline,
        ObjectClass::Silk,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ObjectClass::Track => "track",
            ObjectClass::SmdPad => "SMD pad",
            ObjectClass::ThPad => "TH pad",
            ObjectClass::Via => "via",
            ObjectClass::Zone => "zone",
            ObjectClass::Hole => "hole",
            ObjectClass::Outline => "board outline",
            ObjectClass::Silk => "silkscreen",
        }
    }
}

/// One clearance-matrix cell: the minimum distance between objects of classes `a` and `b`.
/// Stored with `a <= b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearanceCell {
    pub a: ObjectClass,
    pub b: ObjectClass,
    pub clearance: Nm,
}

/// Minimum clearance between copper of two named nets. Stored with `net_a < net_b`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetPairRule {
    pub net_a: String,
    pub net_b: String,
    pub clearance: Nm,
}

/// A declared differential pair (schema v8, ADR-0055). Every figure is optional: an absent one
/// is undeclared, and a check that needs it reports UNKNOWN (never a pass).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffPairRule {
    pub positive: String,
    pub negative: String,
    /// Track width of each member.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Nm>,
    /// Edge-to-edge gap between the members where they run coupled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<Nm>,
    /// Target differential impedance (ohm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_zdiff_ohm: Option<f64>,
    /// Allowed deviation from the target, in percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zdiff_tolerance_pct: Option<f64>,
    /// Largest allowed length difference between the members.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_skew: Option<Nm>,
}

impl DiffPairRule {
    pub fn has(&self, net: &str) -> bool {
        self.positive == net || self.negative == net
    }
}

/// A via layer span the router may use (schema v8, ADR-0055).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViaSpanRule {
    pub from: CopperLayer,
    pub to: CopperLayer,
    pub drill: Nm,
    pub diameter: Nm,
}

/// Standard an insulation (creepage / clearance) check is made against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(non_camel_case_types)] // the variant spells the standard's own number
pub enum InsulationStandard {
    Ipc2221b,
    Iec60664_1,
}

/// Insulation coordination basis (schema v8, ADR-0055).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Insulation {
    pub standard: InsulationStandard,
    /// 1..=4 (IEC 60664-1 pollution degree).
    pub pollution_degree: u8,
    /// Material group by CTI: one of [`MATERIAL_GROUPS`].
    pub material_group: String,
    pub altitude_m: f64,
    /// Conformal coating over the conductors.
    pub coated: bool,
}

/// Material groups `Insulation::material_group` may name.
pub const MATERIAL_GROUPS: [&str; 4] = ["I", "II", "IIIa", "IIIb"];

/// A mounting hole the enclosure requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountingHoleSpec {
    pub position: Point,
    pub drill: Nm,
    /// Allowed position error.
    pub tolerance: Nm,
    /// Copper and component keep-out radius around the hole centre.
    pub keepout_radius: Nm,
}

/// Enclosure limits (schema v8, ADR-0055); the top-side height limit stays
/// `Constraints::max_component_height`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Enclosure {
    /// Tallest part allowed on the bottom side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height_bottom: Option<Nm>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounting_holes: Vec<MountingHoleSpec>,
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
                target_z0_ohm: None,
            },
            net_classes: vec![],
            net_currents_a: BTreeMap::new(),
            net_voltages_v: BTreeMap::new(),
            temp_rise_c: 10.0,
            max_component_height: None,
            rules: BTreeMap::new(),
            clearance_matrix: vec![],
            net_pair_rules: vec![],
            diff_pairs: vec![],
            via_spans: vec![],
            ambient_temp_c: None,
            via_plating_um: None,
            net_max_drop_v: BTreeMap::new(),
            part_dissipation_w: BTreeMap::new(),
            insulation: None,
            enclosure: None,
        }
    }
}

impl Constraints {
    /// The configured cell for an unordered class pair, if any.
    pub fn matrix_cell(&self, a: ObjectClass, b: ObjectClass) -> Option<Nm> {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        self.clearance_matrix
            .iter()
            .find(|c| c.a == a && c.b == b)
            .map(|c| c.clearance)
    }

    /// The configured clearance between two nets, if any (order does not matter).
    pub fn net_pair_clearance(&self, x: &str, y: &str) -> Option<Nm> {
        self.net_pair_rules
            .iter()
            .find(|r| (r.net_a == x && r.net_b == y) || (r.net_a == y && r.net_b == x))
            .map(|r| r.clearance)
    }

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
                format: None,
                order: 0,
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
            properties: BTreeMap::new(),
        }
    }

    /// 1-based position of a sheet in the project's sheet order, if it exists.
    pub fn sheet_number(&self, sheet_id: &str) -> Option<usize> {
        self.sheets_in_order()
            .iter()
            .position(|s| s.id == sheet_id)
            .map(|i| i + 1)
    }

    /// The sheets in the project's sheet order: by `order`, then id (ADR-0043).
    pub fn sheets_in_order(&self) -> Vec<&Sheet> {
        let mut v: Vec<&Sheet> = self.schematic.sheets.values().collect();
        v.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
        v
    }

    /// Text a title-block field shows on `sheet`.
    pub fn title_block_value(&self, sheet: &Sheet, source: &FieldSource) -> String {
        match source {
            FieldSource::ProjectName => self.name.clone(),
            FieldSource::ProjectRevision => self.revision.to_string(),
            FieldSource::SheetName => sheet.name.clone(),
            FieldSource::SheetNumber => self
                .sheet_number(&sheet.id)
                .map(|n| n.to_string())
                .unwrap_or_default(),
            FieldSource::SheetCount => self.schematic.sheets.len().to_string(),
            FieldSource::Property { key } => self.properties.get(key).cloned().unwrap_or_default(),
            FieldSource::Text { text } => text.clone(),
        }
    }

    pub fn first_sheet_id(&self) -> Option<&Id> {
        self.sheets_in_order().into_iter().next().map(|s| &s.id)
    }

    /// Absolute position of a symbol pin.
    pub fn pin_position(&self, sym: &SymbolInstance, pin: &str) -> Option<Point> {
        let def = self.library.get(&sym.component_id)?;
        let p = def.pin(pin)?;
        Some(sym.place(p.position))
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

#[cfg(test)]
mod id_script_tests {
    use super::*;

    #[test]
    fn a_script_replays_recorded_ids_per_prefix_and_records_what_it_hands_out() {
        let ((), first) = with_id_script(&[], || {
            new_id("sym");
            new_id("wir");
            new_id("sym");
        });
        assert_eq!(first.len(), 3);
        // Replayed in order per prefix; an interleaved other prefix does not shift them, and a
        // prefix with nothing recorded gets a fresh id.
        let (ids, second) = with_id_script(&first, || {
            [new_id("op"), new_id("sym"), new_id("sym"), new_id("wir")]
        });
        assert_eq!(ids[1], first[0]);
        assert_eq!(ids[2], first[2]);
        assert_eq!(ids[3], first[1]);
        assert!(ids[0].starts_with("op_") && !first.contains(&ids[0]));
        assert_eq!(second, ids.to_vec());
        // Exhausted queue: random again. Outside a script nothing is recorded or replayed.
        let ((), third) = with_id_script(&first[..1], || {
            new_id("sym");
            new_id("sym");
        });
        assert_eq!(third[0], first[0]);
        assert_ne!(third[1], first[2]);
        assert_ne!(new_id("sym"), first[0]);
    }

    #[test]
    fn the_script_is_removed_even_when_the_closure_panics() {
        let r = std::panic::catch_unwind(|| {
            let replay = ["sym_0000000000000001".to_string()];
            with_id_script(&replay, || -> u8 { panic!("boom") })
        });
        assert!(r.is_err());
        assert_ne!(new_id("sym"), "sym_0000000000000001");
    }
}

#[cfg(test)]
mod schematic_v3_tests {
    use super::*;

    #[test]
    fn bus_names_expand_to_members() {
        let s = |v: &[&str]| Some(v.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert_eq!(bus_members("D[0..3]"), s(&["D0", "D1", "D2", "D3"]));
        assert_eq!(bus_members("A[2..0]"), s(&["A0", "A1", "A2"]));
        assert_eq!(bus_members("{SDA,SCL}"), s(&["SDA", "SCL"]));
        assert_eq!(bus_members("{ SDA SCL INT }"), s(&["SDA", "SCL", "INT"]));
        for bad in [
            "D",
            "D[0..]",
            "[0..3]",
            "D[a..b]",
            "{}",
            "{A,A}",
            "D[0..5000]",
            "{A,B[1]}",
        ] {
            assert_eq!(bus_members(bad), None, "{bad}");
        }
    }

    #[test]
    fn v2_label_and_instance_json_load_with_v2_meaning_and_stay_compact() {
        let l: NetLabel = serde_json::from_str(
            r#"{"id":"l","sheet_id":"s","name":"VCC","position":{"x":0,"y":0},"kind":"power"}"#,
        )
        .unwrap();
        assert_eq!((l.scope, l.power_style), (NetScope::Global, None));
        assert!(l.connects_globally());
        let v = serde_json::to_value(&l).unwrap();
        assert!(v.get("scope").is_none() && v.get("power_style").is_none());
        let local = NetLabel {
            scope: NetScope::Local,
            ..l.clone()
        };
        assert!(local.connects_globally(), "power ports are always global");
        let local_net = NetLabel {
            kind: LabelKind::Net,
            ..local
        };
        assert!(!local_net.connects_globally());
        assert_eq!(serde_json::to_value(&local_net).unwrap()["scope"], "local");
    }

    #[test]
    fn field_visibility_defaults_and_overrides() {
        let mut s = SymbolInstance {
            id: "s".into(),
            sheet_id: "sh".into(),
            component_id: "c".into(),
            refdes: "R1".into(),
            value: "1k".into(),
            position: Point::default(),
            rotation: Rotation::R0,
            mirror: false,
            field_visibility: BTreeMap::new(),
        };
        assert!(s.field_visible(SymbolField::Refdes) && s.field_visible(SymbolField::Value));
        assert!(!s.field_visible(SymbolField::Mpn));
        assert!(serde_json::to_value(&s)
            .unwrap()
            .get("field_visibility")
            .is_none());
        s.field_visibility.insert(SymbolField::Value, false);
        s.field_visibility.insert(SymbolField::Mpn, true);
        assert!(!s.field_visible(SymbolField::Value) && s.field_visible(SymbolField::Mpn));
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(
            v["field_visibility"],
            serde_json::json!({"value": false, "mpn": true})
        );
        assert_eq!(serde_json::from_value::<SymbolInstance>(v).unwrap(), s);
    }

    #[test]
    fn title_block_fields_resolve_against_the_project() {
        let mut p = Project::new("Board");
        p.revision = 7;
        p.properties.insert("company".into(), "ACME".into());
        let sheet = p.schematic.sheets.values().next().unwrap().clone();
        let v = |s: FieldSource| p.title_block_value(&sheet, &s);
        assert_eq!(v(FieldSource::ProjectName), "Board");
        assert_eq!(v(FieldSource::ProjectRevision), "7");
        assert_eq!(v(FieldSource::SheetName), "Sheet 1");
        assert_eq!(v(FieldSource::SheetNumber), "1");
        assert_eq!(v(FieldSource::SheetCount), "1");
        assert_eq!(
            v(FieldSource::Property {
                key: "company".into()
            }),
            "ACME"
        );
        assert_eq!(v(FieldSource::Property { key: "nope".into() }), "");
        assert_eq!(v(FieldSource::Text { text: "t".into() }), "t");
        assert!(DrawingFormat::standard().validate().is_empty());
        let mut bad = DrawingFormat::standard();
        bad.border = false;
        bad.zone_columns = 30;
        bad.title_block.push(TitleBlockField {
            label: "Co".into(),
            value: FieldSource::Property { key: " ".into() },
        });
        assert_eq!(bad.validate().len(), 3, "{:?}", bad.validate());
    }

    #[test]
    fn a_bus_touches_points_on_its_segments() {
        let b = Bus {
            id: "b".into(),
            sheet_id: "s".into(),
            name: "D[0..1]".into(),
            points: vec![
                Point::new(0, 0),
                Point::new(10_000, 0),
                Point::new(10_000, 10_000),
            ],
        };
        assert!(b.touches(Point::new(5_000, 0), ON_SEGMENT_TOL_NM));
        assert!(b.touches(Point::new(10_000, 4_000), ON_SEGMENT_TOL_NM));
        assert!(!b.touches(Point::new(5_000, 5_000), ON_SEGMENT_TOL_NM));
    }

    #[test]
    fn sheets_list_by_order_then_id_and_order_0_is_not_written() {
        let mut p = Project::new("o");
        let first = p.first_sheet_id().unwrap().clone();
        let mk = |id: &str, order: u32| Sheet {
            id: id.into(),
            name: id.into(),
            width: 1,
            height: 1,
            format: None,
            order,
        };
        p.schematic.sheets.insert("a".into(), mk("a", 0));
        p.schematic.sheets.insert("z".into(), mk("z", 0));
        // Every sheet at order 0 (any file from before reordering): id order.
        let ids = |p: &Project| {
            p.sheets_in_order()
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&p), vec!["a".to_string(), first.clone(), "z".into()]);
        assert!(!crate::to_json(&p).contains("\"order\""));
        p.schematic.sheets.get_mut("z").unwrap().order = 1;
        p.schematic.sheets.get_mut(&first).unwrap().order = 2;
        p.schematic.sheets.get_mut("a").unwrap().order = 3;
        assert_eq!(ids(&p), vec!["z".to_string(), first.clone(), "a".into()]);
        assert_eq!(p.first_sheet_id(), Some(&"z".to_string()));
        assert_eq!(p.sheet_number("a"), Some(3));
        let q = crate::from_json(&crate::to_json(&p)).unwrap();
        assert_eq!(p, q);
    }
}
