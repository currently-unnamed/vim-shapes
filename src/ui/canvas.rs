//! The diagram, drawn: outlines and relations in braille, labels in text over the top.
//!
//! Everything is in world cells, and the camera decides which window of the world the area
//! shows. Shapes are painted through one braille canvas into a scratch buffer and only the
//! cells that actually got a dot are composited back — so the labels, the letters and the
//! panel behind the canvas survive untouched.

use super::theme;
use crate::model::{Align, Document, Element, ElementId, ElementInk, Fill, Node, RelationId, VAlign};
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
    /// The element under the mouse, outside any reshape — a fallback so its handles paint
    /// too, and can be found before anything is clicked.
    pub hover: Option<ElementId>,
    /// Which of `hover`'s eight arrow regions the mouse sits in right now, if any — so
    /// `paint_handles` can pick that one out from the other seven faint arrows instead of
    /// leaving all eight looking equally clickable.
    pub hover_arrow: Option<usize>,
    /// A marquee drag in progress: its anchor, and where the mouse is now.
    pub marquee: Option<((f64, f64), (f64, f64))>,
    /// Where the mouse sits in world space right now, over an element or not — the relation
    /// in hand reaches for this when the drag is over open ground, so it appears the instant
    /// the drag leaves `holding`'s element rather than waiting for it to land on another one.
    pub mouse: Option<(f64, f64)>,
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

/// How much of a hovered shape's outline is `bright` rather than its own colour: enough to
/// read as "this one" next to its neighbours, not so much it competes with the cursor's own
/// yellow or a picked shape's green.
const HOVER_TINT: u8 = 35;

