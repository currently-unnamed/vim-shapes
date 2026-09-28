//! The wireframe ink — the diagram in box-drawing line art instead of braille.
//!
//! Braille is fine up close and poor at a distance: a line is a string of pinpricks with the
//! ground showing through, and a screenshare turns it to haze. This ink draws with the
//! characters every terminal synthesises itself — `─│┌┐└┘`, the arc corners `╭╮╰╯`, the
//! diagonals `╱╲`, heavy and double and dashed — so a line is a line.
//!
//! It does not rasterise curves. Each cell holds *connectivity* — which of its eight
//! neighbours it is joined to, and how heavily — and a resolver turns that into the one
//! character with those arms. Shapes and lines MERGE into the same cells, so a line that
//! ends on a box edge becomes a tee and two lines crossing become a cross: the diagram looks
//! drawn rather than pasted. The curved shapes — ellipse, cylinder, cloud — are stencils:
//! staircases of arc corners parametrised by the box, exact and the same every frame.

use super::canvas::Scene;
use super::theme;
use crate::model::{Element, Node};
use crate::ontology::{End, LineStyle, Shape};
use crate::shapes::{self, CurvePrimitive, Point};
use ratatui::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering};

/// Which ink the screen draws with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ink {
    Braille,
    Lines,
}

impl Ink {
    pub fn name(self) -> &'static str {
        match self {
            Ink::Braille => "braille",
            Ink::Lines => "lines",
        }
    }

    pub fn parse(s: &str) -> Option<Ink> {
        match s.trim().to_ascii_lowercase().as_str() {
            "braille" | "b" | "dots" => Some(Ink::Braille),
            "lines" | "l" | "wire" | "wireframe" | "box" => Some(Ink::Lines),
            _ => None,
        }
    }
}

static INK: AtomicU8 = AtomicU8::new(1);

pub fn ink() -> Ink {
    if INK.load(Ordering::Relaxed) == 0 { Ink::Braille } else { Ink::Lines }
}

pub fn set_ink(i: Ink) {
    INK.store(if i == Ink::Braille { 0 } else { 1 }, Ordering::Relaxed);
}

/// How an arm of a cell is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, PartialOrd, Ord)]
pub enum Weight {
    #[default]
    None,
    Dotted,
    Dashed,
    Light,
    Heavy,
    Double,
}

impl Weight {
    fn of(line: LineStyle, stroke: u8) -> Weight {
        match (line, stroke) {
            (LineStyle::Dotted, _) => Weight::Dotted,
            (LineStyle::Dashed, _) => Weight::Dashed,
            (_, 1) => Weight::Light,
            (_, 2) => Weight::Heavy,
            _ => Weight::Double,
        }
    }

    /// The family a junction is drawn in: a dashed arm joins as a light one.
    fn family(self) -> Weight {
        match self {
            Weight::None => Weight::None,
            Weight::Dotted | Weight::Dashed | Weight::Light => Weight::Light,
            Weight::Heavy => Weight::Heavy,
            Weight::Double => Weight::Double,
        }
    }
}

/// One cell's connectivity: four straight arms, four diagonals, and what it is besides.
#[derive(Clone, Copy, Default, Debug)]
pub struct Cell {
    pub n: Weight,
    pub e: Weight,
    pub s: Weight,
    pub w: Weight,
    pub ne: bool,
    pub nw: bool,
    pub se: bool,
    pub sw: bool,
    /// A two-arm corner drawn as an arc.
    pub round: bool,
    /// A character laid down whole — an arrowhead, a mark — which wins over the arms.
    pub fixed: Option<char>,
    pub colour: Option<Color>,
}

