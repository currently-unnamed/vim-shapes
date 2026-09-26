//! The diagram, drawn: outlines and relations in braille, labels in text over the top.
//!
//! Everything is in world cells, and the camera decides which window of the world the area
//! shows. Shapes are painted through one braille canvas into a scratch buffer and only the
//! cells that actually got a dot are composited back — so the labels, the letters and the
//! panel behind the canvas survive untouched.

use super::theme;
use crate::model::{Align, Document, Element, ElementId, Node, RelationId, VAlign};
use crate::ontology::{End, Layer, LineStyle};
use crate::shapes::{self, CurvePrimitive, Point};
use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Context, Line as CLine, Points};

/// What the label under the cursor is being typed into.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Element(ElementId),
    /// One of a relation's three nodes.
    Relation(RelationId, Node),
    /// The diagram itself: its page, grid, ground and the looks that apply to everything.
    Diagram,
    /// The picked set, in visual mode: what the shapes share, set on all of them at once.
    Picked,
}

pub struct Scene<'a> {
    pub doc: &'a Document,
    pub cursor: Option<ElementId>,
    pub focus_rel: Option<RelationId>,
    /// Which of the focused relation's nodes the cursor is on.
    pub focus_node: Node,
    /// The element a relation is being drawn from.
    pub holding: Option<ElementId>,
    pub picked: &'a [ElementId],
    pub camera: (f64, f64),
    /// `f` is pending: every element wears its letter.
    pub letters: Option<&'a [(ElementId, char)]>,
    /// A label being typed, and what it reads so far.
    pub insert: Option<(Target, &'a str)>,
    /// Relations the rules refuse.
    pub refused: &'a [RelationId],
    /// An element being reshaped: which of its eight handles the cursor is on, and whether
    /// that handle is in hand.
    pub reshape: Option<(ElementId, usize, bool)>,
    /// Whether shape labels are written. The PNG renderer leaves them out and sets them in
    /// each shape's own font instead; the screen always writes them.
    pub labels: bool,
    /// Faint dots marking the canvas, so an empty diagram still reads as a surface with a
    /// position on it. Aligned to the world, so they scroll with everything else.
    pub grid: bool,
    /// How the diagram is drawn: line art, or braille.
    pub ink: super::wire::Ink,
}

/// How far a sketched outline wanders on screen, in cells: under half a cell, so a shape
/// still reads as the box it is, and the wobble shows in the braille.
const SKETCH: f64 = 0.35;

impl Widget for Scene<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let (cx, cy) = self.camera;
        let (w, h) = (area.width as f64, area.height as f64);
        let flip = move |y: f64| 2.0 * cy + h - y;

        let page = &self.doc.metadata.page;
        // The ground: the diagram's own colour under everything, when it has one.
        if let Some(bg) = page.background {
            let ground = Style::new().bg(theme::colour(bg));
            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    buf[(x, y)].set_style(ground);
                }
            }
        }
        if self.grid {
            let dot = Style::new().fg(page.grid_color.map(theme::colour).unwrap_or(theme::t().grid));
            let (grid_x, grid_y) = page.grid_step();
            // A shape is opaque: the dots stop at its edge, the way a filled shape on a
            // whiteboard hides the grid behind it. A grouping is open, and shows them.
            let solid: Vec<&crate::model::Element> = self
                .doc
                .elements
                .iter()
                .filter(|e| self.doc.element_visible(e.id) && !e.kind.is_composite() && e.kind.shape() != crate::ontology::Shape::Text)
                .collect();
            let lines = page.grid_style.ruled_on_screen();
            for y in area.top()..area.bottom() {
                let wy = (y - area.y) as i64 + cy as i64;
                let on_row = wy.rem_euclid(grid_y) == 0;
                if !on_row && !lines {
                    continue;
                }
                for x in area.left()..area.right() {
                    let wx = (x - area.x) as i64 + cx as i64;
                    let on_col = wx.rem_euclid(grid_x) == 0;
                    if !on_row && !on_col {
                        continue;
                    }
                    let inside = solid.iter().any(|e| e.contains((wx as f64, wy as f64)));
                    if inside {
                        continue;
                    }
                    // Ruled: a rule along the row, one down the column, a cross where they
                    // meet — box drawing, in the grid's colour, faint by being thin.
                    let mark = match (lines, on_row, on_col) {
                        (false, true, true) => "·",
                        (false, ..) => continue,
                        (true, true, true) => "┼",
                        (true, true, false) => "─",
                        (true, false, true) => "│",
                        (true, false, false) => continue,
                    };
                    buf[(x, y)].set_symbol(mark);
                    buf[(x, y)].set_style(dot);
                }
            }
        }

        if self.ink == super::wire::Ink::Lines {
            super::wire::paint(&self, area, buf);
        } else {
            let mut scratch = Buffer::empty(area);
            let canvas = Canvas::default()
                .marker(Marker::Braille)
                .x_bounds([cx, cx + w])
                .y_bounds([cy, cy + h])
                .paint(|ctx| self.paint(ctx, flip));
            canvas.render(area, &mut scratch);
            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    let cell = &scratch[(x, y)];
                    if cell.symbol() != " " {
                        buf[(x, y)].set_symbol(cell.symbol());
                        buf[(x, y)].set_fg(cell.fg);
                    }
                }
            }
        }
        self.text(area, buf);
    }
}

