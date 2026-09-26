//! The add palette — `:add` with no name. A search, not a list.
//!
//! Every letter typed is a query, so the arrows — not `j`/`k` — move the selection. It matches
//! on what a kind *is* as well as on its name, because the thing you cannot remember is never
//! the term: typing `server` finds Node, typing `database` finds Data Object. Rows are grouped
//! by layer, and only the layers the diagram's view is about are offered — a menu of
//! possibilities rather than of choices. `:add <kind>` skips the search entirely.

use super::{chrome, theme};
use crate::ontology::{self, Layer, RelationKind, ShapeKind, View};
use crate::shapes::{self, CurvePrimitive};
use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line as CLine, Points};
use ratatui::widgets::Paragraph;

/// How tall the picked kind's picture is, in rows.
const PICTURE_ROWS: u16 = 5;

/// The picked kind as a braille picture: its outline at its own proportions, in its layer's
/// colour — what the shape will look like on the diagram, before it is added.
fn picture(kind: ShapeKind, width: u16) -> impl Widget {
    let (w, h) = kind.default_size();
    // Fit the shape to the picture's box with a cell of margin, keeping its proportions.
    let (bw, bh) = (width as f64, PICTURE_ROWS as f64);
    let scale = ((bw - 2.0) / w).min((bh - 1.0) / h).min(1.0);
    let (sw, sh) = (w * scale, h * scale);
    let (x, y) = ((bw - sw) / 2.0, (bh - sh) / 2.0);
    let colour = theme::layer_color(kind.layer());
    Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([0.0, bw])
        .y_bounds([0.0, bh])
        .paint(move |ctx| {
            let flip = |yy: f64| bh - yy;
            for prim in shapes::outline(kind.shape(), x, y, sw, sh) {
                match prim {
                    CurvePrimitive::Points(pts) => {
                        let p: Vec<(f64, f64)> = pts.into_iter().map(|(px, py)| (px, flip(py))).collect();
                        ctx.draw(&Points { coords: &p, color: colour });
                    }
                    CurvePrimitive::Lines(ls) => {
                        for (a, b) in ls {
                            ctx.draw(&CLine { x1: a.0, y1: flip(a.1), x2: b.0, y2: flip(b.1), color: colour });
                        }
                    }
                }
            }
        })
}

pub struct State {
    /// Index into `rows()`, *not* into `ShapeKind::ALL` — the list under the cursor is the
    /// filtered one, so that is the list the cursor indexes.
    pub sel: usize,
    pub filter: String,
    pub view: View,
    pub purpose: Purpose,
    /// Opened out of a handle: which of the eight directions the new shape goes, as a handle
    /// index (clockwise from top-left). Shown as a compass, and changeable before confirming.
    pub dir: Option<usize>,
    /// The compass has the keys: hjkl and yubn move the direction instead of typing.
    pub compass: bool,
}

/// A direction's key, arrow and name, by handle index.
pub const DIRS: [(char, &str, &str); 8] = [
    ('y', "↖", "up-left"),
    ('k', "↑", "up"),
    ('u', "↗", "up-right"),
    ('l', "→", "right"),
    ('n', "↘", "down-right"),
    ('j', "↓", "down"),
    ('b', "↙", "down-left"),
    ('h', "←", "left"),
];

/// The handle a direction key names, if it is one.
pub fn dir_of(c: char) -> Option<usize> {
    DIRS.iter().position(|(k, _, _)| *k == c.to_ascii_lowercase())
}

/// Why the palette is open — and therefore what it says about each row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    /// Adding a shape.
    Add,
    /// Opening a new shape off `from`, related to it. Every kind is still offered, but the
    /// ones that can take only an association from here are dimmed: a menu of possibilities.
    Relate(ShapeKind),
}

impl State {
    pub fn new(view: View) -> State {
        State { sel: 0, filter: String::new(), view, purpose: Purpose::Add, dir: None, compass: false }
    }

