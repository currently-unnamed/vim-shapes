//! The clean picture — the diagram drawn the way a desktop diagram tool draws it: a white
//! ground, a light grid, solid black outlines, filled arrowheads, sans-serif labels.
//!
//! The braille picture is the terminal's; this one is for showing someone. Both are built
//! from the same geometry (`shapes::outline`, `shapes::relation`, `canvas::label_lines`), so
//! they agree about where everything is — but here a sampled curve becomes a polyline, a
//! dashed run becomes a dash pattern, and an arrowhead becomes a filled polygon. It is the
//! picture `V` previews, and the `clean` style of every export.

use image::{Rgba, RgbaImage};

use crate::export::{Appearance, Item, Options, Picture, CELL_H, CELL_W};
use crate::model::{Document, Node};
use crate::ontology::{mix, Colour, End, LineStyle, Paint, Shape};
use crate::shapes::{self, CurvePrimitive, Point};
use crate::ui::canvas;

const INK: [u8; 3] = [0, 0, 0];
const PAPER: [u8; 3] = [255, 255, 255];
const GRID_MINOR: [u8; 3] = [232, 232, 232];
const GRID_MAJOR: [u8; 3] = [208, 208, 208];

fn paint_rgb(c: Colour, light: bool) -> [u8; 3] {
    c.on(light)
}

