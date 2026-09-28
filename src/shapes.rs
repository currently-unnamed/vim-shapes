//! Outlines, as braille. Every shape is built from points and straight segments so the whole
//! canvas draws through one marker and nothing is a box-drawing glyph that would refuse to
//! meet a braille curve at the corner.

use std::f64::consts::PI;

use crate::model::{Element, Page};
use crate::ontology::{LineStyle, Shape};

pub type Point = (f64, f64);

/// A curved or straight primitive.
#[derive(Debug, Clone, PartialEq)]
pub enum CurvePrimitive {
    /// A scatter of sampled points — ellipses, cloud bumps, rounded corners, dashes.
    Points(Vec<Point>),
    /// Straight segments between two points each.
    Lines(Vec<(Point, Point)>),
}

/// A shape's outline the way its diagram draws it — rounded, or by hand, as the page says.
/// Every drawing surface goes through here, so the screen and each export agree.
pub fn drawn(page: &Page, e: &Element, amp: f64) -> Vec<CurvePrimitive> {
    drawn_at(page, e, e.x, e.y, e.w, e.h, amp)
}

/// The same, fitted to another box — a heavier stroke is the outline drawn again a dot out.
pub fn drawn_at(page: &Page, e: &Element, x: f64, y: f64, w: f64, h: f64, amp: f64) -> Vec<CurvePrimitive> {
    let shape = page.shape(e.kind);
    let prims = sheared(outline(shape, x, y, w, h), (e.skew, e.skew_y), (x, y, w, h));
    // A cloud is left to a steady hand: it is bumps already, and which of its arcs show
    // is worked out from where the bumps meet — a wobbled bump meets its neighbours
    // somewhere else, and the outline comes apart at every junction.
    if page.sketch && shape != Shape::Cloud { sketch(prims, e.id as u64, amp) } else { prims }
}

/// The outline leaned over. Across: every point moved sideways by its height's share of
/// `skew.0`, the top edge half of it left and the bottom half of it right. Down: every
/// point moved by its width's share of `skew.1`, the left edge half of it up and the right
/// half of it down. Either way the shape's centre stays where its box is. Zero leaves it
/// alone.
pub fn sheared(prims: Vec<CurvePrimitive>, skew: (f64, f64), b: (f64, f64, f64, f64)) -> Vec<CurvePrimitive> {
    let (x, y, w, h) = b;
    if skew == (0.0, 0.0) || h <= 0.0 || w <= 0.0 {
        return prims;
    }
    let lean = |p: Point| (p.0 + skew.0 * ((p.1 - y) / h - 0.5), p.1 + skew.1 * ((p.0 - x) / w - 0.5));
    prims
        .into_iter()
        .map(|p| match p {
            CurvePrimitive::Lines(ls) => CurvePrimitive::Lines(ls.into_iter().map(|(a, b)| (lean(a), lean(b))).collect()),
            CurvePrimitive::Points(ps) => CurvePrimitive::Points(ps.into_iter().map(lean).collect()),
        })
        .collect()
}

/// An outline drawn dashed or dotted, for the braille surfaces: every straight edge sampled
/// as dashes, every curve thinned to runs of its points. The picture builders that stroke
/// real lines set a dash on the line instead.
pub fn patterned(prims: Vec<CurvePrimitive>, line: LineStyle) -> Vec<CurvePrimitive> {
    let (on, off, keep, of) = match line {
        LineStyle::Solid => return prims,
        LineStyle::Dashed => (1.5, 0.75, 6, 9),
        LineStyle::Dotted => (0.25, 0.75, 1, 4),
    };
    prims
        .into_iter()
        .map(|p| match p {
            CurvePrimitive::Lines(ls) => CurvePrimitive::Points(ls.iter().flat_map(|(a, b)| dashed_line(*a, *b, on, off)).collect()),
            CurvePrimitive::Points(ps) => CurvePrimitive::Points(ps.into_iter().enumerate().filter(|(i, _)| i % of < keep).map(|(_, p)| p).collect()),
        })
        .collect()
}

/// The outline of `shape` fitted to the box `(x, y, w, h)`, y growing downward.
pub fn outline(shape: Shape, x: f64, y: f64, w: f64, h: f64) -> Vec<CurvePrimitive> {
    match shape {
        Shape::Rectangle => vec![CurvePrimitive::Lines(rectangle_lines(x, y, w, h))],
        Shape::RoundedRectangle => rounded_rectangle(x, y, w, h),
        Shape::Ellipse => vec![CurvePrimitive::Points(ellipse_points(
            x + w / 2.0,
            y + h / 2.0,
            w / 2.0,
            h / 2.0,
            0.0,
            2.0 * PI,
            96,
        ))],
        Shape::Diamond => vec![CurvePrimitive::Lines(diamond_lines(x, y, w, h))],
        Shape::Cylinder => cylinder(x, y, w, h),
        Shape::Cloud => cloud_bumps(x, y, w, h).into_iter().map(CurvePrimitive::Points).collect(),
        Shape::Parallelogram => vec![CurvePrimitive::Lines(parallelogram_lines(x, y, w, h))],
        Shape::Hexagon => vec![CurvePrimitive::Lines(hexagon_lines(x, y, w, h))],
        Shape::Dashed => vec![CurvePrimitive::Points(dashed_rectangle(x, y, w, h))],
        Shape::Text => Vec::new(),
        other => general(other, x, y, w, h),
    }
}