    pub fn relating(view: View, from: ShapeKind, dir: Option<usize>) -> State {
        State { purpose: Purpose::Relate(from), dir, ..State::new(view) }
    }

    /// Whether a specific relation — not merely an association or a link — can run from
    /// the shape this palette was opened off to a shape of kind `k`.
    pub fn specific(&self, k: ShapeKind) -> bool {
        match self.purpose {
            Purpose::Add => true,
            Purpose::Relate(from) => RelationKind::ALL
                .iter()
                .any(|r| !matches!(r, RelationKind::Association | RelationKind::Link) && ontology::allowed(*r, from, k).is_ok()),
        }
    }

    /// The kinds this palette is showing, filtered and in layer order.
    pub fn rows(&self) -> Vec<ShapeKind> {
        let f = self.filter.trim().to_ascii_lowercase();
        ShapeKind::ALL
            .into_iter()
            .filter(|k| self.view.shows(*k))
            .filter(|k| {
                f.is_empty()
                    || k.name().to_ascii_lowercase().contains(&f)
                    || k.slug().contains(&f)
                    || k.short().contains(&f)
                    || k.tagline().to_ascii_lowercase().contains(&f)
                    || k.layer().name().contains(&f)
            })
            .collect()
    }

    pub fn picked(&self) -> Option<ShapeKind> {
        self.rows().get(self.sel).copied()
    }

    pub fn move_by(&mut self, delta: isize) {
        let n = self.rows().len();
        if n == 0 {
            self.sel = 0;
            return;
        }
        self.sel = (self.sel as isize + delta).rem_euclid(n as isize) as usize;
    }

    pub fn retype(&mut self, f: impl FnOnce(&mut String)) {
        f(&mut self.filter);
        self.sel = 0;
    }
}

pub const WIDTH: u16 = 100;

pub fn height(avail: u16) -> u16 {
    avail.saturating_sub(2).clamp(8, 32)
}

enum Row {
    Head(Layer),
    Item(usize, ShapeKind),
}

fn rows(state: &State) -> Vec<Row> {
    let mut out = Vec::new();
    let mut layer = None;
    for (i, k) in state.rows().into_iter().enumerate() {
        if layer != Some(k.layer()) {
            layer = Some(k.layer());
            out.push(Row::Head(k.layer()));
        }
        out.push(Row::Item(i, k));
    }
    out
}

pub struct Palette<'a> {
    pub state: &'a State,
}

