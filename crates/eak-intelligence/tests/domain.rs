//! Domain intelligence: registry completeness against DESK and classifier behaviour on sample
//! designs. No model and no API key are involved anywhere in this file.

use eak_core::{Project, Requirement};
use eak_intelligence::domain::{
    classify_project, classify_text, registry, Maturity, Status, LIMITATIONS, NOT_IMPLEMENTED,
};
use std::collections::BTreeSet;

/// DESK `library/pcb_domains` at generation time (2026-11-12): 100 archetypes in 14 domains,
/// 17 overlays; 13 archetypes OFFLINE_TESTED and 87 DRAFT (`EVIDENCE_AND_MATURITY_POLICY.md`).
const DESK_ARCHETYPES: usize = 100;
const DESK_DOMAINS: usize = 14;
const DESK_OVERLAYS: usize = 17;

fn design(name: &str, reqs: &[&str]) -> Project {
    let mut p = Project::new(name);
    for (i, t) in reqs.iter().enumerate() {
        p.requirements.push(Requirement {
            id: format!("req_{i}"),
            text: (*t).into(),
        });
    }
    p
}

#[test]
fn registry_contains_every_desk_class() {
    let r = registry();
    assert_eq!(r.archetypes.len(), DESK_ARCHETYPES);
    assert_eq!(r.domains.len(), DESK_DOMAINS);
    assert_eq!(r.overlays.len(), DESK_OVERLAYS);
    let ids: BTreeSet<_> = r
        .archetypes
        .iter()
        .map(|a| a.archetype_id.as_str())
        .collect();
    assert_eq!(ids.len(), DESK_ARCHETYPES, "archetype ids are unique");
    let count = |m| {
        r.archetypes
            .iter()
            .filter(|a| a.maturity.status == m)
            .count()
    };
    assert_eq!(count(Maturity::OfflineTested), 13);
    assert_eq!(count(Maturity::Draft), 87);
    // Spot-check well-known DESK classes.
    for id in [
        "esp32_dev_board",
        "buck_converter",
        "brushed_dc_h_bridge",
        "battery_management_system",
        "can_bus_interface",
        "gnss_receiver",
    ] {
        assert!(r.archetype(id).is_some(), "{id} missing");
    }
}

#[test]
fn registry_is_internally_consistent_and_every_class_is_classifiable() {
    let r = registry();
    for a in &r.archetypes {
        assert!(
            r.domain(&a.domain).is_some(),
            "{}: unknown domain",
            a.archetype_id
        );
        assert!(
            !a.name.is_empty() && !a.description.is_empty(),
            "{}",
            a.archetype_id
        );
        for o in &a.typical_overlays {
            assert!(
                r.overlay(o).is_some(),
                "{}: unknown overlay {o}",
                a.archetype_id
            );
        }
        assert!(
            !a.classification.keywords.is_empty(),
            "{}: no classification keywords",
            a.archetype_id
        );
        assert!(a.maturity.status != Maturity::Blocked);
    }
    // Every domain lists only registered archetypes, and every archetype is in its domain.
    let in_domain: usize = r
        .domains
        .iter()
        .map(|d| {
            r.archetypes
                .iter()
                .filter(|a| a.domain == d.domain_id)
                .count()
        })
        .sum();
    assert_eq!(in_domain, DESK_ARCHETYPES);
}

#[test]
fn every_class_is_reachable_from_its_own_keywords() {
    // Each of the 100 classes must participate in classification: a design that names all of a
    // class's own keywords must rank that class first.
    let mut misses = Vec::new();
    for a in &registry().archetypes {
        let text = a
            .classification
            .keywords
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(" ; ");
        let c = classify_text(&text);
        if c.candidates[0].archetype_id != a.archetype_id {
            misses.push(format!(
                "{} -> {}",
                a.archetype_id, c.candidates[0].archetype_id
            ));
        }
    }
    assert!(misses.is_empty(), "{misses:#?}");
}

#[test]
fn esp32_dashboard_is_classified_as_an_esp32_board() {
    let p = design(
        "ESP32 dashboard",
        &["ESP32-S3 module with Wi-Fi and BLE, powered from USB-C at 5 V, 3.3 V logic"],
    );
    let c = classify_project(&p);
    assert_eq!(c.status, Status::Classified, "{:?}", c.evidence);
    assert_eq!(c.primary.as_deref(), Some("esp32_dev_board"));
    let top = &c.candidates[0];
    assert!(top
        .reasons
        .iter()
        .any(|r| r.contains("esp32") && r.contains("project name")));
    assert!(top.reasons.iter().any(|r| r.contains("protocols")));
    assert!(c.overlays.iter().any(|o| o == "rf_communication"));
    assert!(c.overlays.iter().any(|o| o == "usb_interface"));
    assert!(c.confidence > 0.5 && c.confidence <= 0.95);
}