/// The horizontal span this outline's own edges reach at world row `row` — the shape's true
/// left and right there, not its bounding box's, so a fill can stay inside a curve or a point
/// instead of tinting the corner the outline never reaches. A `Lines` primitive's pairs are
/// each a real edge; a `Points` primitive is a curve sampled finely, so only a pair close
/// enough to be the next step along it — never a jump to some other arc entirely — counts as
/// one, the same gate `wire::shape`'s own fallback uses to tell the two apart. Two crossings
/// on the same side (a cloud's own bumps, overlapping) take the outermost: the hull at that
/// row, the same simplification the picture export's fill already makes, rather than each
/// bump's own notch.
pub fn row_span(prims: &[CurvePrimitive], row: f64) -> Option<(f64, f64)> {
    let mut span: Option<(f64, f64)> = None;
    let mut edge = |a: Point, b: Point| {
        if (a.1 - row) * (b.1 - row) > 0.0 {
            return;
        }
        let (x0, x1) = if (b.1 - a.1).abs() < f64::EPSILON {
            (a.0, b.0)
        } else {
            let x = a.0 + (b.0 - a.0) * (row - a.1) / (b.1 - a.1);
            (x, x)
        };
        let (lo, hi) = (x0.min(x1), x0.max(x1));
        span = Some(match span {
            Some((l, h)) => (l.min(lo), h.max(hi)),
            None => (lo, hi),
        });
    };
    for prim in prims {
        match prim {
            CurvePrimitive::Lines(ls) => {
                for &(a, b) in ls {
                    edge(a, b);
                }
            }
            CurvePrimitive::Points(ps) => {
                for w in ps.windows(2) {
                    if (w[0].0 - w[1].0).abs() <= 2.0 && (w[0].1 - w[1].1).abs() <= 1.0 {
                        edge(w[0], w[1]);
                    }
                }
            }
        }
    }
    span
}

fn rectangle_lines(x: f64, y: f64, w: f64, h: f64) -> Vec<(Point, Point)> {
    let (r, b) = (x + w, y + h);
    vec![((x, y), (r, y)), ((r, y), (r, b)), ((r, b), (x, b)), ((x, b), (x, y))]
}

fn rounded_rectangle(x: f64, y: f64, w: f64, h: f64) -> Vec<CurvePrimitive> {
    let r = (w.min(h) * 0.3).clamp(0.5, 2.5).min(w / 2.0).min(h / 2.0);
    let lines = vec![
        ((x + r, y), (x + w - r, y)),
        ((x + r, y + h), (x + w - r, y + h)),
        ((x, y + r), (x, y + h - r)),
        ((x + w, y + r), (x + w, y + h - r)),
    ];
    // One primitive per corner, so a polyline through the points of each is an arc and not
    // an arc plus a chord to the next corner.
    vec![
        CurvePrimitive::Lines(lines),
        CurvePrimitive::Points(ellipse_points(x + r, y + r, r, r, PI, 1.5 * PI, 12)),
        CurvePrimitive::Points(ellipse_points(x + w - r, y + r, r, r, 1.5 * PI, 2.0 * PI, 12)),
        CurvePrimitive::Points(ellipse_points(x + w - r, y + h - r, r, r, 0.0, 0.5 * PI, 12)),
        CurvePrimitive::Points(ellipse_points(x + r, y + h - r, r, r, 0.5 * PI, PI, 12)),
    ]
}

fn cylinder_ellipse_height(h: f64) -> f64 {
    // `.max().min()` rather than `.clamp()`: at tiny sizes h/2 can drop below the floor, and
    // `clamp(min, max)` panics when min > max.
    let upper = (h * 0.5).max(0.2);
    (h * 0.3).max(0.2).min(upper)
}

fn cylinder(x: f64, y: f64, w: f64, h: f64) -> Vec<CurvePrimitive> {
    let eh = cylinder_ellipse_height(h);
    let cx = x + w / 2.0;
    let rx = w / 2.0;
    let ry = eh / 2.0;
    let top = y + eh / 2.0;
    let bottom = y + h - eh / 2.0;
    vec![
        CurvePrimitive::Points(ellipse_points(cx, top, rx, ry, 0.0, 2.0 * PI, 64)),
        CurvePrimitive::Points(ellipse_points(cx, bottom, rx, ry, 0.0, PI, 32)),
        CurvePrimitive::Lines(vec![((x, top), (x, bottom)), ((x + w, top), (x + w, bottom))]),
    ]
}

fn parallelogram_lines(x: f64, y: f64, w: f64, h: f64) -> Vec<(Point, Point)> {
    let skew = (w * 0.2).max(1.0).min(w / 2.0);
    let tl = (x + skew, y);
    let tr = (x + w, y);
    let br = (x + w - skew, y + h);
    let bl = (x, y + h);
    vec![(tl, tr), (tr, br), (br, bl), (bl, tl)]
}

fn diamond_lines(x: f64, y: f64, w: f64, h: f64) -> Vec<(Point, Point)> {
    let top = (x + w / 2.0, y);
    let right = (x + w, y + h / 2.0);
    let bottom = (x + w / 2.0, y + h);
    let left = (x, y + h / 2.0);
    vec![(top, right), (right, bottom), (bottom, left), (left, top)]
}

fn hexagon_lines(x: f64, y: f64, w: f64, h: f64) -> Vec<(Point, Point)> {
    let n = (w * 0.15).max(1.0).min(w / 2.0);
    let (my, r, b) = (y + h / 2.0, x + w, y + h);
    let pts = [(x + n, y), (r - n, y), (r, my), (r - n, b), (x + n, b), (x, my)];
    (0..6).map(|i| (pts[i], pts[(i + 1) % 6])).collect()
}

/// A rectangle drawn as dashes: points along each edge, with gaps. Sampled in world units so
/// the dash length is the same whatever the box's size.
fn dashed_rectangle(x: f64, y: f64, w: f64, h: f64) -> Vec<Point> {
    let (r, b) = (x + w, y + h);
    let edges = [((x, y), (r, y)), ((r, y), (r, b)), ((r, b), (x, b)), ((x, b), (x, y))];
    let mut pts = Vec::new();
    for (a, c) in edges {
        pts.extend(dashed_line(a, c, 1.5, 0.75));
    }
    pts
}

