//! Typed design commands. The UI and the AI agent submit exactly these values; there is
//! no other mutation path into EAK-IR.

use eak_core::geom::{point_segment_distance, polygon_self_intersects};
use eak_core::*;
use eak_engineering::{netlist, placer, router};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Command {
    // ---- project
    SetProjectName {
        name: String,
    },
    AddRequirement {
        text: String,
    },
    // ---- library
    /// Copy a built-in library component definition into the project library.
    ImportLibraryComponent {
        component_id: String,
    },
    /// Add or replace a user-authored component definition (symbol + footprint editor output).
    UpsertComponentDefinition {
        definition: Box<ComponentDef>,
    },
    /// Edit the supplier-facing properties of a project library component. `None` (or a blank
    /// string) clears a field. Provenance, ratings and geometry are never touched.
    SetComponentProperties {
        component_id: String,
        manufacturer: Option<String>,
        mpn: Option<String>,
        distributor: Option<String>,
        datasheet: Option<String>,
    },
    /// Assign a different footprint (with its symbol-pin -> pad map) to a project library
    /// component. Refused while PCB footprints of the component exist (re-sync first).
    AssignComponentFootprint {
        component_id: String,
        footprint: FootprintDef,
        pin_map: BTreeMap<String, Vec<String>>,
    },
    // ---- schematic
    AddSheet {
        name: String,
    },
    PlaceSymbol {
        sheet_id: Option<Id>,
        component_id: String,
        position: Point,
        #[serde(default)]
        rotation: Rotation,
        #[serde(default)]
        refdes: Option<String>,
        #[serde(default)]
        value: Option<String>,
    },
    MoveSymbol {
        symbol_id: Id,
        position: Point,
    },
    RotateSymbol {
        symbol_id: Id,
        rotation: Rotation,
    },
    /// Mirror a symbol about a sheet axis through its origin (ADR-0016). Pins move with the body;
    /// attached wires are not dragged (send a Batch with MoveWire steps first, as for a move).
    FlipSymbol {
        symbol_id: Id,
        axis: FlipAxis,
    },
    SetSymbolValue {
        symbol_id: Id,
        value: String,
    },
    SetRefdes {
        symbol_id: Id,
        refdes: String,
    },
    DeleteSymbol {
        symbol_id: Id,
    },
    AddWire {
        sheet_id: Option<Id>,
        a: Point,
        b: Point,
    },
    DeleteWire {
        wire_id: Id,
    },
    /// Set both end points of a wire (whole-wire moves and rubber-banding when a selection is
    /// dragged; ADR-0013).
    MoveWire {
        wire_id: Id,
        a: Point,
        b: Point,
    },
    /// Draw an L-shaped wire between two pins, refusing routes that would touch other pins.
    ConnectPins {
        a_symbol: Id,
        a_pin: String,
        b_symbol: Id,
        b_pin: String,
    },
    AddNetLabel {
        sheet_id: Option<Id>,
        name: String,
        position: Point,
        #[serde(default)]
        kind: LabelKind,
    },
    /// Place a label directly on a pin's connection point.
    LabelPin {
        symbol_id: Id,
        pin: String,
        name: String,
        #[serde(default)]
        kind: LabelKind,
    },
    DeleteNetLabel {
        label_id: Id,
    },
    MoveNetLabel {
        label_id: Id,
        position: Point,
    },
    SetNoConnect {
        symbol_id: Id,
        pin: String,
    },
    /// Assign reference designators to symbols ending in '?'.
    Annotate,
    // ---- PCB
    SetBoardOutline {
        points: Vec<Point>,
    },
    SetStackup {
        stackup: Stackup,
    },
    /// Rebuild the stackup as a default `count`-layer board (even, 2..=16): outer copper 1 oz,
    /// inner 0.5 oz, first and last inner layers planes. Keeps the declared board thickness
    /// unless `board_thickness` is given. Refused if existing tracks, vias or zones would be left
    /// on a layer that no longer exists.
    SetLayerCount {
        count: u8,
        #[serde(default)]
        board_thickness: Option<Nm>,
    },
    /// Forward-annotate: create/update/remove footprints and pad nets from the schematic.
    SyncSchematicToPcb,
    MoveFootprint {
        footprint_id: Id,
        position: Point,
        #[serde(default)]
        rotation: Option<Rotation>,
        #[serde(default)]
        side: Option<Side>,
    },
    SetFootprintLocked {
        footprint_id: Id,
        locked: bool,
    },
    AddTrack {
        layer: CopperLayer,
        a: Point,
        b: Point,
        #[serde(default)]
        width: Option<Nm>,
        net: String,
    },
    DeleteTrack {
        track_id: Id,
    },
    AddVia {
        position: Point,
        net: String,
        #[serde(default)]
        drill: Option<Nm>,
        #[serde(default)]
        diameter: Option<Nm>,
        /// First layer of the barrel; omit for a through via (top to bottom).
        #[serde(default)]
        from: Option<CopperLayer>,
        /// Last layer of the barrel; omit for a through via.
        #[serde(default)]
        to: Option<CopperLayer>,
    },
    DeleteVia {
        via_id: Id,
    },
    AddZone {
        layer: CopperLayer,
        net: String,
        outline: Vec<Point>,
        clearance: Option<Nm>,
    },
    DeleteZone {
        zone_id: Id,
    },
    AutoPlace {
        #[serde(default)]
        grid: Option<Nm>,
    },
    AutoRoute {
        #[serde(default)]
        nets: Option<Vec<String>>,
    },
    /// Remove all tracks and vias (optionally of one net).
    ClearRouting {
        #[serde(default)]
        net: Option<String>,
    },
    /// Stitch copper pours (DESK `stitch_orphans`): an escape via and pad-to-via track for each
    /// SMD pad of the zone net its own layer's fill does not reach, and a stitching via for each
    /// fill island with no via. Needs a zone of the net on two or more layers.
    StitchZones {
        #[serde(default)]
        net: Option<String>,
    },
    // ---- constraints / manufacturing
    SetDesignRules {
        min_track_width: Option<Nm>,
        min_clearance: Option<Nm>,
        min_drill: Option<Nm>,
        min_annular_ring: Option<Nm>,
        board_edge_clearance: Option<Nm>,
        default_track_width: Option<Nm>,
        default_clearance: Option<Nm>,
        temp_rise_c: Option<f64>,
        max_component_height: Option<Nm>,
    },
    UpsertNetClass {
        class: NetClass,
    },
    SetNetCurrent {
        net: String,
        amps: Option<f64>,
    },
    SetNetVoltage {
        net: String,
        volts: Option<f64>,
    },
    SetManufacturing {
        config: ManufacturingConfig,
    },
    // ---- compound
    /// Apply several commands in order as one transaction: all succeed or nothing changes,
    /// and the result is one revision and one undo step (ADR-0013). Batches do not nest.
    Batch {
        commands: Vec<Command>,
        /// Human-readable description for the history (e.g. "Move 3 objects").
        #[serde(default)]
        label: Option<String>,
    },
}