impl Scene<'_> {
    /// The terminal's ground under this diagram: its own colour, or the theme's.
    fn ground(&self) -> Color {
        self.doc.metadata.page.background.map(theme::colour).unwrap_or(theme::t().ground)
    }

    /// What a relation is drawn in: bright when focused, red when refused, else its own.
    pub(crate) fn relation_colour(&self, r: &crate::model::Relation) -> Color {
        let n = r.notation();
        if self.focus_rel == Some(r.id) {
            theme::t().bright
        } else if self.refused.contains(&r.id) {
            theme::t().red
        } else {
            theme::fade(n.color.map(theme::colour).unwrap_or(theme::t().structure), n.opacity, self.ground())
        }
    }

    pub(crate) fn colour_of(&self, id: ElementId, layer: Layer) -> Color {
        if self.cursor == Some(id) && self.focus_rel.is_none() {
            theme::t().yellow
        } else if self.holding == Some(id) || self.picked.contains(&id) {
            theme::t().green
        } else if self.cursor == Some(id) {
            theme::t().bright
        } else {
            let e = self.doc.element(id);
            let own = e.and_then(|e| e.color).map(theme::colour).unwrap_or_else(|| theme::layer_color(layer));
            theme::fade(own, e.map_or(100, |e| e.drawn_opacity()), self.ground())
        }
    }

    fn paint(&self, ctx: &mut Context, flip: impl Fn(f64) -> f64 + Copy) {
        let page = &self.doc.metadata.page;
        // Relations first, so an outline drawn over a line wins the cell; in layer order,
        // and never one on a hidden layer or to a hidden shape.
        for r in self.doc.relations_in_order() {
            if !self.doc.relation_visible(r.id) {
                continue;
            }
            let (Some(a), Some(b)) = (self.doc.element(r.from), self.doc.element(r.to)) else { continue };
            let _ = (a, b);
            let n = r.notation();
            let colour = self.relation_colour(r);
            let Some(pts) = self.doc.route(r) else { continue };
            paint_relation(ctx, &pts, &n, colour, flip);
        }
        // The relation in hand: from the held element to wherever the cursor is.
        if let (Some(from), Some(to)) = (self.holding, self.cursor)
            && from != to
            && let (Some(a), Some(b)) = (self.doc.element(from), self.doc.element(to))
        {
            let p1 = shapes::edge_point(a.x, a.y, a.w, a.h, b.center());
            let p2 = shapes::edge_point(b.x, b.y, b.w, b.h, a.center());
            let hand = crate::ontology::Notation { line: LineStyle::Dashed, tail: End::None, head: End::Arrow, width: 1, color: None, route: crate::ontology::Route::Straight, end_size: crate::ontology::EndSize::Normal, opacity: 100 };
            paint_relation(ctx, &[p1, p2], &hand, theme::t().yellow, flip);
        }
        // The focused relation's three nodes, in braille: a small hollow diamond at each, and
        // at the one the cursor is on a filled diamond inside a ring — a mark you can stand on
        // and see, not a character in a block.
        if let Some(rid) = self.focus_rel
            && let Some(r) = self.doc.relation(rid)
            && let Some((p1, p2)) = self.doc.end_points(r)
        {
            let route = self.doc.route(r).unwrap_or_else(|| vec![p1, p2]);
            let at = |t: f64| shapes::along(&route, t);
            for (node, t) in [(Node::Tail, 0.18), (Node::Centre, 0.5), (Node::Head, 0.82)] {
                let on = node == self.focus_node;
                paint_mark(ctx, at(t), if on { Mark::Focused(theme::t().yellow) } else { Mark::Plain(theme::t().bright) }, flip);
            }
        }
        // Inside a shape, its eight handles the same way: a diamond at each, a ring where a
        // relation is attached, and the one under the cursor filled — green once in hand.
        if let Some((id, on, held)) = self.reshape
            && let Some(e) = self.doc.element(id)
        {
            for (i, h) in e.handles().into_iter().enumerate() {
                let patched = !self.doc.at_port(id, i).is_empty();
                let mark = match (i == on, held, patched) {
                    (true, true, _) => Mark::Focused(theme::t().green),
                    (true, false, _) => Mark::Focused(theme::t().yellow),
                    (false, _, true) => Mark::Patched(theme::t().aqua),
                    (false, _, false) => Mark::Plain(theme::t().sand),
                };
                paint_mark(ctx, h, mark, flip);
            }
        }
        for e in self.doc.elements_in_order() {
            if !self.doc.element_visible(e.id) {
                continue;
            }
            // No outline: the fill's tint and the label are the shape.
            if !e.outline {
                continue;
            }
            let colour = self.colour_of(e.id, e.kind.layer());
            for prim in shapes::patterned(shapes::drawn(page, e, SKETCH), e.drawn_line()) {
                paint_primitive(ctx, prim, colour, flip);
            }
            // A thicker outline is the same outline drawn again a braille dot out — and, for
            // the thickest, a dot in as well — so it stays the shape it was, only heavier.
            for d in [0.5, -0.5].iter().take(e.stroke.saturating_sub(1) as usize) {
                for prim in shapes::patterned(shapes::drawn_at(page, e, e.x - d, e.y - d / 2.0, e.w + 2.0 * d, e.h + d, SKETCH), e.drawn_line()) {
                    paint_primitive(ctx, prim, colour, flip);
                }
            }
        }
        // The page's edge, dashed and dim, when the diagram is laid out on paper.
        if page.page_view {
            let (w, h) = page.size();
            for (a, b) in [((0.0, 0.0), (w, 0.0)), ((w, 0.0), (w, h)), ((w, h), (0.0, h)), ((0.0, h), (0.0, 0.0))] {
                paint_primitive(ctx, CurvePrimitive::Points(shapes::dashed_line(a, b, 1.5, 1.0)), theme::t().structure, flip);
            }
        }
    }

    fn text(&self, area: Rect, buf: &mut Buffer) {
        let (cx, cy) = self.camera;
        let col = |wx: f64| -> Option<u16> {
            let x = (wx - cx).round();
            (x >= 0.0 && x < area.width as f64).then(|| area.x + x as u16)
        };
        let row = |wy: f64| -> Option<u16> {
            let y = (wy - cy).round();
            (y >= 0.0 && y < area.height as f64).then(|| area.y + y as u16)
        };
        // Write `s` at world (wx, wy), clipped to the area.
        let put = |buf: &mut Buffer, wx: f64, wy: f64, s: &str, style: Style| {
            let Some(y) = row(wy) else { return };
            let x = (wx - cx).round() as i64;
            let start = x.max(0) as usize;
            let skip = (start as i64 - x) as usize;
            let s: String = s.chars().skip(skip).collect();
            if s.is_empty() || start as u16 >= area.width {
                return;
            }
            buf.set_stringn(area.x + start as u16, y, &s, (area.width as usize).saturating_sub(start), style);
        };

        for e in self.doc.elements_in_order() {
            if !self.doc.element_visible(e.id) {
                continue;
            }
            let on = self.cursor == Some(e.id) && self.labels;
            let composite = e.kind.is_composite();
            let typing = matches!(self.insert, Some((Target::Element(id), _)) if id == e.id);
            let label = match self.insert {
                Some((Target::Element(id), s)) if id == e.id => format!("{s}█"),
                _ => e.label.clone(),
            };
            // A fill of its own tints the inside: the colour laid over the ground at a third
            // of its opacity, so the label and the grid's absence both read.
            if let (Some(c), false, false) = (e.fill.colour(), composite, matches!(e.kind.shape(), crate::ontology::Shape::Text | crate::ontology::Shape::Dashed)) {
                let Color::Rgb(gr, gg, gb) = self.ground() else { unreachable!("the ground is always rgb") };
                let t = crate::ontology::mix(c.on(theme::mode() == theme::Mode::Light), [gr, gg, gb], 1.0 - 0.35 * e.drawn_opacity() as f64 / 100.0);
                let tint = Color::Rgb(t[0], t[1], t[2]);
                for wy in (e.y as i64 + 1)..(e.bottom() as i64) {
                    for wx in (e.x as i64 + 1)..(e.right() as i64) {
                        if let (Some(x), Some(y)) = (col(wx as f64), row(wy as f64)) {
                            buf[(x, y)].set_bg(tint);
                        }
                    }
                }
            }
            // The cursor's shape is INVERSE: its inside tinted, its label black on yellow. An
            // outline in a different colour was too subtle to find on a busy diagram.
            if on && !composite && self.focus_rel.is_none() {
                for wy in (e.y as i64 + 1)..(e.bottom() as i64) {
                    for wx in (e.x as i64 + 1)..(e.right() as i64) {
                        if let (Some(x), Some(y)) = (col(wx as f64), row(wy as f64)) {
                            buf[(x, y)].set_bg(theme::t().hilite);
                        }
                    }
                }
            }
            let plain = !typing && !on;
            let label_style = if typing {
                Style::new().fg(theme::t().inverse).bg(theme::t().green).bold()
            } else if on && self.focus_rel.is_none() {
                Style::new().fg(theme::t().inverse).bg(theme::t().yellow).bold()
            } else if on {
                Style::new().fg(theme::t().bright).bold()
            } else {
                Style::new().fg(theme::t().ink)
            };
            let label_style = dress(label_style, &e.text, plain, self.ground());
            let tag_style = if on && self.focus_rel.is_none() {
                Style::new().fg(theme::t().ink).bg(theme::t().hilite).bold()
            } else {
                Style::new().fg(theme::t().dim)
            };
            if composite {
                put(buf, e.x + 2.0, e.y, &format!(" {} ", e.kind.short()), tag_style);
                if !label.is_empty() {
                    put(buf, e.x + 2.0, e.y + 1.0, &format!(" {label} "), label_style);
                }
            } else {
                let inner_w = (e.w as usize).saturating_sub(2).max(1);
                // The kind, small, top centre of the first row inside — where every outline,
                // an ellipse's included, has room for it — and never on a plain shape, whose
                // kind is its outline.
                let tag = e.tag();
                if !e.kind.is_sketch() && e.h >= 4.0 && inner_w > tag.chars().count() + 1 {
                    let tw = tag.chars().count() as f64;
                    put(buf, e.x + (e.w - tw) / 2.0, e.y + 1.0, &tag, tag_style);
                }
                if self.labels || typing {
                    for (x, y, l) in label_lines(e, &label) {
                        put(buf, x, y, &l, label_style);
                    }
                }
                // The compartment: a rule under the header, then a row per property, in
                // the kind's colour dimmed — the shape's own text, not its label.
                if let Some(ry) = e.header_rule() {
                    if self.ink == super::wire::Ink::Braille {
                        let rule: String = "─".repeat((e.w as usize).saturating_sub(2));
                        put(buf, e.x + 1.0, ry, &rule, Style::new().fg(self.colour_of(e.id, e.kind.layer())));
                    }
                    // On the cursor's tint the rows are the ink: the inverse is the label's
                    // colour on the saturated yellow, and unreadable on the pale tint.
                    let row_style = if on && self.focus_rel.is_none() { Style::new().fg(theme::t().ink).bg(theme::t().hilite) } else { Style::new().fg(theme::t().ink) };
                    for (x, y, l) in e.row_lines() {
                        put(buf, x, y, &l, row_style);
                    }
                }
            }
            if let Some(letters) = self.letters
                && let Some((_, c)) = letters.iter().find(|(id, _)| *id == e.id)
            {
                put(buf, e.x, e.y, &format!(" {c} "), Style::new().fg(theme::t().inverse).bg(theme::t().yellow).bold());
            }
        }

        // A relation's three nodes: a label may sit at either end and at the centre, the
        // focused node wears a ◆, and a refusal wears a ⚠ at the centre.
        for r in self.doc.relations_in_order() {
            if !self.doc.relation_visible(r.id) {
                continue;
            }
            let focused = self.focus_rel == Some(r.id);
            let refused = self.refused.contains(&r.id);
            for node in Node::ALL {
                let Some((px, py)) = self.doc.label_point(r, node) else { continue };
                let text = match self.insert {
                    Some((Target::Relation(id, n), s)) if id == r.id && n == node => Some(format!("{s}█")),
                    _ => r.label_at(node).map(str::to_string),
                };
                let mut badge = String::new();
                if refused && node == Node::Centre {
                    badge.push('⚠');
                }
                if let Some(t) = text.filter(|t| !t.is_empty()) {
                    if !badge.is_empty() {
                        badge.push(' ');
                    }
                    badge.push_str(&t);
                }
                if badge.is_empty() {
                    continue;
                }
                // The focused node's label is inverse, the same yellow its braille mark wears;
                // labels sit a row above the line so the marks underneath stay whole.
                let plain = !focused && !(refused && node == Node::Centre);
                let style = if focused && node == self.focus_node {
                    Style::new().fg(theme::t().inverse).bg(theme::t().yellow).bold()
                } else if refused && node == Node::Centre {
                    Style::new().fg(theme::t().red).bold()
                } else if focused {
                    Style::new().fg(theme::t().bright).bold()
                } else {
                    Style::new().fg(theme::t().structure)
                };
                let style = dress(style, &r.text, plain, self.ground());
                let bw = badge.chars().count() as f64;
                put(buf, px - bw / 2.0, py - 1.5, &badge, style);
            }
        }
    }
}