impl Cell {
    /// The character with these arms, if the terminal has one.
    pub fn glyph(&self) -> Option<char> {
        if let Some(c) = self.fixed {
            return Some(c);
        }
        let straight = [self.n, self.e, self.s, self.w].iter().filter(|w| **w != Weight::None).count();
        if straight == 0 {
            return match (self.ne || self.sw, self.nw || self.se) {
                (true, true) => Some('╳'),
                (true, false) => Some('╱'),
                (false, true) => Some('╲'),
                (false, false) => None,
            };
        }
        // A run in one style keeps its style; a junction takes its heaviest family.
        let (n, e, s, w) = (self.n != Weight::None, self.e != Weight::None, self.s != Weight::None, self.w != Weight::None);
        if !n && !s && (e || w) {
            let style = self.e.max(self.w);
            return Some(match style {
                Weight::Dotted => '┈',
                Weight::Dashed => '╌',
                Weight::Heavy => '━',
                Weight::Double => '═',
                _ => '─',
            });
        }
        if !e && !w && (n || s) {
            let style = self.n.max(self.s);
            return Some(match style {
                Weight::Dotted => '┊',
                Weight::Dashed => '╎',
                Weight::Heavy => '┃',
                Weight::Double => '║',
                _ => '│',
            });
        }
        let fam = [self.n, self.e, self.s, self.w].iter().map(|w| w.family()).max().unwrap_or(Weight::Light);
        let key = (n, e, s, w);
        let light = match key {
            (false, true, true, false) => if self.round { '╭' } else { '┌' },
            (false, false, true, true) => if self.round { '╮' } else { '┐' },
            (true, true, false, false) => if self.round { '╰' } else { '└' },
            (true, false, false, true) => if self.round { '╯' } else { '┘' },
            (true, true, true, false) => '├',
            (true, false, true, true) => '┤',
            (false, true, true, true) => '┬',
            (true, true, false, true) => '┴',
            _ => '┼',
        };
        Some(match fam {
            Weight::Heavy => match light {
                '┌' | '╭' => '┏',
                '┐' | '╮' => '┓',
                '└' | '╰' => '┗',
                '┘' | '╯' => '┛',
                '├' => '┣',
                '┤' => '┫',
                '┬' => '┳',
                '┴' => '┻',
                _ => '╋',
            },
            Weight::Double => match light {
                '┌' | '╭' => '╔',
                '┐' | '╮' => '╗',
                '└' | '╰' => '╚',
                '┘' | '╯' => '╝',
                '├' => '╠',
                '┤' => '╣',
                '┬' => '╦',
                '┴' => '╩',
                _ => '╬',
            },
            _ => light,
        })
    }
}

/// The grid of cells the ink draws into, in world coordinates offset by the camera.
pub struct Grid {
    cells: Vec<Cell>,
    width: usize,
    height: usize,
    ox: i64,
    oy: i64,
}

impl Grid {
    pub fn new(width: u16, height: u16, camera: (f64, f64)) -> Grid {
        Grid { cells: vec![Cell::default(); width as usize * height as usize], width: width as usize, height: height as usize, ox: camera.0 as i64, oy: camera.1 as i64 }
    }

