//! # eak-library — component library management (library data, not EAK-IR)
//!
//! * `part`    – MPN-keyed part records: identity is (manufacturer, MPN) after normalisation;
//!   a merged part keeps every source's record and names disagreements instead of picking one.
//! * `source`  – the provider-independent [`source::PartSource`] trait and the resolver that
//!   merges exact hits by key, keeps fuzzy hits as suggestions, and never calls a source that
//!   is not ready.
//! * `sources` – adapters: the built-in starter library, and any supplier integration that
//!   already yields `SourcingRecord`s (no network code and no supplier call in this crate).
//! * land patterns – [`LandPatternSpec`] / [`PadSpec`], the datasheet land-pattern table a
//!   footprint is verified against. The type lives in `eak_validation::footprint` (this crate
//!   depends on eak-validation, not the other way round, ADR-0052) and is re-exported here.
//! * `mpn`     – MPN discovery (exact, prefix and partial MPN matches across ready sources) and
//!   manufacturer identification from a sourced alias table, with an explicit unknown result.
//! * `kicad`   – KiCad `.kicad_sym` / `.kicad_mod` export, the inverse of the clean-room reader
//!   in `eak_core::kicad_import`, with every loss reported.
//!
//! Nothing here mutates a design: a part enters a project only through `Engine::execute`
//! (`UpsertComponentDefinition`) in the layers above. Nothing here is verified by import or by
//! being stored.

pub mod kicad;
pub mod mpn;
pub mod part;
pub mod source;
pub mod sources;

pub use eak_validation::footprint::{LandPatternSpec, PadSpec};
pub use mpn::{discover, identify_manufacturer, Discovery, ManufacturerId, MpnMatch};
pub use part::{Part, PartKey, PartKeyError, PartRecord, SourceId};
pub use source::{resolve, PartQuery, PartSource, Resolution, SourceStatus};