/// Where a label's lines sit inside a shape: wrapped to the box, placed by its alignment.
/// World coordinates, one entry per line — shared with the renderer so a label lands in the
/// same place on screen and in the file.
pub fn label_lines(e: &Element, label: &str) -> Vec<(f64, f64, String)> {
    let pad = e.text.padding as f64;
    let inner_w = (e.w as usize).saturating_sub(2 + 2 * e.text.padding as usize).max(1);
    // The width the label wraps to: the shape's inside, or its own, which may be wider.
    let wrap_w = e.text.width.map_or(inner_w, |w| w.max(1) as usize);
    // The kind's tag takes the first row inside a typed shape, so its label starts below it.
    let tag_rows = if e.kind.is_sketch() { 0.0 } else { 1.0 };
    let max_lines = (e.h as usize).saturating_sub(2 + tag_rows as usize + 2 * e.text.padding as usize).max(1);
    let mut lines = if e.text.wrap {
        super::chrome::wrap(label, wrap_w)
    } else {
        // One line, cut short: what a label that must not wrap looks like in a small box.
        let one: String = label.split_whitespace().collect::<Vec<_>>().join(" ");
        vec![if one.chars().count() > wrap_w { one.chars().take(wrap_w.saturating_sub(1)).chain(['…']).collect() } else { one }]
    };
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            last.push('…');
        }
    }
    // A box with rows keeps its name in the header, on one line, whatever the alignment.
    if e.has_rows() {
        lines.truncate(1);
    }
    let n = lines.len() as f64;
    let top = match e.text.valign {
        _ if e.has_rows() => e.y + 1.0 + tag_rows,
        VAlign::Top => e.y + 1.0 + tag_rows + pad,
        VAlign::Middle => (e.y + e.h / 2.0 - n / 2.0 + 0.5).max(e.y + 1.0),
        VAlign::Bottom => (e.bottom() - 1.0 - n - pad).max(e.y + 1.0),
    };
    lines
        .into_iter()
        .enumerate()
        .map(|(i, l)| {
            let lw = l.chars().count() as f64;
            let x = match e.text.align {
                Align::Left => e.x + 1.0 + pad,
                Align::Centre => e.x + (e.w - lw) / 2.0,
                Align::Right => (e.right() - 1.0 - lw - pad).max(e.x + 1.0),
            };
            (x + e.text.offset.0, top + i as f64 + e.text.offset.1, l)
        })
        .collect()
}