    fn at(&mut self, wx: i64, wy: i64) -> Option<&mut Cell> {
        let (x, y) = (wx - self.ox, wy - self.oy);
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return None;
        }
        Some(&mut self.cells[y as usize * self.width + x as usize])
    }

    pub fn get(&self, x: usize, y: usize) -> Cell {
        self.cells[y * self.width + x]
    }

    fn arm(&mut self, wx: i64, wy: i64, dir: char, w: Weight, c: Color) {
        if let Some(cell) = self.at(wx, wy) {
            let slot = match dir {
                'n' => &mut cell.n,
                'e' => &mut cell.e,
                's' => &mut cell.s,
                _ => &mut cell.w,
            };
            *slot = (*slot).max(w);
            cell.colour = Some(c);
        }
    }

    /// A horizontal run from `x0` to `x1` inclusive: every cell gets both arms, the ends one.
    pub fn hline(&mut self, x0: i64, x1: i64, y: i64, w: Weight, c: Color) {
        let (a, b) = (x0.min(x1), x0.max(x1));
        for x in a..=b {
            if x > a {
                self.arm(x, y, 'w', w, c);
            }
            if x < b {
                self.arm(x, y, 'e', w, c);
            }
            if a == b {
                self.arm(x, y, 'e', w, c);
            }
        }
    }

    pub fn vline(&mut self, x: i64, y0: i64, y1: i64, w: Weight, c: Color) {
        let (a, b) = (y0.min(y1), y0.max(y1));
        for y in a..=b {
            if y > a {
                self.arm(x, y, 'n', w, c);
            }
            if y < b {
                self.arm(x, y, 's', w, c);
            }
            if a == b {
                self.arm(x, y, 's', w, c);
            }
        }
    }

    /// A cell on a diagonal, running up-right (`╱`) or down-right (`╲`). A cell keeps the
    /// first diagonal it was given: two edges meeting at a vertex share the cell rather than
    /// crossing in it.
    pub fn diag(&mut self, x: i64, y: i64, up_right: bool, c: Color) {
        if let Some(cell) = self.at(x, y) {
            if cell.ne || cell.nw {
                return;
            }
            if up_right {
                cell.ne = true;
                cell.sw = true;
            } else {
                cell.nw = true;
                cell.se = true;
            }
            cell.colour = Some(c);
        }
    }

    pub fn round(&mut self, x: i64, y: i64) {
        if let Some(cell) = self.at(x, y) {
            cell.round = true;
        }
    }

    pub fn fixed(&mut self, x: i64, y: i64, ch: char, c: Color) {
        if let Some(cell) = self.at(x, y) {
            cell.fixed = Some(ch);
            cell.colour = Some(c);
        }
    }

    /// A rectangle's four edges, with the corners rounded or not.
    #[allow(clippy::too_many_arguments)]
    pub fn rect(&mut self, x: i64, y: i64, r: i64, b: i64, w: Weight, round: bool, c: Color) {
        self.hline(x, r, y, w, c);
        self.hline(x, r, b, w, c);
        self.vline(x, y, b, w, c);
        self.vline(r, y, b, w, c);
        if round {
            for (px, py) in [(x, y), (r, y), (x, b), (r, b)] {
                self.round(px, py);
            }
        }
    }

    /// A straight segment between two cells, each cell in the glyph its slope asks for: a
    /// run for the flat, a post for the steep, a diagonal between.
    pub fn segment(&mut self, a: (i64, i64), b: (i64, i64), w: Weight, c: Color) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        if dy == 0 {
            return self.hline(a.0, b.0, a.1, w, c);
        }
        if dx == 0 {
            return self.vline(a.0, a.1, b.1, w, c);
        }
        // Slope as the eye sees it: a cell is twice as tall as it is wide.
        let steep = (dy.abs() * 2) as f64 / dx.abs() as f64;
        if steep < 0.5 {
            // Mostly flat: runs, stepping a row where the line crosses one.
            let steps = dy.abs();
            let mut x = a.0;
            let sy = dy.signum();
            for i in 0..=steps {
                let nx = a.0 + dx * (i + 1) / (steps + 1);
                let y = a.1 + sy * i;
                let end = if i == steps { b.0 } else { nx };
                self.hline(x, end, y, w, c);
                if i < steps {
                    // The step: a rounded elbow down or up onto the next run.
                    self.vline(end, y, y + sy, w, c);
                    self.round(end, y);
                    self.round(end, y + sy);
                }
                x = end;
            }
            return;
        }
        if steep > 4.0 {
            let steps = dx.abs();
            let mut y = a.1;
            let sx = dx.signum();
            for i in 0..=steps {
                let ny = a.1 + dy * (i + 1) / (steps + 1);
                let x = a.0 + sx * i;
                let end = if i == steps { b.1 } else { ny };
                self.vline(x, y, end, w, c);
                if i < steps {
                    self.hline(x, x + sx, end, w, c);
                    self.round(x, end);
                    self.round(x + sx, end);
                }
                y = end;
            }
            return;
        }
        // Diagonal: walked left to right, each row given its share of the columns. A rising
        // line enters a row at its bottom-left, so the `╱` is that row's first cell and the
        // run follows it; a falling line enters at the top-left, so the run comes first and
        // the `╲` is the row's last cell.
        let (a, b) = if dx < 0 { (b, a) } else { (a, b) };
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let rising = dy < 0;
        let rows = dy.abs() + 1;
        for i in 0..rows {
            let y = a.1 + dy.signum() * i;
            let xa = a.0 + (dx + 1) * i / rows;
            let xb = a.0 + (dx + 1) * (i + 1) / rows - 1;
            if rising {
                self.diag(xa, y, true, c);
                if xb > xa {
                    self.hline(xa + 1, xb, y, w, c);
                }
            } else {
                if xb > xa {
                    self.hline(xa, xb - 1, y, w, c);
                }
                self.diag(xb, y, false, c);
            }
        }
    }

    /// A diamond: `╱╲` at the apex and foot, and every row between a `╱` and a `╲` with
    /// runs reaching toward the row before.
    pub fn diamond(&mut self, x: i64, y: i64, r: i64, b: i64, w: Weight, c: Color) {
        let (cx, cy) = ((x + r) / 2, (y + b) / 2);
        let half_w = (r - x) as f64 / 2.0 - 1.0;
        let half_h = (cy - y).max(1) as f64;
        let step = |row: i64| -> i64 { ((row - y) as f64 * half_w / half_h).round() as i64 };
        let mut prev: Option<(i64, i64)> = None;
        for row in y..=b {
            let s = if row <= cy { step(row) } else { step(y + (b - row)) };
            let (l, rr) = (cx - 1 - s, cx + s);
            let rising = row <= cy;
            self.diag(l, row, rising, c);
            self.diag(rr, row, !rising, c);
            if let Some((pl, pr)) = prev {
                if rising && l < pl - 1 {
                    self.hline(l + 1, pl - 1, row, w, c);
                    self.hline(pr + 1, rr - 1, row, w, c);
                }
                if !rising && l > pl + 1 {
                    self.hline(pl + 1, l - 1, row - 1, w, c);
                    self.hline(rr + 1, pr - 1, row - 1, w, c);
                }
            }
            prev = Some((l, rr));
        }
    }

    /// A stadium: the ellipse as a staircase of arc corners. Each row's edge comes from the
    /// ellipse's own equation, every step is an elbow within its own row, and the two sides
    /// are mirrors — so it closes, and is the same every frame.
    pub fn ellipse(&mut self, x: i64, y: i64, r: i64, b: i64, w: Weight, c: Color) {
        if b - y < 2 || r - x < 4 {
            return self.rect(x, y, r, b, w, true, c);
        }
        let (a, bb) = ((r - x) as f64 / 2.0, (b - y) as f64 / 2.0);
        let (cx, cy) = (x as f64 + a, y as f64 + bb);
        let left_at = |row: i64| -> i64 {
            let t = ((row as f64 + 0.5 - cy) / bb).clamp(-1.0, 1.0);
            let half = a * (1.0 - t * t).sqrt();
            ((cx - half).round() as i64).clamp(x, (cx - 1.0) as i64)
        };
        let right_of = |l: i64| r - (l - x);
        let mut prev = left_at(y);
        self.hline(prev, right_of(prev), y, w, c);
        self.round(prev, y);
        self.round(right_of(prev), y);
        for row in (y + 1)..=b {
            let l = left_at(row);
            let (pl, pr, rr) = (prev, right_of(prev), right_of(l));
            if l < pl {
                // Out: down from the row above, then out to this row's edge.
                self.vline(pl, row - 1, row, w, c);
                self.hline(l, pl, row, w, c);
                self.round(pl, row);
                self.round(l, row);
                self.vline(pr, row - 1, row, w, c);
                self.hline(pr, rr, row, w, c);
                self.round(pr, row);
                self.round(rr, row);
            } else if l > pl {
                // In: down from the row above, then in along this row.
                self.vline(pl, row - 1, row, w, c);
                self.hline(pl, l, row, w, c);
                self.round(pl, row);
                self.round(l, row);
                self.vline(pr, row - 1, row, w, c);
                self.hline(rr, pr, row, w, c);
                self.round(pr, row);
                self.round(rr, row);
            } else {
                self.vline(l, row - 1, row, w, c);
                self.vline(rr, row - 1, row, w, c);
            }
            prev = l;
        }
        // The last row closes: a run between its two edges, arcs at both ends.
        self.hline(prev, right_of(prev), b, w, c);
        self.round(prev, b);
        self.round(right_of(prev), b);
    }

    /// A cylinder: a rounded box with the lid's lower rim drawn as a second rule.
    pub fn cylinder(&mut self, x: i64, y: i64, r: i64, b: i64, w: Weight, c: Color) {
        self.rect(x, y, r, b, w, true, c);
        if b - y >= 3 {
            self.hline(x, r, y + 1, w, c);
        }
    }

    /// A cloud: a rounded body with scallops along the top and the bottom.
    pub fn cloud(&mut self, x: i64, y: i64, r: i64, b: i64, w: Weight, c: Color) {
        if r - x < 6 || b - y < 4 {
            return self.rect(x, y, r, b, w, true, c);
        }
        let bumps = ((r - x - 2) / 5).clamp(2, 6);
        let span = r - x - 2;
        // Sides, a row in from the top and bottom, with arcs at both ends.
        self.vline(x, y + 1, b - 1, w, c);
        self.vline(r, y + 1, b - 1, w, c);
        for (row, top) in [(y, true), (b, false)] {
            let inner = if top { row + 1 } else { row - 1 };
            self.round(x, inner);
            self.round(r, inner);
            let mut start = x + 1;
            for i in 0..bumps {
                let end = x + 1 + span * (i + 1) / bumps;
                self.hline(start, end, row, w, c);
                self.round(start, row);
                self.round(end, row);
                // The valley between this bump and the next, and the join to the side.
                self.vline(start, inner, row, w, c);
                self.vline(end, inner, row, w, c);
                self.round(start, inner);
                self.round(end, inner);
                if i == 0 {
                    self.hline(x, start, inner, w, c);
                }
                if i + 1 == bumps {
                    self.hline(end, r, inner, w, c);
                } else {
                    self.hline(end, end + 1, inner, w, c);
                }
                start = end + 1;
            }
        }
    }
}