/// Points along `a → b`, `on` units drawn then `off` units skipped, sampled four to a cell so
/// the braille fills in.
pub fn dashed_line(a: Point, b: Point, on: f64, off: f64) -> Vec<Point> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < f64::EPSILON {
        return vec![a];
    }
    let (ux, uy) = (dx / len, dy / len);
    let step = 0.25;
    let mut pts = Vec::new();
    let mut t = 0.0;
    while t <= len {
        let phase = t % (on + off);
        if phase <= on {
            pts.push((a.0 + ux * t, a.1 + uy * t));
        }
        t += step;
    }
    pts
}

pub fn ellipse_points(cx: f64, cy: f64, rx: f64, ry: f64, t_start: f64, t_end: f64, steps: usize) -> Vec<Point> {
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = t_start + (t_end - t_start) * (i as f64 / steps as f64);
        points.push((cx + rx * t.cos(), cy + ry * t.sin()));
    }
    points
}

/// A cloud is five overlapping bumps, each its own closed curve.
fn cloud_bumps(x: f64, y: f64, w: f64, h: f64) -> Vec<Vec<Point>> {
    let bumps: [(f64, f64, f64, f64); 5] = [
        (0.30, 0.55, 0.28, 0.45),
        (0.50, 0.30, 0.32, 0.55),
        (0.72, 0.50, 0.30, 0.48),
        (0.40, 0.75, 0.25, 0.35),
        (0.62, 0.75, 0.25, 0.35),
    ];
    bumps
        .into_iter()
        .map(|(cx_frac, cy_frac, rx_frac, ry_frac)| {
            let cx = x + w * cx_frac;
            let cy = y + h * cy_frac;
            let rx = w * rx_frac / 2.0;
            let ry = h * ry_frac / 2.0;
            ellipse_points(cx, cy, rx, ry, 0.0, 2.0 * PI, 32)
        })
        .collect()
}

