//! Interactive routing: the "route step" API.
//!
//! The desktop route tool calls [`route_step`] at pointer rate. Given the last committed point
//! of a track being drawn and the cursor, it returns a path on one copper layer that is legal
//! against the exact copper geometry and the net-class rules:
//!
//! 1. the track width comes from the net class (or an explicit override), the clearance is the
//!    larger of the net-class clearance and the board minimum;
//! 2. the direct 45-degree (or 90-degree) two-segment paths are tried first;
//! 3. if both are blocked, a local A* walkaround on a fine grid finds a path around the
//!    obstacles (conflict mode `walkaround`, the default; `block` skips this step). A grid
//!    cell is only passable when a track centred anywhere within 0.75 of a
//!    pitch of it keeps the clearance, so a straight link between neighbouring passable cells
//!    is legal by construction; every segment is still re-checked exactly afterwards;
//! 4. if no legal path reaches the cursor, the longest legal prefix of the direct path is
//!    returned with `reached = false`.
//!
//! A layer switch is requested with `via_at_start`: a through via is placed at `from` (checked on
//! every copper layer) and the path continues on `layer`.
//!
//! The function never returns a segment or via that violates clearance to other-net copper or
//! the board-edge clearance. Zones are not obstacles: their fill is derived and retreats from new
//! copper (same rule as the autorouter). The result is a proposal only; the caller commits it as
//! `AddTrack`/`AddVia` steps of one `Batch` command.

use crate::copper::{distance, items, CopperItem, Core, Shape};
use eak_core::geom::{point_in_polygon, segment_segment_distance};
use eak_core::{CopperLayer, Nm, Point, Project};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Corner style of the routed path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BendMode {
    /// Horizontal, vertical and 45-degree segments.
    #[default]
    Diagonal45,
    /// Horizontal and vertical segments only.
    Orthogonal90,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteStepRequest {
    pub net: String,
    pub layer: CopperLayer,
    pub from: Point,
    pub to: Point,
    #[serde(default)]
    pub mode: BendMode,
    /// Track width override; the net-class width when omitted.
    #[serde(default)]
    pub width: Option<Nm>,
    /// Place a through via at `from` before continuing on `layer` (layer switch).
    #[serde(default)]
    pub via_at_start: bool,
    /// What to do when other-net copper is in the way.
    #[serde(default)]
    pub conflict: ConflictMode,
    /// Via drill and diameter override for `via_at_start`; the net-class via when omitted.
    #[serde(default)]
    pub via: Option<ViaSize>,
}

/// How the route step treats other-net copper in the way of the direct path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictMode {
    /// Go around obstacles (local A* walkaround), else stop before them.
    #[default]
    Walkaround,
    /// Stop before the first obstacle (no walkaround).
    Block,
}

/// A via size chosen for a layer switch (a preset or the net class).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViaSize {
    pub drill: Nm,
    pub diameter: Nm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteSegment {
    pub a: Point,
    pub b: Point,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepVia {
    pub position: Point,
    pub drill: Nm,
    pub diameter: Nm,
}

/// One clearance measurement shown to the user while routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearanceFeedback {
    /// "pad U1.3", "track trk_..", "via ..", or "board edge".
    pub object: String,
    /// Copper-to-copper gap in nm (negative = overlap).
    pub measured: f64,
    pub required: Nm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteStepResult {
    pub net: String,
    pub layer: CopperLayer,
    pub width: Nm,
    pub clearance: Nm,
    pub mode: BendMode,
    /// Legal segments, end to end from `from` (empty if nothing legal could be drawn).
    pub segments: Vec<RouteSegment>,
    /// Via placed at `from` when a layer switch was requested and it is legal.
    pub via: Option<StepVia>,
    /// Last point of the legal path (`from` if nothing was drawn).
    pub end: Point,
    /// Whether the legal path ends at the cursor.
    pub reached: bool,
    /// Whether the path goes around obstacles rather than taking the direct route.
    pub walkaround: bool,
    /// Objects that block the direct path (live clearance feedback).
    pub blocking: Vec<ClearanceFeedback>,
    /// Smallest gap kept by the returned path, if it has segments.
    pub min_gap: Option<ClearanceFeedback>,
    /// Why nothing (or only part) could be routed.
    pub message: Option<String>,
}

/// Upper bound on walkaround grid cells and A* expansions (keeps a step interactive).
const MAX_CELLS: i64 = 160_000;
const MAX_EXPANSIONS: usize = 120_000;

struct Rules<'a> {
    obstacles: Vec<&'a CopperItem>,
    outline: &'a [Point],
    half_width: f64,
    clearance: f64,
    edge_clearance: f64,
}