/// The end glyph for a relation at a cell, given the direction the line arrives from.
fn end_glyph(end: End, dir: (i64, i64)) -> Option<char> {
    let (dx, dy) = dir;
    let horizontal = dx.abs() >= dy.abs();
    let (right, down) = (dx > 0, dy > 0);
    Some(match end {
        End::None => return None,
        End::Arrow => match (horizontal, right, down) {
            (true, true, _) => '▶',
            (true, false, _) => '◀',
            (false, _, true) => '▼',
            (false, _, false) => '▲',
        },
        End::Open => match (horizontal, right, down) {
            (true, true, _) => '>',
            (true, false, _) => '<',
            (false, _, true) => 'v',
            (false, _, false) => '^',
        },
        End::Triangle => match (horizontal, right, down) {
            (true, true, _) => '▷',
            (true, false, _) => '◁',
            (false, _, true) => '▽',
            (false, _, false) => '△',
        },
        End::Diamond => '◆',
        End::HollowDiamond => '◇',
        End::Dot => '●',
        End::Circle => '○',
        // The ER bar: a stroke across the line's end, the way a tee closes it.
        End::Bar => match (horizontal, right, down) {
            (true, true, _) => '┤',
            (true, false, _) => '├',
            (false, _, true) => '┴',
            (false, _, false) => '┬',
        },
        End::Crow => match (horizontal, right, down) {
            (true, true, _) => '⟩',
            (true, false, _) => '⟨',
            (false, _, true) => '⋎',
            (false, _, false) => '⋏',
        },
    })
}