/// Most commands a single `Batch` may carry.
pub const MAX_BATCH: usize = 1000;

/// Risk class driving the approval policy (Spec §6.1; Execution Instructions §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationClass {
    ReadOnly,
    ReversibleLocal,
    EngineeringImpacting,
    Destructive,
}

impl Command {
    pub fn name(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or_default()
    }

    pub fn class(&self) -> OperationClass {
        use Command::*;
        use OperationClass::*;
        match self {
            SetProjectName { .. }
            | AddRequirement { .. }
            | ImportLibraryComponent { .. }
            | SetComponentProperties { .. }
            | AddSheet { .. }
            | PlaceSymbol { .. }
            | MoveSymbol { .. }
            | RotateSymbol { .. }
            | FlipSymbol { .. }
            | AddNetLabel { .. }
            | MoveFootprint { .. }
            | SetFootprintLocked { .. }
            | Annotate
            | SetSymbolValue { .. }
            | SetRefdes { .. } => ReversibleLocal,
            AddWire { .. }
            | ConnectPins { .. }
            | LabelPin { .. }
            | SetNoConnect { .. }
            | DeleteWire { .. }
            | DeleteNetLabel { .. }
            | MoveWire { .. }
            | MoveNetLabel { .. }
            | UpsertComponentDefinition { .. }
            | AssignComponentFootprint { .. }
            | SetBoardOutline { .. }
            | SetStackup { .. }
            | SetLayerCount { .. }
            | SyncSchematicToPcb
            | AddTrack { .. }
            | DeleteTrack { .. }
            | AddVia { .. }
            | DeleteVia { .. }
            | AddZone { .. }
            | DeleteZone { .. }
            | AutoPlace { .. }
            | AutoRoute { .. }
            | StitchZones { .. }
            | SetDesignRules { .. }
            | UpsertNetClass { .. }
            | SetNetCurrent { .. }
            | SetNetVoltage { .. }
            | SetManufacturing { .. } => EngineeringImpacting,
            DeleteSymbol { .. } | ClearRouting { .. } => Destructive,
            // A batch carries the risk of its riskiest step, so approval covers every step.
            Batch { commands, .. } => commands
                .iter()
                .map(Command::class)
                .max()
                .unwrap_or(ReversibleLocal),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "code", content = "message")]
pub enum CommandError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid argument: {0}")]
    Invalid(String),
    #[error("precondition failed: {0}")]
    Precondition(String),
    #[error("stale revision: command expected {expected}, project is at {actual}")]
    StaleRevision { expected: u64, actual: u64 },
    #[error("approval required: {0}")]
    ApprovalRequired(String),
    #[error("integrity violation after apply: {0}")]
    Integrity(String),
    #[error("storage failure: {0}")]
    Storage(String),
    #[error("nothing to {0}")]
    NothingTo(String),
}

pub struct Applied {
    pub created: Vec<Id>,
    pub warnings: Vec<String>,
}

/// Refuse a layer the board's stackup does not have.
fn require_layer(p: &Project, layer: CopperLayer, what: &str) -> Result<(), CommandError> {
    if p.pcb.board.stackup.contains(layer) {
        Ok(())
    } else {
        Err(CommandError::Invalid(format!(
            "{what} is on {}, which this board's stackup ({} copper layers) does not have",
            layer.name(),
            p.pcb.board.stackup.copper_layers()
        )))
    }
}

/// Objects (tracks, vias, zones) that sit on a copper layer `stackup` does not contain.
fn orphaned_by(p: &Project, stackup: &Stackup) -> Vec<String> {
    let mut out = Vec::new();
    for t in p.pcb.tracks.values() {
        if !stackup.contains(t.layer) {
            out.push(format!("track {} on {}", t.id, t.layer.name()));
        }
    }
    for z in p.pcb.zones.values() {
        if !stackup.contains(z.layer) {
            out.push(format!("zone {} on {}", z.id, z.layer.name()));
        }
    }
    for v in p.pcb.vias.values() {
        if !stackup.contains(v.from) || !stackup.contains(v.to) {
            out.push(format!("via {} ({}→{})", v.id, v.from.name(), v.to.name()));
        }
    }
    out
}

fn set_stackup(p: &mut Project, stackup: &Stackup) -> Result<(), CommandError> {
    let problems = stackup.validate();
    if !problems.is_empty() {
        return Err(CommandError::Invalid(problems.join("; ")));
    }
    let orphans = orphaned_by(p, stackup);
    if !orphans.is_empty() {
        return Err(CommandError::Precondition(format!(
            "the new stackup removes layers still in use: {}; delete or move them first",
            orphans.join(", ")
        )));
    }
    p.pcb.board.stackup = stackup.clone();
    Ok(())
}

fn sheet_or_first(p: &Project, s: &Option<Id>) -> Result<Id, CommandError> {
    match s {
        Some(id) if p.schematic.sheets.contains_key(id) => Ok(id.clone()),
        Some(id) => Err(CommandError::NotFound(format!("sheet {id}"))),
        None => p
            .first_sheet_id()
            .cloned()
            .ok_or_else(|| CommandError::Precondition("project has no sheet".into())),
    }
}

