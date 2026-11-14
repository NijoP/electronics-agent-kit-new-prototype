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

/// File format of an attached 3D model. Only STEP is rendered today; WRL and GLB are
/// recorded so a reference survives, and the viewer falls back to the box body for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFormat {
    Step,
    Wrl,
    Glb,
}

impl ModelFormat {
    /// Format implied by a file extension (case-insensitive), if supported.
    pub fn from_extension(ext: &str) -> Option<ModelFormat> {
        match ext.to_ascii_lowercase().as_str() {
            "step" | "stp" => Some(ModelFormat::Step),
            "wrl" => Some(ModelFormat::Wrl),
            "glb" => Some(ModelFormat::Glb),
            _ => None,
        }
    }
}

/// Where the model bytes live. Models are never bundled with EAK: the user points at their
/// own library directory (for example a local copy of the KiCad 3D library).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelSource {
    /// Absolute path of a file on this machine.
    File { path: String },
}

/// Placement of a model relative to its footprint origin. Offset is in millimetres in
/// footprint coordinates (x right, y down as in the PCB editor, z up from the mounting
/// surface). Rotation is in degrees about the model's own x, y, z axes, applied as
/// `Rz * Ry * Rx` (KiCad convention). Scale is per axis and must be positive.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelTransform {
    pub offset_mm: [f64; 3],
    pub rotation_deg: [f64; 3],
    pub scale: [f64; 3],
}

impl Default for ModelTransform {
    fn default() -> Self {
        ModelTransform {
            offset_mm: [0.0; 3],
            rotation_deg: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

/// How far a model is trusted. Declared by whoever attaches the model; EAK does not yet
/// verify model geometry against the footprint, so `None` on a `ModelRef` means
/// "unclassified" and the UI must never present such a model as exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelQuality {
    /// Published by the component manufacturer for this exact part.
    VendorExact,
    /// From a trusted library and checked against the footprint/package.
    LibraryVerified,
    /// Generated (for example from package dimensions) and validated against the footprint.
    DerivedValidated,
}

/// Typed reference to a physical 3D model of a component, with its licence and provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRef {
    pub source: ModelSource,
    pub format: ModelFormat,
    /// Declared trust level; `None` = unclassified (shown as such, never as exact).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<ModelQuality>,
    #[serde(default)]
    pub transform: ModelTransform,
    /// SPDX identifier or free-text licence of the model file; `None` means not recorded.
    #[serde(default)]
    pub licence: Option<String>,
    /// Where the model came from (library name, URL, author). Required.
    pub provenance: String,
}

impl ModelRef {
    /// Structural problems of the reference; empty when it is usable.
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        let ModelSource::File { path } = &self.source;
        if path.trim().is_empty() {
            out.push("3D model path is empty".to_string());
        } else {
            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if ModelFormat::from_extension(ext) != Some(self.format) {
                out.push(format!(
                    "3D model file extension does not match format {:?}",
                    self.format
                ));
            }
        }
        let t = &self.transform;
        if t.offset_mm
            .iter()
            .chain(t.rotation_deg.iter())
            .any(|v| !v.is_finite())
        {
            out.push("3D model offset/rotation must be finite".to_string());
        }
        if t.scale.iter().any(|s| !s.is_finite() || *s <= 0.0) {
            out.push("3D model scale must be positive and finite".to_string());
        }
        if self.provenance.trim().is_empty() {
            out.push("3D model provenance is empty".to_string());
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model3d {
    /// Simple parametric body used when no external model is mapped (also the fallback when
    /// the referenced model cannot be loaded).
    pub kind: String,
    pub color: String,
    #[serde(default)]
    pub external_ref: Option<String>,
    /// Typed reference to a real 3D model; `None` means box body only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<ModelRef>,
}

/// One quantity break of a supplier offer. The price is decimal text exactly as the supplier
/// stated it; EAK never rounds or converts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferPrice {
    pub min_quantity: u64,
    pub unit_price: String,
    pub currency: String,
}

/// What one supplier says about selling a component: its own part number, stock, price and
/// lifecycle status, and when that was fetched.
///
/// This is *commercial* data and is deliberately not part of the component's identity
/// (manufacturer + MPN, package, symbol, footprint, pin map, 3D model). It goes stale, differs
/// per supplier, and is never proof that the part suits the design: nothing here feeds
/// electrical checks. `lifecycle` is the supplier's own wording, not an EAK judgement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupplierOffer {
    /// Lower-case supplier key, for example `lcsc`.
    pub supplier: String,
    /// The supplier's own identifier (an LCSC `C` number for `lcsc`).
    pub supplier_part: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub stock: Option<u64>,
    #[serde(default)]
    pub price_breaks: Vec<OfferPrice>,
    /// Lifecycle status as the supplier reported it; `None` = not reported.
    #[serde(default)]
    pub lifecycle: Option<String>,
    /// When the supplier data was fetched (Unix seconds); `None` for a bare legacy id.
    #[serde(default)]
    pub fetched_at_unix_s: Option<u64>,
    /// SHA-256 of the raw supplier response this offer was read from, when known.
    #[serde(default)]
    pub source_sha256: Option<String>,
}

impl SupplierOffer {
    /// An offer that records only a supplier part number (no stock, price or fetch time).
    pub fn bare(supplier: &str, supplier_part: &str) -> SupplierOffer {
        SupplierOffer {
            supplier: supplier.trim().to_ascii_lowercase(),
            supplier_part: supplier_part.trim().to_string(),
            url: None,
            stock: None,
            price_breaks: vec![],
            lifecycle: None,
            fetched_at_unix_s: None,
            source_sha256: None,
        }
    }
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
    /// Legacy single LCSC part number kept so older project files and the BOM/JLCPCB code
    /// keep working. Supplier data lives in `supplier_offers`; read it through
    /// [`ComponentDef::supplier_part`], which prefers the offer and falls back to this field.
    #[serde(default)]
    pub lcsc: Option<String>,
    /// Supplier-specific offers, separate from the component's identity (ADR-0027).
    /// Absent in older files (loads as empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supplier_offers: Vec<SupplierOffer>,
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
    /// The part number `supplier` uses for this component: its offer if present, else (for
    /// `lcsc` only) the legacy `lcsc` field. A part number is an identifier, never evidence of
    /// electrical suitability.
    pub fn supplier_part(&self, supplier: &str) -> Option<&str> {
        let key = supplier.trim().to_ascii_lowercase();
        self.supplier_offers
            .iter()
            .find(|o| o.supplier == key && !o.supplier_part.trim().is_empty())
            .map(|o| o.supplier_part.as_str())
            .or_else(|| {
                if key == "lcsc" {
                    self.lcsc.as_deref().filter(|s| !s.trim().is_empty())
                } else {
                    None
                }
            })
    }

