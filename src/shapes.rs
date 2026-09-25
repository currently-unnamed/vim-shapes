use crate::model::{Node, ShapeKind};
use std::f64::consts::PI;

type Point = (f64, f64);

/// A curved or straight primitive; every shape outline is now built purely
/// from these so the whole canvas renders in braille (no box-drawing text).
#[derive(Debug, Clone, PartialEq)]
pub enum CurvePrimitive {
    /// A scatter of sampled points (used for ellipse/cloud outlines and
    /// rounded-corner arcs).
    Points(Vec<Point>),
    /// Straight line segments between two points each.
    Lines(Vec<(Point, Point)>),
}

/// Braille-rendered primitives for a node's shape outline.
pub fn curve_primitives(node: &Node) -> Vec<CurvePrimitive> {
    let (x, y, w, h) = (node.x, node.y, node.w, node.h);
    match node.kind {
        ShapeKind::RoundedRectangle => rounded_rectangle_primitives(x, y, w, h),
        ShapeKind::Ellipse => vec![CurvePrimitive::Points(ellipse_points(
            x + w / 2.0,
            y + h / 2.0,
            w / 2.0,
            h / 2.0,
            0.0,
            2.0 * PI,
            96,
        ))],
        ShapeKind::Diamond => vec![CurvePrimitive::Lines(diamond_lines(x, y, w, h))],
        ShapeKind::Cylinder => cylinder_primitives(x, y, w, h),
        ShapeKind::Cloud => vec![CurvePrimitive::Points(cloud_points(x, y, w, h))],
        ShapeKind::Parallelogram => vec![CurvePrimitive::Lines(parallelogram_lines(x, y, w, h))],
    }
}

fn rounded_rectangle_primitives(x: f64, y: f64, w: f64, h: f64) -> Vec<CurvePrimitive> {
    let r = (w.min(h) * 0.3).clamp(0.5, 2.5).min(w / 2.0).min(h / 2.0);
    let lines = vec![
        ((x + r, y), (x + w - r, y)),
        ((x + r, y + h), (x + w - r, y + h)),
        ((x, y + r), (x, y + h - r)),
        ((x + w, y + r), (x + w, y + h - r)),
    ];
    let mut corners = Vec::new();
    corners.extend(ellipse_points(x + r, y + r, r, r, PI, 1.5 * PI, 12));
    corners.extend(ellipse_points(x + w - r, y + r, r, r, 1.5 * PI, 2.0 * PI, 12));
    corners.extend(ellipse_points(x + w - r, y + h - r, r, r, 0.0, 0.5 * PI, 12));
    corners.extend(ellipse_points(x + r, y + h - r, r, r, 0.5 * PI, PI, 12));
    vec![CurvePrimitive::Lines(lines), CurvePrimitive::Points(corners)]
}

fn cylinder_ellipse_height(h: f64) -> f64 {
    // .max().min() rather than .clamp(): at small icon scales h/2 can be
    // less than the 1.0 floor, and clamp(min, max) panics when min > max.
    let upper = (h * 0.5).max(0.2);
    (h * 0.3).max(0.2).min(upper)
}

fn cylinder_primitives(x: f64, y: f64, w: f64, h: f64) -> Vec<CurvePrimitive> {
    let eh = cylinder_ellipse_height(h);
    let cx = x + w / 2.0;
    let rx = w / 2.0;
    let ry = eh / 2.0;
    let top_center = y + eh / 2.0;
    let bottom_center = y + h - eh / 2.0;
    vec![
        CurvePrimitive::Points(ellipse_points(cx, top_center, rx, ry, 0.0, 2.0 * PI, 64)),
        CurvePrimitive::Points(ellipse_points(cx, bottom_center, rx, ry, 0.0, PI, 32)),
        CurvePrimitive::Lines(vec![
            ((x, top_center), (x, bottom_center)),
            ((x + w, top_center), (x + w, bottom_center)),
        ]),
    ]
}

fn parallelogram_lines(x: f64, y: f64, w: f64, h: f64) -> Vec<(Point, Point)> {
    let skew = (w * 0.2).max(1.0).min(w / 2.0);
    let top_left = (x + skew, y);
    let top_right = (x + w, y);
    let bottom_right = (x + w - skew, y + h);
    let bottom_left = (x, y + h);
    vec![
        (top_left, top_right),
        (top_right, bottom_right),
        (bottom_right, bottom_left),
        (bottom_left, top_left),
    ]
}