/// The terminal's own bold, italic and underline, and the label's colour and band, laid over
/// a base style — the cursor's inverse and the typing green keep their colours, so a label
/// stays findable whatever it wears.
pub fn dress(base: Style, t: &crate::model::TextStyle, plain: bool, ground: Color) -> Style {
    let mut s = base;
    if plain {
        if let Some(c) = t.color {
            s = s.fg(theme::colour(c));
        }
        if t.band {
            s = s.bg(theme::band(ground));
        }
    }
    if t.bold {
        s = s.bold();
    }
    if t.italic {
        s = s.italic();
    }
    if t.underline {
        s = s.underlined();
    }
    s
}

fn paint_primitive(ctx: &mut Context, prim: CurvePrimitive, color: Color, flip: impl Fn(f64) -> f64) {
    match prim {
        CurvePrimitive::Points(points) => {
            let flipped: Vec<Point> = points.iter().map(|&(x, y)| (x, flip(y))).collect();
            ctx.draw(&Points { coords: &flipped, color });
        }
        CurvePrimitive::Lines(lines) => {
            for (a, b) in lines {
                ctx.draw(&CLine { x1: a.0, y1: flip(a.1), x2: b.0, y2: flip(b.1), color });
            }
        }
    }
}

/// A node mark: what a handle or a link's node looks like.
#[derive(Clone, Copy)]
enum Mark {
    /// A small hollow diamond.
    Plain(Color),
    /// A ring — something is attached here.
    Patched(Color),
    /// A filled diamond in a ring — the one the cursor is on.
    Focused(Color),
}