#[test]
fn power_supply_is_classified_as_a_buck_converter() {
    let p = design(
        "12-24 V to 5 V 3 A buck board",
        &["Step-down buck converter, 12 V to 24 V input, 5 V 3 A output, 500 kHz switching"],
    );
    let c = classify_project(&p);
    assert_eq!(
        c.primary.as_deref(),
        Some("buck_converter"),
        "{:?}",
        c.candidates[0]
    );
    assert!(c.overlays.iter().any(|o| o == "switching_power"));
    assert!(c.candidates[0]
        .reasons
        .iter()
        .any(|r| r.contains("converter")));
}

#[test]
fn motor_driver_is_classified_as_a_brushed_h_bridge() {
    let p = design(
        "Brushed DC motor driver",
        &["H-bridge driver for a brushed DC motor, 12 V supply, 10 A peak, DRV8 class driver"],
    );
    let c = classify_project(&p);
    assert_eq!(c.primary.as_deref(), Some("brushed_dc_h_bridge"));
    assert!(c.overlays.iter().any(|o| o == "motor_drive"));
    assert!(c
        .required_component_families
        .iter()
        .any(|f| f == "gate_driver"));
}

#[test]
fn mains_involvement_is_never_resolved_silently() {
    let p = design(
        "ESP32 relay node",
        &["ESP32 controller switching a 230 V AC mains relay, Wi-Fi"],
    );
    let c = classify_project(&p);
    assert_ne!(
        c.status,
        Status::Classified,
        "mains + controller must need review"
    );
    assert!(c.conflicts.iter().any(|x| x.contains("mains")));
    assert!(c.overlays.iter().any(|o| o == "mains_safety_isolation"));
}

#[test]
fn empty_design_is_unclassified_not_guessed() {
    let c = classify_project(&Project::new("untitled"));
    assert_eq!(c.status, Status::Unclassified);
    assert!(c.primary.is_none() && c.guidance.is_none());
    assert!(!c.missing_information.is_empty());
}

#[test]
fn classification_is_deterministic() {
    let p = design("ESP32 dashboard", &["ESP32 with Wi-Fi, USB-C, 3.3 V"]);
    let a = serde_json::to_string(&classify_project(&p)).unwrap();
    let b = serde_json::to_string(&classify_project(&p)).unwrap();
    assert_eq!(a, b);
}

#[test]
fn guidance_is_capped_by_the_desk_maturity_label() {
    // A DRAFT class: prompts only, never requirements; nothing implies verification.
    let draft = classify_text("flyback converter, 12 v output, 5 w");
    assert_eq!(
        draft.primary.as_deref(),
        Some("flyback_converter"),
        "{:?}",
        draft.candidates[0]
    );
    let g = draft.guidance.expect("guidance");
    assert_eq!(g.ceiling.label, Maturity::Draft);
    assert!(!g.ceiling.may_present_as_requirement);
    assert_eq!(g.heading, "Review prompts (unverified)");

    // An OFFLINE_TESTED class: still not a requirement, still not verified or hardware-tested.
    let tested = classify_text("esp32 board with wifi and usb");
    let g = tested.guidance.expect("guidance");
    assert_eq!(g.ceiling.label, Maturity::OfflineTested);
    assert!(!g.ceiling.may_present_as_requirement);
    assert!(g.ceiling.claim.contains("Not verified"));

    // No label in the shipped data may exceed OFFLINE_TESTED.
    for a in &registry().archetypes {
        assert!(matches!(
            a.maturity.status,
            Maturity::Draft | Maturity::OfflineTested
        ));
    }
    // Only human-verified labels may be presented as requirements; BLOCKED shows nothing.
    assert!(
        Maturity::VerifiedAgainstSource
            .ceiling()
            .may_present_as_requirement
    );
    assert!(Maturity::LiveVerified.ceiling().may_present_as_requirement);
    for m in [
        Maturity::Draft,
        Maturity::Advisory,
        Maturity::Implemented,
        Maturity::OfflineTested,
        Maturity::NeedsHardwareValidation,
        Maturity::Blocked,
    ] {
        assert!(!m.ceiling().may_present_as_requirement, "{m:?}");
    }
    assert!(!Maturity::Blocked.ceiling().may_show_guidance);
}

#[test]
fn results_always_state_what_is_not_implemented() {
    let c = classify_text("esp32");
    assert_eq!(c.not_implemented.len(), NOT_IMPLEMENTED.len());
    assert_eq!(c.limitations.len(), LIMITATIONS.len());
    assert_eq!(c.registry_classes, DESK_ARCHETYPES);
}