/// Draw the whole scene in line art into `buf`. Grid dots, the ground and every label are
/// the canvas's own and come before and after this, as they do around the braille.
pub fn paint(scene: &Scene, area: Rect, buf: &mut Buffer) {
    let mut g = Grid::new(area.width, area.height, scene.camera);
    let doc = scene.doc;
    let page = &doc.metadata.page;
    let cell = |p: Point| (p.0.round() as i64, p.1.round() as i64);

    // Relations first, so a box edge over a line wins the cell as a tee.
    for r in doc.relations_in_order() {
        if !doc.relation_visible(r.id) {
            continue;
        }
        let n = r.notation();
        let colour = scene.relation_colour(r);
        let Some(pts) = doc.route(r) else { continue };
        let w = Weight::of(n.line, n.width);
        let cells: Vec<(i64, i64)> = pts.iter().map(|p| cell(*p)).collect();
        for pair in cells.windows(2) {
            g.segment(pair[0], pair[1], w, colour);
        }
        // Rounded elbows where an orthogonal route turns.
        for turn in cells.windows(3) {
            g.round(turn[1].0, turn[1].1);
        }
        if cells.len() >= 2 {
            let last = cells.len() - 1;
            let head_dir = (cells[last].0 - cells[last - 1].0, cells[last].1 - cells[last - 1].1);
            if let Some(ch) = end_glyph(n.head, head_dir) {
                g.fixed(cells[last].0, cells[last].1, ch, colour);
            }
            let tail_dir = (cells[0].0 - cells[1].0, cells[0].1 - cells[1].1);
            if let Some(ch) = end_glyph(n.tail, tail_dir) {
                g.fixed(cells[0].0, cells[0].1, ch, colour);
            }
        }
    }
    // The relation in hand: an element's edge if the cursor is over one, else the raw mouse
    // point over open ground — the same fallback `canvas::Scene::paint`'s braille path uses,
    // so the line appears the moment the drag leaves `holding`'s element.
    if let Some(from) = scene.holding
        && let Some(a) = doc.element(from)
    {
        let to_elem = scene.cursor.filter(|&to| to != from).and_then(|to| doc.element(to));
        let far = to_elem.map(|b| b.center()).or(scene.mouse);
        if let Some(far) = far {
            let p1 = cell(shapes::edge_point(a.x, a.y, a.w, a.h, far));
            let p2 = cell(to_elem.map(|b| shapes::edge_point(b.x, b.y, b.w, b.h, a.center())).unwrap_or(far));
            g.segment(p1, p2, Weight::Dashed, theme::t().yellow);
            g.fixed(p2.0, p2.1, end_glyph(End::Arrow, (p2.0 - p1.0, p2.1 - p1.1)).unwrap_or('▶'), theme::t().yellow);
        }
    }
    // A marquee in progress: a dashed box from its anchor to wherever the mouse is now.
    if let Some(((ax, ay), (bx, by))) = scene.marquee {
        let (x0, x1) = (ax.min(bx) as i64, ax.max(bx) as i64);
        let (y0, y1) = (ay.min(by) as i64, ay.max(by) as i64);
        g.rect(x0, y0, x1, y1, Weight::Dashed, false, theme::t().bright);
    }

    for e in doc.elements_in_order() {
        if !doc.element_visible(e.id) || !e.outline {
            continue;
        }
        // An element overridden to braille is left for `canvas::Scene::render`'s secondary
        // pass — drawing it here would merge its outline into this Grid's own connectivity.
        if scene.ink_of(e) != Ink::Lines {
            continue;
        }
        let colour = scene.colour_of(e.id, e.kind.layer());
        shape(&mut g, e, page.shape(e.kind), colour);
        // The compartment's rule joins the box's sides.
        if let Some(ry) = e.header_rule() {
            g.hline(e.x as i64, e.right() as i64, ry as i64, Weight::of(e.drawn_line(), e.stroke), colour);
        }
    }

    // The page's edge, dashed.
    if page.page_view {
        let (w, h) = page.size();
        g.rect(0, 0, w as i64, h as i64, Weight::Dashed, false, theme::t().structure);
    }

    // The focused relation's own nodes, as marks — a shape's handles are `paint_handles`'s,
    // drawn once after either ink pass rather than here.
    if let Some(rid) = scene.focus_rel
        && let Some(r) = doc.relation(rid)
        && let Some(route) = doc.route(r)
    {
        for (node, t) in [(Node::Tail, 0.18), (Node::Centre, 0.5), (Node::Head, 0.82)] {
            let p = cell(shapes::along(&route, t));
            let on = node == scene.focus_node;
            g.fixed(p.0, p.1, if on { '◆' } else { '◇' }, if on { theme::t().yellow } else { theme::t().bright });
        }
    }
    splice(&g, area, buf);
}