/// Paint a mark at `(cx, cy)`, in braille. Sizes are in cells, the aspect corrected so a
/// diamond is a diamond and a ring is round.
fn paint_mark(ctx: &mut Context, (cx, cy): Point, mark: Mark, flip: impl Fn(f64) -> f64 + Copy) {
    let diamond = |rx: f64, ry: f64| vec![((cx, cy - ry), (cx + rx, cy)), ((cx + rx, cy), (cx, cy + ry)), ((cx, cy + ry), (cx - rx, cy)), ((cx - rx, cy), (cx, cy - ry))];
    let ring = |rx: f64, ry: f64| CurvePrimitive::Points(shapes::ellipse_points(cx, cy, rx, ry, 0.0, std::f64::consts::TAU, 48));
    match mark {
        Mark::Plain(c) => paint_primitive(ctx, CurvePrimitive::Lines(diamond(1.0, 0.5)), c, flip),
        Mark::Patched(c) => {
            paint_primitive(ctx, ring(1.4, 0.7), c, flip);
            paint_primitive(ctx, CurvePrimitive::Lines(diamond(0.5, 0.25)), c, flip);
        }
        Mark::Focused(c) => {
            let (rx, ry) = (1.6, 0.8);
            // Filled: nested diamonds down to the centre, and a ring round the lot.
            for k in [1.0, 0.75, 0.5, 0.25] {
                paint_primitive(ctx, CurvePrimitive::Lines(diamond(rx * k, ry * k)), c, flip);
            }
            paint_primitive(ctx, ring(rx + 0.9, ry + 0.45), c, flip);
        }
    }
}