fn diamond_lines(x: f64, y: f64, w: f64, h: f64) -> Vec<(Point, Point)> {
    let top = (x + w / 2.0, y);
    let right = (x + w, y + h / 2.0);
    let bottom = (x + w / 2.0, y + h);
    let left = (x, y + h / 2.0);
    vec![(top, right), (right, bottom), (bottom, left), (left, top)]
}

fn cloud_points(x: f64, y: f64, w: f64, h: f64) -> Vec<Point> {
    let bumps: [(f64, f64, f64, f64); 5] = [
        (0.30, 0.55, 0.28, 0.45),
        (0.50, 0.30, 0.32, 0.55),
        (0.72, 0.50, 0.30, 0.48),
        (0.40, 0.75, 0.25, 0.35),
        (0.62, 0.75, 0.25, 0.35),
    ];
    let mut points = Vec::new();
    for (cx_frac, cy_frac, rx_frac, ry_frac) in bumps {
        let cx = x + w * cx_frac;
        let cy = y + h * cy_frac;
        let rx = w * rx_frac / 2.0;
        let ry = h * ry_frac / 2.0;
        points.extend(ellipse_points(cx, cy, rx, ry, 0.0, 2.0 * PI, 32));
    }
    points
}

fn ellipse_points(cx: f64, cy: f64, rx: f64, ry: f64, t_start: f64, t_end: f64, steps: usize) -> Vec<Point> {
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = t_start + (t_end - t_start) * (i as f64 / steps as f64);
        points.push((cx + rx * t.cos(), cy + ry * t.sin()));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeStyle;

    fn node(kind: ShapeKind) -> Node {
        let (w, h) = kind.default_size();
        Node { id: 0, kind, x: 0.0, y: 0.0, w, h, label: String::new(), style: NodeStyle::default() }
    }

    #[test]
    fn every_shape_kind_produces_at_least_one_primitive() {
        for kind in ShapeKind::ALL {
            assert!(!curve_primitives(&node(kind)).is_empty(), "{kind:?} produced no primitives");
        }
    }

    #[test]
    fn every_shape_kind_survives_the_sidebar_icon_scale_without_panicking() {
        // Regression test: at the sidebar's tiny icon size (a couple of
        // rows tall), cylinder_ellipse_height used to panic via
        // `.clamp(1.0, h / 2.0)` once h / 2.0 dropped below 1.0.
        for kind in ShapeKind::ALL {
            let tiny = Node {
                id: 0,
                kind,
                x: 0.6,
                y: 0.3,
                w: 4.0,
                h: 1.4,
                label: String::new(),
                style: NodeStyle::default(),
            };
            let _ = curve_primitives(&tiny);
        }
    }

    #[test]
    fn diamond_vertices_touch_bounding_box_edges() {
        let n = node(ShapeKind::Diamond);
        let lines = diamond_lines(n.x, n.y, n.w, n.h);
        assert_eq!(lines[0].0, (n.x + n.w / 2.0, n.y));
        assert_eq!(lines[1].0, (n.x + n.w, n.y + n.h / 2.0));
    }

    #[test]
    fn rounded_rectangle_edges_stay_within_bounding_box() {
        let n = node(ShapeKind::RoundedRectangle);
        if let CurvePrimitive::Lines(lines) = &curve_primitives(&n)[0] {
            for (a, b) in lines {
                for p in [a, b] {
                    assert!(p.0 >= n.x - 0.001 && p.0 <= n.x + n.w + 0.001);
                    assert!(p.1 >= n.y - 0.001 && p.1 <= n.y + n.h + 0.001);
                }
            }
        } else {
            panic!("expected Lines primitive first");
        }
    }

    #[test]
    fn parallelogram_has_four_edges() {
        let n = node(ShapeKind::Parallelogram);
        if let CurvePrimitive::Lines(lines) = &curve_primitives(&n)[0] {
            assert_eq!(lines.len(), 4);
        } else {
            panic!("expected a single Lines primitive");
        }
    }
}