impl Rules<'_> {
    /// Clearance violations of a track segment of half-width `hw` (exact geometry).
    fn violations_hw(&self, a: Point, b: Point, hw: f64) -> Vec<ClearanceFeedback> {
        let s = Shape {
            core: Core::Segment(a, b),
            radius: hw,
        };
        let mut out = Vec::new();
        for it in &self.obstacles {
            let d = distance(&s, &it.shape);
            if d < self.clearance - 1.0 {
                out.push(ClearanceFeedback {
                    object: it.label(),
                    measured: d,
                    required: self.clearance as Nm,
                });
            }
        }
        if let Some(e) = self.edge_violation(a, b, hw) {
            out.push(e);
        }
        out
    }

    fn violations(&self, a: Point, b: Point) -> Vec<ClearanceFeedback> {
        self.violations_hw(a, b, self.half_width)
    }

    fn edge_violation(&self, a: Point, b: Point, hw: f64) -> Option<ClearanceFeedback> {
        let o = self.outline;
        if o.len() < 3 {
            return None;
        }
        let mut d = f64::INFINITY;
        for i in 0..o.len() {
            d = d.min(segment_segment_distance(a, b, o[i], o[(i + 1) % o.len()]));
        }
        let gap = d - hw;
        let inside = point_in_polygon(a, o) && point_in_polygon(b, o);
        if !inside || gap < self.edge_clearance - 1.0 {
            Some(ClearanceFeedback {
                object: "board edge".into(),
                measured: if inside { gap } else { -gap.abs() },
                required: self.edge_clearance as Nm,
            })
        } else {
            None
        }
    }

    fn legal(&self, a: Point, b: Point) -> bool {
        self.violations(a, b).is_empty()
    }

    /// Smallest copper gap kept by a set of segments.
    fn min_gap(&self, segs: &[RouteSegment]) -> Option<ClearanceFeedback> {
        let mut best: Option<ClearanceFeedback> = None;
        for sg in segs {
            let s = Shape {
                core: Core::Segment(sg.a, sg.b),
                radius: self.half_width,
            };
            for it in &self.obstacles {
                let d = distance(&s, &it.shape);
                if best.as_ref().is_none_or(|b| d < b.measured) {
                    best = Some(ClearanceFeedback {
                        object: it.label(),
                        measured: d,
                        required: self.clearance as Nm,
                    });
                }
            }
        }
        best
    }
}

/// Direct two-segment paths from `a` to `b` in the given bend mode (without zero-length
/// segments), in preference order.
fn direct_paths(a: Point, b: Point, mode: BendMode) -> Vec<Vec<RouteSegment>> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let corners: Vec<Point> = match mode {
        BendMode::Orthogonal90 => vec![Point::new(b.x, a.y), Point::new(a.x, b.y)],
        BendMode::Diagonal45 => {
            let d = dx.abs().min(dy.abs());
            // Straight first then diagonal, or diagonal first then straight.
            let straight = Point::new(b.x - d * dx.signum(), b.y - d * dy.signum());
            let diag = Point::new(a.x + d * dx.signum(), a.y + d * dy.signum());
            vec![straight, diag]
        }
    };
    let mut out: Vec<Vec<RouteSegment>> = Vec::new();
    for c in corners {
        let segs: Vec<RouteSegment> = [(a, c), (c, b)]
            .into_iter()
            .filter(|(p, q)| p != q)
            .map(|(a, b)| RouteSegment { a, b })
            .collect();
        if !out.contains(&segs) {
            out.push(segs);
        }
    }
    out
}