fn symbol<'a>(p: &'a Project, id: &str) -> Result<&'a SymbolInstance, CommandError> {
    p.schematic
        .symbols
        .get(id)
        .ok_or_else(|| CommandError::NotFound(format!("symbol {id}")))
}

fn next_refdes(p: &Project, prefix: &str) -> String {
    let mut n = 1;
    loop {
        let r = format!("{prefix}{n}");
        if !p.schematic.symbols.values().any(|s| s.refdes == r) {
            return r;
        }
        n += 1;
    }
}

fn ensure_component(
    p: &mut Project,
    id: &str,
    created_warn: &mut Vec<String>,
) -> Result<(), CommandError> {
    if p.library.contains_key(id) {
        return Ok(());
    }
    match eak_core::builtin::get(id) {
        Some(def) => {
            created_warn.push(format!("imported library component {id} into the project"));
            p.library.insert(id.into(), def);
            Ok(())
        }
        None => Err(CommandError::NotFound(format!(
            "component {id} (not in project or built-in library)"
        ))),
    }
}

/// Points on a sheet that a new wire must not pass through by accident.
fn connection_points(p: &Project, sheet: &str) -> Vec<Point> {
    let mut v = vec![];
    for s in p.schematic.symbols.values().filter(|s| s.sheet_id == sheet) {
        if let Some(def) = p.library.get(&s.component_id) {
            for pin in &def.symbol.pins {
                v.push(s.place(pin.position));
            }
        }
    }
    v
}

/// Absolute connection points of one symbol's pins.
fn connection_points_of(p: &Project, s: &SymbolInstance) -> Vec<Point> {
    p.library
        .get(&s.component_id)
        .map(|def| {
            def.symbol
                .pins
                .iter()
                .map(|pin| s.place(pin.position))
                .collect()
        })
        .unwrap_or_default()
}

fn passes_through(a: Point, b: Point, pts: &[Point], allowed: &[Point]) -> Vec<Point> {
    pts.iter()
        .filter(|q| !allowed.contains(q) && point_segment_distance(**q, a, b) <= 1_000.0)
        .copied()
        .collect()
}