    /// Insert or replace this supplier's offer and keep the legacy `lcsc` mirror in step.
    pub fn set_offer(&mut self, offer: SupplierOffer) {
        if offer.supplier == "lcsc" {
            self.lcsc = Some(offer.supplier_part.clone());
        }
        match self
            .supplier_offers
            .iter_mut()
            .find(|o| o.supplier == offer.supplier)
        {
            Some(slot) => *slot = offer,
            None => self.supplier_offers.push(offer),
        }
    }

    /// Migrate a legacy definition: a bare `lcsc` id with no matching offer becomes a bare
    /// `lcsc` offer. Idempotent. Identity fields are not touched.
    pub fn migrate_offers(&mut self) {
        if let Some(part) = self.lcsc.clone().filter(|s| !s.trim().is_empty()) {
            if !self.supplier_offers.iter().any(|o| o.supplier == "lcsc") {
                self.supplier_offers.push(SupplierOffer::bare("lcsc", &part));
            }
        }
    }

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
        if let Some(m) = &self.model3d.model_ref {
            for problem in m.validate() {
                out.push(format!("{}: {problem}", self.id));
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

impl ComponentDef {
    /// Case-insensitive AND-match of whitespace-separated terms against the fields a user
    /// searches by (id, name, category, footprint, manufacturer, MPN, LCSC number, refdes
    /// prefix). An empty query matches everything.
    pub fn matches_query(&self, query: &str) -> bool {
        let hay = format!(
            "{} {} {} {} {} {} {} {}",
            self.id,
            self.name,
            self.category,
            self.footprint.name,
            self.manufacturer.as_deref().unwrap_or(""),
            self.mpn.as_deref().unwrap_or(""),
            self.lcsc.as_deref().unwrap_or(""),
            self.refdes_prefix
        )
        .to_lowercase();
        query
            .split_whitespace()
            .all(|t| hay.contains(&t.to_lowercase()))
    }
}

#[cfg(test)]
mod model_ref_tests {
    use super::*;

    fn good() -> ModelRef {
        ModelRef {
            source: ModelSource::File {
                path: "/lib/R_0603.step".into(),
            },
            format: ModelFormat::Step,
            quality: Some(ModelQuality::LibraryVerified),
            transform: ModelTransform::default(),
            licence: Some("CC-BY-SA-4.0".into()),
            provenance: "user 3D library".into(),
        }
    }

    #[test]
    fn valid_reference_has_no_problems_and_round_trips() {
        let m = good();
        assert!(m.validate().is_empty());
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<ModelRef>(&json).unwrap(), m);
    }

    #[test]
    fn rejects_mismatched_extension_bad_scale_and_missing_provenance() {
        let mut m = good();
        m.format = ModelFormat::Glb;
        m.transform.scale = [1.0, 0.0, 1.0];
        m.provenance = " ".into();
        assert_eq!(m.validate().len(), 3);
    }

    #[test]
    fn legacy_model3d_json_without_model_ref_still_loads() {
        let m: Model3d =
            serde_json::from_str(r##"{"kind":"box","color":"#000","external_ref":null}"##)
                .unwrap();
        assert!(m.model_ref.is_none());
    }
}

#[cfg(test)]
mod search_tests {
    use crate::builtin;

    #[test]
    fn search_matches_all_terms_across_fields() {
        let all = builtin::components();
        let hits = |q: &str| all.iter().filter(|c| c.matches_query(q)).count();
        assert_eq!(hits(""), all.len());
        assert!(hits("resistor") >= 1);
        assert!(hits("RESISTOR 0603") >= 1);
        assert_eq!(hits("resistor no-such-term"), 0);
        // LCSC number and MPN are searchable.
        let with_lcsc = all
            .iter()
            .find(|c| c.lcsc.is_some())
            .expect("a part with LCSC");
        assert!(with_lcsc.matches_query(with_lcsc.lcsc.as_deref().unwrap()));
    }
}

#[cfg(test)]
mod supplier_offer_tests {
    use crate::builtin;
    use super::*;

    fn part() -> ComponentDef {
        builtin::components().into_iter().next().expect("a builtin part")
    }

    #[test]
    fn legacy_json_without_offers_loads_and_migrates() {
        let mut v = serde_json::to_value(part()).unwrap();
        v.as_object_mut().unwrap().remove("supplier_offers");
        v["lcsc"] = serde_json::json!("C25804");
        let mut def: ComponentDef = serde_json::from_value(v).unwrap();
        assert!(def.supplier_offers.is_empty());
        assert_eq!(def.supplier_part("LCSC"), Some("C25804"));
        def.migrate_offers();
        def.migrate_offers();
        assert_eq!(def.supplier_offers, vec![SupplierOffer::bare("lcsc", "C25804")]);
    }

    #[test]
    fn empty_offers_are_not_serialised_and_identity_is_untouched() {
        let def = part();
        let v = serde_json::to_value(&def).unwrap();
        assert!(v.get("supplier_offers").is_none());
        let mut with = def.clone();
        with.set_offer(SupplierOffer::bare("lcsc", "C1"));
        assert_eq!(with.lcsc.as_deref(), Some("C1"));
        assert_eq!((&with.mpn, &with.manufacturer), (&def.mpn, &def.manufacturer));
        let back: ComponentDef =
            serde_json::from_value(serde_json::to_value(&with).unwrap()).unwrap();
        assert_eq!(back, with);
    }

    #[test]
    fn set_offer_replaces_the_same_supplier_only() {
        let mut def = part();
        def.set_offer(SupplierOffer::bare("lcsc", "C1"));
        def.set_offer(SupplierOffer::bare("mouser", "M9"));
        def.set_offer(SupplierOffer::bare("lcsc", "C2"));
        assert_eq!(def.supplier_offers.len(), 2);
        assert_eq!(def.supplier_part("lcsc"), Some("C2"));
        assert_eq!(def.supplier_part("mouser"), Some("M9"));
        assert_eq!(def.supplier_part("digikey"), None);
    }
}