/// Build the clean picture.
pub fn picture(doc: &Document, o: &Options) -> Picture {
    let (ox, oy, wc, hc) = crate::export::frame_of(doc, o.border);
    let s = o.scale();
    let px = |x: f64| (x - ox) * CELL_W * s;
    let py = |y: f64| (y - oy) * CELL_H * s;
    let (width, height) = o.size_px(doc);
    let dark = o.appearance == Appearance::Dark;
    let ink = if dark { [235, 219, 178] } else { INK };
    let paper = if dark { [29, 32, 33] } else { PAPER };
    let page = &doc.metadata.page;
    let ground = page.background.map(|c| c.on(!dark)).unwrap_or(paper);
    let mut items = Vec::new();

    // The grid, at the diagram's own density: ruled, a line every quarter of a grid step
    // and a heavier one at the step, the way graph paper is; or a dot at each step, the way
    // the screen marks it.
    if o.grid {
        let (gx, _) = page.grid_step();
        let step = CELL_W * s * gx as f64 / 4.0;
        let (gw, gh) = (wc * CELL_W * s, hc * CELL_H * s);
        // The grid in the diagram's own colour when it has one, the major line a shade
        // darker; otherwise a light grey on paper, and nearly nothing on a dark ground.
        let (major, minor) = match page.grid_color.map(|c| c.on(!dark)) {
            Some(c) => (c.map(|v| (v as f64 * 0.85) as u8), c),
            None if dark => (GRID_MAJOR.map(|v| v / 5), GRID_MINOR.map(|v| v / 5)),
            None => (GRID_MAJOR, GRID_MINOR),
        };
        if !page.grid_style.ruled_on_paper() {
            let big = step * 4.0;
            let mut y = 0.0;
            while y <= gh {
                let mut x = 0.0;
                while x <= gw {
                    items.push(Item::Dot { p: (x, y), color: major });
                    x += big;
                }
                y += big;
            }
        } else {
            let mut i = 0;
            let mut x = 0.0;
            while x <= gw {
                let color = if i % 4 == 0 { major } else { minor };
                items.push(Item::Polyline { pts: vec![(x, 0.0), (x, gh)], color, width: 1.0, dash: None });
                x += step;
                i += 1;
            }
            let mut i = 0;
            let mut y = 0.0;
            while y <= gh {
                let color = if i % 4 == 0 { major } else { minor };
                items.push(Item::Polyline { pts: vec![(0.0, y), (gw, y)], color, width: 1.0, dash: None });
                y += step;
                i += 1;
            }
        }
    }

    // Shapes: filled with the paper so the grid stops at their edge, outlined in ink.
    for e in &doc.elements {
        // Opacity: every colour of the shape moved toward the ground by how see-through it
        // is — on a flat ground, what a real alpha would show.
        let through = |c: [u8; 3]| mix(c, ground, 1.0 - e.drawn_opacity() as f64 / 100.0);
        let color = through(e.color.map(|c| paint_rgb(c, !dark)).unwrap_or(ink));
        let width = 1.5 * s * e.stroke.max(1) as f64;
        let composite = e.kind.is_composite();
        // The fill: `auto` is the layer's pastel — what architects read — on paper, and the
        // paper itself on a dark ground, where a pastel would glare; a plain shape is paper.
        let fill = match (e.fill, dark) {
            (crate::model::Fill::Auto, true) => Some(paper),
            _ => e.fill_on(!dark).map(|c| if e.fill == crate::model::Fill::Auto && e.kind.is_sketch() { paper } else { c }),
        }
        .map(through);
        let prims = shapes::drawn(page, e, 0.3);
        let mut hull: Vec<Point> = Vec::new();
        let mut polylines: Vec<Vec<Point>> = Vec::new();
        for prim in &prims {
            match prim {
                CurvePrimitive::Lines(ls) => {
                    for (a, b) in ls {
                        polylines.push(vec![(px(a.0), py(a.1)), (px(b.0), py(b.1))]);
                        hull.push((px(a.0), py(a.1)));
                        hull.push((px(b.0), py(b.1)));
                    }
                }
                CurvePrimitive::Points(ps) => {
                    let pts: Vec<Point> = ps.iter().map(|p| (px(p.0), py(p.1))).collect();
                    hull.extend(pts.iter().copied());
                    polylines.push(pts);
                }
            }
        }
        // The fill: the convex hull of the outline's points, which is the shape itself for
        // everything convex and covers the rest. Not for a grouping — it is a boundary, and
        // the things inside it must stay visible — nor for a figure or bare text.
        let filled = !composite && !matches!(e.kind.shape(), Shape::Text | Shape::StickFigure | Shape::Dashed) && fill.is_some();
        let paper = fill.unwrap_or(paper);
        // The shadow: the shape's own fill again, in shade, a few pixels down and right,
        // under the shape — so it shows only past the edge, as a shadow does.
        let (sdx, sdy) = (3.0 * s, 3.0 * s);
        let shade = if dark { [0, 0, 0] } else { [170, 170, 170] };
        // A cloud is five bumps, and what a desktop tool draws is their OUTLINE: each bump
        // filled with paper, then only the arcs that lie outside every other bump stroked.
        let cloud = e.kind.shape() == Shape::Cloud;
        let bumps: Vec<Vec<Point>> = if cloud {
            prims
                .iter()
                .filter_map(|p| match p {
                    CurvePrimitive::Points(ps) => Some(ps.iter().map(|q| (px(q.0), py(q.1))).collect()),
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };
        if filled && hull.len() >= 3 {
            let hull = convex_hull(hull);
            // The shadow goes under every fill: a cloud's is its bumps', so it dips between
            // them as the cloud does; anything else's is its hull.
            if page.shadow {
                let shadows: Vec<&Vec<Point>> = if cloud { bumps.iter().collect() } else { vec![&hull] };
                for b in shadows {
                    items.push(Item::Polygon { pts: b.iter().map(|p| (p.0 + sdx, p.1 + sdy)).collect(), fill: shade, stroke: None });
                }
            }
            items.push(Item::Polygon { pts: hull, fill: paper, stroke: None });
        }
        if cloud {
            for b in &bumps {
                items.push(Item::Polygon { pts: b.clone(), fill: paper, stroke: None });
            }
            // Neighbours are tested a little enlarged, so an arc that merely grazes another
            // bump's edge is taken as inside it rather than left as a sliver.
            let grown: Vec<Vec<Point>> = bumps
                .iter()
                .map(|b| {
                    let n = b.len().max(1) as f64;
                    let (cx, cy) = b.iter().fold((0.0, 0.0), |(x, y), p| (x + p.0 / n, y + p.1 / n));
                    b.iter().map(|p| (cx + (p.0 - cx) * 1.12, cy + (p.1 - cy) * 1.12)).collect()
                })
                .collect();
            for (i, b) in bumps.iter().enumerate() {
                let mut run: Vec<Point> = Vec::new();
                for &q in b {
                    let inside = grown.iter().enumerate().any(|(j, o)| j != i && inside_polygon(q, o));
                    // A run shorter than a few points is a sliver at a junction, not an arc.
                    if inside {
                        if run.len() >= 5 && e.outline {
                            items.push(Item::Polyline { pts: std::mem::take(&mut run), color, width, dash: dash_of(e.drawn_line(), width) });
                        }
                        run.clear();
                    } else {
                        run.push(q);
                    }
                }
                if run.len() >= 5 && e.outline {
                    items.push(Item::Polyline { pts: run, color, width, dash: dash_of(e.drawn_line(), width) });
                }
            }
            for (x, y, line) in canvas::label_lines(e, &e.label) {
                let size = e.text.size.map(|n| n as f64 * s * 0.8).unwrap_or(15.0 * s);
                let t = &e.text;
                items.push(Item::Text { x: px(x), y: py(y), text: line, color: t.color.map(|c| c.on(!dark)).unwrap_or(ink), font: t.font.clone(), size, bold: t.bold, italic: t.italic, underline: t.underline, band: t.band.then_some(paper) });
            }
            continue;
        }
        let dashed = e.kind.shape() == Shape::Dashed;
        if dashed {
            // A grouping is a dashed rectangle, not the braille dashes' dots.
            let (x0, y0, x1, y1) = (px(e.x), py(e.y), px(e.right()), py(e.bottom()));
            items.push(Item::Polyline { pts: vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)], color, width, dash: dash_of(LineStyle::Dashed, width) });
        } else if e.outline {
            for pts in polylines {
                items.push(Item::Polyline { pts, color, width, dash: dash_of(e.drawn_line(), width) });
            }
        }
        // Labels: sans-serif, centred as on screen, the kind's tag small in the corner.
        let size = e.text.size.map(|n| n as f64 * s * 0.8).unwrap_or(15.0 * s);
        if composite {
            items.push(Item::Text { x: px(e.x + 2.0), y: py(e.y) + 2.0 * s, text: e.kind.short().into(), color: [120, 120, 120], font: None, size: 12.0 * s, bold: false, italic: false, underline: false, band: None });
            if !e.label.is_empty() {
                items.push(Item::Text { x: px(e.x + 2.0), y: py(e.y + 1.0) + 2.0 * s, text: e.label.clone(), color: ink, font: e.text.font.clone(), size, bold: true, italic: false, underline: false, band: None });
            }
            continue;
        }
        // The kind's tag, top centre: inside every outline, an ellipse's included, where a
        // corner would leave it hanging outside the curve.
        if !e.kind.is_sketch() && e.h >= 4.0 {
            let size = 11.0 * s;
            let tag = e.tag_marked(true);
            let w = tag.chars().count() as f64 * size * 0.55;
            let x = px(e.x + e.w / 2.0) - w / 2.0;
            items.push(Item::Text { x, y: py(e.y) + 4.0 * s, text: e.kind.short().into(), color: [120, 120, 120], font: None, size, bold: false, italic: false, underline: false, band: None });
            // Paper's mark is a letter, since a sans face may have no ✗ — red all the same.
            if let Some(m) = e.status.mark(true) {
                let x = x + (e.kind.short().chars().count() + 1) as f64 * size * 0.55;
                items.push(Item::Text { x, y: py(e.y) + 4.0 * s, text: m.into(), color: paint_rgb(Paint::Red.into(), !dark), font: None, size, bold: true, italic: false, underline: false, band: None });
            }
        }
        let t = &e.text;
        let text_ink = t.color.map(|c| c.on(!dark)).unwrap_or(ink);
        for (x, y, line) in canvas::label_lines(e, &e.label) {
            items.push(Item::Text { x: px(x), y: py(y), text: line, color: text_ink, font: t.font.clone(), size, bold: t.bold || e.has_rows(), italic: t.italic, underline: t.underline, band: t.band.then_some(paper) });
        }
        // The compartment: a rule under the header, a row per property in a smaller sans,
        // the type set against the right edge the way a class diagram does it.
        if let Some(ry) = e.header_rule() {
            items.push(Item::Polyline { pts: vec![(px(e.x), py(ry)), (px(e.right()), py(ry))], color, width, dash: None });
            let row_size = 12.0 * s;
            // The parts run in property order; the overflow row, last, has no property of its
            // own. A deprecated row's letter is set apart in red, and the name where it stood.
            let gone = crate::model::Status::Deprecated.mark(true).unwrap_or_default();
            for (i, (x, y, head, ty)) in e.row_parts(true).into_iter().enumerate() {
                let (x, head) = match head.strip_prefix(gone).filter(|_| e.properties.get(i).is_some_and(|p| p.mark(true).trim() == gone)) {
                    Some(rest) => {
                        items.push(Item::Text { x: px(x), y: py(y) + 2.0 * s, text: gone.into(), color: paint_rgb(Paint::Red.into(), !dark), font: None, size: row_size, bold: true, italic: false, underline: false, band: None });
                        (px(x) + (gone.chars().count() as f64) * row_size * 0.55, rest.to_string())
                    }
                    None => (px(x), head),
                };
                items.push(Item::Text { x, y: py(y) + 2.0 * s, text: head, color: ink, font: None, size: row_size, bold: false, italic: false, underline: false, band: None });
                let tw = ty.chars().count() as f64 * row_size * 0.55;
                items.push(Item::Text { x: px(e.right() - 2.0) - tw, y: py(y) + 2.0 * s, text: ty, color: [120, 120, 120], font: None, size: row_size, bold: false, italic: false, underline: false, band: None });
            }
        }
    }

    // Relations: a line, dashed by its style, and the ends as real arrowheads.
    let refused: Vec<_> = doc.lint().into_iter().map(|p| p.relation).collect();
    for r in &doc.relations {
        let Some(route) = doc.route(r) else { continue };
        let n = r.notation();
        let color = if refused.contains(&r.id) { paint_rgb(Paint::Red.into(), !dark) } else { n.color.map(|c| paint_rgb(c, !dark)).unwrap_or(ink) };
        let color = mix(color, ground, 1.0 - n.opacity as f64 / 100.0);
        let width = 1.5 * s * n.width.max(1) as f64;
        // The ends are sized by the notation; so is what the line leaves clear for them.
        let es = s * n.end_size.scale();
        let mut pts: Vec<Point> = route.iter().map(|p| (px(p.0), py(p.1))).collect();
        // Shorten to the base of a hollow head or a diamond, so the line does not cross it.
        let trim = |e: End| -> f64 {
            match e {
                End::Triangle | End::Diamond | End::HollowDiamond | End::Arrow => 14.0 * es,
                End::Dot | End::Circle => 6.0 * es,
                _ => 0.0,
            }
        };
        let dir = |a: Point, b: Point| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt().max(f64::EPSILON);
            (dx / len, dy / len)
        };
        let last = pts.len() - 1;
        let (a, b) = (pts[0], pts[last]);
        let (ua, ub) = (dir(pts[0], pts[1]), dir(pts[last - 1], pts[last]));
        pts[0] = (a.0 + ua.0 * trim(n.tail), a.1 + ua.1 * trim(n.tail));
        pts[last] = (b.0 - ub.0 * trim(n.head), b.1 - ub.1 * trim(n.head));
        items.push(Item::Polyline { pts, color, width, dash: dash_of(n.line, width) });
        end(&mut items, b, ub, n.head, color, width, paper, es);
        end(&mut items, a, (-ua.0, -ua.1), n.tail, color, width, paper, es);
        for node in Node::ALL {
            if let (Some(text), Some((cx, cy))) = (r.label_at(node), doc.label_point(r, node)) {
                let (cx, cy) = (px(cx), py(cy));
                let t = &r.text;
                let size = t.size.map_or(13.0 * s, |n| n as f64 * s * 0.8);
                let w = text.chars().count() as f64 * size * 0.55;
                items.push(Item::Text { x: cx - w / 2.0, y: cy - 10.0 * s, text: text.to_string(), color: t.color.map(|c| c.on(!dark)).unwrap_or(color), font: t.font.clone(), size, bold: t.bold, italic: t.italic, underline: false, band: t.band.then_some(paper) });
            }
        }
    }

    Picture { width, height, ground: if o.transparent { None } else { Some(ground) }, items }
}