/// Paint a relation from its geometry.
fn paint_relation(ctx: &mut Context, pts: &[Point], n: &crate::ontology::Notation, color: Color, flip: impl Fn(f64) -> f64 + Copy) {
    let (lines, points) = shapes::relation_along(pts, n);
    for (a, b) in lines {
        ctx.draw(&CLine { x1: a.0, y1: flip(a.1), x2: b.0, y2: flip(b.1), color });
    }
    if !points.is_empty() {
        let pts: Vec<Point> = points.into_iter().map(|(x, y)| (x, flip(y))).collect();
        ctx.draw(&Points { coords: &pts, color });
    }
}

#[cfg(test)]
mod page_tests {
    use super::*;
    use crate::ontology::ShapeKind;
    use ratatui::{backend::TestBackend, Terminal};

    fn cells(doc: &Document) -> (usize, usize) {
        let mut term = Terminal::new(TestBackend::new(60, 16)).unwrap();
        term.draw(|f| {
            f.render_widget(
                Scene { doc, cursor: None, focus_rel: None, focus_node: Node::Centre, holding: None, picked: &[], camera: (0.0, 0.0), letters: None, insert: None, refused: &[], reshape: None, labels: false, grid: true, ink: crate::ui::wire::Ink::Braille },
                f.area(),
            )
        })
        .unwrap();
        let buf = term.backend().buffer();
        // Braille DOTS, not cells: a dashed edge still touches every cell it crosses.
        let braille = buf
            .content
            .iter()
            .filter_map(|c| c.symbol().chars().next())
            .filter(|ch| ('\u{2800}'..='\u{28ff}').contains(ch))
            .map(|ch| (ch as u32 - 0x2800).count_ones() as usize)
            .sum();
        let dots = buf.content.iter().filter(|c| c.symbol() == "·").count();
        (braille, dots)
    }

    #[test]
    fn a_label_wraps_to_its_width_keeps_its_padding_or_stays_on_one_line_and_wears_its_look() {
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::Box, "alpha beta gamma delta", 0.0, 0.0);
        doc.element_mut(id).unwrap().w = 30.0;
        doc.element_mut(id).unwrap().h = 8.0;
        let e = doc.element(id).unwrap();
        assert_eq!(label_lines(e, &e.label).len(), 1, "fits on one line inside a wide box");
        doc.element_mut(id).unwrap().text.width = Some(6);
        let e = doc.element(id).unwrap();
        let lines = label_lines(e, &e.label);
        assert_eq!(lines.len(), 4, "wrapped to its own width: {lines:?}");
        doc.element_mut(id).unwrap().text.wrap = false;
        let e = doc.element(id).unwrap();
        let lines = label_lines(e, &e.label);
        assert_eq!((lines.len(), lines[0].2.as_str()), (1, "alpha…"), "one line, cut short");
        doc.element_mut(id).unwrap().text = crate::model::TextStyle { padding: 3, align: Align::Left, valign: VAlign::Top, ..Default::default() };
        let e = doc.element(id).unwrap();
        let (x, y, _) = label_lines(e, &e.label)[0].clone();
        assert_eq!((x, y), (4.0, 4.0), "a cell in and three of padding — a plain shape has no tag row");
        let mut t = crate::model::TextStyle { bold: true, italic: true, underline: true, band: true, color: Some(crate::ontology::Colour::Hex([1, 2, 3])), ..Default::default() };
        let s = dress(Style::new().fg(theme::t().ink), &t, true, theme::t().panel);
        assert!(s.add_modifier.contains(Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED));
        assert_eq!(s.fg, Some(Color::Rgb(1, 2, 3)));
        assert_eq!(s.bg, Some(theme::band(theme::t().panel)));
        t.bold = false;
        let s = dress(Style::new().fg(theme::t().inverse).bg(theme::t().yellow), &t, false, theme::t().panel);
        assert_eq!((s.fg, s.bg), (Some(theme::t().inverse), Some(theme::t().yellow)), "the cursor's inverse keeps its colours");
        assert!(s.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn a_ruled_grid_is_box_drawing_at_the_grid_s_density_and_a_dotted_one_is_dots() {
        let mut doc = Document::default();
        let count = |doc: &Document, sym: &str| {
            let mut term = Terminal::new(TestBackend::new(60, 16)).unwrap();
            term.draw(|f| {
                f.render_widget(
                    Scene { doc, cursor: None, focus_rel: None, focus_node: Node::Centre, holding: None, picked: &[], camera: (0.0, 0.0), letters: None, insert: None, refused: &[], reshape: None, labels: false, grid: true, ink: crate::ui::wire::Ink::Braille },
                    f.area(),
                )
            })
            .unwrap();
            term.backend().buffer().content.iter().filter(|c| c.symbol() == sym).count()
        };
        assert!(count(&doc, "·") > 0 && count(&doc, "┼") == 0);
        doc.metadata.page.grid_style = crate::model::GridStyle::Lines;
        let (crosses, rules) = (count(&doc, "┼"), count(&doc, "─") + count(&doc, "│"));
        assert!(crosses > 0 && rules > crosses, "crossings where the rules meet, rules between: {crosses} / {rules}");
        assert_eq!(count(&doc, "·"), 0);
        doc.metadata.page.grid_size = 8;
        assert!(count(&doc, "┼") < crosses, "a wider grid is fewer crossings");
    }

    #[test]
    fn an_outline_turned_off_is_not_drawn_and_a_dashed_one_is_lighter() {
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::Box, "a", 4.0, 2.0);
        let (solid, _) = cells(&doc);
        doc.element_mut(id).unwrap().line = crate::ontology::LineStyle::Dashed;
        let (dashed, _) = cells(&doc);
        assert!(dashed < solid, "dashes leave dots unset: {dashed} < {solid}");
        doc.element_mut(id).unwrap().outline = false;
        let (none, _) = cells(&doc);
        assert_eq!(none, 0, "no outline at all");
    }