pub fn apply(p: &mut Project, cmd: &Command) -> Result<Applied, CommandError> {
    use Command::*;
    let mut created = vec![];
    let mut warnings = vec![];
    match cmd {
        SetProjectName { name } => {
            if name.trim().is_empty() {
                return Err(CommandError::Invalid("project name is empty".into()));
            }
            p.name = name.trim().into();
        }
        AddRequirement { text } => {
            let id = new_id("req");
            p.requirements.push(Requirement {
                id: id.clone(),
                text: text.clone(),
            });
            created.push(id);
        }
        ImportLibraryComponent { component_id } => {
            if p.library.contains_key(component_id) {
                return Err(CommandError::NothingTo(format!(
                    "import: {component_id} is already in the project"
                )));
            }
            ensure_component(p, component_id, &mut warnings)?;
        }
        UpsertComponentDefinition { definition } => {
            let problems = definition.validate();
            if !problems.is_empty() {
                return Err(CommandError::Invalid(problems.join("; ")));
            }
            if let Some(old) = p.library.get(&definition.id) {
                // Existing instances must keep valid pins.
                for s in p
                    .schematic
                    .symbols
                    .values()
                    .filter(|s| s.component_id == definition.id)
                {
                    for pin in &old.symbol.pins {
                        if definition.pin(&pin.number).is_none() {
                            return Err(CommandError::Precondition(format!(
                                "{} uses pin {} which the new definition removes",
                                s.refdes, pin.number
                            )));
                        }
                    }
                }
            }
            p.library
                .insert(definition.id.clone(), (**definition).clone());
        }
        SetComponentProperties {
            component_id,
            manufacturer,
            mpn,
            distributor,
            datasheet,
        } => {
            let def = p.library.get_mut(component_id).ok_or_else(|| {
                CommandError::NotFound(format!("component {component_id} in the project library"))
            })?;
            let clean = |v: &Option<String>| {
                v.as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
            };
            def.manufacturer = clean(manufacturer);
            def.mpn = clean(mpn);
            let new_distributor = clean(distributor);
            if new_distributor != def.distributor {
                // Keep the offer list in step; a different part number drops the old offer's
                // stock/price, which described another part.
                def.supplier_offers.retain(|o| o.supplier != "distributor");
                def.distributor = new_distributor;
                def.migrate_offers();
            }
            def.datasheet = clean(datasheet);
        }
        AssignComponentFootprint {
            component_id,
            footprint,
            pin_map,
        } => {
            if p.pcb
                .footprints
                .values()
                .any(|f| f.component_id == *component_id)
            {
                return Err(CommandError::Precondition(format!(
                    "{component_id} has placed PCB footprints; remove them (and re-sync) before changing its footprint"
                )));
            }
            let def = p.library.get_mut(component_id).ok_or_else(|| {
                CommandError::NotFound(format!("component {component_id} in the project library"))
            })?;
            let mut next = def.clone();
            next.footprint = footprint.clone();
            next.pin_map = pin_map.clone();
            let problems = next.validate();
            if !problems.is_empty() {
                return Err(CommandError::Invalid(problems.join("; ")));
            }
            next.provenance = format!(
                "{}; footprint reassigned to {}",
                def.provenance, footprint.name
            );
            *def = next;
        }
        AddSheet { name } => {
            let id = new_id("sht");
            p.schematic.sheets.insert(
                id.clone(),
                Sheet {
                    id: id.clone(),
                    name: name.clone(),
                    width: 297_000_000,
                    height: 210_000_000,
                },
            );
            created.push(id);
        }
        PlaceSymbol {
            sheet_id,
            component_id,
            position,
            rotation,
            refdes,
            value,
        } => {
            let sheet = sheet_or_first(p, sheet_id)?;
            ensure_component(p, component_id, &mut warnings)?;
            let def = &p.library[component_id];
            let refdes = match refdes {
                Some(r) if p.schematic.symbols.values().any(|s| &s.refdes == r) => {
                    return Err(CommandError::Precondition(format!(
                        "reference designator {r} already used"
                    )));
                }
                Some(r) => r.clone(),
                None => next_refdes(p, &def.refdes_prefix),
            };
            let value = value.clone().unwrap_or_else(|| def.default_value.clone());
            let id = new_id("sym");
            p.schematic.symbols.insert(
                id.clone(),
                SymbolInstance {
                    id: id.clone(),
                    sheet_id: sheet,
                    component_id: component_id.clone(),
                    refdes,
                    value,
                    position: *position,
                    rotation: *rotation,
                    mirror: false,
                },
            );
            created.push(id);
        }
        MoveSymbol {
            symbol_id,
            position,
        } => {
            let old = symbol(p, symbol_id)?.clone();
            if old.position == *position {
                return Err(CommandError::NothingTo(
                    "move: symbol already at that position".into(),
                ));
            }
            let old_pins = connection_points_of(p, &old);
            p.schematic
                .symbols
                .get_mut(symbol_id)
                .expect("checked")
                .position = *position;
            let new_pins = connection_points_of(p, &p.schematic.symbols[symbol_id]);
            // Wire ends left behind on the old pin positions (a batch that rubber-bands the
            // wires moves them first, so it gets no warning).
            let left = p
                .schematic
                .wires
                .values()
                .filter(|w| w.sheet_id == old.sheet_id)
                .flat_map(|w| [w.a, w.b])
                .filter(|e| old_pins.contains(e) && !new_pins.contains(e))
                .count();
            if left > 0 {
                warnings.push(format!("{left} wire end(s) stay at the old pin positions and are no longer connected; drag them too or re-check connectivity (ERC)"));
            }
        }
        RotateSymbol {
            symbol_id,
            rotation,
        } => {
            symbol(p, symbol_id)?;
            p.schematic
                .symbols
                .get_mut(symbol_id)
                .expect("checked")
                .rotation = *rotation;
        }
        FlipSymbol { symbol_id, axis } => {
            // Sheet transform T = R(rotation) . M^mirror. A horizontal flip H = M gives
            // M . R(t) . M^m = R(-t) . M^(m+1); a vertical flip is R180 . H.
            let s = p
                .schematic
                .symbols
                .get_mut(symbol_id)
                .ok_or_else(|| CommandError::NotFound(format!("symbol {symbol_id}")))?;
            let r = s.rotation.inverse();
            s.rotation = match axis {
                FlipAxis::Horizontal => r,
                FlipAxis::Vertical => r.next().next(),
            };
            s.mirror = !s.mirror;
        }
        SetSymbolValue { symbol_id, value } => {
            symbol(p, symbol_id)?;
            p.schematic
                .symbols
                .get_mut(symbol_id)
                .expect("checked")
                .value = value.clone();
        }
        SetRefdes { symbol_id, refdes } => {
            symbol(p, symbol_id)?;
            if refdes.trim().is_empty() {
                return Err(CommandError::Invalid("empty reference designator".into()));
            }
            if p.schematic
                .symbols
                .values()
                .any(|s| &s.refdes == refdes && &s.id != symbol_id)
            {
                return Err(CommandError::Precondition(format!(
                    "reference designator {refdes} already used"
                )));
            }
            p.schematic
                .symbols
                .get_mut(symbol_id)
                .expect("checked")
                .refdes = refdes.clone();
            if let Some(fp) = p
                .pcb
                .footprints
                .values_mut()
                .find(|f| &f.symbol_id == symbol_id)
            {
                fp.refdes = refdes.clone();
            }
        }
        DeleteSymbol { symbol_id } => {
            symbol(p, symbol_id)?;
            p.schematic.symbols.remove(symbol_id);
            p.schematic
                .no_connects
                .retain(|_, n| &n.symbol_id != symbol_id);
            let fps: Vec<Id> = p
                .pcb
                .footprints
                .values()
                .filter(|f| &f.symbol_id == symbol_id)
                .map(|f| f.id.clone())
                .collect();
            for f in fps {
                p.pcb.footprints.remove(&f);
                warnings.push(format!(
                    "removed footprint {f} realising the deleted symbol"
                ));
            }
        }
        AddWire { sheet_id, a, b } => {
            let sheet = sheet_or_first(p, sheet_id)?;
            if a == b {
                return Err(CommandError::Invalid("wire has zero length".into()));
            }
            let hits = passes_through(*a, *b, &connection_points(p, &sheet), &[*a, *b]);
            if !hits.is_empty() {
                warnings.push(format!(
                    "wire passes over {} pin(s) and connects them",
                    hits.len()
                ));
            }
            let id = new_id("wir");
            p.schematic.wires.insert(
                id.clone(),
                Wire {
                    id: id.clone(),
                    sheet_id: sheet,
                    a: *a,
                    b: *b,
                },
            );
            created.push(id);
        }
        DeleteWire { wire_id } => {
            p.schematic
                .wires
                .remove(wire_id)
                .ok_or_else(|| CommandError::NotFound(format!("wire {wire_id}")))?;
        }
        MoveWire { wire_id, a, b } => {
            let w = p
                .schematic
                .wires
                .get(wire_id)
                .ok_or_else(|| CommandError::NotFound(format!("wire {wire_id}")))?
                .clone();
            if a == b {
                return Err(CommandError::Invalid("wire has zero length".into()));
            }
            if (w.a, w.b) == (*a, *b) {
                return Err(CommandError::NothingTo(
                    "move: wire already at that position".into(),
                ));
            }
            let hits = passes_through(*a, *b, &connection_points(p, &w.sheet_id), &[*a, *b]);
            if !hits.is_empty() {
                warnings.push(format!(
                    "wire passes over {} pin(s) and connects them",
                    hits.len()
                ));
            }
            let w = p.schematic.wires.get_mut(wire_id).expect("checked");
            w.a = *a;
            w.b = *b;
        }
        ConnectPins {
            a_symbol,
            a_pin,
            b_symbol,
            b_pin,
        } => {
            let sa = symbol(p, a_symbol)?.clone();
            let sb = symbol(p, b_symbol)?.clone();
            if sa.sheet_id != sb.sheet_id {
                return Err(CommandError::Precondition(
                    "pins are on different sheets; use net labels".into(),
                ));
            }
            let pa = p
                .pin_position(&sa, a_pin)
                .ok_or_else(|| CommandError::NotFound(format!("{} pin {a_pin}", sa.refdes)))?;
            let pb = p
                .pin_position(&sb, b_pin)
                .ok_or_else(|| CommandError::NotFound(format!("{} pin {b_pin}", sb.refdes)))?;
            if pa == pb {
                return Err(CommandError::NothingTo(
                    "connect: pins already coincide".into(),
                ));
            }
            let pts = connection_points(p, &sa.sheet_id);
            let candidates = [Point::new(pb.x, pa.y), Point::new(pa.x, pb.y)];
            let mut chosen = None;
            for corner in candidates {
                let segs: Vec<(Point, Point)> = [(pa, corner), (corner, pb)]
                    .into_iter()
                    .filter(|(x, y)| x != y)
                    .collect();
                if segs
                    .iter()
                    .all(|(x, y)| passes_through(*x, *y, &pts, &[pa, pb]).is_empty())
                {
                    chosen = Some(segs);
                    break;
                }
            }
            let segs = chosen.ok_or_else(|| CommandError::Precondition(format!("every L-shaped route between {}.{a_pin} and {}.{b_pin} crosses another pin; draw the wire manually or use net labels", sa.refdes, sb.refdes)))?;
            for (x, y) in segs {
                let id = new_id("wir");
                p.schematic.wires.insert(
                    id.clone(),
                    Wire {
                        id: id.clone(),
                        sheet_id: sa.sheet_id.clone(),
                        a: x,
                        b: y,
                    },
                );
                created.push(id);
            }
        }
        AddNetLabel {
            sheet_id,
            name,
            position,
            kind,
        } => {
            let sheet = sheet_or_first(p, sheet_id)?;
            if name.trim().is_empty() {
                return Err(CommandError::Invalid("net name is empty".into()));
            }
            let id = new_id("lbl");
            p.schematic.labels.insert(
                id.clone(),
                NetLabel {
                    id: id.clone(),
                    sheet_id: sheet,
                    name: name.trim().into(),
                    position: *position,
                    kind: *kind,
                },
            );
            created.push(id);
        }
        LabelPin {
            symbol_id,
            pin,
            name,
            kind,
        } => {
            let s = symbol(p, symbol_id)?.clone();
            let pos = p
                .pin_position(&s, pin)
                .ok_or_else(|| CommandError::NotFound(format!("{} pin {pin}", s.refdes)))?;
            if name.trim().is_empty() {
                return Err(CommandError::Invalid("net name is empty".into()));
            }
            let id = new_id("lbl");
            p.schematic.labels.insert(
                id.clone(),
                NetLabel {
                    id: id.clone(),
                    sheet_id: s.sheet_id,
                    name: name.trim().into(),
                    position: pos,
                    kind: *kind,
                },
            );
            created.push(id);
        }
        DeleteNetLabel { label_id } => {
            p.schematic
                .labels
                .remove(label_id)
                .ok_or_else(|| CommandError::NotFound(format!("label {label_id}")))?;
        }
        MoveNetLabel { label_id, position } => {
            let l = p
                .schematic
                .labels
                .get_mut(label_id)
                .ok_or_else(|| CommandError::NotFound(format!("label {label_id}")))?;
            if l.position == *position {
                return Err(CommandError::NothingTo(
                    "move: label already at that position".into(),
                ));
            }
            l.position = *position;
        }
        SetNoConnect { symbol_id, pin } => {
            let s = symbol(p, symbol_id)?;
            let def = &p.library[&s.component_id];
            def.pin(pin)
                .ok_or_else(|| CommandError::NotFound(format!("{} pin {pin}", s.refdes)))?;
            let id = new_id("nc");
            p.schematic.no_connects.insert(
                id.clone(),
                NoConnect {
                    id: id.clone(),
                    symbol_id: symbol_id.clone(),
                    pin: pin.clone(),
                },
            );
            created.push(id);
        }
        Annotate => {
            let ids: Vec<Id> = p
                .schematic
                .symbols
                .values()
                .filter(|s| s.refdes.ends_with('?') || s.refdes.is_empty())
                .map(|s| s.id.clone())
                .collect();
            if ids.is_empty() {
                return Err(CommandError::NothingTo(
                    "annotate: every symbol already has a reference designator".into(),
                ));
            }
            for id in ids {
                let prefix = p.library[&p.schematic.symbols[&id].component_id]
                    .refdes_prefix
                    .clone();
                let r = next_refdes(p, &prefix);
                p.schematic.symbols.get_mut(&id).expect("exists").refdes = r;
            }
        }
        SetBoardOutline { points } => {
            if points.len() < 3 || polygon_self_intersects(points) {
                return Err(CommandError::Invalid(
                    "board outline must be a simple polygon with at least 3 vertices".into(),
                ));
            }
            p.pcb.board.outline = points.clone();
        }
        SetStackup { stackup } => set_stackup(p, stackup)?,
        SetLayerCount {
            count,
            board_thickness,
        } => {
            let cur = &p.pcb.board.stackup;
            let thickness = board_thickness.unwrap_or(cur.board_thickness);
            let (dielectric, er) = (
                cur.dielectric.clone(),
                cur.layers.first().map_or(eak_core::FR4_ER, |l| l.er),
            );
            let s = Stackup::multilayer(usize::from(*count), thickness, 1.0, 0.5, &dielectric, er);
            set_stackup(p, &s)?;
        }
        SyncSchematicToPcb => {
            let nl = netlist::build(p);
            // Remove footprints whose symbol no longer exists.
            let orphans: Vec<Id> = p
                .pcb
                .footprints
                .values()
                .filter(|f| !p.schematic.symbols.contains_key(&f.symbol_id))
                .map(|f| f.id.clone())
                .collect();
            for o in orphans {
                warnings.push(format!("removed footprint {o} (symbol deleted)"));
                p.pcb.footprints.remove(&o);
            }
            let staging_x = eak_core::Rect::from_points(&p.pcb.board.outline)
                .map(|r| r.max.x + 5_000_000)
                .unwrap_or(0);
            let mut staging_y = 0;
            let symbols: Vec<SymbolInstance> = p.schematic.symbols.values().cloned().collect();
            for s in symbols {
                let def = p.library[&s.component_id].clone();
                let mut pad_nets = BTreeMap::new();
                for (pin, pads) in &def.pin_map {
                    if let Some(net) = nl.pin_net.get(&(s.id.clone(), pin.clone())) {
                        let real = nl
                            .nets
                            .get(net)
                            .is_some_and(|n| n.pins.len() + n.label_ids.len() > 1);
                        if real {
                            for pad in pads {
                                pad_nets.insert(pad.clone(), net.clone());
                            }
                        }
                    }
                }
                match p.pcb.footprints.values_mut().find(|f| f.symbol_id == s.id) {
                    Some(fp) => {
                        fp.refdes = s.refdes.clone();
                        fp.component_id = s.component_id.clone();
                        fp.pad_nets = pad_nets;
                    }
                    None => {
                        let id = new_id("fp");
                        p.pcb.footprints.insert(
                            id.clone(),
                            FootprintInstance {
                                id: id.clone(),
                                symbol_id: s.id.clone(),
                                component_id: s.component_id.clone(),
                                refdes: s.refdes.clone(),
                                position: Point::new(staging_x, staging_y),
                                rotation: Rotation::R0,
                                side: Side::Top,
                                pad_nets,
                                locked: false,
                            },
                        );
                        staging_y += def.footprint.courtyard_half_h * 2 + 1_000_000;
                        created.push(id);
                    }
                }
            }
            if !created.is_empty() {
                warnings.push(format!("{} new footprint(s) staged outside the board; place them (AutoPlace or manual)", created.len()));
            }
        }
        MoveFootprint {
            footprint_id,
            position,
            rotation,
            side,
        } => {
            let fp = p
                .pcb
                .footprints
                .get_mut(footprint_id)
                .ok_or_else(|| CommandError::NotFound(format!("footprint {footprint_id}")))?;
            if fp.locked {
                return Err(CommandError::Precondition(format!(
                    "{} is locked",
                    fp.refdes
                )));
            }
            fp.position = *position;
            if let Some(r) = rotation {
                fp.rotation = *r;
            }
            if let Some(s) = side {
                fp.side = *s;
            }
            if !p.pcb.tracks.is_empty() {
                warnings.push("existing tracks are not dragged with the footprint; run DRC".into());
            }
        }
        SetFootprintLocked {
            footprint_id,
            locked,
        } => {
            p.pcb
                .footprints
                .get_mut(footprint_id)
                .ok_or_else(|| CommandError::NotFound(format!("footprint {footprint_id}")))?
                .locked = *locked;
        }
        AddTrack {
            layer,
            a,
            b,
            width,
            net,
        } => {
            if a == b {
                return Err(CommandError::Invalid("track has zero length".into()));
            }
            require_layer(p, *layer, "track")?;
            let w = width.unwrap_or(p.constraints.class_for(net).track_width);
            if w <= 0 {
                return Err(CommandError::Invalid("track width must be positive".into()));
            }
            if w < p.constraints.min_track_width {
                warnings.push(
                    "track is narrower than the minimum track width (DRC will report it)".into(),
                );
            }
            let id = new_id("trk");
            p.pcb.tracks.insert(
                id.clone(),
                Track {
                    id: id.clone(),
                    layer: *layer,
                    width: w,
                    a: *a,
                    b: *b,
                    net: net.clone(),
                },
            );
            created.push(id);
        }
        DeleteTrack { track_id } => {
            p.pcb
                .tracks
                .remove(track_id)
                .ok_or_else(|| CommandError::NotFound(format!("track {track_id}")))?;
        }
        AddVia {
            position,
            net,
            drill,
            diameter,
            from,
            to,
        } => {
            let (from, to) = (
                from.unwrap_or(CopperLayer::Top),
                to.unwrap_or(CopperLayer::Bottom),
            );
            require_layer(p, from, "via start")?;
            require_layer(p, to, "via end")?;
            if from == to {
                return Err(CommandError::Invalid(
                    "a via must span at least two copper layers".into(),
                ));
            }
            // Store the span top to bottom whichever way round it was given.
            let (from, to) =
                if p.pcb.board.stackup.index_of(from) <= p.pcb.board.stackup.index_of(to) {
                    (from, to)
                } else {
                    (to, from)
                };
            let class = p.constraints.class_for(net).clone();
            let (d, dia) = (
                drill.unwrap_or(class.via_drill),
                diameter.unwrap_or(class.via_diameter),
            );
            if d <= 0 || dia <= d {
                return Err(CommandError::Invalid(
                    "via diameter must exceed a positive drill".into(),
                ));
            }
            let id = new_id("via");
            p.pcb.vias.insert(
                id.clone(),
                Via {
                    id: id.clone(),
                    position: *position,
                    drill: d,
                    diameter: dia,
                    net: net.clone(),
                    from,
                    to,
                },
            );
            created.push(id);
        }
        DeleteVia { via_id } => {
            p.pcb
                .vias
                .remove(via_id)
                .ok_or_else(|| CommandError::NotFound(format!("via {via_id}")))?;
        }
        AddZone {
            layer,
            net,
            outline,
            clearance,
        } => {
            if outline.len() < 3 || polygon_self_intersects(outline) {
                return Err(CommandError::Invalid(
                    "zone outline must be a simple polygon".into(),
                ));
            }
            require_layer(p, *layer, "zone")?;
            let id = new_id("zon");
            let cl = clearance.unwrap_or(
                p.constraints
                    .class_for(net)
                    .clearance
                    .max(p.constraints.min_clearance),
            );
            p.pcb.zones.insert(
                id.clone(),
                Zone {
                    id: id.clone(),
                    layer: *layer,
                    net: net.clone(),
                    outline: outline.clone(),
                    clearance: cl,
                },
            );
            created.push(id);
        }
        DeleteZone { zone_id } => {
            p.pcb
                .zones
                .remove(zone_id)
                .ok_or_else(|| CommandError::NotFound(format!("zone {zone_id}")))?;
        }
        AutoPlace { grid } => {
            if p.pcb.board.outline.len() < 3 {
                return Err(CommandError::Precondition(
                    "set a board outline before placement".into(),
                ));
            }
            let res = placer::place(p, grid.unwrap_or(500_000), 300_000);
            for (id, pos, rot) in &res.moves {
                let fp = p
                    .pcb
                    .footprints
                    .get_mut(id)
                    .expect("placer returns existing ids");
                fp.position = *pos;
                fp.rotation = *rot;
            }
            for (id, why) in res.unplaced {
                warnings.push(format!(
                    "could not place {}: {why}",
                    p.pcb.footprints[&id].refdes
                ));
            }
            // Measurable reasoning: recompute constraints on the result (not model-asserted).
            let a = eak_engineering::placement_intel::analyze(p);
            warnings.push(format!(
                "placement: {} snapped part(s), total airwire {:.1} mm, {} finding(s) (call analyze_placement; heuristic, not a certification)",
                a.snaps.len(),
                a.total_airwire_mm,
                a.findings.len()
            ));
            for f in a.findings.iter().take(10) {
                warnings.push(format!("{}: {}", f.rule, f.message));
            }
        }
        AutoRoute { nets } => {
            if p.pcb.board.outline.len() < 3 {
                return Err(CommandError::Precondition(
                    "set a board outline before routing".into(),
                ));
            }
            let res = router::route(
                p,
                &router::RouterOptions {
                    nets: nets.clone(),
                    ..Default::default()
                },
            );
            if res.routed_connections == 0 && res.failed_connections.is_empty() {
                // Say which nets are still open, so a filtered call is not read as "all done".
                let open: std::collections::BTreeSet<String> = eak_engineering::copper::analyse(p)
                    .ratsnest
                    .into_iter()
                    .map(|(n, _, _)| n)
                    .collect();
                return Err(CommandError::NothingTo(match (nets, open.is_empty()) {
                    (_, true) => "route: no unrouted connections".into(),
                    (Some(only), false) => format!(
                        "route: none of {} has an unrouted connection; still unrouted: {}",
                        only.join(", "),
                        open.into_iter().collect::<Vec<_>>().join(", ")
                    ),
                    (None, false) => format!(
                        "route: nothing the router can attempt; still unrouted: {}",
                        open.into_iter().collect::<Vec<_>>().join(", ")
                    ),
                }));
            }
            if res.tracks.is_empty() && res.vias.is_empty() {
                // Every attempt failed: the empty change would otherwise surface as a bare
                // "no change" and drop the reasons (dev-board run A/B, issue E2).
                let why: Vec<String> = res
                    .failed_connections
                    .iter()
                    .take(10)
                    .map(|(net, a, b, why)| {
                        format!(
                            "{net} ({:.2},{:.2})→({:.2},{:.2}) mm: {why}",
                            geom::to_mm(a.x),
                            geom::to_mm(a.y),
                            geom::to_mm(b.x),
                            geom::to_mm(b.y)
                        )
                    })
                    .collect();
                return Err(CommandError::Precondition(format!(
                    "router could not route any of {} connection(s): {}. Move parts apart (AutoPlace / MoveFootprint), add a layer (SetLayerCount), or route these by hand (AddTrack / AddVia); a GND pad can join a two-layer pour with StitchZones",
                    res.failed_connections.len(),
                    why.join("; ")
                )));
            }
            for t in res.tracks {
                let id = new_id("trk");
                p.pcb.tracks.insert(
                    id.clone(),
                    Track {
                        id: id.clone(),
                        layer: t.layer,
                        width: t.width,
                        a: t.a,
                        b: t.b,
                        net: t.net,
                    },
                );
                created.push(id);
            }
            for v in res.vias {
                let id = new_id("via");
                p.pcb.vias.insert(
                    id.clone(),
                    Via {
                        id: id.clone(),
                        position: v.position,
                        drill: v.drill,
                        diameter: v.diameter,
                        net: v.net,
                        from: CopperLayer::Top,
                        to: CopperLayer::Bottom,
                    },
                );
                created.push(id);
            }
            warnings.push(format!(
                "router: {} connection(s) routed, {} failed",
                res.routed_connections,
                res.failed_connections.len()
            ));
            if res.passes.len() > 1 || res.ripups > 0 || res.stagnated {
                warnings.push(format!(
                    "router: {} pass(es), {} rip-up(s) of lower-priority routes{}",
                    res.passes.len(),
                    res.ripups,
                    if res.stagnated {
                        ", stopped on stagnation (more passes would not change the result)"
                    } else {
                        ""
                    }
                ));
            }
            for note in res.notes.iter().take(10) {
                warnings.push(format!("router: {note}"));
            }
            warnings.push(
                "router: connectivity and clearance are not certified by the router; run run_check (eak-validation) and verify_routing"
                    .into(),
            );
            for (net, a, b, why) in res.failed_connections.iter().take(20) {
                warnings.push(format!(
                    "unrouted {net} ({:.2},{:.2})→({:.2},{:.2}) mm: {why}",
                    geom::to_mm(a.x),
                    geom::to_mm(a.y),
                    geom::to_mm(b.x),
                    geom::to_mm(b.y)
                ));
            }
        }
        StitchZones { net } => {
            if !p
                .pcb
                .zones
                .values()
                .any(|z| net.as_ref().is_none_or(|n| &z.net == n))
            {
                return Err(CommandError::Precondition(match net {
                    Some(n) => format!("no zone on net {n}: add {n} zones with AddZone on every copper layer first (DESK pours ground on all layers)"),
                    None => "no zones to stitch: add zones with AddZone first".into(),
                }));
            }
            let plan = eak_engineering::stitch::plan(p, net.as_deref());
            if plan.vias.is_empty() {
                return Err(CommandError::NothingTo(if plan.notes.is_empty() {
                    "stitch: every pad and pour island is already joined".into()
                } else {
                    format!("stitch: nothing placed; {}", plan.notes.join("; "))
                }));
            }
            for t in plan.tracks {
                let id = new_id("trk");
                p.pcb.tracks.insert(
                    id.clone(),
                    Track {
                        id: id.clone(),
                        layer: t.layer,
                        width: t.width,
                        a: t.a,
                        b: t.b,
                        net: t.net,
                    },
                );
                created.push(id);
            }
            let n_vias = plan.vias.len();
            for v in plan.vias {
                let id = new_id("via");
                p.pcb.vias.insert(
                    id.clone(),
                    Via {
                        id: id.clone(),
                        position: v.position,
                        drill: v.drill,
                        diameter: v.diameter,
                        net: v.net,
                        from: CopperLayer::Top,
                        to: CopperLayer::Bottom,
                    },
                );
                created.push(id);
            }
            warnings.push(format!(
                "stitch: {n_vias} via(s): {} pad escape(s), {} pour island(s) stitched",
                plan.pads_stitched, plan.islands_stitched
            ));
            for note in plan.notes.iter().take(20) {
                warnings.push(format!("stitch: {note}"));
            }
            warnings.push(
                "stitch: connectivity is not certified here; run run_check drc (DRC-004/013/014)"
                    .into(),
            );
        }
        ClearRouting { net } => {
            let before = p.pcb.tracks.len() + p.pcb.vias.len();
            p.pcb
                .tracks
                .retain(|_, t| net.as_ref().is_some_and(|n| &t.net != n));
            p.pcb
                .vias
                .retain(|_, v| net.as_ref().is_some_and(|n| &v.net != n));
            if before == p.pcb.tracks.len() + p.pcb.vias.len() {
                return Err(CommandError::NothingTo(
                    "clear: no routing to remove".into(),
                ));
            }
        }
        SetDesignRules {
            min_track_width,
            min_clearance,
            min_drill,
            min_annular_ring,
            board_edge_clearance,
            default_track_width,
            default_clearance,
            temp_rise_c,
            max_component_height,
        } => {
            let c = &mut p.constraints;
            for v in [
                min_track_width,
                min_clearance,
                min_drill,
                min_annular_ring,
                board_edge_clearance,
                default_track_width,
                default_clearance,
                max_component_height,
            ]
            .into_iter()
            .flatten()
            {
                if *v <= 0 {
                    return Err(CommandError::Invalid(
                        "design rule values must be positive".into(),
                    ));
                }
            }
            if let Some(t) = temp_rise_c {
                if !(*t > 0.0 && *t <= 100.0) {
                    return Err(CommandError::Invalid(
                        "temperature rise must be in (0, 100] °C".into(),
                    ));
                }
                c.temp_rise_c = *t;
            }
            if let Some(v) = min_track_width {
                c.min_track_width = *v;
            }
            if let Some(v) = min_clearance {
                c.min_clearance = *v;
            }
            if let Some(v) = min_drill {
                c.min_drill = *v;
            }
            if let Some(v) = min_annular_ring {
                c.min_annular_ring = *v;
            }
            if let Some(v) = board_edge_clearance {
                c.board_edge_clearance = *v;
            }
            if let Some(v) = default_track_width {
                c.default_class.track_width = *v;
            }
            if let Some(v) = default_clearance {
                c.default_class.clearance = *v;
            }
            if max_component_height.is_some() {
                c.max_component_height = *max_component_height;
            }
        }
        UpsertNetClass { class } => {
            if class.track_width <= 0
                || class.clearance <= 0
                || class.via_drill <= 0
                || class.via_diameter <= class.via_drill
            {
                return Err(CommandError::Invalid(
                    "net class values must be positive and via diameter > drill".into(),
                ));
            }
            p.constraints.net_classes.retain(|c| c.name != class.name);
            for n in &class.nets {
                for other in p.constraints.net_classes.iter_mut() {
                    other.nets.retain(|x| x != n);
                }
            }
            p.constraints.net_classes.push(class.clone());
        }
        SetNetCurrent { net, amps } => match amps {
            Some(a) if !(*a > 0.0 && a.is_finite()) => {
                return Err(CommandError::Invalid("current must be positive".into()))
            }
            Some(a) => {
                p.constraints.net_currents_a.insert(net.clone(), *a);
            }
            None => {
                p.constraints.net_currents_a.remove(net);
            }
        },
        SetNetVoltage { net, volts } => match volts {
            Some(v) if !v.is_finite() => {
                return Err(CommandError::Invalid("voltage must be finite".into()))
            }
            Some(v) => {
                p.constraints.net_voltages_v.insert(net.clone(), *v);
            }
            None => {
                p.constraints.net_voltages_v.remove(net);
            }
        },
        SetManufacturing { config } => {
            if config.mask_expansion < 0 {
                return Err(CommandError::Invalid("mask expansion must be ≥ 0".into()));
            }
            p.manufacturing = config.clone();
        }
        Batch { commands, label } => {
            if commands.is_empty() {
                return Err(CommandError::Invalid("batch has no commands".into()));
            }
            if commands.len() > MAX_BATCH {
                return Err(CommandError::Invalid(format!(
                    "batch has {} commands; the limit is {MAX_BATCH}",
                    commands.len()
                )));
            }
            if label.as_deref().is_some_and(|l| l.chars().count() > 200) {
                return Err(CommandError::Invalid(
                    "batch label is longer than 200 characters".into(),
                ));
            }
            // Steps run in order on the same working copy; the engine applies the whole
            // batch to a clone, so a failing step leaves the project untouched.
            for (i, c) in commands.iter().enumerate() {
                if matches!(c, Batch { .. }) {
                    return Err(CommandError::Invalid(format!(
                        "batch step {}: batches do not nest",
                        i + 1
                    )));
                }
                let step = apply(p, c).map_err(|e| {
                    let msg = format!("batch step {} ({}): {e}", i + 1, c.name());
                    match e {
                        CommandError::NotFound(_) => CommandError::NotFound(msg),
                        CommandError::Precondition(_) => CommandError::Precondition(msg),
                        CommandError::NothingTo(_) => CommandError::NothingTo(msg),
                        _ => CommandError::Invalid(msg),
                    }
                })?;
                created.extend(step.created);
                warnings.extend(
                    step.warnings
                        .into_iter()
                        .map(|w| format!("step {} ({}): {w}", i + 1, c.name())),
                );
            }
        }
    }
    Ok(Applied { created, warnings })
}