/// An arrowhead at `tip`, pointing along `u`, drawn the way a diagram tool draws it: filled
/// or paper-filled polygons, real strokes, real circles.
#[allow(clippy::too_many_arguments)]
fn end(items: &mut Vec<Item>, tip: Point, u: (f64, f64), e: End, color: [u8; 3], width: f64, paper: [u8; 3], s: f64) {
    if e == End::None {
        return;
    }
    let (ux, uy) = u;
    let (nx, ny) = (-uy, ux);
    let along = |d: f64, side: f64| (tip.0 - ux * d + nx * side, tip.1 - uy * d + ny * side);
    let (l, hw) = (14.0 * s, 6.0 * s);
    match e {
        End::None => {}
        End::Arrow => items.push(Item::Polygon { pts: vec![tip, along(l, hw), along(l, -hw)], fill: color, stroke: None }),
        End::Triangle => items.push(Item::Polygon { pts: vec![tip, along(l, hw), along(l, -hw)], fill: paper, stroke: Some((color, width)) }),
        End::Open => {
            items.push(Item::Polyline { pts: vec![along(l, hw), tip, along(l, -hw)], color, width, dash: None });
        }
        End::Diamond => items.push(Item::Polygon { pts: vec![tip, along(l / 2.0, hw), along(l, 0.0), along(l / 2.0, -hw)], fill: color, stroke: None }),
        End::HollowDiamond => items.push(Item::Polygon { pts: vec![tip, along(l / 2.0, hw), along(l, 0.0), along(l / 2.0, -hw)], fill: paper, stroke: Some((color, width)) }),
        End::Dot | End::Circle => {
            let c = along(6.0 * s, 0.0);
            let r = 5.0 * s;
            let pts: Vec<Point> = (0..24).map(|i| { let t = i as f64 / 24.0 * std::f64::consts::TAU; (c.0 + r * t.cos(), c.1 + r * t.sin()) }).collect();
            let fill = if e == End::Dot { color } else { paper };
            items.push(Item::Polygon { pts, fill, stroke: Some((color, width)) });
        }
        End::Crow => {
            let back = along(l, 0.0);
            for side in [hw, 0.0, -hw] {
                items.push(Item::Polyline { pts: vec![back, along(0.0, side * 1.2)], color, width, dash: None });
            }
        }
        End::Bar => items.push(Item::Polyline { pts: vec![along(6.0 * s, hw), along(6.0 * s, -hw)], color, width, dash: None }),
    }
}

