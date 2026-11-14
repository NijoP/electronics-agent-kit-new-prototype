//! Integer geometry. All canonical coordinates are signed nanometres (`Nm`) so that
//! geometry is deterministic and exactly reproducible across platforms.

use serde::{Deserialize, Serialize};

/// Signed length in nanometres.
pub type Nm = i64;

pub const NM_PER_MM: Nm = 1_000_000;
pub const NM_PER_MIL: Nm = 25_400;

pub fn mm(v: f64) -> Nm {
    (v * NM_PER_MM as f64).round() as Nm
}

pub fn to_mm(v: Nm) -> f64 {
    v as f64 / NM_PER_MM as f64
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub struct Point {
    pub x: Nm,
    pub y: Nm,
}

impl Point {
    pub const fn new(x: Nm, y: Nm) -> Self {
        Point { x, y }
    }
    pub fn mm(x: f64, y: f64) -> Self {
        Point { x: mm(x), y: mm(y) }
    }
    pub fn plus(self, o: Point) -> Point {
        Point::new(self.x + o.x, self.y + o.y)
    }
    pub fn minus(self, o: Point) -> Point {
        Point::new(self.x - o.x, self.y - o.y)
    }
    pub fn dist(self, o: Point) -> f64 {
        let dx = (self.x - o.x) as f64;
        let dy = (self.y - o.y) as f64;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Rotation restricted to quarter turns, which keeps pin and pad geometry exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    pub fn degrees(self) -> u32 {
        match self {
            Rotation::R0 => 0,
            Rotation::R90 => 90,
            Rotation::R180 => 180,
            Rotation::R270 => 270,
        }
    }
    pub fn from_degrees(d: i64) -> Option<Rotation> {
        match d.rem_euclid(360) {
            0 => Some(Rotation::R0),
            90 => Some(Rotation::R90),
            180 => Some(Rotation::R180),
            270 => Some(Rotation::R270),
            _ => None,
        }
    }
    pub fn next(self) -> Rotation {
        Rotation::from_degrees(self.degrees() as i64 + 90).unwrap_or_default()
    }
    /// The opposite turn (R90 <-> R270).
    pub fn inverse(self) -> Rotation {
        Rotation::from_degrees(-(self.degrees() as i64)).unwrap_or_default()
    }
    /// Rotate a local offset counter-clockwise (Y up) by this rotation.
    pub fn apply(self, p: Point) -> Point {
        match self {
            Rotation::R0 => p,
            Rotation::R90 => Point::new(-p.y, p.x),
            Rotation::R180 => Point::new(-p.x, -p.y),
            Rotation::R270 => Point::new(p.y, -p.x),
        }
    }
    /// Rotate a size (w,h) – swaps for 90/270.
    pub fn apply_size(self, w: Nm, h: Nm) -> (Nm, Nm) {
        match self {
            Rotation::R0 | Rotation::R180 => (w, h),
            _ => (h, w),
        }
    }
}

/// Axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    pub fn centered(c: Point, w: Nm, h: Nm) -> Rect {
        Rect {
            min: Point::new(c.x - w / 2, c.y - h / 2),
            max: Point::new(c.x + w / 2, c.y + h / 2),
        }
    }
    pub fn from_points(pts: &[Point]) -> Option<Rect> {
        let first = *pts.first()?;
        let mut r = Rect {
            min: first,
            max: first,
        };
        for p in pts {
            r.min.x = r.min.x.min(p.x);
            r.min.y = r.min.y.min(p.y);
            r.max.x = r.max.x.max(p.x);
            r.max.y = r.max.y.max(p.y);
        }
        Some(r)
    }
    pub fn width(&self) -> Nm {
        self.max.x - self.min.x
    }
    pub fn height(&self) -> Nm {
        self.max.y - self.min.y
    }
    pub fn center(&self) -> Point {
        Point::new((self.min.x + self.max.x) / 2, (self.min.y + self.max.y) / 2)
    }
    pub fn inflate(&self, d: Nm) -> Rect {
        Rect {
            min: Point::new(self.min.x - d, self.min.y - d),
            max: Point::new(self.max.x + d, self.max.y + d),
        }
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.min.x < o.max.x && o.min.x < self.max.x && self.min.y < o.max.y && o.min.y < self.max.y
    }
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
    pub fn union(&self, o: &Rect) -> Rect {
        Rect {
            min: Point::new(self.min.x.min(o.min.x), self.min.y.min(o.min.y)),
            max: Point::new(self.max.x.max(o.max.x), self.max.y.max(o.max.y)),
        }
    }
}

