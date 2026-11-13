//! Native component, symbol and footprint definitions.
//!
//! A component definition binds one schematic symbol, one footprint and an explicit
//! symbol-pin -> footprint-pad map. Electrical ratings carry provenance; a rating with
//! no source is never treated as verified.

use crate::geom::{Nm, Point};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinType {
    Input,
    Output,
    Bidirectional,
    Passive,
    PowerIn,
    PowerOut,
    OpenCollector,
    NoConnect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolPin {
    pub number: String,
    pub name: String,
    pub electrical_type: PinType,
    /// Connection point relative to the symbol origin.
    pub position: Point,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolDef {
    pub pins: Vec<SymbolPin>,
    /// Body rectangle half-extent (drawing only).
    pub body_half_w: Nm,
    pub body_half_h: Nm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadShape {
    Rect,
    Circle,
    Oval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadKind {
    /// Surface mount on the component side.
    Smd,
    /// Plated through-hole on all copper layers.
    Tht,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PadDef {
    pub number: String,
    pub kind: PadKind,
    pub shape: PadShape,
    pub position: Point,
    pub size_x: Nm,
    pub size_y: Nm,
    /// Finished hole diameter for THT pads.
    #[serde(default)]
    pub drill: Option<Nm>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootprintDef {
    pub name: String,
    pub pads: Vec<PadDef>,
    /// Courtyard half extents around origin (keep-out between components).
    pub courtyard_half_w: Nm,
    pub courtyard_half_h: Nm,
    /// Body height above board for 3D / mechanical checks; `None` means unknown.
    pub body_height: Option<Nm>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rating {
    pub name: String,
    pub value: f64,
    pub unit: String,
    /// Where the value came from (datasheet URL, standard, or "unverified").
    pub source: String,
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model3d {
    /// Simple parametric body used when no external model is mapped.
    pub kind: String,
    pub color: String,
    #[serde(default)]
    pub external_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentDef {
    pub id: String,
    pub name: String,
    pub category: String,
    pub refdes_prefix: String,
    #[serde(default)]
    pub default_value: String,
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub mpn: Option<String>,
    #[serde(default)]
    pub lcsc: Option<String>,
    #[serde(default)]
    pub datasheet: Option<String>,
    pub symbol: SymbolDef,
    pub footprint: FootprintDef,
    /// symbol pin number -> footprint pad numbers.
    pub pin_map: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub ratings: Vec<Rating>,
    pub model3d: Model3d,
    /// Provenance of the definition itself.
    pub provenance: String,
}

impl ComponentDef {
    pub fn pin(&self, number: &str) -> Option<&SymbolPin> {
        self.symbol.pins.iter().find(|p| p.number == number)
    }
    pub fn pad(&self, number: &str) -> Option<&PadDef> {
        self.footprint.pads.iter().find(|p| p.number == number)
    }

    /// Structural validation of a library definition. Returns human-readable problems.
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.symbol.pins.is_empty() {
            out.push(format!("{}: symbol has no pins", self.id));
        }
        let mut seen = std::collections::BTreeSet::new();
        for p in &self.symbol.pins {
            if !seen.insert(&p.number) {
                out.push(format!("{}: duplicate symbol pin {}", self.id, p.number));
            }
            match self.pin_map.get(&p.number) {
                None if p.electrical_type != PinType::NoConnect => out.push(format!(
                    "{}: symbol pin {} not mapped to a pad",
                    self.id, p.number
                )),
                Some(pads) => {
                    for pad in pads {
                        if self.pad(pad).is_none() {
                            out.push(format!(
                                "{}: pin {} maps to missing pad {}",
                                self.id, p.number, pad
                            ));
                        }
                    }
                }
                None => {}
            }
        }
        let mut pad_seen = std::collections::BTreeSet::new();
        for pad in &self.footprint.pads {
            if !pad_seen.insert(&pad.number) {
                out.push(format!("{}: duplicate pad {}", self.id, pad.number));
            }
            if pad.size_x <= 0 || pad.size_y <= 0 {
                out.push(format!(
                    "{}: pad {} has non-positive size",
                    self.id, pad.number
                ));
            }
            match (pad.kind, pad.drill) {
                (PadKind::Tht, None) => {
                    out.push(format!("{}: THT pad {} has no drill", self.id, pad.number))
                }
                (PadKind::Tht, Some(d)) if d >= pad.size_x.min(pad.size_y) => out.push(format!(
                    "{}: pad {} drill leaves no annular ring",
                    self.id, pad.number
                )),
                (PadKind::Smd, Some(_)) => {
                    out.push(format!("{}: SMD pad {} has a drill", self.id, pad.number))
                }
                _ => {}
            }
        }
        for pin in self.pin_map.keys() {
            if self.pin(pin).is_none() {
                out.push(format!(
                    "{}: pin map references missing symbol pin {}",
                    self.id, pin
                ));
            }
        }
        out
    }
}
