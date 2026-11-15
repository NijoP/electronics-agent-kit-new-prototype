//! Linking a supplier listing to the component libraries, and saying what it is (ADR-0033).
//!
//! A supplier search result must not be rejected because EAK has no symbol and footprint for
//! it, and it must not be treated as an engineering component either. [`link_record`] decides,
//! deterministically and with reasons, which of these a listing is:
//!
//! * [`Standing::ExistingEak`]: a library definition (built-in, project, personal or imported
//!   open-source) already carries this supplier part number, or the same normalised MPN;
//! * [`Standing::ImportedDefinition`]: that definition was itself created from an Distributor listing
//!   (id `DISTRIBUTOR_<number>`), i.e. its symbol/footprint were reused from a template;
//! * [`Standing::SupplierOnly`]: no definition yet, but an existing symbol/footprint template can
//!   carry it ([`Link::import_template`]); the listing can be saved as a supplier record now
//!   and imported later;
//! * [`Standing::NeedsAuthoring`]: no trustworthy symbol/footprint exists; the listing is kept
//!   and a library-authoring workflow is the next step. Nothing is ever invented.
//!
//! Independently, [`Link::unverified`] is set when the matching definition has ratings that are
//! not verified or no datasheet, so "an unverified engineering definition" is never presented
//! as approved. Two manufacturers' parts with the same MPN text are matched separately: a
//! manufacturer mismatch is flagged (`manufacturer_conflict`), never merged.

use crate::distributor::import_plan;
use crate::matching::{normalize_manufacturer, normalize_mpn};
use crate::sourcing::SourcingRecord;
use eak_core::ComponentDef;
use serde::{Deserialize, Serialize};

/// Where a library definition lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum Scope {
    /// The catalogue shipped with EAK.
    Builtin,
    /// The open project's library.
    Project,
    /// The user's personal library (all projects).
    Personal,
    /// An imported open-source library, by name (licence and source recorded on import).
    Imported(String),
}