    #[test]
    fn the_page_turns_corners_sketches_outlines_spaces_the_grid_and_shows_its_edge() {
        let mut doc = Document::default();
        doc.add(ShapeKind::Box, "a", 4.0, 2.0);
        let (plain, dots4) = cells(&doc);
        doc.metadata.page.rounded = true;
        let (rounded, _) = cells(&doc);
        assert_ne!(plain, rounded, "a rounded box is drawn differently");
        doc.metadata.page.rounded = false;
        doc.metadata.page.sketch = true;
        let (sketched, _) = cells(&doc);
        assert!(sketched >= plain, "a sketched edge touches at least as many cells");
        assert_eq!(cells(&doc), (sketched, dots4), "and the same ones every frame");
        doc.metadata.page.grid_size = 8;
        let (_, dots8) = cells(&doc);
        assert!(dots8 < dots4, "a wider grid is fewer dots");
        doc.metadata.page.page_view = true;
        doc.metadata.page.paper = crate::model::Paper::Custom(30, 8);
        let (with_edge, _) = cells(&doc);
        assert!(with_edge > sketched, "the page's edge is drawn");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::{ShapeKind, RelationKind};
    use ratatui::{backend::TestBackend, Terminal};

    fn screen(scene: Scene, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| f.render_widget(scene, f.area())).unwrap();
        let buf = term.backend().buffer();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn labels_and_kind_tags_are_written_over_the_outlines() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::ApplicationComponent, "Billing", 2.0, 2.0);
        let b = doc.add(ShapeKind::DataObject, "invoice", 30.0, 2.0);
        doc.connect(RelationKind::Access, a, b).unwrap();
        let out = screen(
            Scene {
                doc: &doc,
                cursor: Some(a),
                focus_rel: None,
                focus_node: Node::Centre,
                holding: None,
                picked: &[],
                camera: (0.0, 0.0),
                letters: None,
                insert: None,
                refused: &[],
                reshape: None,
                labels: true,
                grid: false,
                ink: crate::ui::wire::Ink::Braille,
            },
            60,
            12,
        );
        assert!(out.contains("Billing"), "{out}");
        assert!(out.contains("component"), "{out}");
        assert!(out.contains("invoice"), "{out}");
        assert!(out.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)), "something was drawn in braille");
    }

    #[test]
    fn the_cursors_shape_is_inverse_and_the_focused_node_too() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Box, "Billing", 2.0, 2.0);
        let b = doc.add(ShapeKind::Box, "invoice", 30.0, 2.0);
        let r = doc.connect(RelationKind::Link, a, b).unwrap();
        let scene = |focus_rel| Scene {
            doc: &doc,
            cursor: Some(a),
            focus_rel,
            focus_node: Node::Centre,
            holding: None,
            picked: &[],
            camera: (0.0, 0.0),
            letters: None,
            insert: None,
            refused: &[],
            reshape: None,
            labels: true,
            grid: true,
            ink: crate::ui::wire::Ink::Braille,
        };
        let mut term = Terminal::new(TestBackend::new(60, 10)).unwrap();
        term.draw(|f| f.render_widget(scene(None), f.area())).unwrap();
        let buf = term.backend().buffer();
        assert_eq!(buf[(4, 4)].bg, theme::t().hilite, "inside the cursor's box is tinted");
        assert_eq!(buf[(31, 4)].bg, Color::Reset, "the other box is not");
        let find = |buf: &Buffer, sym: &str| (0..10).flat_map(|y| (0..60).map(move |x| (x, y))).find(|&(x, y)| buf[(x, y)].symbol() == sym);
        let (lx, ly) = find(buf, "B").expect("the label");
        assert_eq!((buf[(lx, ly)].fg, buf[(lx, ly)].bg), (theme::t().inverse, theme::t().yellow), "and its label is reverse");
        term.draw(|f| f.render_widget(scene(Some(r)), f.area())).unwrap();
        let buf = term.backend().buffer();
        assert_eq!(buf[(4, 4)].bg, Color::Reset, "on a relation, the shape is no longer the thing selected");
        let yellow_braille = (0..10)
            .flat_map(|y| (0..60).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].fg == theme::t().yellow && buf[(x, y)].symbol().chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)))
            .count();
        assert!(yellow_braille >= 4, "the focused node is a braille mark, several cells of it: {yellow_braille}");
    }

    #[test]
    fn a_painted_shape_and_a_wide_painted_link_draw_in_their_colour() {
        use ratatui::{backend::TestBackend, Terminal};
        use crate::ontology::Paint;
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Box, "", 2.0, 2.0);
        let b = doc.add(ShapeKind::Box, "", 40.0, 2.0);
        let r = doc.connect(RelationKind::Link, a, b).unwrap();
        doc.element_mut(b).unwrap().color = Some(Paint::Purple.into());
        let dots = |doc: &Document| {
            let mut term = Terminal::new(TestBackend::new(60, 10)).unwrap();
            let scene = Scene {
                doc,
                cursor: None,
                focus_rel: None,
                focus_node: Node::Centre,
                holding: None,
                picked: &[],
                camera: (0.0, 0.0),
                letters: None,
                insert: None,
                refused: &[],
                reshape: None,
                labels: true,
                grid: false,
                ink: crate::ui::wire::Ink::Braille,
            };
            term.draw(|f| f.render_widget(scene, f.area())).unwrap();
            let buf = term.backend().buffer().clone();
            let count = |c: Color| (0..10).flat_map(|y| (0..60).map(move |x| (x, y))).filter(|&(x, y)| buf[(x, y)].fg == c && buf[(x, y)].symbol() != " ").count();
            (count(theme::paint(Paint::Purple)), count(theme::paint(Paint::Red)), count(theme::t().structure))
        };
        let (purple, _, grey_before) = dots(&doc);
        assert!(purple > 10, "the painted box is purple");
        doc.element_mut(b).unwrap().stroke = 3;
        let (thick, _, _) = dots(&doc);
        assert!(thick > purple, "a heavier stroke lights more cells: {thick} > {purple}");
        {
            let rel = doc.relation_mut(r).unwrap();
            let mut n = rel.notation();
            n.color = Some(Paint::Red.into());
            n.width = 3;
            rel.style = Some(n);
        }
        let (_, red, grey_after) = dots(&doc);
        assert!(red > 0, "the painted link is red");
        assert!(grey_after < grey_before, "and no longer grey");
    }

    #[test]
    fn a_label_sits_where_its_alignment_says() {
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::Box, "hi", 0.0, 0.0);
        let e = doc.element(id).unwrap();
        assert_eq!(label_lines(e, "hi")[0].0, (e.w - 2.0) / 2.0, "centred");
        let mut e = e.clone();
        e.text.align = Align::Left;
        e.text.valign = VAlign::Top;
        assert_eq!((label_lines(&e, "hi")[0].0, label_lines(&e, "hi")[0].1), (1.0, 1.0));
        e.text.align = Align::Right;
        e.text.valign = VAlign::Bottom;
        assert_eq!((label_lines(&e, "hi")[0].0, label_lines(&e, "hi")[0].1), (e.w - 3.0, e.h - 2.0));
        e.kind = ShapeKind::Node;
        e.text.valign = VAlign::Top;
        assert_eq!(label_lines(&e, "hi")[0].1, 2.0, "under the kind's tag on a typed shape");
    }

    #[test]
    fn the_camera_moves_the_picture_and_a_typed_label_shows_its_cursor() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Node, "db", 40.0, 30.0);
        let scene = |camera, insert| Scene {
            doc: &doc,
            cursor: Some(a),
            focus_rel: None,
            focus_node: Node::Centre,
            holding: None,
            picked: &[],
            camera,
            letters: None,
            insert,
            refused: &[],
            reshape: None,
            labels: true,
            grid: true,
            ink: crate::ui::wire::Ink::Braille,
        };
        assert!(!screen(scene((0.0, 0.0), None), 30, 10).contains("db"), "off screen");
        assert!(screen(scene((38.0, 28.0), None), 30, 10).contains("db"), "the camera brings it in");
        assert!(screen(scene((38.0, 28.0), Some((Target::Element(a), "dat"))), 30, 10).contains("dat█"));
    }
}