/// How wide a relation's own label wraps to when nothing has set `text.width` — an element's
/// blank width is its own inside, but a relation has no box to fall back on, so this is the
/// line the app draws instead: narrow enough that a long sentence stands as a short, tall
/// paragraph near the line it is on rather than a wide one bleeding into whatever is beside it.
const DEFAULT_LABEL_WRAP: usize = 32;

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
            // An element overridden to braille: the whole-scene pass above left it alone
            // (`wire::paint` skips anything `ink_of` doesn't call Lines), so its outline is
            // still whatever the ground and grid dots were — drawn now, on its own.
            for e in self.doc.elements_in_order() {
                if self.doc.element_visible(e.id) && e.outline && self.ink_of(e) == super::wire::Ink::Braille {
                    self.paint_one_braille(e, area, buf);
                }
            }
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
            // The reverse: an element overridden to lines, left alone by the braille pass
            // above for the same reason.
            for e in self.doc.elements_in_order() {
                if self.doc.element_visible(e.id) && e.outline && self.ink_of(e) == super::wire::Ink::Lines {
                    super::wire::paint_one(&self, e, area, buf);
                }
            }
        }
        // The hover/reshape handles, always last and always in line art — a control is not
        // part of the sketch, it is the interface for editing it, and the crisp fixed glyphs
        // in `wire::paint_handles` read better under a mouse than a braille mark ever would,
        // whatever ink the diagram itself is drawn in.
        super::wire::paint_handles(&self, area, buf);
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

    /// The ink this element's own outline (and header rule) draws in: its own override, or
    /// the document's. Never the relations reaching it, its ports, or its cursor tint — those
    /// stay in the document's ink regardless, so a mixed diagram changes only the shape.
    pub(crate) fn ink_of(&self, e: &Element) -> super::wire::Ink {
        match e.ink {
            ElementInk::Auto => self.ink,
            ElementInk::Lines => super::wire::Ink::Lines,
            ElementInk::Braille => super::wire::Ink::Braille,
        }
    }

    /// One element only, in braille, when the rest of the scene is drawn in lines —
    /// [`Element::ink`]'s override. Painted into a scratch canvas the size of the whole area
    /// and composited back the same way the whole scene is in [`Widget::render`], so only the
    /// cells this one shape's outline actually touched change.
    fn paint_one_braille(&self, e: &Element, area: Rect, buf: &mut Buffer) {
        let (cx, cy) = self.camera;
        let (w, h) = (area.width as f64, area.height as f64);
        let flip = move |y: f64| 2.0 * cy + h - y;
        let page = &self.doc.metadata.page;
        let colour = self.colour_of(e.id, e.kind.layer());
        let mut scratch = Buffer::empty(area);
        let canvas = Canvas::default()
            .marker(Marker::Braille)
            .x_bounds([cx, cx + w])
            .y_bounds([cy, cy + h])
            .paint(|ctx| paint_shape_outline(ctx, page, e, colour, flip));
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
            // A plain mouse hover, on top of whatever the shape's own colour already is — the
            // same tint regardless of layer or fill, so it reads as "the mouse is here" and
            // not as a colour the ontology assigned meaning to.
            let own = if self.hover == Some(id) { theme::fade(theme::t().bright, HOVER_TINT, own) } else { own };
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
        // The relation in hand: from the held element to whatever the cursor is over, or —
        // over open ground, where there's no element to aim at — the raw mouse point, so the
        // line follows the drag out from the start instead of waiting for it to land on
        // something.
        if let Some(from) = self.holding
            && let Some(a) = self.doc.element(from)
        {
            let to_elem = self.cursor.filter(|&to| to != from).and_then(|to| self.doc.element(to));
            let far = to_elem.map(|b| b.center()).or(self.mouse);
            if let Some(far) = far {
                let p1 = shapes::edge_point(a.x, a.y, a.w, a.h, far);
                let p2 = to_elem.map(|b| shapes::edge_point(b.x, b.y, b.w, b.h, a.center())).unwrap_or(far);
                let hand = crate::ontology::Notation { line: LineStyle::Dashed, tail: End::None, head: End::Arrow, width: 1, color: None, route: crate::ontology::Route::Straight, end_size: crate::ontology::EndSize::Normal, opacity: 100 };
                paint_relation(ctx, &[p1, p2], &hand, theme::t().yellow, flip);
            }
        }
        // A marquee in progress: a dashed box from its anchor to wherever the mouse is now.
        if let Some(((ax, ay), (bx, by))) = self.marquee {
            let (x0, x1) = (ax.min(bx), ax.max(bx));
            let (y0, y1) = (ay.min(by), ay.max(by));
            let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)];
            let dashed = crate::ontology::Notation { line: LineStyle::Dashed, tail: End::None, head: End::None, width: 1, color: None, route: crate::ontology::Route::Straight, end_size: crate::ontology::EndSize::Normal, opacity: 100 };
            paint_relation(ctx, &corners, &dashed, theme::t().bright, flip);
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
        // A shape's own eight handles, and the arrows just outside them, are never drawn in
        // this braille pass — they are a control, not the diagram, and paint once, always in
        // line art, after `render`'s ink branch: see `wire::paint_handles`.
        for e in self.doc.elements_in_order() {
            if !self.doc.element_visible(e.id) {
                continue;
            }
            // No outline: the fill's tint and the label are the shape.
            if !e.outline {
                continue;
            }
            // An element overridden to the other ink is left for `render`'s secondary pass —
            // drawing it here would merge it into this braille canvas's own composite.
            if self.ink_of(e) != super::wire::Ink::Braille {
                continue;
            }
            let colour = self.colour_of(e.id, e.kind.layer());
            paint_shape_outline(ctx, page, e, colour, flip);
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
            let shape = e.kind.shape();
            // The outline's own silhouette, sampled once — masks the fill below and the
            // cursor's own highlight to the shape itself. `None` for a plain label or a
            // figure too sparse to have an inside of its own: those keep the whole box.
            let outline = (!composite && !matches!(shape, crate::ontology::Shape::Text | crate::ontology::Shape::StickFigure))
                .then(|| shapes::drawn(&self.doc.metadata.page, e, SKETCH));
            // A fill of its own tints the inside: the colour laid over the ground at a third
            // of its opacity, so the label and the grid's absence both read. `auto`, on any
            // architecture layer, is the layer's own pastel — the same colour the exports
            // already give it, so a diagram reads filled on screen too; a plain sketch
            // shape's `auto` stays untinted, the blank whiteboard box it always was.
            let rgb = match e.fill {
                Fill::Colour(c) => Some(c.on(theme::mode() == theme::Mode::Light)),
                Fill::Auto if e.kind.layer() != Layer::Sketch => e.kind.layer().pastel(),
                _ => None,
            };
            // Whether a tint is actually painted under this shape — `dim`'s own contrast was
            // only ever checked against the plain ground, and in dark mode a layer's tint
            // sits close enough in tone to wash it right out; the tag needs `ink` instead
            // wherever that tint is the ground it is actually read against.
            let mut tinted = false;
            if let Some(rgb) = rgb
                && !composite
                && !matches!(shape, crate::ontology::Shape::Text | crate::ontology::Shape::Dashed | crate::ontology::Shape::StickFigure)
            {
                let Color::Rgb(gr, gg, gb) = self.ground() else { unreachable!("the ground is always rgb") };
                let t = crate::ontology::mix(rgb, [gr, gg, gb], 1.0 - 0.35 * e.drawn_opacity() as f64 / 100.0);
                let tint = Color::Rgb(t[0], t[1], t[2]);
                tint_inside(e, outline.as_deref(), col, row, buf, tint);
                tinted = true;
            }
            // The cursor's shape is INVERSE: its inside tinted, its label black on yellow. An
            // outline in a different colour was too subtle to find on a busy diagram.
            if on && !composite && self.focus_rel.is_none() {
                tint_inside(e, outline.as_deref(), col, row, buf, theme::t().hilite);
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
            } else if tinted {
                Style::new().fg(theme::t().ink)
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
                    if let Some(m) = e.status.mark(false) {
                        put(buf, e.x + (e.w - tw) / 2.0 + tw - 1.0, e.y + 1.0, m, tag_style.fg(theme::t().red));
                    }
                }
                if self.labels || typing {
                    for (x, y, l) in label_lines(e, &label) {
                        put(buf, x, y, &l, label_style);
                    }
                }
                // The compartment: a rule under the header, then a row per property, in
                // the kind's colour dimmed — the shape's own text, not its label.
                if let Some(ry) = e.header_rule() {
                    // In lines, wire::paint (or, overridden, wire::paint_one) already merged
                    // this rule into the box's own sides — drawing it again here would double it.
                    if self.ink_of(e) == super::wire::Ink::Braille {
                        let rule: String = "─".repeat((e.w as usize).saturating_sub(2));
                        put(buf, e.x + 1.0, ry, &rule, Style::new().fg(self.colour_of(e.id, e.kind.layer())));
                    }
                    // On the cursor's tint the rows are the ink: the inverse is the label's
                    // colour on the saturated yellow, and unreadable on the pale tint.
                    let row_style = if on && self.focus_rel.is_none() { Style::new().fg(theme::t().ink).bg(theme::t().hilite) } else { Style::new().fg(theme::t().ink) };
                    let gone = crate::model::Status::Deprecated.mark(false).unwrap_or_default();
                    for (x, y, l) in e.row_lines() {
                        put(buf, x, y, &l, row_style);
                        // A row opens with its mark, so a row that opens with ✗ wears it.
                        if l.starts_with(gone) {
                            put(buf, x, y, gone, row_style.fg(theme::t().red));
                        }
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
                let plain = !(focused || refused && node == Node::Centre);
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
                // A relation has no box to wrap inside the way an element's label does, so a
                // long one — an ontology link's own description, most often — gets a default
                // width of its own rather than running the full length of the line it is on
                // and bleeding into whatever else is nearby, or across a diagonal.
                let wrap_w = r.text.width.map_or(DEFAULT_LABEL_WRAP, |w| w.max(1) as usize);
                let lines: Vec<String> = if r.text.wrap { super::chrome::wrap(&badge, wrap_w) } else { vec![badge] };
                // Stacked upward, so the line nearest the connector never moves and a longer
                // label just grows away from it.
                let n = lines.len();
                for (i, line) in lines.iter().enumerate() {
                    let bw = line.chars().count() as f64;
                    let y = py - 1.5 - (n - 1 - i) as f64;
                    put(buf, px - bw / 2.0, y, line, style);
                }
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

/// A tint over the inside of `e`'s own box — its fill, or the cursor's highlight — a cell in
/// from every side either way. `mask`, when there is one, is the shape's own outline: each row
/// is clipped to where that row actually reaches, so a curve or a point never bleeds into the
/// corner it doesn't cover. `None` — a plain label, or a figure too sparse to have an inside of
/// its own — keeps the whole box, the way every shape did before there was a mask to ask.
fn tint_inside(e: &Element, mask: Option<&[CurvePrimitive]>, col: impl Fn(f64) -> Option<u16>, row: impl Fn(f64) -> Option<u16>, buf: &mut Buffer, tint: Color) {
    for wy in (e.y as i64 + 1)..(e.bottom() as i64) {
        let span = match mask {
            Some(outline) => shapes::row_span(outline, wy as f64 + 0.5).map(|(lo, hi)| (lo.max(e.x + 1.0), hi.min(e.right() - 1.0))),
            None => Some((e.x + 1.0, e.right() - 1.0)),
        };
        let Some((lo, hi)) = span else { continue };
        if lo >= hi {
            continue;
        }
        for wx in (lo.ceil() as i64)..=(hi.floor() as i64) {
            if let (Some(x), Some(y)) = (col(wx as f64), row(wy as f64)) {
                buf[(x, y)].set_bg(tint);
            }
        }
    }
}

/// One shape's own outline — never a relation, a port or a highlight — so [`Scene::ink_of`]'s
/// secondary pass can draw a single overridden element without the rest of what a normal pass
/// over every element would touch.
fn paint_shape_outline(ctx: &mut Context, page: &crate::model::Page, e: &Element, colour: Color, flip: impl Fn(f64) -> f64 + Copy) {
    for prim in shapes::patterned(shapes::drawn(page, e, SKETCH), e.drawn_line()) {
        paint_primitive(ctx, prim, colour, flip);
    }
    // A thicker outline is the same outline drawn again a braille dot out — and, for the
    // thickest, a dot in as well — so it stays the shape it was, only heavier.
    for d in [0.5, -0.5].iter().take(e.stroke.saturating_sub(1) as usize) {
        for prim in shapes::patterned(shapes::drawn_at(page, e, e.x - d, e.y - d / 2.0, e.w + 2.0 * d, e.h + d, SKETCH), e.drawn_line()) {
            paint_primitive(ctx, prim, colour, flip);
        }
    }
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

/// A relation node's mark — a handle's own marks are `wire::paint_handles`' fixed glyphs,
/// always line art, not this braille one.
#[derive(Clone, Copy)]
enum Mark {
    /// A small hollow diamond.
    Plain(Color),
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
                Scene { doc, cursor: None, focus_rel: None, focus_node: Node::Centre, holding: None, picked: &[], camera: (0.0, 0.0), letters: None, insert: None, refused: &[], reshape: None, hover: None, hover_arrow: None, marquee: None, mouse: None, labels: false, grid: true, ink: crate::ui::wire::Ink::Braille },
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
                    Scene { doc, cursor: None, focus_rel: None, focus_node: Node::Centre, holding: None, picked: &[], camera: (0.0, 0.0), letters: None, insert: None, refused: &[], reshape: None, hover: None, hover_arrow: None, marquee: None, mouse: None, labels: false, grid: true, ink: crate::ui::wire::Ink::Braille },
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
                reshape: None, hover: None, hover_arrow: None, marquee: None, mouse: None,
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
    fn a_relation_label_longer_than_the_default_wraps_to_several_lines_and_a_short_one_does_not() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::ObjectType, "Location", 2.0, 2.0);
        let b = doc.add(ShapeKind::ObjectType, "Asset", 40.0, 20.0);
        let r = doc.connect(RelationKind::LinkType, a, b).unwrap();
        doc.relation_mut(r).unwrap().label = Some("Connects an operational location to the physical assets currently placed there.".into());
        let out = screen(
            Scene {
                doc: &doc,
                cursor: None,
                focus_rel: None,
                focus_node: Node::Centre,
                holding: None,
                picked: &[],
                camera: (0.0, 0.0),
                letters: None,
                insert: None,
                refused: &[],
                reshape: None, hover: None, hover_arrow: None, marquee: None, mouse: None,
                labels: true,
                grid: false,
                ink: crate::ui::wire::Ink::Braille,
            },
            60,
            26,
        );
        // No row is anywhere near the full sentence's own length — it was broken up, not
        // left to run the width of the line it sits on.
        for line in out.lines() {
            assert!(line.trim().chars().count() <= DEFAULT_LABEL_WRAP, "a row ran past the default wrap width: {line:?}");
        }
        assert!(out.contains("Connects"), "{out}");
        assert!(out.contains("there."), "the last word of the sentence still made it in, on its own line: {out}");
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
            reshape: None, hover: None, hover_arrow: None, marquee: None, mouse: None,
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
    fn a_hover_handle_stays_line_art_even_when_the_document_s_own_ink_is_braille() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Box, "a", 10.0, 6.0);
        let out = screen(
            Scene {
                doc: &doc,
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
                hover: Some(a),
                hover_arrow: None,
                marquee: None, mouse: None,
                labels: false,
                grid: false,
                ink: crate::ui::wire::Ink::Braille,
            },
            60,
            20,
        );
        assert!(out.contains('◇'), "an unpatched hover handle is a fixed line-art glyph, not a braille mark, whatever the document's own ink: {out}");
        assert!(out.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)), "the shape's own outline is still braille — only the control changed: {out}");
    }

    #[test]
    fn the_relation_in_hand_reaches_the_raw_mouse_point_over_open_ground() {
        // `cursor` here equals `holding` — the same state `App::mouse_left_drag` leaves it in
        // while a ctrl-drag is out over nothing, since it has no other element to fall back to.
        // Only `mouse` — the raw point — tells the renderer where the drag actually is; without
        // it the preview has nothing to reach for and paints nothing until the drag lands on a
        // second shape. Checked against both inks — `canvas::Scene::paint`'s braille path and
        // `wire::draw`'s line-art path carry the same fallback, and a fix landed in only one
        // would leave the other still waiting for the drag to land on a shape.
        for ink in [crate::ui::wire::Ink::Braille, crate::ui::wire::Ink::Lines] {
            let mut doc = Document::default();
            let a = doc.add(ShapeKind::Box, "a", 2.0, 2.0);
            let out = screen(
                Scene {
                    doc: &doc,
                    cursor: Some(a),
                    focus_rel: None,
                    focus_node: Node::Centre,
                    holding: Some(a),
                    picked: &[],
                    camera: (0.0, 0.0),
                    letters: None,
                    insert: None,
                    refused: &[],
                    reshape: None,
                    hover: None,
                    hover_arrow: None,
                    marquee: None,
                    mouse: Some((50.0, 15.0)),
                    labels: false,
                    grid: false,
                    ink,
                },
                60,
                20,
            );
            let far_corner_marked = (40..60)
                .flat_map(|x| (10..20).map(move |y| (x, y)))
                .any(|(x, y)| out.lines().nth(y).and_then(|line| line.chars().nth(x)).is_some_and(|c| c != ' '));
            assert!(far_corner_marked, "{ink:?}: a dashed line reaches toward the raw mouse point, well outside the shape's own box: {out}");
        }
    }

    #[test]
    fn hovering_a_shape_tints_its_own_outline_and_leaves_its_neighbour_alone() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Box, "a", 10.0, 6.0);
        let b = doc.add(ShapeKind::Box, "b", 40.0, 6.0);
        let layer = doc.element(a).unwrap().kind.layer();
        let scene = |hover| Scene {
            doc: &doc,
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
            hover,
            hover_arrow: None,
            marquee: None, mouse: None,
            labels: false,
            grid: false,
            ink: crate::ui::wire::Ink::Braille,
        };
        let base = scene(None).colour_of(a, layer);
        assert_ne!(scene(Some(a)).colour_of(a, layer), base, "hovering a shape changes its own outline colour");
        assert_eq!(scene(Some(a)).colour_of(b, layer), base, "a shape someone else is hovering stays its own colour");
    }

    #[test]
    fn an_architecture_shape_s_auto_fill_tints_the_screen_but_a_basic_shape_s_stays_off() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut doc = Document::default();
        let arch = doc.add(ShapeKind::ApplicationComponent, "Billing", 0.0, 0.0);
        let plain = doc.add(ShapeKind::Box, "note", 40.0, 0.0);
        let scene = Scene {
            doc: &doc,
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
            hover: None,
            hover_arrow: None,
            marquee: None, mouse: None,
            labels: false,
            grid: false,
            ink: crate::ui::wire::Ink::Braille,
        };
        let mut term = Terminal::new(TestBackend::new(80, 14)).unwrap();
        term.draw(|f| f.render_widget(scene, f.area())).unwrap();
        let buf = term.backend().buffer();
        let arch_e = doc.element(arch).unwrap();
        let tinted = (arch_e.x as u16 + 2..arch_e.right() as u16).any(|x| buf[(x, arch_e.y as u16 + 2)].bg != Color::Reset);
        assert!(tinted, "an architecture-layer shape's own auto fill is the layer's pastel on screen, not just in the exports");
        let plain_e = doc.element(plain).unwrap();
        let untinted = (plain_e.x as u16 + 2..plain_e.right() as u16).all(|x| buf[(x, plain_e.y as u16 + 2)].bg == Color::Reset);
        assert!(untinted, "a plain sketch shape's own auto fill is still nothing on screen — the blank whiteboard box it always was");
    }

    #[test]
    fn every_layer_s_auto_fill_still_lets_the_kind_tag_read_over_it() {
        use ratatui::{backend::TestBackend, Terminal};
        for layer in Layer::ALL {
            if layer == Layer::Composite || layer == Layer::Sketch {
                continue; // Composite has no pastel; a sketch shape's auto fill stays untinted.
            }
            let kind = ShapeKind::ALL.into_iter().find(|k| k.layer() == layer).unwrap_or_else(|| panic!("{layer:?} has no kind to test with"));
            let mut doc = Document::default();
            let id = doc.add(kind, "x", 0.0, 0.0);
            {
                let e = doc.element_mut(id).unwrap();
                e.w = 24.0;
                e.h = 6.0;
            }
            let scene = Scene {
                doc: &doc,
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
                hover: None,
                hover_arrow: None,
                marquee: None, mouse: None,
                labels: false,
                grid: false,
                ink: crate::ui::wire::Ink::Braille,
            };
            let mut term = Terminal::new(TestBackend::new(40, 12)).unwrap();
            term.draw(|f| f.render_widget(scene, f.area())).unwrap();
            let buf = term.backend().buffer();
            let e = doc.element(id).unwrap();
            // The exact cell `put` writes the tag's first character to, in `text()` — an
            // outline stroke can sit on the same row, so only this column is guaranteed to
            // be the tag itself and not a braille dot with no tint under it.
            let tag = e.tag();
            let tw = tag.chars().count() as f64;
            let (x, y) = ((e.x + (e.w - tw) / 2.0).round() as u16, (e.y + 1.0) as u16);
            let cell = &buf[(x, y)];
            assert_eq!(cell.symbol(), &tag[..1], "{layer:?}: not the tag's own cell");
            assert_ne!(cell.bg, Color::Reset, "{layer:?}'s shape is tinted, or this isn't testing what it thinks");
            let c = theme::contrast(cell.fg, cell.bg);
            assert!(c >= 3.0, "{layer:?}'s kind tag on its own full-strength tint: {c:.2}");
        }
    }

    #[test]
    fn a_curved_shape_s_fill_stays_inside_its_own_outline_and_never_bleeds_into_the_corner() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::Ellipse, "", 0.0, 0.0);
        {
            let e = doc.element_mut(id).unwrap();
            e.w = 20.0;
            e.h = 10.0;
            e.fill = crate::model::Fill::Colour(crate::ontology::Colour::Hex([80, 80, 200]));
        }
        let scene = Scene {
            doc: &doc,
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
            hover: None,
            hover_arrow: None,
            marquee: None, mouse: None,
            labels: false,
            grid: false,
            ink: crate::ui::wire::Ink::Braille,
        };
        let mut term = Terminal::new(TestBackend::new(30, 15)).unwrap();
        term.draw(|f| f.render_widget(scene, f.area())).unwrap();
        let buf = term.backend().buffer();
        assert_ne!(buf[(10, 5)].bg, Color::Reset, "the middle of the ellipse is filled");
        assert_eq!(buf[(1, 1)].bg, Color::Reset, "the box's own corner, well outside the ellipse's own curve, stays unfilled");
        assert_eq!(buf[(18, 1)].bg, Color::Reset, "the opposite corner too");
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
                reshape: None, hover: None, hover_arrow: None, marquee: None, mouse: None,
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
            reshape: None, hover: None, hover_arrow: None, marquee: None, mouse: None,
            labels: true,
            grid: true,
            ink: crate::ui::wire::Ink::Braille,
        };
        assert!(!screen(scene((0.0, 0.0), None), 30, 10).contains("db"), "off screen");
        assert!(screen(scene((38.0, 28.0), None), 30, 10).contains("db"), "the camera brings it in");
        assert!(screen(scene((38.0, 28.0), Some((Target::Element(a), "dat"))), 30, 10).contains("dat█"));
    }
}
