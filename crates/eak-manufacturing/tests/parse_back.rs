//! Parse-back verifier (manufacturing integrity, item 2): the written Gerber and Excellon files are
//! read back with a parser of their own, not the generator's code, and compared with the board
//! database. Per copper layer, flashes must equal pads plus via lands and draws must equal tracks
//! (zone fills are regions, counted apart). The profile's extents must equal the outline's, and
//! the plated drill file must hold one hole per through via and through-hole pad, size by size.

use eak_core::{CopperLayer, Nm, PadKind, Project, ViaKind};
use eak_manufacturing::{compile, file_stem};
use eak_runtime::{fixtures, Engine};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Default, PartialEq)]
struct Gerber {
    flashes: usize,
    draws: usize,
    region_ops: usize,
    /// (min x, min y, max x, max y) over every coordinate outside regions, in nm.
    extents: Option<(Nm, Nm, Nm, Nm)>,
}

/// A minimal RS-274X reader: coordinates are integers in the file's 4.6 mm format (nm), modal X/Y.
fn read_gerber(text: &str) -> Gerber {
    let mut g = Gerber::default();
    let (mut x, mut y, mut region) = (0, 0, false);
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with("G04") || line.starts_with('%') {
            continue;
        }
        if line.starts_with("G36") {
            region = true;
            continue;
        }
        if line.starts_with("G37") {
            region = false;
            continue;
        }
        let op = ["D01*", "D02*", "D03*"]
            .into_iter()
            .find(|op| line.ends_with(op));
        let Some(op) = op else { continue };
        x = field(line, 'X').unwrap_or(x);
        y = field(line, 'Y').unwrap_or(y);
        if region {
            g.region_ops += 1;
            continue;
        }
        match op {
            "D03*" => g.flashes += 1,
            "D01*" => g.draws += 1,
            _ => {}
        }
        let e = g.extents.get_or_insert((x, y, x, y));
        *e = (e.0.min(x), e.1.min(y), e.2.max(x), e.3.max(y));
    }
    g
}