/// A shape's own eight handles, and the hover arrows just outside them, painted once after
/// either ink pass and always in line art — never braille, even when the document's own ink
/// is. A control is not the diagram, it is the interface for editing it: it should stay the
/// clearest glyphs the terminal has, not follow a choice made for the sketch underneath it.
pub(super) fn paint_handles(scene: &Scene, area: Rect, buf: &mut Buffer) {
    let mut g = Grid::new(area.width, area.height, scene.camera);
    let doc = scene.doc;
    let cell = |p: Point| (p.0.round() as i64, p.1.round() as i64);
    // Outside a reshape, the hovered element's handles paint too, unfocused — how a mouse
    // finds them before it has clicked one.
    let handles_on = scene.reshape.map(|(id, on, held)| (id, Some(on), held)).or(scene.hover.map(|id| (id, None, false)));
    if let Some((id, on, held)) = handles_on
        && let Some(e) = doc.element(id)
    {
        for (i, h) in e.handles().into_iter().enumerate() {
            let patched = !doc.at_port(id, i).is_empty();
            let (ch, c) = match (Some(i) == on, held, patched) {
                (true, true, _) => ('◆', theme::t().green),
                (true, false, _) => ('◆', theme::t().yellow),
                (false, _, true) => ('●', theme::t().aqua),
                (false, _, false) => ('◇', theme::t().sand),
            };
            let p = cell(h);
            g.fixed(p.0, p.1, ch, c);
        }
        // A plain hover, not a reshape in hand: an arrow just outside each handle too,
        // faint — click one to add a new connected shape that way, the mouse's own `o`. The
        // one the mouse is actually over right now stays full-strength, so a diagonal
        // approach can tell which of the eight it is about to click before it does. The
        // corners are solid quadrant blocks, not thin arrows — a diagonal target is easy to
        // undershoot on a grid this coarse, and a filled wedge reads as "aim here" the way a
        // one-cell-wide arrowhead does not.
        if on.is_none() {
            const GLYPH: [char; 8] = ['◤', '▲', '◥', '▶', '◢', '▼', '◣', '◀'];
            let faint = theme::fade(theme::t().aqua, 65, theme::t().ground);
            for (i, a) in e.arrows(super::ARROW_GAP).into_iter().enumerate() {
                let p = cell(a);
                let c = if scene.hover_arrow == Some(i) { theme::t().aqua } else { faint };
                g.fixed(p.0, p.1, GLYPH[i], c);
            }
        }
    }
    splice(&g, area, buf);
}