/// The convex hull of a point set — Andrew's monotone chain.
fn convex_hull(mut pts: Vec<Point>) -> Vec<Point> {
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.partial_cmp(&b.1).unwrap()));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: Point, a: Point, b: Point| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut lower: Vec<Point> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Point> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Whether a point lies strictly inside a polygon — the even-odd rule.
fn inside_polygon(q: Point, poly: &[Point]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.1 > q.1) != (b.1 > q.1) {
            let x = a.0 + (q.1 - a.1) / (b.1 - a.1) * (b.0 - a.0);
            if q.0 < x {
                inside = !inside;
            }
        }
    }
    inside
}

// ─── a small vector rasteriser ───────────────────────────────────────────

fn blend(img: &mut RgbaImage, x: i32, y: i32, color: [u8; 3], a: f64) {
    if x < 0 || y < 0 || x as u32 >= img.width() || y as u32 >= img.height() || a <= 0.0 {
        return;
    }
    let a = a.min(1.0);
    let p = img.get_pixel(x as u32, y as u32).0;
    let ba = p[3] as f64 / 255.0;
    let mut v = [0u8; 4];
    for k in 0..3 {
        v[k] = if ba > 0.0 { (p[k] as f64 + (color[k] as f64 - p[k] as f64) * a).round() as u8 } else { color[k] };
    }
    v[3] = ((ba + a * (1.0 - ba)) * 255.0).round() as u8;
    img.put_pixel(x as u32, y as u32, Rgba(v));
}

/// A stroke from `a` to `b`, `w` wide, antialiased by distance from the segment.
fn stroke(img: &mut RgbaImage, a: Point, b: Point, w: f64, color: [u8; 3]) {
    let r = (w / 2.0).max(0.5);
    let (x0, y0) = ((a.0.min(b.0) - r - 1.0).floor() as i32, (a.1.min(b.1) - r - 1.0).floor() as i32);
    let (x1, y1) = ((a.0.max(b.0) + r + 1.0).ceil() as i32, (a.1.max(b.1) + r + 1.0).ceil() as i32);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let t = if len2 > 0.0 { ((px - a.0) * dx + (py - a.1) * dy) / len2 } else { 0.0 }.clamp(0.0, 1.0);
            let (cx, cy) = (a.0 + dx * t, a.1 + dy * t);
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            let cov = (r + 0.5 - d).clamp(0.0, 1.0);
            blend(img, x, y, color, cov);
        }
    }
}

/// The dash a line style strokes with, at a stroke width: `None` for solid.
pub fn dash_of(line: LineStyle, width: f64) -> Option<(f64, f64)> {
    let w = width.max(1.0);
    match line {
        LineStyle::Solid => None,
        LineStyle::Dashed => Some((8.0 * w / 1.5, 6.0 * w / 1.5)),
        // A dot is a dash as long as the line is wide, drawn with round ends.
        LineStyle::Dotted => Some((w * 0.6, w * 2.4)),
    }
}

fn polyline(img: &mut RgbaImage, pts: &[Point], w: f64, color: [u8; 3], dash: Option<(f64, f64)>) {
    for seg in pts.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let Some((on, off)) = dash else {
            stroke(img, a, b, w, color);
            continue;
        };
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < f64::EPSILON {
            continue;
        }
        let mut t = 0.0;
        while t < len {
            let e = (t + on).min(len);
            stroke(img, (a.0 + dx * t / len, a.1 + dy * t / len), (a.0 + dx * e / len, a.1 + dy * e / len), w, color);
            t += on + off;
        }
    }
}