/// Compute one interactive route step. Never returns geometry that violates clearance.
pub fn route_step(p: &Project, req: &RouteStepRequest) -> RouteStepResult {
    let class = p.constraints.class_for(&req.net);
    let width = req.width.unwrap_or(class.track_width);
    let clearance = class.clearance.max(p.constraints.min_clearance);
    let mut res = RouteStepResult {
        net: req.net.clone(),
        layer: req.layer,
        width,
        clearance,
        mode: req.mode,
        segments: vec![],
        via: None,
        end: req.from,
        reached: false,
        walkaround: false,
        blocking: vec![],
        min_gap: None,
        message: None,
    };
    if req.net.is_empty() {
        res.message = Some("start the route on copper that belongs to a net".into());
        return res;
    }
    if width <= 0 {
        res.message = Some("track width must be positive".into());
        return res;
    }
    if !p.pcb.board.stackup.layer_ids().contains(&req.layer) {
        res.message = Some(format!("layer {:?} is not in the stackup", req.layer));
        return res;
    }
    let copper = items(p);
    let outline = &p.pcb.board.outline;
    let edge_clearance = p.constraints.board_edge_clearance as f64;
    // Update PCB stages new footprints beside the board: every segment from there fails the
    // board-edge rule, so say why instead of returning an unexplained empty proposal.
    if outline.len() >= 3 && !point_in_polygon(req.from, outline) {
        res.blocking.push(ClearanceFeedback {
            object: "board edge".into(),
            measured: 0.0,
            required: p.constraints.board_edge_clearance,
        });
        res.message = Some(
            "the route starts outside the board outline; move the footprints onto the board first"
                .into(),
        );
        return res;
    }

    if req.via_at_start {
        let size = req.via.unwrap_or(ViaSize {
            drill: class.via_drill,
            diameter: class.via_diameter,
        });
        if size.drill <= 0 || size.diameter <= size.drill {
            res.message = Some("the via diameter must be larger than its drill".into());
            return res;
        }
        let via_rules = Rules {
            obstacles: copper.iter().filter(|i| i.net != req.net).collect(),
            outline,
            half_width: size.diameter as f64 / 2.0,
            clearance: clearance as f64,
            edge_clearance,
        };
        let v = via_rules.violations(req.from, req.from);
        if !v.is_empty() {
            res.blocking = v;
            res.message = Some("a via here would violate clearance".into());
            return res;
        }
        res.via = Some(StepVia {
            position: req.from,
            drill: size.drill,
            diameter: size.diameter,
        });
    }

    let rules = Rules {
        obstacles: copper
            .iter()
            .filter(|i| i.layer == req.layer && i.net != req.net)
            .collect(),
        outline,
        half_width: width as f64 / 2.0,
        clearance: clearance as f64,
        edge_clearance,
    };
    if req.from == req.to {
        res.reached = true;
        return res;
    }

    // 1. Direct paths.
    let candidates = direct_paths(req.from, req.to, req.mode);
    for segs in &candidates {
        if segs.iter().all(|s| rules.legal(s.a, s.b)) {
            res.min_gap = rules.min_gap(segs);
            res.segments = segs.clone();
            res.end = req.to;
            res.reached = true;
            return res;
        }
    }
    for s in &candidates[0] {
        for v in rules.violations(s.a, s.b) {
            if !res.blocking.iter().any(|b| b.object == v.object) {
                res.blocking.push(v);
            }
        }
    }

    // 2. Walkaround (not in block mode, which stops before the obstacle).
    let around = match req.conflict {
        ConflictMode::Walkaround => walkaround(&rules, req),
        ConflictMode::Block => None,
    };
    if let Some(segs) = around {
        if !segs.is_empty() && segs.iter().all(|s| rules.legal(s.a, s.b)) {
            res.min_gap = rules.min_gap(&segs);
            res.end = segs.last().map_or(req.from, |s| s.b);
            res.reached = res.end == req.to;
            res.segments = segs;
            res.walkaround = true;
            if !res.reached {
                res.message = Some(
                    "the cursor point itself is too close to other copper; stopped at the nearest legal point"
                        .into(),
                );
            }
            return res;
        }
    }

    // 3. Longest legal prefix of the preferred direct path.
    let mut segs = Vec::new();
    for s in &candidates[0] {
        if rules.legal(s.a, s.b) {
            segs.push(s.clone());
            continue;
        }
        if let Some(e) = legal_prefix(&rules, s.a, s.b) {
            segs.push(RouteSegment { a: s.a, b: e });
        }
        break;
    }
    res.min_gap = rules.min_gap(&segs);
    res.end = segs.last().map_or(req.from, |s| s.b);
    res.segments = segs;
    res.message = Some("no legal path to the cursor; stopped before the obstacle".into());
    res
}