/// One element only, in lines, when the rest of the scene is drawn in braille —
/// [`Element::ink`]'s override. A `Grid` the size of the whole area, but with only this one
/// shape's outline (and header rule) drawn into it, so nothing merges with a relation or
/// another element the way the whole-scene `Grid` above lets neighbours merge on purpose.
pub(super) fn paint_one(scene: &Scene, e: &Element, area: Rect, buf: &mut Buffer) {
    let mut g = Grid::new(area.width, area.height, scene.camera);
    let page = &scene.doc.metadata.page;
    let colour = scene.colour_of(e.id, e.kind.layer());
    shape(&mut g, e, page.shape(e.kind), colour);
    if let Some(ry) = e.header_rule() {
        g.hline(e.x as i64, e.right() as i64, ry as i64, Weight::of(e.drawn_line(), e.stroke), colour);
    }
    splice(&g, area, buf);
}

/// The Grid's glyphs, wherever it has one, into the buffer — the compositing step both the
/// whole-scene `paint` and the single-element `paint_one` share.
fn splice(g: &Grid, area: Rect, buf: &mut Buffer) {
    for y in 0..g.height {
        for x in 0..g.width {
            let c = g.get(x, y);
            if let Some(ch) = c.glyph() {
                let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
                let mut s = String::new();
                s.push(ch);
                cell.set_symbol(&s);
                if let Some(col) = c.colour {
                    cell.set_fg(col);
                }
            }
        }
    }
}

