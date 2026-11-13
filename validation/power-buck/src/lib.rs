//! Power-buck validation project: a 12-24 V to 5 V / 3 A synchronous buck board authored
//! end to end through EAK's typed command engine, verified with EAK's ERC/DRC/MECH and
//! compiled with EAK's manufacturing compiler.

pub mod calcs;
pub mod design;
pub mod parts;

use eak_runtime::Engine;
use eak_validation::{drc, erc, mech, Report};

/// Run the three EAK verification passes against the current revision.
pub fn verify(e: &Engine) -> (Report, Report, Report) {
    let p = e.project();
    (erc::run(p), drc::run(p), mech::run(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::Project;
    use eak_validation::Severity;

    fn built() -> Engine {
        let mut e = Engine::in_memory(Project::new("t"));
        design::build(&mut e).expect("design builds through the command engine");
        e
    }

    fn dump(r: &Report) -> String {
        r.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .map(|f| format!("{} {}", f.rule_id, f.message))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn verification_has_no_errors() {
        let e = built();
        let (erc_r, drc_r, mech_r) = verify(&e);
        assert_eq!(erc_r.errors, 0, "ERC:\n{}", dump(&erc_r));
        assert_eq!(drc_r.errors, 0, "DRC:\n{}", dump(&drc_r));
        assert_eq!(mech_r.errors, 0, "MECH:\n{}", dump(&mech_r));
    }

    #[test]
    fn every_net_is_physically_connected() {
        let e = built();
        let c = eak_engineering::copper::analyse(e.project());
        for (net, groups) in &c.net_groups {
            assert_eq!(*groups, 1, "net {net} is split into {groups} copper groups");
        }
        assert!(c.shorts.is_empty(), "shorts: {:?}", c.shorts);
    }

    #[test]
    fn calculations_meet_their_requirements() {
        let e = built();
        let r = calcs::run(e.project()).unwrap();
        for l in &r.lines {
            assert!(
                !l.status.starts_with("FAIL") || l.item.contains("JEDEC"),
                "{} -> {}",
                l.item,
                l.status
            );
        }
    }

    #[test]
    fn injected_narrow_power_track_is_caught_by_drc() {
        // Regression guard: the declared SW current must make EAK reject an undersized track.
        let mut e = built();
        let rev = e.project().revision;
        let r = e.execute(eak_runtime::CommandRequest {
            expected_revision: rev,
            actor: eak_runtime::Actor::human(),
            command: eak_runtime::Command::AddTrack {
                layer: eak_core::CopperLayer::Top,
                a: eak_core::Point::mm(42.0, 30.0),
                b: eak_core::Point::mm(42.0, 32.0),
                width: Some(eak_core::geom::mm(0.3)),
                net: "SW".into(),
            },
            approval: None,
        });
        assert!(r.accepted);
        let d = eak_validation::drc::run(e.project());
        assert!(d
            .findings
            .iter()
            .any(|f| f.rule_id == "DRC-008" && f.severity == Severity::Error));
    }
}