/// Furthest point `e` on `a..b` with `a..e` legal (binary search to 1 µm), if any.
fn legal_prefix(rules: &Rules, a: Point, b: Point) -> Option<Point> {
    let at = |t: f64| {
        Point::new(
            a.x + ((b.x - a.x) as f64 * t).round() as Nm,
            a.y + ((b.y - a.y) as f64 * t).round() as Nm,
        )
    };
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    let len = a.dist(b).max(1.0);
    while (hi - lo) * len > 1_000.0 {
        let mid = (lo + hi) / 2.0;
        if rules.legal(a, at(mid)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let e = at(lo);
    (e != a && rules.legal(a, e)).then_some(e)
}

/// Grid walkaround from `req.from` to `req.to`. The grid is anchored at `from`; the last step
/// goes from the cell nearest the cursor to the cursor itself when that link is legal and keeps
/// the bend mode.
fn walkaround(rules: &Rules, req: &RouteStepRequest) -> Option<Vec<RouteSegment>> {
    let (from, to) = (req.from, req.to);
    let span = (to.x - from.x).abs().max((to.y - from.y).abs());
    let margin = span / 2 + 3_000_000;
    let mut pitch = ((rules.half_width + rules.clearance) / 2.0).max(25_000.0) as Nm;
    while ((span + 2 * margin) / pitch + 1).pow(2) > MAX_CELLS {
        pitch = pitch * 5 / 4 + 1;
    }
    let lo = Point::new(from.x.min(to.x) - margin, from.y.min(to.y) - margin);
    let hi = Point::new(from.x.max(to.x) + margin, from.y.max(to.y) + margin);
    // Cell (i, j) is the point from + (i, j) * pitch.
    let (i_lo, i_hi) = ((lo.x - from.x) / pitch - 1, (hi.x - from.x) / pitch + 1);
    let (j_lo, j_hi) = ((lo.y - from.y) / pitch - 1, (hi.y - from.y) / pitch + 1);
    let at = |i: i64, j: i64| Point::new(from.x + i * pitch, from.y + j * pitch);
    let goal = (
        ((to.x - from.x) as f64 / pitch as f64).round() as i64,
        ((to.y - from.y) as f64 / pitch as f64).round() as i64,
    );

    // A cell is free when a disc 0.75 pitch wider than the track keeps every clearance.
    let cell_hw = rules.half_width + pitch as f64 * 0.75;
    let mut cache: HashMap<(i64, i64), bool> = HashMap::new();
    let mut free = |c: (i64, i64)| -> bool {
        if c == (0, 0) {
            return true;
        }
        *cache.entry(c).or_insert_with(|| {
            let pt = at(c.0, c.1);
            rules.violations_hw(pt, pt, cell_hw).is_empty()
        })
    };

    let moves: &[(i64, i64)] = match req.mode {
        BendMode::Diagonal45 => &[
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ],
        BendMode::Orthogonal90 => &[(1, 0), (-1, 0), (0, 1), (0, -1)],
    };
    let h = |c: (i64, i64)| -> u64 {
        let dx = (c.0 - goal.0).unsigned_abs();
        let dy = (c.1 - goal.1).unsigned_abs();
        match req.mode {
            BendMode::Diagonal45 => dx.min(dy) * 14 + (dx.max(dy) - dx.min(dy)) * 10,
            BendMode::Orthogonal90 => (dx + dy) * 10,
        }
    };
    // State: cell + incoming move index (bends cost extra, which keeps paths clean).
    type State = ((i64, i64), usize);
    let mut g: HashMap<State, u64> = HashMap::new();
    let mut parent: HashMap<State, State> = HashMap::new();
    let mut heap = BinaryHeap::new();
    let start: State = ((0, 0), usize::MAX);
    g.insert(start, 0);
    heap.push(Reverse((h((0, 0)), 0u64, start)));
    let mut expansions = 0;
    let mut found = None;
    while let Some(Reverse((_, cost, s))) = heap.pop() {
        if g.get(&s).is_some_and(|&v| cost > v) {
            continue;
        }
        if s.0 == goal {
            found = Some(s);
            break;
        }
        expansions += 1;
        if expansions > MAX_EXPANSIONS {
            return None;
        }
        let (ci, cj) = s.0;
        for (k, &(di, dj)) in moves.iter().enumerate() {
            let n = (ci + di, cj + dj);
            if n.0 < i_lo || n.0 > i_hi || n.1 < j_lo || n.1 > j_hi || !free(n) {
                continue;
            }
            let diag = di != 0 && dj != 0;
            if diag && (!free((ci + di, cj)) || !free((ci, cj + dj))) {
                continue;
            }
            let bend = if s.1 != usize::MAX && s.1 != k { 3 } else { 0 };
            let ns: State = (n, k);
            let nc = cost + if diag { 14 } else { 10 } + bend;
            if g.get(&ns).is_none_or(|&v| nc < v) {
                g.insert(ns, nc);
                parent.insert(ns, s);
                heap.push(Reverse((nc + h(n), nc, ns)));
            }
        }
    }
    let mut cur = found?;
    let mut cells = vec![cur.0];
    while let Some(&pr) = parent.get(&cur) {
        cells.push(pr.0);
        cur = pr;
    }
    cells.reverse();
    let mut pts: Vec<Point> = cells.iter().map(|&(i, j)| at(i, j)).collect();
    let last = *pts.last()?;
    // Finish at the exact cursor with a legal one- or two-segment tail in the bend mode (the
    // cursor is generally not on the walkaround grid).
    if last != to {
        if let Some(tail) = direct_paths(last, to, req.mode)
            .into_iter()
            .find(|segs| segs.iter().all(|s| rules.legal(s.a, s.b)))
        {
            pts.extend(tail.iter().map(|s| s.b));
        }
    }
    // Merge collinear runs.
    let mut segs: Vec<RouteSegment> = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a == b {
            continue;
        }
        if let Some(prev) = segs.last_mut() {
            let (d1x, d1y) = (prev.b.x - prev.a.x, prev.b.y - prev.a.y);
            let (d2x, d2y) = (b.x - a.x, b.y - a.y);
            let collinear = d1x as i128 * d2y as i128 == d1y as i128 * d2x as i128
                && (d1x.signum(), d1y.signum()) == (d2x.signum(), d2y.signum());
            if collinear {
                prev.b = b;
                continue;
            }
        }
        segs.push(RouteSegment { a, b });
    }
    Some(segs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::{Track, Via};

    fn board() -> Project {
        let mut p = Project::new("route-step");
        p.pcb.board.outline = vec![
            Point::mm(0.0, 0.0),
            Point::mm(50.0, 0.0),
            Point::mm(50.0, 40.0),
            Point::mm(0.0, 40.0),
        ];
        p
    }

    fn add_track(p: &mut Project, id: &str, net: &str, a: Point, b: Point, w: Nm) {
        p.pcb.tracks.insert(
            id.into(),
            Track {
                id: id.into(),
                layer: CopperLayer::Top,
                width: w,
                a,
                b,
                net: net.into(),
                arc_deg: None,
            },
        );
    }

    fn req(from: Point, to: Point, mode: BendMode) -> RouteStepRequest {
        RouteStepRequest {
            net: "SIG".into(),
            layer: CopperLayer::Top,
            from,
            to,
            mode,
            width: None,
            via_at_start: false,
            conflict: ConflictMode::Walkaround,
            via: None,
        }
    }

    /// Independent exact check: every segment keeps the clearance to other-net copper on its
    /// layer, and the segments are connected end to end.
    fn assert_legal(p: &Project, r: &RouteStepResult) {
        let copper = items(p);
        for s in &r.segments {
            let shape = Shape {
                core: Core::Segment(s.a, s.b),
                radius: r.width as f64 / 2.0,
            };
            for it in copper
                .iter()
                .filter(|i| i.layer == r.layer && i.net != r.net)
            {
                let d = distance(&shape, &it.shape);
                assert!(
                    d >= r.clearance as f64 - 1.0,
                    "segment {s:?} is {d} nm from {} (needs {})",
                    it.label(),
                    r.clearance
                );
            }
        }
        for w in r.segments.windows(2) {
            assert_eq!(w[0].b, w[1].a);
        }
    }

    fn assert_mode(r: &RouteStepResult) {
        for s in &r.segments {
            let (dx, dy) = (s.b.x - s.a.x, s.b.y - s.a.y);
            match r.mode {
                BendMode::Orthogonal90 => assert!(dx == 0 || dy == 0, "{s:?}"),
                BendMode::Diagonal45 => {
                    assert!(dx == 0 || dy == 0 || dx.abs() == dy.abs(), "{s:?}")
                }
            }
        }
    }

    // Written without running (no local builds in this phase): label "unrun".
    #[test]
    fn start_outside_the_board_is_explained_not_silently_empty() {
        let p = board();
        let from = Point::mm(60.0, 5.0);
        let r = route_step(&p, &req(from, Point::mm(20.0, 5.0), BendMode::Diagonal45));
        assert!(r.segments.is_empty() && !r.reached);
        assert!(r
            .message
            .as_deref()
            .unwrap_or("")
            .contains("outside the board outline"));
        assert!(r.blocking.iter().any(|b| b.object == "board edge"));
    }

    #[test]
    fn free_space_routes_directly_with_net_class_width() {
        let p = board();
        let (from, to) = (Point::mm(5.0, 5.0), Point::mm(20.0, 10.0));
        let r = route_step(&p, &req(from, to, BendMode::Diagonal45));
        assert!(r.reached && !r.walkaround);
        assert_eq!(r.width, p.constraints.default_class.track_width);
        assert_eq!(r.segments.len(), 2);
        assert_eq!(r.segments[0].a, from);
        assert_eq!(r.segments[1].b, to);
        assert_mode(&r);
    }

    #[test]
    fn orthogonal_mode_uses_only_horizontal_and_vertical_segments() {
        let p = board();
        let r = route_step(
            &p,
            &req(
                Point::mm(5.0, 5.0),
                Point::mm(20.0, 12.0),
                BendMode::Orthogonal90,
            ),
        );
        assert!(r.reached);
        assert_mode(&r);
    }

    #[test]
    fn walks_around_a_blocking_track_of_another_net() {
        let mut p = board();
        // A wall of another net between start and cursor, with room above it.
        add_track(
            &mut p,
            "wall",
            "GND",
            Point::mm(15.0, 2.0),
            Point::mm(15.0, 25.0),
            500_000,
        );
        let r = route_step(
            &p,
            &req(
                Point::mm(5.0, 10.0),
                Point::mm(25.0, 10.0),
                BendMode::Diagonal45,
            ),
        );
        assert!(r.reached, "{r:?}");
        assert!(r.walkaround);
        assert!(r.blocking.iter().any(|b| b.object == "track wall"));
        assert_legal(&p, &r);
        assert_mode(&r);
        let g = r.min_gap.unwrap();
        assert!(g.measured >= r.clearance as f64 - 1.0);
    }

    #[test]
    fn enclosed_cursor_stops_before_the_obstacle() {
        let mut p = board();
        // A closed box of another net around the cursor: no legal way in.
        let (a, b, c, d) = (
            Point::mm(20.0, 5.0),
            Point::mm(30.0, 5.0),
            Point::mm(30.0, 15.0),
            Point::mm(20.0, 15.0),
        );
        for (i, (s, e)) in [(a, b), (b, c), (c, d), (d, a)].into_iter().enumerate() {
            add_track(&mut p, &format!("box{i}"), "GND", s, e, 300_000);
        }
        let r = route_step(
            &p,
            &req(
                Point::mm(5.0, 10.0),
                Point::mm(25.0, 10.0),
                BendMode::Diagonal45,
            ),
        );
        assert!(!r.reached);
        assert!(r.message.is_some());
        assert!(!r.segments.is_empty(), "the legal prefix is drawn");
        assert!(r.end.x < Point::mm(20.0, 0.0).x);
        assert_legal(&p, &r);
    }

    // Slice C3 (unrun: no local builds): block mode stops before the wall that walkaround
    // mode goes around.
    #[test]
    fn block_mode_stops_before_an_obstacle_instead_of_walking_around() {
        let mut p = board();
        add_track(
            &mut p,
            "wall",
            "GND",
            Point::mm(15.0, 2.0),
            Point::mm(15.0, 25.0),
            500_000,
        );
        let block = RouteStepRequest {
            conflict: ConflictMode::Block,
            ..req(
                Point::mm(5.0, 10.0),
                Point::mm(25.0, 10.0),
                BendMode::Diagonal45,
            )
        };
        let r = route_step(&p, &block);
        assert!(!r.reached && !r.walkaround, "{r:?}");
        assert!(r.blocking.iter().any(|b| b.object == "track wall"));
        assert!(r.end.x < Point::mm(15.0, 0.0).x);
        assert_legal(&p, &r);
    }

    #[test]
    fn layer_switch_uses_the_via_preset_and_refuses_a_bad_one() {
        let p = board();
        let size = ViaSize {
            drill: 200_000,
            diameter: 450_000,
        };
        let switch = RouteStepRequest {
            layer: CopperLayer::Bottom,
            via_at_start: true,
            via: Some(size),
            ..req(
                Point::mm(5.0, 10.0),
                Point::mm(15.0, 10.0),
                BendMode::Diagonal45,
            )
        };
        let v = route_step(&p, &switch).via.expect("via inserted");
        assert_eq!((v.drill, v.diameter), (200_000, 450_000));
        let bad = RouteStepRequest {
            via: Some(ViaSize {
                drill: 400_000,
                diameter: 400_000,
            }),
            ..switch
        };
        let r = route_step(&p, &bad);
        assert!(r.via.is_none() && r.segments.is_empty() && !r.reached);
        assert!(r.message.is_some());
    }

    #[test]
    fn same_net_copper_is_not_an_obstacle() {
        let mut p = board();
        add_track(
            &mut p,
            "own",
            "SIG",
            Point::mm(15.0, 2.0),
            Point::mm(15.0, 25.0),
            500_000,
        );
        let r = route_step(
            &p,
            &req(
                Point::mm(5.0, 10.0),
                Point::mm(25.0, 10.0),
                BendMode::Diagonal45,
            ),
        );
        assert!(r.reached && !r.walkaround);
    }

    #[test]
    fn layer_switch_inserts_a_via_and_refuses_one_on_foreign_copper() {
        let mut p = board();
        let switch = RouteStepRequest {
            layer: CopperLayer::Bottom,
            via_at_start: true,
            ..req(
                Point::mm(5.0, 10.0),
                Point::mm(15.0, 10.0),
                BendMode::Diagonal45,
            )
        };
        let r = route_step(&p, &switch);
        let v = r.via.expect("via inserted");
        assert_eq!(v.position, Point::mm(5.0, 10.0));
        assert_eq!(v.diameter, p.constraints.default_class.via_diameter);
        assert_eq!(r.layer, CopperLayer::Bottom);
        assert!(r.reached);

        // A via of another net right next to the start: the layer switch is refused.
        p.pcb.vias.insert(
            "v1".into(),
            Via {
                id: "v1".into(),
                position: Point::mm(5.4, 10.0),
                drill: 300_000,
                diameter: 600_000,
                net: "GND".into(),
                from: CopperLayer::Top,
                to: CopperLayer::Bottom,
            },
        );
        let r = route_step(&p, &switch);
        assert!(r.via.is_none() && r.segments.is_empty() && !r.reached);
        assert!(r.blocking.iter().any(|b| b.object == "via v1"));
    }

    #[test]
    fn never_leaves_the_board() {
        let p = board();
        let r = route_step(
            &p,
            &req(
                Point::mm(5.0, 10.0),
                Point::mm(70.0, 10.0),
                BendMode::Diagonal45,
            ),
        );
        assert!(!r.reached);
        let limit = Point::mm(50.0, 0.0).x - p.constraints.board_edge_clearance;
        assert!(r.segments.iter().all(|s| s.b.x <= limit));
    }

    /// Zones are not obstacles, and their fill is derived: refilling after a routed track is
    /// added keeps the clearance (the fill retreats), so a committed route never leaves a stale
    /// fill behind.
    #[test]
    fn zone_fill_is_derived_and_retreats_from_a_routed_track() {
        let mut p = board();
        p.pcb.zones.insert(
            "z".into(),
            eak_core::Zone {
                id: "z".into(),
                layer: CopperLayer::Top,
                net: "GND".into(),
                outline: p.pcb.board.outline.clone(),
                clearance: 300_000,
            },
        );
        // A pour island that touches no copper of its own net is dead copper and is removed
        // (ADR-0030), so the zone needs a GND anchor, well away from the signal route below.
        add_track(
            &mut p,
            "gnd",
            "GND",
            Point::mm(40.0, 35.0),
            Point::mm(45.0, 35.0),
            250_000,
        );
        let before = crate::copper::fill_zone(&p, "z", &items(&p)).unwrap();
        assert!(before.filled_area_mm2() > 0.0, "the anchored pour is kept");
        let (from, to) = (Point::mm(5.0, 10.0), Point::mm(30.0, 20.0));
        let r = route_step(&p, &req(from, to, BendMode::Diagonal45));
        assert!(r.reached && !r.walkaround, "a zone is not an obstacle");
        for (k, s) in r.segments.iter().enumerate() {
            add_track(&mut p, &format!("t{k}"), "SIG", s.a, s.b, r.width);
        }
        let after = crate::copper::fill_zone(&p, "z", &items(&p)).unwrap();
        assert!(after.filled_area_mm2() < before.filled_area_mm2());
        for row in 0..after.rows {
            for col in 0..after.cols {
                if after.cells[row * after.cols + col] == 0 {
                    continue;
                }
                let c = after.cell_center(col, row);
                for s in &r.segments {
                    let d =
                        eak_core::geom::point_segment_distance(c, s.a, s.b) - r.width as f64 / 2.0;
                    assert!(d > 0.0, "fill cell {c:?} overlaps the new track");
                }
            }
        }
    }

    /// Property test: over generated obstacle fields and cursor positions, the returned path
    /// never violates clearance, always obeys the bend mode and starts at `from`.
    #[test]
    fn property_route_step_never_violates_clearance() {
        let mut seed: u64 = 0x5eed_1234_abcd;
        let mut rnd = |lo: f64, hi: f64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            lo + (hi - lo) * ((seed >> 11) as f64 / (1u64 << 53) as f64)
        };
        for case in 0..24 {
            let mut p = board();
            for k in 0..6 {
                let (x, y) = (rnd(8.0, 42.0), rnd(4.0, 36.0));
                let (len, horiz) = (rnd(2.0, 12.0), rnd(0.0, 1.0) < 0.5);
                let e = if horiz {
                    Point::mm((x + len).min(46.0), y)
                } else {
                    Point::mm(x, (y + len).min(36.0))
                };
                let w = rnd(150_000.0, 600_000.0) as Nm;
                add_track(&mut p, &format!("o{k}"), "GND", Point::mm(x, y), e, w);
            }
            let from = Point::mm(3.0, rnd(5.0, 35.0));
            let to = Point::mm(rnd(10.0, 47.0), rnd(3.0, 37.0));
            let mode = if case % 2 == 0 {
                BendMode::Diagonal45
            } else {
                BendMode::Orthogonal90
            };
            let r = route_step(&p, &req(from, to, mode));
            assert_legal(&p, &r);
            assert_mode(&r);
            if let Some(first) = r.segments.first() {
                assert_eq!(first.a, from, "case {case}");
            }
            if r.reached {
                assert_eq!(r.end, to, "case {case}");
            }
        }
    }
}