/// Where a straight line from the centre of a box toward `toward` leaves the box — so a
/// relation starts at the edge of a shape rather than in the middle of its label.
pub fn edge_point(x: f64, y: f64, w: f64, h: f64, toward: Point) -> Point {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let (dx, dy) = (toward.0 - cx, toward.1 - cy);
    if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
        return (cx, cy);
    }
    let (hw, hh) = (w / 2.0, h / 2.0);
    // Scale the direction so that whichever axis hits its half-extent first decides.
    let tx = if dx.abs() < f64::EPSILON { f64::INFINITY } else { hw / dx.abs() };
    let ty = if dy.abs() < f64::EPSILON { f64::INFINITY } else { hh / dy.abs() };
    let t = tx.min(ty);
    (cx + dx * t, cy + dy * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Shape; 30] = [
        Shape::Rectangle,
        Shape::RoundedRectangle,
        Shape::Ellipse,
        Shape::Diamond,
        Shape::Cylinder,
        Shape::Cloud,
        Shape::Parallelogram,
        Shape::Hexagon,
        Shape::Dashed,
        Shape::Triangle,
        Shape::PredefinedProcess,
        Shape::Document,
        Shape::InternalStorage,
        Shape::Cube,
        Shape::Step,
        Shape::Trapezoid,
        Shape::Tape,
        Shape::Note,
        Shape::Card,
        Shape::Callout,
        Shape::StickFigure,
        Shape::DataStorage,
        Shape::Delay,
        Shape::Display,
        Shape::ManualInput,
        Shape::OffPage,
        Shape::BlockArrow,
        Shape::DoubleArrow,
        Shape::And,
        Shape::Or,
    ];

    #[test]
    fn every_shape_produces_at_least_one_primitive() {
        for s in ALL {
            assert!(!outline(s, 0.0, 0.0, 12.0, 5.0).is_empty(), "{s:?} produced nothing");
        }
    }

    #[test]
    fn every_shape_survives_a_tiny_box_without_panicking() {
        // A cylinder's cap height used to `clamp(1.0, h / 2.0)` and panic once h / 2 < 1.
        for s in ALL {
            let _ = outline(s, 0.6, 0.3, 4.0, 1.4);
        }
    }

    #[test]
    fn every_outline_stays_within_its_box() {
        for s in ALL {
            for p in outline(s, 3.0, 4.0, 12.0, 5.0) {
                let pts: Vec<Point> = match p {
                    CurvePrimitive::Points(v) => v,
                    CurvePrimitive::Lines(l) => l.into_iter().flat_map(|(a, b)| [a, b]).collect(),
                };
                for (x, y) in pts {
                    // A wave may dip half a cell past its edge; nothing else may leave the box.
                    assert!((2.4..=15.6).contains(&x) && (3.4..=9.6).contains(&y), "{s:?} at ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn row_span_never_panics_and_never_reaches_past_the_box() {
        for s in ALL {
            let prims = outline(s, 3.0, 4.0, 12.0, 5.0);
            for r in 0..12 {
                let row = 4.0 + r as f64 * 0.5;
                if let Some((lo, hi)) = row_span(&prims, row) {
                    assert!(lo <= hi, "{s:?} at row {row}: {lo} > {hi}");
                    // A wave may dip half a cell past its edge, same as the outline itself does.
                    assert!(lo >= 2.4 && hi <= 15.6, "{s:?} at row {row}: ({lo}, {hi})");
                }
            }
        }
    }

    #[test]
    fn row_span_hugs_a_rectangle_s_full_width_and_a_diamond_s_and_an_ellipse_s_taper() {
        let rect = outline(Shape::Rectangle, 0.0, 0.0, 10.0, 6.0);
        assert_eq!(row_span(&rect, 3.0), Some((0.0, 10.0)), "a rectangle is the same width top to bottom");
        assert_eq!(row_span(&rect, -1.0), None, "above the box, nothing");

        let diamond = outline(Shape::Diamond, 0.0, 0.0, 10.0, 6.0);
        let (lo, hi) = row_span(&diamond, 3.0).expect("the diamond's own widest row");
        assert!((hi - lo - 10.0).abs() < 0.01, "at its centre the diamond is as wide as its box: {lo} {hi}");
        let (lo, hi) = row_span(&diamond, 0.0).expect("the diamond's own point");
        assert!((hi - lo).abs() < 0.01, "at its very top the diamond is a point: {lo} {hi}");

        let ellipse = outline(Shape::Ellipse, 0.0, 0.0, 10.0, 6.0);
        let (elo, ehi) = row_span(&ellipse, 3.0).expect("the ellipse's own widest row");
        let (dlo, dhi) = row_span(&diamond, 3.0).expect("the diamond's own widest row");
        assert!((elo - dlo).abs() < 0.5 && (ehi - dhi).abs() < 0.5, "both reach the same box at its middle: ellipse ({elo},{ehi}) diamond ({dlo},{dhi})");
        let (elo, ehi) = row_span(&ellipse, 0.5).expect("a row near the ellipse's own top");
        assert!(ehi - elo < 10.0, "a curve is narrower near its cap than at its middle: {elo} {ehi}");
    }

    #[test]
    fn a_relation_leaves_a_box_at_its_edge_not_its_centre() {
        // Straight right: hits the right edge at mid-height.
        assert_eq!(edge_point(0.0, 0.0, 10.0, 4.0, (50.0, 2.0)), (10.0, 2.0));
        // Straight down: hits the bottom edge.
        assert_eq!(edge_point(0.0, 0.0, 10.0, 4.0, (5.0, 50.0)), (5.0, 4.0));
    }
}

// ─── the general palette ────────────────────────────────────────────────

/// A wave along `a → b`: `n` half-periods of sine, `amp` deep, perpendicular to the run.
fn wave(a: Point, b: Point, n: usize, amp: f64) -> Vec<Point> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(f64::EPSILON);
    let (nx, ny) = (-dy / len, dx / len);
    let steps = (len * 4.0) as usize + 8;
    (0..=steps)
        .map(|i| {
            let t = i as f64 / steps as f64;
            let s = (t * n as f64 * PI).sin() * amp;
            (a.0 + dx * t + nx * s, a.1 + dy * t + ny * s)
        })
        .collect()
}

fn poly(pts: &[Point]) -> Vec<(Point, Point)> {
    (0..pts.len()).map(|i| (pts[i], pts[(i + 1) % pts.len()])).collect()
}

/// A half-ellipse on the right of `(cx, top..bottom)`, bulging `rx` to the right.
fn right_arc(cx: f64, y: f64, h: f64, rx: f64) -> Vec<Point> {
    ellipse_points(cx, y + h / 2.0, rx, h / 2.0, -0.5 * PI, 0.5 * PI, 32)
}

pub(crate) fn general(shape: Shape, x: f64, y: f64, w: f64, h: f64) -> Vec<CurvePrimitive> {
    let (r, b) = (x + w, y + h);
    let k = (w * 0.18).clamp(1.0, 3.0);
    match shape {
        Shape::Triangle => vec![CurvePrimitive::Lines(poly(&[(x + w / 2.0, y), (r, b), (x, b)]))],
        Shape::PredefinedProcess => {
            let mut l = rectangle_lines(x, y, w, h);
            l.push(((x + 2.0, y), (x + 2.0, b)));
            l.push(((r - 2.0, y), (r - 2.0, b)));
            vec![CurvePrimitive::Lines(l)]
        }
        Shape::Document => {
            let foot = b - 0.7;
            vec![
                CurvePrimitive::Lines(vec![((x, y), (r, y)), ((x, y), (x, foot)), ((r, y), (r, foot))]),
                CurvePrimitive::Points(wave((x, foot), (r, foot), 2, 0.6)),
            ]
        }
        Shape::InternalStorage => {
            let mut l = rectangle_lines(x, y, w, h);
            l.push(((x, y + 1.2), (r, y + 1.2)));
            l.push(((x + 3.0, y), (x + 3.0, b)));
            vec![CurvePrimitive::Lines(l)]
        }
        Shape::Cube => {
            let (dx, dy) = (3.0_f64.min(w / 3.0), 1.0_f64.min(h / 3.0));
            let mut l = rectangle_lines(x, y + dy, w - dx, h - dy);
            l.extend([
                ((x, y + dy), (x + dx, y)),
                ((x + dx, y), (r, y)),
                ((r, y), (r, b - dy)),
                ((r, b - dy), (r - dx, b)),
                ((r - dx, y + dy), (r, y)),
            ]);
            vec![CurvePrimitive::Lines(l)]
        }
        Shape::Step => vec![CurvePrimitive::Lines(poly(&[
            (x, y),
            (r - k, y),
            (r, y + h / 2.0),
            (r - k, b),
            (x, b),
            (x + k, y + h / 2.0),
        ]))],
        Shape::Trapezoid => vec![CurvePrimitive::Lines(poly(&[(x + k, y), (r - k, y), (r, b), (x, b)]))],
        Shape::Tape => vec![
            CurvePrimitive::Lines(vec![((x, y + 0.6), (x, b - 0.6)), ((r, y + 0.6), (r, b - 0.6))]),
            CurvePrimitive::Points(wave((x, y + 0.6), (r, y + 0.6), 2, 0.5)),
            CurvePrimitive::Points(wave((x, b - 0.6), (r, b - 0.6), 2, 0.5)),
        ],
        Shape::Note => {
            let f = k.min(h / 2.0);
            let mut l = poly(&[(x, y), (r - f, y), (r, y + f), (r, b), (x, b)]);
            l.push(((r - f, y), (r - f, y + f)));
            l.push(((r - f, y + f), (r, y + f)));
            vec![CurvePrimitive::Lines(l)]
        }
        Shape::Card => {
            let f = k.min(h / 2.0);
            vec![CurvePrimitive::Lines(poly(&[(x + f, y), (r, y), (r, b), (x, b), (x, y + f)]))]
        }
        Shape::Callout => {
            let foot = b - 1.5_f64.min(h / 3.0);
            let mut l = poly(&[(x, y), (r, y), (r, foot), (x + 6.0_f64.min(w / 2.0), foot), (x + 2.0_f64.min(w / 4.0), b), (x + 3.0_f64.min(w / 3.0), foot), (x, foot)]);
            l.retain(|(p, q)| p != q);
            vec![CurvePrimitive::Lines(l)]
        }
        Shape::StickFigure => {
            let cx = x + w / 2.0;
            let head_r = (w / 5.0).min(h / 6.0).max(0.6);
            let head_y = y + head_r;
            let neck = y + head_r * 2.0;
            let hip = y + h * 0.65;
            vec![
                CurvePrimitive::Points(ellipse_points(cx, head_y, head_r * 1.6, head_r, 0.0, 2.0 * PI, 32)),
                CurvePrimitive::Lines(vec![
                    ((cx, neck), (cx, hip)),
                    ((x + w * 0.15, neck + (hip - neck) * 0.35), (r - w * 0.15, neck + (hip - neck) * 0.35)),
                    ((cx, hip), (x + w * 0.2, b)),
                    ((cx, hip), (r - w * 0.2, b)),
                ]),
            ]
        }
        Shape::DataStorage => {
            let rx = (w * 0.12).clamp(1.0, 3.0);
            vec![
                CurvePrimitive::Lines(vec![((x + rx, y), (r - rx, y)), ((x + rx, b), (r - rx, b))]),
                CurvePrimitive::Points(right_arc(r - rx, y, h, rx)),
                CurvePrimitive::Points(right_arc(x + rx, y, h, rx)),
            ]
        }
        Shape::Delay | Shape::And => {
            let rx = (w * 0.3).min(w / 2.0);
            vec![
                CurvePrimitive::Lines(vec![((x, y), (r - rx, y)), ((x, b), (r - rx, b)), ((x, y), (x, b))]),
                CurvePrimitive::Points(right_arc(r - rx, y, h, rx)),
            ]
        }
        Shape::Display => {
            let rx = (w * 0.2).min(w / 3.0);
            vec![
                CurvePrimitive::Lines(vec![((x, y + h / 2.0), (x + k, y)), ((x, y + h / 2.0), (x + k, b)), ((x + k, y), (r - rx, y)), ((x + k, b), (r - rx, b))]),
                CurvePrimitive::Points(right_arc(r - rx, y, h, rx)),
            ]
        }
        Shape::ManualInput => vec![CurvePrimitive::Lines(poly(&[(x, y + 1.0_f64.min(h / 2.0)), (r, y), (r, b), (x, b)]))],
        Shape::OffPage => {
            let f = 1.5_f64.min(h / 2.0);
            vec![CurvePrimitive::Lines(poly(&[(x, y), (r, y), (r, b - f), (x + w / 2.0, b), (x, b - f)]))]
        }
        Shape::BlockArrow => {
            let (t, m) = (y + h * 0.28, y + h / 2.0);
            let bt = b - h * 0.28;
            vec![CurvePrimitive::Lines(poly(&[(x, t), (r - k, t), (r - k, y), (r, m), (r - k, b), (r - k, bt), (x, bt)]))]
        }
        Shape::DoubleArrow => {
            let (t, m) = (y + h * 0.28, y + h / 2.0);
            let bt = b - h * 0.28;
            vec![CurvePrimitive::Lines(poly(&[
                (x, m),
                (x + k, y),
                (x + k, t),
                (r - k, t),
                (r - k, y),
                (r, m),
                (r - k, b),
                (r - k, bt),
                (x + k, bt),
                (x + k, b),
            ]))]
        }
        Shape::Or => {
            // Flat on the left; two curves meeting at a point on the right.
            let mut pts = Vec::new();
            let steps = 40;
            for i in 0..=steps {
                let t = i as f64 / steps as f64;
                // A quadratic from (x, y) through a control at (r, y) to (r, mid) — and its mirror.
                let (cx, cy) = (r, y);
                let px = (1.0 - t) * (1.0 - t) * x + 2.0 * (1.0 - t) * t * cx + t * t * r;
                let py = (1.0 - t) * (1.0 - t) * y + 2.0 * (1.0 - t) * t * cy + t * t * (y + h / 2.0);
                pts.push((px, py));
                pts.push((px, b - (py - y)));
            }
            vec![CurvePrimitive::Points(pts), CurvePrimitive::Lines(vec![((x, y), (x, b))])]
        }
        _ => Vec::new(),
    }
}

// ─── relations, as geometry ─────────────────────────────────────────────

/// A terminal cell is about twice as tall as it is wide. Arrowheads and thickness are worked
/// out in a space where that is corrected, or a symmetrical head comes out squashed.
pub const ASPECT: f64 = 2.0;

fn to_visual(p: Point) -> Point {
    (p.0, p.1 * ASPECT)
}

fn from_visual(p: Point) -> Point {
    (p.0, p.1 / ASPECT)
}

/// Pull each end of `a → b` in by the given amounts, in visual units.
fn shorten(a: Point, b: Point, by_a: f64, by_b: f64) -> (Point, Point) {
    let (va, vb) = (to_visual(a), to_visual(b));
    let (dx, dy) = (vb.0 - va.0, vb.1 - va.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < by_a + by_b + 0.5 {
        return (a, b);
    }
    let (ux, uy) = (dx / len, dy / len);
    (from_visual((va.0 + ux * by_a, va.1 + uy * by_a)), from_visual((vb.0 - ux * by_b, vb.1 - uy * by_b)))
}

/// A relation's drawing: straight segments and scattered points, in world cells. What the
/// canvas paints in braille and the vector exports write as lines and dots — one geometry,
/// so the picture on screen and the picture in the file agree.
/// The line a relation takes from `p1` to `p2`, as the points it passes through.
///
/// Straight is the two ends. Orthogonal leaves along the longer axis, turns at `elbow` cells
/// from the tail (half way when blank), crosses, and turns again into the head — three legs
/// at right angles, the way a desktop tool routes. Curved is a cubic whose handles point
/// along that same axis, sampled finely enough that braille and paper both see a curve.
/// Told which way to leave first: across (from a side) or down (from a top or bottom edge),
/// so an orthogonal route never runs along the edge it starts on; `None` picks the longer
/// axis.
pub fn route_from(p1: Point, p2: Point, route: crate::ontology::Route, elbow: Option<i64>, across_first: Option<bool>) -> Vec<Point> {
    use crate::ontology::Route;
    let (dx, dy) = (p2.0 - p1.0, p2.1 - p1.1);
    // Which way the line mostly goes, in what the eye sees: a cell is twice as tall as wide.
    let across = across_first.unwrap_or(dx.abs() >= dy.abs() * ASPECT);
    match route {
        Route::Straight => vec![p1, p2],
        Route::Orthogonal => {
            if across {
                let mx = p1.0 + elbow.map_or(dx / 2.0, |e| e as f64 * dx.signum());
                vec![p1, (mx, p1.1), (mx, p2.1), p2]
            } else {
                let my = p1.1 + elbow.map_or(dy / 2.0, |e| e as f64 * dy.signum());
                vec![p1, (p1.0, my), (p2.0, my), p2]
            }
        }
        Route::Curved => {
            let (c1, c2) = if across { ((p1.0 + dx / 2.0, p1.1), (p2.0 - dx / 2.0, p2.1)) } else { ((p1.0, p1.1 + dy / 2.0), (p2.0, p2.1 - dy / 2.0)) };
            let n = ((dx.abs() + dy.abs() * ASPECT) * 2.0).clamp(8.0, 200.0) as usize;
            (0..=n)
                .map(|i| {
                    let t = i as f64 / n as f64;
                    let (u, tt) = (1.0 - t, t);
                    let w = [u * u * u, 3.0 * u * u * tt, 3.0 * u * tt * tt, tt * tt * tt];
                    (w[0] * p1.0 + w[1] * c1.0 + w[2] * c2.0 + w[3] * p2.0, w[0] * p1.1 + w[1] * c1.1 + w[2] * c2.1 + w[3] * p2.1)
                })
                .collect()
        }
    }
}

/// The point `t` of the way along a route, by length as the eye sees it — where a label
/// or a node mark sits on a line that is not straight.
pub fn along(pts: &[Point], t: f64) -> Point {
    if pts.len() < 2 {
        return pts.first().copied().unwrap_or((0.0, 0.0));
    }
    let seg_len = |a: Point, b: Point| {
        let (va, vb) = (to_visual(a), to_visual(b));
        ((vb.0 - va.0).powi(2) + (vb.1 - va.1).powi(2)).sqrt()
    };
    let total: f64 = pts.windows(2).map(|w| seg_len(w[0], w[1])).sum();
    let mut want = total * t.clamp(0.0, 1.0);
    for w in pts.windows(2) {
        let l = seg_len(w[0], w[1]);
        if want <= l || l <= 0.0 {
            let f = if l > 0.0 { want / l } else { 0.0 };
            return (w[0].0 + (w[1].0 - w[0].0) * f, w[0].1 + (w[1].1 - w[0].1) * f);
        }
        want -= l;
    }
    pts[pts.len() - 1]
}

/// A relation drawn along a route: every leg in the line's style and width, the ends at the
/// first and last legs' directions, sized as the notation says.
pub fn relation_along(pts: &[Point], n: &crate::ontology::Notation) -> (Vec<(Point, Point)>, Vec<Point>) {
    use crate::ontology::{End, LineStyle};
    let mut lines = Vec::new();
    let mut points = Vec::new();
    if pts.len() < 2 {
        return (lines, points);
    }
    let scale = n.end_size.scale();
    // Shorten the line so a hollow head or a diamond is not drawn over by it.
    let trim = |e: End| match e {
        End::Triangle | End::Diamond | End::HollowDiamond => 2.0 * scale,
        End::Dot | End::Circle => 0.6 * scale,
        _ => 0.0,
    };
    let mut line = |a: Point, b: Point| match n.line {
        LineStyle::Dotted => points.extend(dashed_line(a, b, 0.4, 0.9)),
        LineStyle::Dashed => points.extend(dashed_line(a, b, 1.6, 1.0)),
        LineStyle::Solid => lines.push((a, b)),
    };
    let last = pts.len() - 1;
    for i in 0..last {
        let (mut a, mut b) = (pts[i], pts[i + 1]);
        if i == 0 {
            a = shorten(a, b, trim(n.tail), 0.0).0;
        }
        if i == last - 1 {
            b = shorten(a, b, 0.0, trim(n.head)).1;
        }
        line(a, b);
        // Width is extra strokes a braille dot to either side — one for 2, both for 3.
        let (va, vb) = (to_visual(a), to_visual(b));
        let (dx, dy) = (vb.0 - va.0, vb.1 - va.1);
        let len = (dx * dx + dy * dy).sqrt();
        if n.width > 1 && len > f64::EPSILON {
            let (nx, ny) = (-dy / len * 0.5, dx / len * 0.5);
            let side = |s: f64| (from_visual((va.0 + nx * s, va.1 + ny * s)), from_visual((vb.0 + nx * s, vb.1 + ny * s)));
            let (a2, b2) = side(1.0);
            line(a2, b2);
            if n.width > 2 {
                let (a3, b3) = side(-1.0);
                line(a3, b3);
            }
        }
    }
    end(pts[last], pts[last - 1], n.head, scale, &mut lines, &mut points);
    end(pts[0], pts[1], n.tail, scale, &mut lines, &mut points);
    (lines, points)
}

/// What sits at `tip`, pointing away from `from`, `scale` times the usual size.
fn end(tip: Point, from: Point, end: crate::ontology::End, scale: f64, lines: &mut Vec<(Point, Point)>, points: &mut Vec<Point>) {
    use crate::ontology::End;
    if end == End::None {
        return;
    }
    let (vt, vf) = (to_visual(tip), to_visual(from));
    let (dx, dy) = (vt.0 - vf.0, vt.1 - vf.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < f64::EPSILON {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let (nx, ny) = (-uy, ux);
    let mut seg = |p: Point, q: Point| lines.push((from_visual(p), from_visual(q)));
    let along = |d: f64, s: f64| (vt.0 - ux * d * scale + nx * s * scale, vt.1 - uy * d * scale + ny * s * scale);
    // Heads are small and made of few strokes: at braille resolution a filled shape is a
    // blob, so "filled" is a spine down the middle and "hollow" is its absence.
    match end {
        End::None => {}
        End::Arrow => {
            let (l, r) = (along(2.0, 0.9), along(2.0, -0.9));
            seg(vt, l);
            seg(vt, r);
            seg(l, r);
            seg(vt, along(2.0, 0.0));
        }
        End::Open => {
            seg(vt, along(2.0, 0.9));
            seg(vt, along(2.0, -0.9));
        }
        End::Triangle => {
            let (l, r) = (along(2.0, 1.0), along(2.0, -1.0));
            seg(vt, l);
            seg(vt, r);
            seg(l, r);
        }
        End::Diamond | End::HollowDiamond => {
            let (mid_l, mid_r, back) = (along(1.0, 0.8), along(1.0, -0.8), along(2.0, 0.0));
            seg(vt, mid_l);
            seg(vt, mid_r);
            seg(mid_l, back);
            seg(mid_r, back);
            if end == End::Diamond {
                seg(vt, back);
            }
        }
        End::Dot | End::Circle => {
            let c = from_visual(along(0.6, 0.0));
            let (rx, ry) = if end == End::Dot { (0.5 * scale, 0.25 * scale) } else { (0.7 * scale, 0.35 * scale) };
            points.extend(ellipse_points(c.0, c.1, rx, ry, 0.0, std::f64::consts::TAU, 20));
        }
        // Entity-relationship ends. A crow's foot fans three strokes from a point back on
        // the line out to the tip; a bar crosses the line just short of it.
        End::Crow => {
            let back = along(2.4, 0.0);
            seg(back, along(0.0, 1.2));
            seg(back, along(0.0, -1.2));
            seg(back, vt);
        }
        End::Bar => seg(along(1.0, 1.2), along(1.0, -1.2)),
    }
}

/// The same outline drawn by hand: every straight edge wanders a little either side of
/// where it should be, every curve too, the way a whiteboard marker does. Deterministic —
/// the wobble comes from `seed`, so a shape sketches the same way every frame and in every
/// export, and does not shimmer.
///
/// `amp` is how far, in cells across; down is half that, so the wobble reads the same in
/// both directions on cells twice as tall as they are wide.
pub fn sketch(prims: Vec<CurvePrimitive>, seed: u64, amp: f64) -> Vec<CurvePrimitive> {
    let mut n = 0u64;
    // A slow wave along the stroke — wavelengths of several cells, the way a hand drifts —
    // with a faint faster one over it. Not a jitter: a jitter reads as torn paper.
    let mut wave = move |seed_n: u64| {
        n += 1;
        let phase = hash(seed ^ (seed_n << 8) ^ n) as f64 / u64::MAX as f64 * std::f64::consts::TAU;
        let k = 0.8 + hash(seed ^ n) as f64 / u64::MAX as f64 * 0.4;
        move |t: f64| (t * 0.9 * k + phase).sin() * 0.75 + (t * 2.3 + phase * 3.0).sin() * 0.25
    };
    // Ends stay put, so a corner still meets and an arc still joins its edge; the middle
    // wanders.
    let pinned = |t: f64| 1.0 - (2.0 * t - 1.0).powi(4);
    prims
        .into_iter()
        .map(|p| match p {
            CurvePrimitive::Lines(ls) => {
                let mut pts = Vec::new();
                for (a, b) in ls {
                    let w = wave(0);
                    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                    let len = (dx * dx + dy * dy).sqrt().max(0.01);
                    let (nx, ny) = (-dy / len, dx / len * 0.5);
                    let steps = ((len * 2.0).ceil() as usize).clamp(2, 60);
                    let mut prev = a;
                    for i in 1..=steps {
                        let t = i as f64 / steps as f64;
                        let off = w(t * len) * amp * pinned(t);
                        let q = (a.0 + dx * t + nx * off, a.1 + dy * t + ny * off);
                        pts.push((prev, q));
                        prev = q;
                    }
                }
                CurvePrimitive::Lines(pts)
            }
            CurvePrimitive::Points(ps) => {
                if ps.len() < 3 {
                    return CurvePrimitive::Points(ps);
                }
                let w = wave(1);
                // Distance along the curve at each point, so the wave has the same
                // wavelength on an arc as on an edge.
                let mut along = vec![0.0; ps.len()];
                for i in 1..ps.len() {
                    let (dx, dy) = (ps[i].0 - ps[i - 1].0, ps[i].1 - ps[i - 1].1);
                    along[i] = along[i - 1] + (dx * dx + dy * dy).sqrt();
                }
                let total = along[ps.len() - 1].max(0.01);
                let (f, l) = (ps[0], ps[ps.len() - 1]);
                let closed = (f.0 - l.0).abs() < 0.5 && (f.1 - l.1).abs() < 0.5;
                let out = (0..ps.len())
                    .map(|i| {
                        let (p0, p1) = (ps[i.saturating_sub(1)], ps[(i + 1).min(ps.len() - 1)]);
                        let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
                        let d = (dx * dx + dy * dy).sqrt().max(0.01);
                        let env = if closed { 1.0 } else { pinned(along[i] / total) };
                        let off = w(along[i]) * amp * env;
                        (ps[i].0 - dy / d * off, ps[i].1 + dx / d * off * 0.5)
                    })
                    .collect();
                CurvePrimitive::Points(out)
            }
        })
        .collect()
}

fn hash(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^ (x >> 33)
}

#[cfg(test)]
mod route_tests {
    use super::*;
    use crate::ontology::{End, EndSize, LineStyle, Notation, Route};

    fn plain() -> Notation {
        Notation { line: LineStyle::Solid, tail: End::None, head: End::Arrow, width: 1, color: None, route: Route::Straight, end_size: EndSize::Normal, opacity: 100 }
    }

    #[test]
    fn an_orthogonal_route_turns_twice_a_curve_bends_and_a_point_along_follows_the_length() {
        let (p1, p2) = ((0.0, 0.0), (40.0, 10.0));
        assert_eq!(route_from(p1, p2, Route::Straight, None, None), vec![p1, p2]);
        let o = route_from(p1, p2, Route::Orthogonal, None, None);
        assert_eq!(o, vec![p1, (20.0, 0.0), (20.0, 10.0), p2], "half way across, then down");
        let o = route_from(p1, p2, Route::Orthogonal, Some(5), None);
        assert_eq!(o[1], (5.0, 0.0), "an elbow: five cells from the tail");
        let o = route_from((0.0, 0.0), (4.0, 20.0), Route::Orthogonal, None, None);
        assert_eq!(o[1], (0.0, 10.0), "mostly down: leaves downward first");
        let c = route_from(p1, p2, Route::Curved, None, None);
        assert!(c.len() > 8 && c[0] == p1 && c[c.len() - 1] == p2);
        let mid = c[c.len() / 2];
        assert!(mid.1 > 2.0 && mid.1 < 8.0 && mid.0 > 15.0 && mid.0 < 25.0, "the curve passes near the middle: {mid:?}");
        assert_eq!(along(&[p1, (20.0, 0.0), (20.0, 10.0), p2], 0.5).0, 20.0, "half way along the orthogonal route is on its cross leg");
        assert_eq!(along(&[p1, p2], 0.25), (10.0, 2.5));
    }

    #[test]
    fn a_relation_along_a_route_draws_every_leg_and_sizes_its_ends() {
        let pts = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (40.0, 10.0)];
        let (lines, _) = relation_along(&pts, &plain());
        assert!(lines.len() >= 3 + 4, "three legs and an arrowhead's strokes: {}", lines.len());
        let mut big = plain();
        big.end_size = EndSize::Large;
        let (large, _) = relation_along(&pts, &big);
        let span = |ls: &[(Point, Point)]| ls[ls.len() - 4..].iter().map(|(a, b)| (a.0 - b.0).abs()).fold(0.0, f64::max);
        assert!(span(&large) > span(&lines) * 1.3, "a large head reaches further back");
    }
}

#[cfg(test)]
mod sketch_tests {
    use super::*;

    #[test]
    fn a_shear_leans_the_top_left_and_the_bottom_right_by_half_the_skew_each() {
        let b = (0.0, 0.0, 20.0, 6.0);
        let prims = sheared(outline(Shape::Rectangle, 0.0, 0.0, 20.0, 6.0), (4.0, 0.0), b);
        let CurvePrimitive::Lines(ls) = &prims[0] else { panic!() };
        assert_eq!(ls[0].0, (-2.0, 0.0), "top left");
        assert_eq!(ls[1].1, (22.0, 6.0), "bottom right");
        let prims = sheared(outline(Shape::Rectangle, 0.0, 0.0, 20.0, 6.0), (0.0, 2.0), b);
        let CurvePrimitive::Lines(ls) = &prims[0] else { panic!() };
        assert_eq!(ls[0].0, (0.0, -1.0), "down-lean: the left edge up a cell");
        assert_eq!(ls[1].1, (20.0, 7.0), "and the right edge down one");
        assert_eq!(sheared(outline(Shape::Rectangle, 0.0, 0.0, 20.0, 6.0), (0.0, 0.0), b), outline(Shape::Rectangle, 0.0, 0.0, 20.0, 6.0));
    }

    #[test]
    fn a_sketched_outline_keeps_its_corners_wanders_between_them_and_repeats() {
        let prims = outline(Shape::Rectangle, 0.0, 0.0, 20.0, 6.0);
        let a = sketch(prims.clone(), 7, 0.4);
        let b = sketch(prims, 7, 0.4);
        assert_eq!(a, b, "the same seed draws the same hand");
        let CurvePrimitive::Lines(ls) = &a[0] else { panic!() };
        assert!(ls.len() > 4, "each edge became several pieces");
        assert_eq!(ls[0].0, (0.0, 0.0), "the corner stays");
        let off = ls.iter().map(|(p, _)| (p.1 - if p.0 > 0.5 && p.0 < 19.5 && p.1 < 3.0 { 0.0 } else { p.1 }).abs()).fold(0.0, f64::max);
        assert!(off > 0.05 && off <= 0.4, "the top edge wanders, but not far: {off}");
    }
}