/// One shape, as its stencil where it has one and as its outline's segments where it does
/// not: every polygon draws itself through `segment`, the curves through their own hands.
fn shape(g: &mut Grid, e: &Element, shape: Shape, colour: Color) {
    let (x, y, r, b) = (e.x.round() as i64, e.y.round() as i64, e.right().round() as i64, e.bottom().round() as i64);
    let w = Weight::of(e.drawn_line(), e.stroke);
    // An interface is the abstract shape: double-ruled, whatever its stroke.
    let w = if e.kind == crate::ontology::ShapeKind::Interface { Weight::Double } else { w };
    match shape {
        Shape::Rectangle => g.rect(x, y, r, b, w, false, colour),
        Shape::RoundedRectangle => g.rect(x, y, r, b, w, true, colour),
        Shape::Dashed => g.rect(x, y, r, b, Weight::Dashed, true, colour),
        Shape::Ellipse => g.ellipse(x, y, r, b, w, colour),
        Shape::Cylinder => g.cylinder(x, y, r, b, w, colour),
        Shape::Cloud => g.cloud(x, y, r, b, w, colour),
        Shape::Diamond => g.diamond(x, y, r, b, w, colour),
        Shape::Text => {}
        _ => {
            // The outline's own geometry, unsketched: straight edges as segments, curved
            // pieces as the runs and posts and diagonals their points call for.
            for prim in shapes::outline(shape, e.x, e.y, e.w, e.h) {
                match prim {
                    CurvePrimitive::Lines(ls) => {
                        for (a, bb) in ls {
                            g.segment((a.0.round() as i64, a.1.round() as i64), (bb.0.round() as i64, bb.1.round() as i64), w, colour);
                        }
                    }
                    CurvePrimitive::Points(ps) => {
                        let cells: Vec<(i64, i64)> = ps.iter().map(|p| (p.0.round() as i64, p.1.round() as i64)).collect();
                        let mut last: Option<(i64, i64)> = None;
                        for c in cells {
                            if let Some(l) = last
                                && l != c
                                && (l.0 - c.0).abs() <= 2
                                && (l.1 - c.1).abs() <= 1
                            {
                                g.segment(l, c, w, colour);
                            }
                            last = Some(c);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump(g: &Grid) -> Vec<String> {
        (0..g.height).map(|y| (0..g.width).map(|x| g.get(x, y).glyph().unwrap_or(' ')).collect()).collect()
    }

    #[test]
    fn a_cell_s_arms_resolve_to_the_one_character_with_them() {
        let mut c = Cell::default();
        assert_eq!(c.glyph(), None);
        c.e = Weight::Light;
        c.s = Weight::Light;
        assert_eq!(c.glyph(), Some('┌'));
        c.round = true;
        assert_eq!(c.glyph(), Some('╭'));
        c.n = Weight::Light;
        assert_eq!(c.glyph(), Some('├'));
        c.w = Weight::Heavy;
        assert_eq!(c.glyph(), Some('╋'), "a junction takes its heaviest family");
        let mut run = Cell { e: Weight::Dashed, w: Weight::Dashed, ..Default::default() };
        assert_eq!(run.glyph(), Some('╌'));
        run.e = Weight::Double;
        run.w = Weight::Double;
        assert_eq!(run.glyph(), Some('═'));
        let d = Cell { ne: true, sw: true, ..Default::default() };
        assert_eq!(d.glyph(), Some('╱'));
        let f = Cell { fixed: Some('▶'), n: Weight::Light, ..Default::default() };
        assert_eq!(f.glyph(), Some('▶'), "a laid character wins");
    }

    #[test]
    fn a_box_closes_a_line_meeting_it_becomes_a_tee_and_a_stadium_reads_as_a_curve() {
        let mut g = Grid::new(20, 8, (0.0, 0.0));
        g.rect(1, 1, 8, 4, Weight::Light, false, theme::t().ink);
        g.hline(8, 14, 2, Weight::Light, theme::t().ink);
        let rows = dump(&g);
        let slice = |s: &str, from: usize, n: usize| s.chars().skip(from).take(n).collect::<String>();
        assert_eq!(slice(&rows[1], 1, 8), "┌──────┐");
        assert_eq!(slice(&rows[2], 1, 14), "│      ├──────", "{rows:?}");
        assert_eq!(slice(&rows[4], 1, 8), "└──────┘");
        let mut g = Grid::new(20, 8, (0.0, 0.0));
        g.ellipse(0, 0, 15, 5, Weight::Light, theme::t().ink);
        let rows = dump(&g);
        assert!(rows[0].trim().starts_with('╭') && rows[0].trim().ends_with('╮'), "{rows:?}");
        assert!(rows[5].trim().starts_with('╰') && rows[5].trim().ends_with('╯'), "{rows:?}");
        assert!(rows[2].starts_with('│') || rows[2].starts_with('╭') || rows[2].starts_with('╰'), "the widest row reaches the left edge: {rows:?}");
        for row in &rows[..6] {
            assert!(!row.trim().is_empty(), "every row of a stadium has an edge: {rows:?}");
        }
        let mut g = Grid::new(20, 8, (0.0, 0.0));
        g.cloud(0, 0, 17, 5, Weight::Light, theme::t().ink);
        let rows = dump(&g);
        assert!(rows[0].matches('╭').count() >= 2 && rows[0].matches('╮').count() >= 2, "scallops on top: {rows:?}");
        assert!(rows[1].contains("╰╯"), "a valley between bumps: {rows:?}");
    }

    #[test]
    fn a_segment_takes_the_glyph_its_slope_asks_for() {
        let mut g = Grid::new(20, 8, (0.0, 0.0));
        g.segment((0, 0), (5, 5), Weight::Light, theme::t().ink);
        let rows = dump(&g);
        assert_eq!(rows[0].chars().next(), Some('╲'));
        assert_eq!(rows[5].chars().nth(5), Some('╲'));
        let mut g = Grid::new(20, 8, (0.0, 0.0));
        g.segment((0, 0), (12, 1), Weight::Light, theme::t().ink);
        let rows = dump(&g);
        assert!(rows[0].contains("───") && rows[1].contains("───"), "a flat slope is two runs and a step: {rows:?}");
        assert!(rows[0].contains('╮') && rows[1].contains('╰'), "with a rounded elbow: {rows:?}");
        let mut g = Grid::new(20, 8, (0.0, 0.0));
        g.segment((0, 3), (9, 0), Weight::Light, theme::t().ink);
        let rows = dump(&g);
        assert!(rows[3].starts_with("╱─") || rows[3].starts_with("╱"), "a rising line: the slash first, then its run: {rows:?}");
        let mut g = Grid::new(20, 8, (0.0, 0.0));
        g.segment((0, 0), (9, 3), Weight::Light, theme::t().ink);
        let rows = dump(&g);
        assert!(rows[0].trim_end().ends_with('╲'), "a falling line: the run first, then its slash: {rows:?}");
        let mut g = Grid::new(20, 8, (0.0, 0.0));
        g.diamond(0, 0, 12, 6, Weight::Light, theme::t().ink);
        let rows = dump(&g);
        assert!(rows[0].contains("╱╲"), "the apex: {rows:?}");
        assert!(rows[6].contains("╲╱"), "the foot: {rows:?}");
        assert!(!rows.iter().any(|r| r.contains('╳')), "no crossings at the vertices: {rows:?}");
        assert_eq!(end_glyph(End::Arrow, (1, 0)), Some('▶'));
        assert_eq!(end_glyph(End::Crow, (0, 1)), Some('⋎'));
        assert_eq!(end_glyph(End::Bar, (0, 1)), Some('┴'), "a bar under a line arriving from above");
        assert_eq!(end_glyph(End::Bar, (-1, 0)), Some('├'));
        assert_eq!(Ink::parse("wire"), Some(Ink::Lines));
    }
}
