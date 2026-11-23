//! Drawing primitives shared by symbol bodies and schematic sheets (schema v3, ADR-0040).
//!
//! Graphics are drawings only: they never take part in connectivity. Coordinates are symbol-local
//! for a symbol body and sheet coordinates for a sheet drawing (Y up, nanometres).

use crate::geom::{Nm, Point, Rotation};
use serde::{Deserialize, Serialize};

/// How a line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    DashDot,
}

/// Outline of a graphic. A width of 0 means the renderer's default line width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Stroke {
    #[serde(default)]
    pub width: Nm,
    #[serde(default)]
    pub style: LineStyle,
}

/// One drawing primitive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Graphic {
    Line {
        a: Point,
        b: Point,
        #[serde(default)]
        stroke: Stroke,
    },
    Polyline {
        points: Vec<Point>,
        #[serde(default)]
        stroke: Stroke,
    },
    /// Part of a circle, counter-clockwise from `start_deg` to `end_deg` (0 = +X, Y up).
    Arc {
        center: Point,
        radius: Nm,
        start_deg: f64,
        end_deg: f64,
        #[serde(default)]
        stroke: Stroke,
    },
    Circle {
        center: Point,
        radius: Nm,
        #[serde(default)]
        fill: bool,
        #[serde(default)]
        stroke: Stroke,
    },
    /// Axis-aligned rectangle from `min` (lower left) to `max` (upper right).
    Rectangle {
        min: Point,
        max: Point,
        #[serde(default)]
        fill: bool,
        #[serde(default)]
        stroke: Stroke,
    },
    /// Closed polygon (the last point joins the first).
    Polygon {
        points: Vec<Point>,
        #[serde(default)]
        fill: bool,
        #[serde(default)]
        stroke: Stroke,
    },
    /// Free text anchored at `position`; `size` is the character height.
    Text {
        position: Point,
        text: String,
        size: Nm,
        #[serde(default)]
        rotation: Rotation,
    },
}

impl Graphic {
    /// The rectangle that draws a v2 box body of half-extents `half_w` x `half_h`.
    pub fn body_box(half_w: Nm, half_h: Nm) -> Graphic {
        Graphic::Rectangle {
            min: Point::new(-half_w, -half_h),
            max: Point::new(half_w, half_h),
            fill: false,
            stroke: Stroke::default(),
        }
    }

    /// Problems that make this graphic undrawable; empty when it is valid.
    pub fn validate(&self) -> Vec<String> {
        let mut e = Vec::new();
        let stroke = match self {
            Graphic::Line { stroke, .. }
            | Graphic::Polyline { stroke, .. }
            | Graphic::Arc { stroke, .. }
            | Graphic::Circle { stroke, .. }
            | Graphic::Rectangle { stroke, .. }
            | Graphic::Polygon { stroke, .. } => Some(stroke),
            Graphic::Text { .. } => None,
        };
        if stroke.is_some_and(|s| s.width < 0) {
            e.push("line width must not be negative".to_string());
        }
        match self {
            Graphic::Line { a, b, .. } => {
                if a == b {
                    e.push("line has zero length".into());
                }
            }
            Graphic::Polyline { points, .. } => {
                if points.len() < 2 {
                    e.push("polyline needs at least 2 points".into());
                } else if points.windows(2).any(|w| w[0] == w[1]) {
                    e.push("polyline has a zero-length segment".into());
                }
            }
            Graphic::Arc {
                radius,
                start_deg,
                end_deg,
                ..
            } => {
                if *radius <= 0 {
                    e.push("arc radius must be positive".into());
                }
                if !(start_deg.is_finite() && end_deg.is_finite()) {
                    e.push("arc angles must be finite".into());
                } else if start_deg == end_deg {
                    e.push("arc has zero sweep".into());
                }
            }
            Graphic::Circle { radius, .. } => {
                if *radius <= 0 {
                    e.push("circle radius must be positive".into());
                }
            }
            Graphic::Rectangle { min, max, .. } => {
                if min.x >= max.x || min.y >= max.y {
                    e.push("rectangle min must be below and left of max".into());
                }
            }
            Graphic::Polygon { points, .. } => {
                if points.len() < 3 {
                    e.push("polygon needs at least 3 points".into());
                }
            }
            Graphic::Text { text, size, .. } => {
                if text.trim().is_empty() {
                    e.push("text is empty".into());
                }
                if *size <= 0 {
                    e.push("text size must be positive".into());
                }
            }
        }
        e
    }

    /// The same graphic moved by `d`.
    pub fn translated(&self, d: Point) -> Graphic {
        let mv = |p: &Point| p.plus(d);
        let mut g = self.clone();
        match &mut g {
            Graphic::Line { a, b, .. } => {
                *a = mv(a);
                *b = mv(b);
            }
            Graphic::Polyline { points, .. } | Graphic::Polygon { points, .. } => {
                for p in points.iter_mut() {
                    *p = mv(p);
                }
            }
            Graphic::Arc { center, .. } | Graphic::Circle { center, .. } => *center = mv(center),
            Graphic::Rectangle { min, max, .. } => {
                *min = mv(min);
                *max = mv(max);
            }
            Graphic::Text { position, .. } => *position = mv(position),
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape_is_tagged_by_kind_and_defaults_the_stroke() {
        let g: Graphic =
            serde_json::from_str(r#"{"kind":"line","a":{"x":0,"y":0},"b":{"x":5,"y":0}}"#).unwrap();
        assert_eq!(
            g,
            Graphic::Line {
                a: Point::new(0, 0),
                b: Point::new(5, 0),
                stroke: Stroke::default()
            }
        );
        let v = serde_json::to_value(Graphic::body_box(2, 1)).unwrap();
        assert_eq!(v["kind"], "rectangle");
        assert_eq!(v["min"], serde_json::json!({"x": -2, "y": -1}));
        assert_eq!(v["stroke"]["style"], "solid");
    }

    #[test]
    fn degenerate_graphics_are_refused() {
        let p = Point::new(0, 0);
        let bad = [
            Graphic::Line {
                a: p,
                b: p,
                stroke: Stroke::default(),
            },
            Graphic::Polyline {
                points: vec![p],
                stroke: Stroke::default(),
            },
            Graphic::Arc {
                center: p,
                radius: 0,
                start_deg: 0.0,
                end_deg: 90.0,
                stroke: Stroke::default(),
            },
            Graphic::Circle {
                center: p,
                radius: 1,
                fill: false,
                stroke: Stroke {
                    width: -1,
                    style: LineStyle::Dashed,
                },
            },
            Graphic::Rectangle {
                min: Point::new(1, 1),
                max: p,
                fill: true,
                stroke: Stroke::default(),
            },
            Graphic::Polygon {
                points: vec![p, Point::new(1, 0)],
                fill: true,
                stroke: Stroke::default(),
            },
            Graphic::Text {
                position: p,
                text: " ".into(),
                size: 1,
                rotation: Rotation::R0,
            },
        ];
        for g in bad {
            assert!(!g.validate().is_empty(), "{g:?}");
        }
        assert!(Graphic::body_box(2, 1).validate().is_empty());
    }

    #[test]
    fn translation_moves_every_point() {
        let g = Graphic::Polygon {
            points: vec![Point::new(0, 0), Point::new(2, 0), Point::new(0, 2)],
            fill: false,
            stroke: Stroke::default(),
        };
        assert_eq!(
            g.translated(Point::new(10, 20)),
            Graphic::Polygon {
                points: vec![Point::new(10, 20), Point::new(12, 20), Point::new(10, 22)],
                fill: false,
                stroke: Stroke::default(),
            }
        );
    }
}