fn field(line: &str, key: char) -> Option<Nm> {
    let at = line.find(key)? + 1;
    let digits: String = line[at..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    digits.parse().ok()
}

/// A minimal Excellon reader: tool sizes from the header, then one hole per coordinate line.
fn read_excellon(text: &str) -> BTreeMap<Nm, usize> {
    let (mut sizes, mut holes) = (BTreeMap::new(), BTreeMap::new());
    let (mut header, mut tool) = (true, 0u32);
    for raw in text.lines() {
        let line = raw.trim();
        if line == "%" {
            header = false;
        } else if let Some(rest) = line.strip_prefix('T') {
            let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            let n: u32 = n.parse().unwrap_or(0);
            if header {
                let mm: f64 = rest.split('C').nth(1).and_then(|d| d.parse().ok()).unwrap();
                sizes.insert(n, (mm * 1e6).round() as Nm);
            } else {
                tool = n;
            }
        } else if !header && line.starts_with('X') {
            *holes.entry(sizes[&tool]).or_insert(0) += 1;
        }
    }
    holes
}

fn layer_file(p: &Project, layer: CopperLayer) -> String {
    let stem = file_stem(p);
    match layer {
        CopperLayer::Top => format!("{stem}-F_Cu.gtl"),
        CopperLayer::Inner(n) => format!("{stem}-In{n}_Cu.g{}", n + 1),
        CopperLayer::Bottom => format!("{stem}-B_Cu.gbl"),
    }
}

fn check_board(p: &Project, dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    compile(p, &[], dir).expect("compile");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).expect(name);
    let st = &p.pcb.board.stackup;
    for layer in st.layer_ids() {
        let g = read_gerber(&read(&layer_file(p, layer)));
        let mut pads = 0;
        for fp in p.pcb.footprints.values() {
            if let Some(def) = p.library.get(&fp.component_id) {
                pads += def
                    .footprint
                    .pads
                    .iter()
                    .filter(|pad| pad.kind == PadKind::Tht || fp.side.copper() == layer)
                    .count();
            }
        }
        let vias = p
            .pcb
            .vias
            .values()
            .filter(|v| v.layers(st).contains(&layer))
            .count();
        let tracks = p.pcb.tracks.values().filter(|t| t.layer == layer).count();
        assert_eq!(g.flashes, pads + vias, "{layer:?}: flashes vs pads + vias");
        assert_eq!(g.draws, tracks, "{layer:?}: draws vs tracks");
    }
    // The profile: its extents are the outline's.
    let g = read_gerber(&read(&format!("{}-Edge_Cuts.gm1", file_stem(p))));
    let o = &p.pcb.board.outline;
    let want = (
        o.iter().map(|v| v.x).min().unwrap(),
        o.iter().map(|v| v.y).min().unwrap(),
        o.iter().map(|v| v.x).max().unwrap(),
        o.iter().map(|v| v.y).max().unwrap(),
    );
    assert_eq!(g.extents, Some(want), "outline extents");
    assert_eq!(g.flashes, 0);
    // Plated through holes: count and size per tool (the file states sizes to the micrometre).
    let mut want: BTreeMap<Nm, usize> = BTreeMap::new();
    let um = |d: Nm| (d + 500) / 1000 * 1000;
    for v in p.pcb.vias.values().filter(|v| v.kind() == ViaKind::Through) {
        *want.entry(um(v.drill)).or_insert(0) += 1;
    }
    for fp in p.pcb.footprints.values() {
        if let Some(def) = p.library.get(&fp.component_id) {
            for pad in def.footprint.pads.iter().filter(|p| p.kind == PadKind::Tht) {
                if let Some(d) = pad.drill {
                    *want.entry(um(d)).or_insert(0) += 1;
                }
            }
        }
    }
    let got = read_excellon(&read(&format!("{}-PTH.drl", file_stem(p))));
    assert_eq!(got, want, "drill sizes and counts");
    assert!(!want.is_empty(), "the fixture has plated holes");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn two_layer_reference_board_reads_back_as_the_database() {
    let mut e = Engine::in_memory(Project::new("parse-back"));
    fixtures::build_schematic(&mut e).expect("schematic");
    fixtures::build_pcb(&mut e).expect("pcb");
    let p = e.project();
    assert!(!p.pcb.tracks.is_empty() && !p.pcb.footprints.is_empty());
    let dir = std::env::temp_dir().join(format!("eak-parse-back-2-{}", std::process::id()));
    check_board(p, &dir);
}

#[test]
fn four_layer_board_reads_back_as_the_database() {
    let mut e = Engine::in_memory(Project::new("parse-back-4"));
    fixtures::build_schematic(&mut e).expect("schematic");
    fixtures::build_pcb_layers(&mut e, 4).expect("pcb");
    let dir = std::env::temp_dir().join(format!("eak-parse-back-4-{}", std::process::id()));
    check_board(e.project(), &dir);
}

#[test]
fn the_reader_counts_what_it_is_given() {
    let g = read_gerber("%FSLAX46Y46*%\nG04 X99 D03*\nD10*\nX0Y0D02*\nX1000Y0D01*\nX500Y-200D03*\nG36*\nX0Y0D02*\nX5Y5D01*\nG37*\nM02*\n");
    assert_eq!((g.flashes, g.draws, g.region_ops), (1, 1, 2));
    assert_eq!(g.extents, Some((0, -200, 1000, 0)));
    let d = read_excellon(
        "M48\nT1C0.300\nT2C1.000\n%\nT1\nX1.0Y1.0\nX2.0Y1.0\nT2\nX3.0Y3.0\nT0\nM30\n",
    );
    assert_eq!(d, BTreeMap::from([(300_000, 2), (1_000_000, 1)]));
}