/// Distance from point `p` to segment `a`-`b` (nanometres, as f64).
pub fn point_segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let (px, py) = (p.x as f64, p.y as f64);
    let (ax, ay) = (a.x as f64, a.y as f64);
    let (bx, by) = (b.x as f64, b.y as f64);
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t = (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0);
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

fn orient(a: Point, b: Point, c: Point) -> i128 {
    let v = (b.x - a.x) as i128 * (c.y - a.y) as i128 - (b.y - a.y) as i128 * (c.x - a.x) as i128;
    v.signum()
}

fn on_segment(a: Point, b: Point, p: Point) -> bool {
    p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
}

/// Exact segment intersection test (integer arithmetic).
pub fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let (o1, o2, o3, o4) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    if o1 != o2 && o3 != o4 {
        return true;
    }
    (o1 == 0 && on_segment(a, b, c))
        || (o2 == 0 && on_segment(a, b, d))
        || (o3 == 0 && on_segment(c, d, a))
        || (o4 == 0 && on_segment(c, d, b))
}

/// Minimum distance between two segments.
pub fn segment_segment_distance(a: Point, b: Point, c: Point, d: Point) -> f64 {
    if segments_intersect(a, b, c, d) {
        return 0.0;
    }
    point_segment_distance(a, c, d)
        .min(point_segment_distance(b, c, d))
        .min(point_segment_distance(c, a, b))
        .min(point_segment_distance(d, a, b))
}

/// Point-in-polygon (even-odd rule); points on the boundary count as inside.
pub fn point_in_polygon(p: Point, poly: &[Point]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if point_segment_distance(p, a, b) < 0.5 {
            return true;
        }
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (pi, pj) = (poly[i], poly[j]);
        if (pi.y > p.y) != (pj.y > p.y) {
            let x_cross =
                pj.x as f64 + (p.y - pj.y) as f64 * (pi.x - pj.x) as f64 / (pi.y - pj.y) as f64;
            if (p.x as f64) < x_cross {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Distance from a point to a closed polygon boundary.
pub fn point_polygon_boundary_distance(p: Point, poly: &[Point]) -> f64 {
    let mut best = f64::INFINITY;
    for i in 0..poly.len() {
        best = best.min(point_segment_distance(
            p,
            poly[i],
            poly[(i + 1) % poly.len()],
        ));
    }
    best
}

/// Signed area * 2 (positive when counter-clockwise).
pub fn polygon_area2(poly: &[Point]) -> i128 {
    let mut s: i128 = 0;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        s += a.x as i128 * b.y as i128 - b.x as i128 * a.y as i128;
    }
    s
}

/// True when any two non-adjacent edges of the polygon cross.
pub fn polygon_self_intersects(poly: &[Point]) -> bool {
    let n = poly.len();
    if n < 4 {
        return false;
    }
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let (c, d) = (poly[j], poly[(j + 1) % n]);
            if segments_intersect(a, b, c, d) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_is_exact() {
        let p = Point::new(10, 3);
        assert_eq!(Rotation::R90.apply(p), Point::new(-3, 10));
        assert_eq!(Rotation::R180.apply(p), Point::new(-10, -3));
        assert_eq!(Rotation::R270.apply(Rotation::R90.apply(p)), p);
        assert_eq!(Rotation::from_degrees(-90), Some(Rotation::R270));
        assert_eq!(Rotation::from_degrees(45), None);
    }

    #[test]
    fn segment_distance_and_intersection() {
        let a = Point::new(0, 0);
        let b = Point::new(100, 0);
        assert!(segments_intersect(
            a,
            b,
            Point::new(50, -10),
            Point::new(50, 10)
        ));
        assert!(!segments_intersect(
            a,
            b,
            Point::new(0, 5),
            Point::new(100, 5)
        ));
        assert_eq!(
            segment_segment_distance(a, b, Point::new(0, 5), Point::new(100, 5)),
            5.0
        );
        assert_eq!(point_segment_distance(Point::new(150, 0), a, b), 50.0);
    }

    #[test]
    fn polygon_tests() {
        let sq = [
            Point::new(0, 0),
            Point::new(10, 0),
            Point::new(10, 10),
            Point::new(0, 10),
        ];
        assert!(point_in_polygon(Point::new(5, 5), &sq));
        assert!(point_in_polygon(Point::new(0, 5), &sq));
        assert!(!point_in_polygon(Point::new(15, 5), &sq));
        assert!(polygon_area2(&sq) > 0);
        assert!(!polygon_self_intersects(&sq));
        let bow = [
            Point::new(0, 0),
            Point::new(10, 10),
            Point::new(10, 0),
            Point::new(0, 10),
        ];
        assert!(polygon_self_intersects(&bow));
    }
}