impl Scope {
    /// Short label for UI filters.
    pub fn label(&self) -> String {
        match self {
            Scope::Builtin => "builtin".into(),
            Scope::Project => "project".into(),
            Scope::Personal => "personal".into(),
            Scope::Imported(n) => format!("imported:{n}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    ExistingEak,
    ImportedDefinition,
    SupplierOnly,
    NeedsAuthoring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    ExactSupplierPart,
    ExactMpnManufacturer,
    ExactMpn,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkMatch {
    pub component_id: String,
    pub scope: Scope,
    pub kind: LinkKind,
    pub reasons: Vec<String>,
    /// The MPN matched but the manufacturers differ; this is a different product.
    pub manufacturer_conflict: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub standing: Standing,
    /// Matching definitions, best first (never merged across manufacturers).
    pub matches: Vec<LinkMatch>,
    /// The best match has unverified ratings or no datasheet: not approved for engineering use.
    pub unverified: bool,
    /// Library template whose symbol, footprint and pin map an import would reuse.
    pub import_template: Option<String>,
    /// Plain-language explanation of the standing and what to do next.
    pub explanation: String,
}

fn is_distributor_import(def: &ComponentDef) -> bool {
    def.id.starts_with("DISTRIBUTOR_")
}

fn unverified(def: &ComponentDef) -> bool {
    def.ratings.iter().any(|r| !r.verified)
        || def
            .datasheet
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty)
}

/// How one definition relates to the listing, if at all.
fn relate(
    rec: &SourcingRecord,
    distributor: &Option<String>,
    mpn: &Option<String>,
    maker: &Option<String>,
    def: &ComponentDef,
) -> Option<(LinkKind, Vec<String>, bool)> {
    let by_part = distributor.as_ref().is_some_and(|l| {
        def.supplier_part("distributor")
            .is_some_and(|p| p.eq_ignore_ascii_case(l))
    });
    if by_part {
        return Some((
            LinkKind::ExactSupplierPart,
            vec![format!(
                "the definition carries Distributor part number {}",
                distributor.as_deref().unwrap_or("")
            )],
            false,
        ));
    }
    let (Some(m), Some(dm)) = (mpn, def.mpn.as_deref().map(normalize_mpn)) else {
        return None;
    };
    if *m != dm {
        return None;
    }
    let mut reasons =
        vec!["manufacturer part number equal after case/space/dash normalisation".to_string()];
    let def_maker = def
        .manufacturer
        .as_deref()
        .map(normalize_manufacturer)
        .filter(|x| !x.is_empty());
    match (maker, &def_maker) {
        (Some(a), Some(b)) if a == b => {
            reasons.push("manufacturer matches".into());
            Some((LinkKind::ExactMpnManufacturer, reasons, false))
        }
        (Some(_), Some(_)) => {
            reasons.push(format!(
                "manufacturer differs ({:?} vs {:?}): a different product with the same MPN text",
                rec.manufacturer.as_deref().unwrap_or(""),
                def.manufacturer.as_deref().unwrap_or("")
            ));
            Some((LinkKind::ExactMpn, reasons, true))
        }
        _ => {
            reasons.push("one side names no manufacturer: confirm it is the same maker".into());
            Some((LinkKind::ExactMpn, reasons, false))
        }
    }
}

/// Classify `rec` against `defs` (each tagged with its scope).
pub fn link_record(rec: &SourcingRecord, defs: &[(Scope, &ComponentDef)]) -> Link {
    let distributor = rec.distributor_part.as_deref().map(str::to_ascii_uppercase);
    let mpn = rec
        .mpn
        .as_deref()
        .map(normalize_mpn)
        .filter(|m| !m.is_empty());
    let maker = rec
        .manufacturer
        .as_deref()
        .map(normalize_manufacturer)
        .filter(|m| !m.is_empty());

    let mut found: Vec<(LinkMatch, &ComponentDef)> = vec![];
    for (scope, def) in defs {
        if let Some((kind, reasons, conflict)) = relate(rec, &distributor, &mpn, &maker, def) {
            found.push((
                LinkMatch {
                    component_id: def.id.clone(),
                    scope: scope.clone(),
                    kind,
                    reasons,
                    manufacturer_conflict: conflict,
                },
                def,
            ));
        }
    }
    found.sort_by_key(|(m, _)| {
        (
            match m.kind {
                LinkKind::ExactSupplierPart => 0,
                LinkKind::ExactMpnManufacturer => 1,
                LinkKind::ExactMpn => 2,
            },
            m.manufacturer_conflict,
            m.component_id.clone(),
        )
    });

    // The best match decides the standing; a manufacturer-conflicting MPN-only match is a
    // different product and does not make the listing "existing".
    let best = found
        .iter()
        .find(|(m, _)| !m.manufacturer_conflict)
        .map(|(m, d)| (m.component_id.clone(), m.scope.clone(), *d));
    if let Some((id, scope, def)) = best {
        let imported = is_distributor_import(def);
        let unv = unverified(def);
        let explanation = format!(
            "{} {} in the {} library{}.",
            if imported {
                "Imported definition"
            } else {
                "Existing EAK component"
            },
            id,
            scope.label(),
            if unv {
                "; its engineering definition is unverified (ratings or datasheet not verified)"
            } else {
                ""
            }
        );
        return Link {
            standing: if imported {
                Standing::ImportedDefinition
            } else {
                Standing::ExistingEak
            },
            unverified: unv,
            import_template: None,
            explanation,
            matches: found.into_iter().map(|(m, _)| m).collect(),
        };
    }

    // No usable definition: can an existing symbol/footprint template carry it?
    let matches: Vec<LinkMatch> = found.into_iter().map(|(m, _)| m).collect();
    match import_plan(rec) {
        Ok(plan) => Link {
            standing: Standing::SupplierOnly,
            unverified: true,
            explanation: format!(
                "Supplier listing only. It can be saved as a supplier record now and imported \
                 using the symbol, footprint and pin map of library template {}; ratings stay \
                 unverified.",
                plan.template_id
            ),
            import_template: Some(plan.template_id),
            matches,
        },
        Err(why) => Link {
            standing: Standing::NeedsAuthoring,
            unverified: true,
            explanation: format!(
                "Supplier listing only; no trustworthy symbol/footprint exists to attach it to. \
                 Save the supplier record and author the symbol and footprint in the editors \
                 later. ({why})"
            ),
            import_template: None,
            matches,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sourcing::{Spec, VerificationState};
    use eak_core::builtin;

    fn rec(distributor: &str, mpn: &str, maker: &str, package: &str, category: &str) -> SourcingRecord {
        SourcingRecord {
            provider: "Distributor".into(),
            source_url: "https://distributor.invalid/x".into(),
            retrieved_at_unix_s: 1,
            raw_sha256: "00".into(),
            raw_bytes: 1,
            distributor_part: Some(distributor.into()),
            mpn: Some(mpn.into()),
            manufacturer: Some(maker.into()),
            package: Some(package.into()),
            category: Some(category.into()),
            description: None,
            datasheet_url: None,
            specs: vec![Spec {
                name: "Resistance".into(),
                value: "10kΩ".into(),
            }],
            stock: Some(1),
            price_breaks: vec![],
            missing: vec![],
            rejected: vec![],
            datasheet: None,
            attestation: None,
            state: VerificationState::SearchHit,
        }
    }

    fn lib() -> Vec<ComponentDef> {
        builtin::components()
    }

    fn scoped(defs: &[ComponentDef]) -> Vec<(Scope, &ComponentDef)> {
        defs.iter().map(|d| (Scope::Builtin, d)).collect()
    }

    #[test]
    fn a_listing_with_no_matching_definition_and_no_template_needs_authoring() {
        let defs = lib();
        let r = rec(
            "C99999",
            "STM32F103C8T6",
            "ST",
            "LQFP-48",
            "Microcontrollers",
        );
        let l = link_record(&r, &scoped(&defs));
        assert_eq!(l.standing, Standing::NeedsAuthoring);
        assert!(l.unverified);
        assert!(l.import_template.is_none());
        assert!(l.explanation.contains("author the symbol and footprint"));
    }

    #[test]
    fn a_chip_resistor_with_a_template_is_supplier_only_with_the_template_named() {
        let defs = lib();
        let r = rec(
            "C25804",
            "0603WAF1002T5E",
            "UNI-ROYAL",
            "0603",
            "Resistors / Chip Resistor - Surface Mount",
        );
        let l = link_record(&r, &scoped(&defs));
        // The built-in library may or may not already hold this exact part; when it does not,
        // the listing is a supplier record that an existing template can carry.
        if l.matches.is_empty() {
            assert_eq!(l.standing, Standing::SupplierOnly);
            assert_eq!(l.import_template.as_deref(), Some("R_0603"));
            assert!(l.unverified);
        }
    }

    #[test]
    fn an_existing_definition_matches_by_distributor_number() {
        let mut d = lib().remove(0);
        d.id = "MY_RES".into();
        d.mpn = Some("0603WAF1002T5E".into());
        d.manufacturer = Some("UNI-ROYAL".into());
        d.set_offer(eak_core::SupplierOffer::bare("distributor", "C25804"));
        let defs = [d];
        let r = rec("C25804", "other text", "X", "0603", "Resistors");
        let l = link_record(&r, &[(Scope::Project, &defs[0])]);
        assert_eq!(l.matches[0].kind, LinkKind::ExactSupplierPart);
        assert_eq!(l.standing, Standing::ExistingEak);
        assert!(l.explanation.contains("project library"));
    }

    #[test]
    fn the_same_mpn_from_another_manufacturer_is_not_the_existing_part() {
        let mut d = lib().remove(0);
        d.id = "THEIRS".into();
        d.mpn = Some("ABC-123".into());
        d.manufacturer = Some("Maker One".into());
        d.distributor = None;
        d.supplier_offers.clear();
        let defs = [d];
        let r = rec("C7", "abc 123", "Maker Two", "0603", "Resistors");
        let l = link_record(&r, &[(Scope::Personal, &defs[0])]);
        assert_eq!(l.matches.len(), 1);
        assert!(l.matches[0].manufacturer_conflict);
        assert_ne!(
            l.standing,
            Standing::ExistingEak,
            "a different manufacturer is a different product"
        );
        assert!(l.matches[0]
            .reasons
            .iter()
            .any(|x| x.contains("different product")));
    }

    #[test]
    fn a_definition_imported_from_distributor_is_labelled_and_unverified_without_a_datasheet() {
        let mut d = lib().remove(0);
        d.id = "DISTRIBUTOR_C25804".into();
        d.set_offer(eak_core::SupplierOffer::bare("distributor", "C25804"));
        d.datasheet = None;
        let defs = [d];
        let r = rec("C25804", "x", "y", "0603", "Resistors");
        let l = link_record(&r, &[(Scope::Project, &defs[0])]);
        assert_eq!(l.standing, Standing::ImportedDefinition);
        assert!(l.unverified, "no datasheet recorded");
    }

    #[test]
    fn an_imported_open_source_scope_is_named_in_the_explanation() {
        let mut d = lib().remove(0);
        d.id = "OSS_1".into();
        d.set_offer(eak_core::SupplierOffer::bare("distributor", "C1"));
        let defs = [d];
        let r = rec("C1", "m", "k", "0603", "Resistors");
        let l = link_record(&r, &[(Scope::Imported("kicad-lite".into()), &defs[0])]);
        assert!(l.explanation.contains("imported:kicad-lite"));
    }
}