/// A filled polygon by scanline, with the edge antialiased by sub-sampling each row.
fn polygon(img: &mut RgbaImage, pts: &[Point], fill: [u8; 3]) {
    if pts.len() < 3 {
        return;
    }
    let (ymin, ymax) = pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
    let (y0, y1) = (ymin.floor() as i32, ymax.ceil() as i32);
    const SUB: usize = 4;
    for y in y0..=y1 {
        // Coverage per pixel column across this row, from SUB scanlines.
        let mut cov: std::collections::BTreeMap<i32, f64> = std::collections::BTreeMap::new();
        for sy in 0..SUB {
            let yy = y as f64 + (sy as f64 + 0.5) / SUB as f64;
            let mut xs: Vec<f64> = Vec::new();
            for i in 0..pts.len() {
                let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
                if (p.1 <= yy && q.1 > yy) || (q.1 <= yy && p.1 > yy) {
                    xs.push(p.0 + (yy - p.1) / (q.1 - p.1) * (q.0 - p.0));
                }
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for pair in xs.chunks(2) {
                if pair.len() < 2 {
                    break;
                }
                let (xa, xb) = (pair[0], pair[1]);
                let (ia, ib) = (xa.floor() as i32, xb.ceil() as i32);
                for x in ia..ib {
                    let l = (x as f64).max(xa);
                    let r = (x as f64 + 1.0).min(xb);
                    if r > l {
                        *cov.entry(x).or_insert(0.0) += (r - l) / SUB as f64;
                    }
                }
            }
        }
        for (x, c) in cov {
            blend(img, x, y, fill, c);
        }
    }
}

/// Fonts a desktop tool would set its labels in.
const SANS: &[&str] = &[
    "/System/Library/Fonts/Helvetica.ttc",
    "/System/Library/Fonts/HelveticaNeue.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
];

fn load(path: &str) -> Option<fontdue::Font> {
    let bytes = std::fs::read(path).ok()?;
    fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()
}

/// Rasterise a picture — any picture — to pixels.
pub fn raster(p: &Picture) -> RgbaImage {
    let mut img = match p.ground {
        Some(g) => RgbaImage::from_pixel(p.width.max(1), p.height.max(1), Rgba([g[0], g[1], g[2], 255])),
        None => RgbaImage::from_pixel(p.width.max(1), p.height.max(1), Rgba([0, 0, 0, 0])),
    };
    let sans = SANS.iter().find_map(|f| load(f));
    let mut cache: Vec<((String, bool, bool), Option<fontdue::Font>)> = Vec::new();
    for it in &p.items {
        match it {
            Item::Line { a, b, color, width } => stroke(&mut img, *a, *b, width * 1.5, *color),
            Item::Dot { p: c, color } => {
                let r = (p.width as f64 / 400.0).clamp(0.9, 3.0);
                stroke(&mut img, *c, *c, r * 2.0, *color);
            }
            Item::Polyline { pts, color, width, dash } => polyline(&mut img, pts, *width, *color, *dash),
            Item::Polygon { pts, fill, stroke: st } => {
                polygon(&mut img, pts, *fill);
                if let Some((c, w)) = st {
                    let mut closed = pts.clone();
                    closed.push(pts[0]);
                    polyline(&mut img, &closed, *w, *c, None);
                }
            }
            Item::Text { x, y, text, color, font, size, bold, italic, underline, band } => {
                // The family's own face for the look, cached by file; the sans stands in
                // for a family this machine lacks, and then bold and italic are faked.
                let key = font.as_deref().map(|name| (name.to_string(), *bold, *italic));
                let own = key.as_ref().and_then(|k| {
                    if !cache.iter().any(|(n, _)| n == k) {
                        cache.push((k.clone(), crate::fonts::face_of(&k.0, k.1, k.2).and_then(|p| load(&p.to_string_lossy()))));
                    }
                    cache.iter().find(|(n, _)| n == k).and_then(|(_, f)| f.as_ref())
                });
                let real = own.is_some();
                let Some(f) = own.or(sans.as_ref()) else { continue };
                let px = *size as f32;
                let m = f.horizontal_line_metrics(px);
                let baseline = *y + m.map(|m| m.ascent as f64).unwrap_or(*size * 0.8) + (CELL_H * (size / 16.0) - size) / 2.0;
                let width: f64 = text.chars().map(|ch| f.metrics(ch, px).advance_width as f64).sum();
                if let Some(b) = band {
                    let (x0, y0) = (*x - 2.0, baseline - size * 0.9);
                    polygon(&mut img, &[(x0, y0), (x0 + width + 4.0, y0), (x0 + width + 4.0, y0 + size * 1.2), (x0, y0 + size * 1.2)], *b);
                }
                // A face the family lacks is faked: bold is the glyph drawn twice a pixel
                // apart, italic each row shifted by its height above the baseline.
                let (fake_bold, fake_italic) = (*bold && !real, *italic && !real);
                let mut cx = *x;
                for ch in text.chars() {
                    let (met, bitmap) = f.rasterize(ch, px);
                    let gy = baseline as i32 - met.height as i32 - met.ymin;
                    for yy in 0..met.height {
                        let shear = if fake_italic { ((gy + yy as i32 - baseline as i32) as f64 * -0.2).round() as i32 } else { 0 };
                        for xx in 0..met.width {
                            let a = bitmap[yy * met.width + xx] as f64 / 255.0;
                            let gx = cx.round() as i32 + met.xmin + xx as i32 + shear;
                            blend(&mut img, gx, gy + yy as i32, *color, a);
                            if fake_bold {
                                blend(&mut img, gx + 1, gy + yy as i32, *color, a);
                            }
                        }
                    }
                    cx += met.advance_width as f64 + if fake_bold { 1.0 } else { 0.0 };
                }
                if *underline {
                    let uy = baseline + size * 0.12;
                    stroke(&mut img, (*x, uy), (*x + width, uy), (size / 14.0).max(1.0), *color);
                }
            }
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::{RelationKind, ShapeKind};

    fn doc() -> Document {
        let mut d = Document::default();
        let a = d.add(ShapeKind::Box, "test", 2.0, 2.0);
        let b = d.add(ShapeKind::Box, "test", 40.0, 2.0);
        d.connect(RelationKind::Link, a, b).unwrap();
        d
    }

    #[test]
    fn the_clean_picture_is_paper_ink_and_a_filled_arrowhead() {
        let o = Options { style: crate::export::Style::Clean, appearance: Appearance::Light, grid: true, ..Options::default() };
        let p = picture(&doc(), &o);
        assert_eq!(p.ground, Some(PAPER));
        assert!(p.items.iter().any(|i| matches!(i, Item::Polygon { fill, stroke: None, .. } if *fill == INK)), "the arrowhead is a filled polygon");
        assert!(p.items.iter().any(|i| matches!(i, Item::Polygon { fill, .. } if *fill == PAPER)), "shapes are filled with the paper");
        assert!(p.items.iter().filter(|i| matches!(i, Item::Polyline { .. })).count() > 20, "the grid is ruled");
        let img = raster(&p);
        assert_eq!((img.width(), img.height()), (p.width, p.height));
        let black = img.pixels().filter(|px| px.0[0] < 128 && px.0[3] == 255).count();
        assert!(black > 300, "ink was laid: {black} dark pixels");
        // Half way across, the link runs through the middle — so look just under the boxes.
        let below = img.get_pixel(p.width / 2, p.height - 6).0;
        assert!(below[0] > 200, "under the boxes is paper or grid, not ink");
    }

    #[test]
    fn the_diagram_can_set_the_ground_the_grid_colour_a_shadow_and_a_sketched_hand() {
        let o = Options { style: crate::export::Style::Clean, appearance: Appearance::Light, grid: true, ..Options::default() };
        let mut d = doc();
        d.metadata.page.background = Some(Colour::Hex([250, 240, 230]));
        d.metadata.page.grid_color = Some(Colour::Hex([200, 220, 240]));
        d.metadata.page.shadow = true;
        let p = picture(&d, &o);
        assert_eq!(p.ground, Some([250, 240, 230]));
        assert!(p.items.iter().any(|i| matches!(i, Item::Polyline { color, .. } if *color == [200, 220, 240])), "the grid in its colour");
        assert!(p.items.iter().any(|i| matches!(i, Item::Polygon { fill, .. } if *fill == [170, 170, 170])), "a shade under each box");
        assert!(p.items.iter().any(|i| matches!(i, Item::Polygon { fill, .. } if *fill == PAPER)), "the box itself still white");
        d.metadata.page.sketch = true;
        let sketched = picture(&d, &o);
        let pieces = |p: &Picture| p.items.iter().filter(|i| matches!(i, Item::Polyline { .. })).count();
        assert!(pieces(&sketched) > pieces(&p), "a sketched edge is many short pieces");
        assert_eq!(picture(&d, &o).items, sketched.items, "and the same ones every time");
    }

    /// `cargo test eyeball_the_three_looks -- --ignored`: the looks together, to look at.
    #[test]
    #[ignore]
    fn eyeball_the_three_looks() {
        use crate::ontology::Colour;
        let mut d = Document::default();
        let a = d.add(ShapeKind::Box, "Order service", 2.0, 2.0);
        let b = d.add(ShapeKind::Ellipse, "Customer", 40.0, 2.0);
        let c = d.add(ShapeKind::Cloud, "Payments", 20.0, 12.0);
        d.connect(RelationKind::Link, a, b).unwrap();
        d.connect(RelationKind::Link, a, c).unwrap();
        d.metadata.page.rounded = true;
        d.metadata.page.sketch = true;
        d.metadata.page.shadow = true;
        d.metadata.page.background = Some(Colour::Hex([250, 246, 236]));
        let o = Options { style: crate::export::Style::Clean, appearance: Appearance::Light, grid: true, zoom: 200, ..Options::default() };
        let out = std::env::var("EYEBALL_DIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
        let path = std::path::PathBuf::from(out).join("three-looks.png");
        crate::export::write(&d, "looks", &o, &path).unwrap();
        eprintln!("wrote {}", path.display());
    }

    #[test]
    fn a_fill_an_outline_off_a_dotted_line_and_an_opacity_all_reach_the_clean_picture() {
        use crate::model::Fill;
        let o = Options { style: crate::export::Style::Clean, appearance: Appearance::Light, grid: true, ..Options::default() };
        let mut d = Document::default();
        let a = d.add(ShapeKind::ApplicationComponent, "auto", 2.0, 2.0);
        let b = d.add(ShapeKind::Box, "none", 30.0, 2.0);
        let c = d.add(ShapeKind::Box, "own", 60.0, 2.0);
        d.element_mut(b).unwrap().fill = Fill::None;
        d.element_mut(b).unwrap().outline = false;
        d.element_mut(c).unwrap().fill = Fill::Colour(Colour::Hex([0, 0, 200]));
        d.element_mut(c).unwrap().line = LineStyle::Dotted;
        d.element_mut(c).unwrap().opacity = 50;
        let p = picture(&d, &o);
        let fills: Vec<[u8; 3]> = p.items.iter().filter_map(|i| match i { Item::Polygon { fill, stroke: None, pts } if pts.len() == 4 => Some(*fill), _ => None }).collect();
        assert!(fills.contains(&[181, 255, 255]), "auto on an application shape is its pastel: {fills:?}");
        assert!(fills.contains(&[128, 128, 228]), "a blue at half opacity is half way to paper: {fills:?}");
        assert_eq!(fills.len(), 2, "the fill-less box has no polygon");
        let dotted = p.items.iter().filter(|i| matches!(i, Item::Polyline { dash: Some((on, _)), .. } if *on < 2.0)).count();
        assert_eq!(dotted, 4, "the dotted box's four edges");
        let _ = a;
        let stroked_at_b = p.items.iter().any(|i| matches!(i, Item::Polyline { pts, .. } if pts.iter().any(|q| (q.0 - 300.0).abs() < 1.0 && q.1 > 30.0 && q.1 < 70.0)));
        assert!(!stroked_at_b, "no outline on the box that turned it off");
        let dark = Options { appearance: Appearance::Dark, ..o };
        let p = picture(&d, &dark);
        assert!(p.items.iter().any(|i| matches!(i, Item::Polygon { fill, .. } if *fill == [29, 32, 33])), "on a dark ground auto is the paper, not a pastel");
    }

    /// `cargo test eyeball_the_style_tab -- --ignored`: the eight looks, a fill of none, a
    /// dotted outline, no outline, and a half-solid shape, to look at.
    #[test]
    #[ignore]
    fn eyeball_the_style_tab() {
        use crate::model::Fill;
        use crate::ontology::{Colour, LOOKS};
        let mut d = Document::default();
        for (i, (name, _, _)) in LOOKS.iter().enumerate() {
            let id = d.add(ShapeKind::Box, *name, 2.0 + (i % 4) as f64 * 22.0, 2.0 + (i / 4) as f64 * 7.0);
            d.element_mut(id).unwrap().set_look(name).unwrap();
        }
        let a = d.add(ShapeKind::ApplicationComponent, "auto", 2.0, 17.0);
        let b = d.add(ShapeKind::Box, "none", 24.0, 17.0);
        let c = d.add(ShapeKind::Ellipse, "dotted", 46.0, 17.0);
        let e = d.add(ShapeKind::Box, "no outline", 68.0, 17.0);
        let f = d.add(ShapeKind::Box, "40 %", 2.0, 25.0);
        d.element_mut(b).unwrap().fill = Fill::None;
        d.element_mut(c).unwrap().line = LineStyle::Dotted;
        d.element_mut(e).unwrap().outline = false;
        d.element_mut(e).unwrap().fill = Fill::Colour(Colour::Hex([200, 220, 240]));
        d.element_mut(f).unwrap().opacity = 40;
        d.element_mut(f).unwrap().set_look("red").unwrap();
        d.connect(RelationKind::Link, a, f).unwrap();
        let o = Options { style: crate::export::Style::Clean, appearance: Appearance::Light, grid: true, zoom: 200, ..Options::default() };
        let out = std::env::var("EYEBALL_DIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
        let path = std::path::PathBuf::from(out).join("style-tab.png");
        crate::export::write(&d, "style", &o, &path).unwrap();
        eprintln!("wrote {}", path.display());
    }

    /// `cargo test eyeball_the_text_tab -- --ignored`: bold, italic, underline, a colour, a
    /// band, no wrap, a width, padding — and a link's labels — to look at.
    #[test]
    #[ignore]
    fn eyeball_the_text_tab() {
        use crate::model::TextStyle;
        use crate::ontology::Colour;
        let mut d = Document::default();
        let looks: [(&str, TextStyle); 8] = [
            ("bold", TextStyle { bold: true, ..Default::default() }),
            ("italic", TextStyle { italic: true, ..Default::default() }),
            ("underline", TextStyle { underline: true, ..Default::default() }),
            ("all three", TextStyle { bold: true, italic: true, underline: true, ..Default::default() }),
            ("coloured", TextStyle { color: Some(Colour::Hex([180, 40, 40])), bold: true, ..Default::default() }),
            ("a band on a fill", TextStyle { band: true, ..Default::default() }),
            ("no wrap: a label that runs on and on", TextStyle { wrap: false, ..Default::default() }),
            ("padding 2, top left", TextStyle { padding: 2, align: crate::model::Align::Left, valign: crate::model::VAlign::Top, ..Default::default() }),
        ];
        for (i, (label, t)) in looks.into_iter().enumerate() {
            let id = d.add(ShapeKind::Box, label, 2.0 + (i % 4) as f64 * 24.0, 2.0 + (i / 4) as f64 * 8.0);
            let e = d.element_mut(id).unwrap();
            e.text = t;
            if i == 5 {
                e.fill = crate::model::Fill::Colour(Colour::Hex([218, 232, 252]));
            }
        }
        let a = d.add(ShapeKind::Box, "from", 2.0, 20.0);
        let b = d.add(ShapeKind::Box, "to", 74.0, 20.0);
        let r = d.connect(RelationKind::Link, a, b).unwrap();
        let rel = d.relation_mut(r).unwrap();
        rel.label = Some("a banded, bold label at 30 %".into());
        rel.text = TextStyle { bold: true, band: true, color: Some(Colour::Hex([30, 90, 160])), ..Default::default() };
        rel.label_at = Some(30);
        let o = Options { style: crate::export::Style::Clean, appearance: Appearance::Light, grid: true, zoom: 200, ..Options::default() };
        let out = std::env::var("EYEBALL_DIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
        let path = std::path::PathBuf::from(out).join("text-tab.png");
        crate::export::write(&d, "text", &o, &path).unwrap();
        eprintln!("wrote {}", path.display());
    }

    #[test]
    fn an_object_type_s_rows_are_set_under_a_rule_with_the_type_against_the_right_edge() {
        let o = Options { style: crate::export::Style::Clean, appearance: Appearance::Light, grid: false, ..Options::default() };
        let mut d = Document::default();
        d.metadata.view = crate::ontology::View::Ontology;
        let id = d.add(ShapeKind::ObjectType, "Airport", 2.0, 2.0);
        let mut code = crate::model::Property::new("code");
        code.primary_key = true;
        let mut loc = crate::model::Property::new("location");
        loc.base_type = crate::model::BaseType::Geopoint;
        d.element_mut(id).unwrap().properties = vec![code, loc];
        d.element_mut(id).unwrap().fit_rows();
        let p = picture(&d, &o);
        let texts: Vec<&str> = p.items.iter().filter_map(|i| match i { Item::Text { text, .. } => Some(text.as_str()), _ => None }).collect();
        assert!(texts.contains(&"PK code") && texts.contains(&"string") && texts.contains(&"location") && texts.contains(&"geopoint"), "{texts:?}");
        assert!(texts.contains(&"Airport"));
        let rules = p.items.iter().filter(|i| matches!(i, Item::Polyline { pts, .. } if pts.len() == 2 && (pts[0].1 - pts[1].1).abs() < 1e-9)).count();
        assert!(rules >= 1, "a horizontal rule under the header");
    }

    #[test]
    fn the_clean_grid_follows_the_diagram_s_style_and_density() {
        let o = Options { style: crate::export::Style::Clean, appearance: Appearance::Light, grid: true, ..Options::default() };
        let mut d = doc();
        let rules = |p: &Picture| p.items.iter().filter(|i| matches!(i, Item::Polyline { width, .. } if *width == 1.0)).count();
        let dots = |p: &Picture| p.items.iter().filter(|i| matches!(i, Item::Dot { .. })).count();
        let four = picture(&d, &o);
        assert!(rules(&four) > 20 && dots(&four) == 0, "ruled by default");
        d.metadata.page.grid_size = 8;
        let eight = picture(&d, &o);
        assert!(rules(&eight) < rules(&four), "a wider grid is fewer rules");
        d.metadata.page.grid_style = crate::model::GridStyle::Dots;
        let dotted = picture(&d, &o);
        assert!(dots(&dotted) > 10 && rules(&dotted) == 0, "dots: {} / {}", dots(&dotted), rules(&dotted));
    }

    #[test]
    fn a_routed_link_is_one_polyline_through_its_turns_with_its_ends_on_the_last_legs() {
        let o = Options { style: crate::export::Style::Clean, appearance: Appearance::Light, grid: false, ..Options::default() };
        let mut d = Document::default();
        let a = d.add(ShapeKind::Box, "a", 0.0, 0.0);
        let b = d.add(ShapeKind::Box, "b", 40.0, 20.0);
        let r = d.connect(RelationKind::Link, a, b).unwrap();
        // Orthogonal is the connect default now, so the straight case has to ask for it.
        let mut look = d.relation(r).unwrap().notation();
        look.route = crate::ontology::Route::Straight;
        d.relation_mut(r).unwrap().style = Some(look);
        let straight = picture(&d, &o);
        let mut look = d.relation(r).unwrap().notation();
        look.route = crate::ontology::Route::Orthogonal;
        d.relation_mut(r).unwrap().style = Some(look);
        let p = picture(&d, &o);
        let legs = |p: &Picture| p.items.iter().filter_map(|i| match i { Item::Polyline { pts, .. } if pts.len() >= 2 => Some(pts.len()), _ => None }).max().unwrap();
        assert_eq!((legs(&straight), legs(&p)), (2, 4), "two points straight, four with two turns");
        let head = |p: &Picture| p.items.iter().find_map(|i| match i { Item::Polygon { pts, stroke: None, .. } if pts.len() == 3 => Some(pts.clone()), _ => None }).unwrap();
        let (hs, ho) = (head(&straight), head(&p));
        assert_ne!(hs, ho, "the arrowhead points along the last leg, not the chord");
    }

    /// `cargo test eyeball_the_routes -- --ignored`: straight, orthogonal with and without
    /// an elbow, curved, and the three end sizes, to look at.
    #[test]
    #[ignore]
    fn eyeball_the_routes() {
        use crate::ontology::{EndSize, Route};
        let mut d = Document::default();
        let mut pair = |label: &str, y: f64, route: Route, elbow: Option<i64>, size: EndSize| {
            let a = d.add(ShapeKind::Box, label, 2.0, y);
            let b = d.add(ShapeKind::Ellipse, "", 60.0, y + 8.0);
            let r = d.connect(RelationKind::Link, a, b).unwrap();
            let rel = d.relation_mut(r).unwrap();
            let mut n = rel.notation();
            n.route = route;
            n.end_size = size;
            n.tail = crate::ontology::End::Circle;
            rel.style = Some(n);
            rel.elbow = elbow;
            rel.label = Some(label.to_string());
        };
        pair("straight", 2.0, Route::Straight, None, EndSize::Normal);
        pair("orthogonal", 20.0, Route::Orthogonal, None, EndSize::Small);
        pair("elbow at 8", 38.0, Route::Orthogonal, Some(8), EndSize::Large);
        pair("curved", 56.0, Route::Curved, None, EndSize::Normal);
        let o = Options { style: crate::export::Style::Clean, appearance: Appearance::Light, grid: true, zoom: 150, ..Options::default() };
        let out = std::env::var("EYEBALL_DIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
        let path = std::path::PathBuf::from(out).join("routes.png");
        crate::export::write(&d, "routes", &o, &path).unwrap();
        eprintln!("wrote {}", path.display());
    }

    /// `cargo test eyeball_the_ontology -- --ignored`: an ontology diagram on paper.
    #[test]
    #[ignore]
    fn eyeball_the_ontology() {
        use crate::model::{BaseType, Property, Status};
        use crate::ontology::View;
        let mut d = Document::default();
        d.metadata.view = View::Ontology;
        let airport = d.add(ShapeKind::ObjectType, "Airport", 2.0, 2.0);
        let flight = d.add(ShapeKind::ObjectType, "Flight", 40.0, 2.0);
        let place = d.add(ShapeKind::Interface, "Place", 2.0, 16.0);
        let delay = d.add(ShapeKind::ActionType, "Delay flight", 40.0, 16.0);
        let ds = d.add(ShapeKind::Datasource, "flights.parquet", 78.0, 4.0);
        let row = |name: &str, ty: BaseType| { let mut p = Property::new(name); p.base_type = ty; p };
        let e = d.element_mut(airport).unwrap();
        let mut code = row("code", BaseType::String);
        code.primary_key = true;
        let mut name = row("name", BaseType::String);
        name.title = true;
        name.value_type = Some("Name".into());
        let mut email = row("email", BaseType::String);
        email.shared = true;
        e.properties = vec![code, name, row("location", BaseType::Geopoint), email];
        e.fit_rows();
        let e = d.element_mut(flight).unwrap();
        let mut id = row("id", BaseType::String);
        id.primary_key = true;
        let mut dep = row("departs", BaseType::Timestamp);
        dep.title = true;
        e.properties = vec![id, dep, row("delayed", BaseType::Boolean)];
        e.status = Status::Experimental;
        e.fit_rows();
        let e = d.element_mut(place).unwrap();
        e.properties = vec![row("location", BaseType::Geopoint)];
        e.fit_rows();
        let e = d.element_mut(delay).unwrap();
        let mut m = row("minutes", BaseType::Integer);
        m.required = true;
        e.properties = vec![row("flight", BaseType::ObjectReference), m];
        e.fit_rows();
        let l = d.connect(RelationKind::LinkType, airport, flight).unwrap();
        let r = d.relation_mut(l).unwrap();
        r.label = Some("departures".into());
        r.tail_label = Some("origin".into());
        r.head_label = Some("departures".into());
        d.connect(RelationKind::Implements, airport, place).unwrap();
        d.connect(RelationKind::Modifies, delay, flight).unwrap();
        d.connect(RelationKind::BackedBy, flight, ds).unwrap();
        let o = Options { style: crate::export::Style::Clean, appearance: Appearance::Light, grid: true, zoom: 150, ..Options::default() };
        let out = std::env::var("EYEBALL_DIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
        let path = std::path::PathBuf::from(out).join("ontology.png");
        crate::export::write(&d, "ontology", &o, &path).unwrap();
        std::fs::write(std::path::PathBuf::from(std::env::var("EYEBALL_DIR").unwrap_or_default()).join("ontology.md"), crate::ontology::doc::markdown(&d)).ok();
        eprintln!("wrote {}", path.display());
    }

    #[test]
    fn the_hull_of_a_square_is_its_four_corners_and_a_cloud_shows_only_its_outline() {
        let hull = convex_hull(vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0), (5.0, 5.0), (2.0, 3.0)]);
        assert_eq!(hull.len(), 4);
        assert!(inside_polygon((5.0, 5.0), &hull) && !inside_polygon((20.0, 5.0), &hull));
        let mut d = Document::default();
        d.add(ShapeKind::Cloud, "net", 2.0, 2.0);
        let o = Options::preview();
        let p = picture(&d, &o);
        let strokes = p.items.iter().filter(|i| matches!(i, Item::Polyline { pts, .. } if pts.len() > 2)).count();
        assert!(strokes >= 5, "the outline is several arcs: {strokes}");
        let full: Vec<usize> = p.items.iter().filter_map(|i| match i { Item::Polyline { pts, .. } => Some(pts.len()), _ => None }).collect();
        assert!(full.iter().all(|n| *n < 33), "no bump is stroked whole: {full:?}");
    }

    #[test]
    fn a_polygon_fill_covers_its_inside_and_not_its_outside() {
        let mut img = RgbaImage::from_pixel(20, 20, Rgba([255, 255, 255, 255]));
        polygon(&mut img, &[(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)], [0, 0, 0]);
        assert_eq!(img.get_pixel(10, 10).0, [0, 0, 0, 255]);
        assert_eq!(img.get_pixel(2, 2).0, [255, 255, 255, 255]);
    }
}