impl Widget for Palette<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = match self.state.purpose {
            Purpose::Add => format!("add — kinds of shapes · {} view", self.state.view.name()),
            Purpose::Relate(from) => format!("open a shape off {} — kinds of shapes · {} view", from.name().to_ascii_lowercase(), self.state.view.name()),
        };
        let inner = chrome::panel(buf, area, &title, theme::t().green);
        if inner.height < 4 || inner.width < 30 {
            return;
        }
        let hint = match (self.state.purpose, self.state.dir.is_some(), self.state.compass) {
            (Purpose::Add, ..) => " type to search   ↑/↓ pick   enter add   esc",
            (_, true, true) => " hjkl / yubn change the direction   tab back to the search   enter add and relate   esc",
            (_, true, false) => " type to search   ↑/↓ pick   ^hjkl / ^yubn or tab: the direction   enter add and relate   esc",
            (Purpose::Relate(_), ..) => " type to search   ↑/↓ pick   enter add and relate   esc   dimmed: only an association from here",
        };
        let body = chrome::hint(buf, inner, hint);
        // The search line on top.
        let query = Line::from(vec![
            Span::styled(" > ", Style::new().fg(theme::t().green).bold()),
            Span::styled(self.state.filter.clone(), Style::new().fg(theme::t().ink)),
            Span::styled("█", Style::new().fg(theme::t().green)),
        ]);
        Paragraph::new(query).render(Rect { height: 1, ..body }, buf);
        let body = Rect { y: body.y + 1, height: body.height.saturating_sub(1), ..body };

        // Two columns: the list, and what the picked kind is.
        let list_w = (body.width * 11 / 20).clamp(24, 52);
        let list = Rect { width: list_w, ..body };
        let detail = Rect { x: body.x + list_w + 2, width: body.width.saturating_sub(list_w + 2), ..body };

        let all = rows(self.state);
        let view = list.height as usize;
        let cur = all
            .iter()
            .position(|r| matches!(r, Row::Item(i, _) if *i == self.state.sel))
            .unwrap_or(0);
        let max_off = all.len().saturating_sub(view);
        let off = cur.saturating_sub(view / 2).min(max_off);
        let mut lines: Vec<Line> = Vec::new();
        if all.is_empty() {
            lines.push(Line::styled(
                format!(" nothing matches {:?} in a {} view", self.state.filter, self.state.view.name()),
                Style::new().fg(theme::t().red),
            ));
        }
        for row in all.iter().skip(off).take(view) {
            lines.push(match row {
                Row::Head(l) => Line::styled(
                    format!(" ── {} ", l.name()),
                    Style::new().fg(theme::layer_color(*l)).italic(),
                ),
                Row::Item(i, k) => {
                    let on = *i == self.state.sel;
                    let name = format!("{}{:<22}", chrome::marker(on), k.name());
                    let room = (list.width as usize).saturating_sub(name.chars().count());
                    let tag: String = k.tagline().chars().take(room).collect();
                    let (ns, ts) = if on {
                        (Style::new().fg(theme::t().inverse).bg(theme::t().green).bold(), Style::new().fg(theme::t().ink))
                    } else if !self.state.specific(*k) {
                        (Style::new().fg(theme::t().dim), Style::new().fg(theme::t().dim))
                    } else {
                        (Style::new().fg(theme::layer_color(k.layer())), Style::new().fg(theme::t().dim))
                    };
                    Line::from(vec![Span::styled(name, ns), Span::styled(tag, ts)])
                }
            });
        }
        Paragraph::new(lines).render(list, buf);

        // The compass: where the new shape goes, changeable before it is added.
        let detail = match self.state.dir {
            Some(dir) => {
                let on = self.state.compass;
                let title = Style::new().fg(if on { theme::t().green } else { theme::t().dim });
                buf.set_stringn(detail.x, detail.y, format!("{}direction   {}", chrome::marker(on), DIRS[dir].2), detail.width as usize, title);
                let cells = [[0usize, 1, 2], [7, 8, 3], [6, 5, 4]];
                for (row, line) in cells.iter().enumerate() {
                    let y = detail.y + 1 + row as u16;
                    if y >= detail.bottom() {
                        break;
                    }
                    for (col, &i) in line.iter().enumerate() {
                        let x = detail.x + 1 + col as u16 * 4;
                        let (glyph, style) = if i == 8 {
                            ("·", Style::new().fg(theme::t().dim))
                        } else if i == dir {
                            (DIRS[i].1, Style::new().fg(theme::t().inverse).bg(if on { theme::t().green } else { theme::t().yellow }).bold())
                        } else {
                            (DIRS[i].1, Style::new().fg(if on { theme::t().bright } else { theme::t().dim }))
                        };
                        buf.set_stringn(x, y, format!(" {glyph} "), 3, style);
                    }
                }
                Rect { y: detail.y + 5, height: detail.height.saturating_sub(5), ..detail }
            }
            None => detail,
        };
        if let Some(k) = self.state.picked() {
            let head: Vec<Line> = vec![
                Line::styled(k.name(), Style::new().fg(theme::layer_color(k.layer())).bold()),
                Line::styled(
                    format!("{} · {} · {}", k.layer().name(), k.category().name(), k.shape().name()),
                    Style::new().fg(theme::t().dim),
                ),
            ];
            Paragraph::new(head).render(Rect { height: 2.min(detail.height), ..detail }, buf);
            // The picture, when there is room for it and the words below.
            let mut rest = Rect { y: detail.y + 2, height: detail.height.saturating_sub(2), ..detail };
            if rest.height >= PICTURE_ROWS + 3 {
                picture(k, rest.width).render(Rect { height: PICTURE_ROWS, ..rest }, buf);
                rest = Rect { y: rest.y + PICTURE_ROWS, height: rest.height - PICTURE_ROWS, ..rest };
            }
            let mut d: Vec<Line> = vec![Line::raw("")];
            for l in chrome::wrap(k.summary(), detail.width as usize) {
                d.push(Line::styled(l, Style::new().fg(theme::t().muted)));
            }
            d.push(Line::raw(""));
            d.push(Line::styled(":help ".to_string() + k.slug(), Style::new().fg(theme::t().aqua)));
            Paragraph::new(d).render(rest, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn the_palette_shows_a_picture_of_the_picked_kind_in_its_layer_s_colour() {
        let mut s = State::new(View::Freeform);
        s.retype(|f| f.push_str("ellipse"));
        let k = s.picked().expect("a kind matched");
        let mut term = Terminal::new(TestBackend::new(100, 24)).unwrap();
        term.draw(|f| f.render_widget(Palette { state: &s }, f.area())).unwrap();
        let buf = term.backend().buffer();
        let braille: Vec<&ratatui::buffer::Cell> = buf.content.iter().filter(|c| c.symbol().chars().next().is_some_and(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch))).collect();
        assert!(braille.len() > 12, "an outline in braille: {} cells", braille.len());
        assert!(braille.iter().all(|c| c.fg == theme::layer_color(k.layer())), "in the layer's colour");
        // Too short for a picture and the words: the words win, and no picture is drawn.
        let mut term = Terminal::new(TestBackend::new(100, 9)).unwrap();
        term.draw(|f| f.render_widget(Palette { state: &s }, f.area())).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(!out.chars().any(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch)) && out.contains("Circle"), "{out}");
    }

    #[test]
    fn the_palette_matches_on_what_a_kind_is_not_only_its_name() {
        let mut s = State::new(View::Free);
        s.retype(|f| f.push_str("server"));
        assert!(s.rows().contains(&ShapeKind::Node), "typing server finds Node");
        s.filter = "database".into();
        assert!(s.rows().contains(&ShapeKind::DataObject));
    }

    #[test]
    fn a_view_narrows_what_the_palette_offers() {
        let s = State::new(View::Technology);
        let rows = s.rows();
        assert!(rows.contains(&ShapeKind::Node));
        assert!(!rows.contains(&ShapeKind::BusinessActor));
        assert!(rows.contains(&ShapeKind::Grouping));
    }

    #[test]
    fn the_direction_keys_name_the_eight_handles() {
        assert_eq!(dir_of('l'), Some(3));
        assert_eq!(dir_of('Y'), Some(0));
        assert_eq!(dir_of('n'), Some(4));
        assert_eq!(dir_of('q'), None);
    }

    #[test]
    fn relating_dims_the_kinds_that_can_only_be_associated_from_here() {
        let s = State::relating(View::Free, ShapeKind::ApplicationComponent, None);
        assert!(s.specific(ShapeKind::ApplicationService), "a component realizes a service");
        assert!(!s.specific(ShapeKind::Stakeholder), "a component and a stakeholder can only be associated");
        assert!(State::new(View::Free).specific(ShapeKind::Stakeholder), "adding dims nothing");
    }

    #[test]
    fn the_selection_wraps_and_survives_an_empty_filter() {
        let mut s = State::new(View::Free);
        s.move_by(-1);
        assert_eq!(s.sel, s.rows().len() - 1);
        s.retype(|f| f.push_str("zzzz"));
        s.move_by(1);
        assert_eq!(s.sel, 0);
        assert_eq!(s.picked(), None);
    }
}
