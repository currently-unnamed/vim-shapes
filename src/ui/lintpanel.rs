//! The header's `⚠ N` badge, expanded: every relation the rules refuse, in the same order
//! `Document::lint()` gives them, each with its reason in full — the sentence `allowed`
//! wrote to be read at the moment it can change a mind, not truncated to fit one status line.
//!
//! A row is two things wide open at once: a header naming the relation (its kind, and the
//! two elements it joins) and its `why`, word-wrapped rather than clipped — some reasons run
//! long, and a reason too short to read is a reason nobody reads. `enter` or a click jumps to
//! it, the same landing `:lint`'s own jump-to-first already uses, so a row picked here and a
//! row jumped to by ex-command can never disagree about where "look at this one" means.
//!
//! Opened three ways — `gl`, `:lint`, or a click on the badge — and drawn as a dropdown at
//! whichever point it opened from: `at` is `Some` for a click, `None` for the keyboard, in
//! which case [`State::area`] anchors under the badge itself instead.

use super::{chrome, theme};
use crate::model::{Document, Problem, RelationId};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub const WIDTH: u16 = 60;

/// The text width a row's `why` wraps to — `WIDTH` less the panel's own column of margin
/// either side ([`chrome::panel`]'s own `- 2`).
const TEXT_WIDTH: usize = (WIDTH - 2) as usize;

/// One refused relation, named for the header line, with its reason kept whole — the panel's
/// width is fixed, so there is only one wrapping to do, and it happens at render time.
pub struct Row {
    pub relation: RelationId,
    /// "composition: Pay → db" — the kind and the two elements it joins, named the way
    /// `:lint`'s own status line names them, so the two can never disagree.
    pub header: String,
    pub why: &'static str,
}

impl Row {
    fn new(doc: &Document, p: &Problem) -> Row {
        let (kind, from, to) = match doc.relation(p.relation) {
            Some(r) => {
                let name = |id| doc.element(id).map(|e| e.display()).unwrap_or_else(|| "?".into());
                (r.kind.name(), name(r.from), name(r.to))
            }
            None => ("relation", "?".into(), "?".into()),
        };
        Row { relation: p.relation, header: format!("{kind}: {from} → {to}"), why: p.why }
    }
}

pub struct State {
    pub rows: Vec<Row>,
    pub sel: usize,
    /// The screen point it was opened at, for a click — the same idea as [`super::ctxmenu`]'s
    /// own `at`. `None` when opened from the keyboard: [`State::area`] anchors under the
    /// badge instead, since there is no click point to open at.
    pub at: Option<(u16, u16)>,
}

impl State {
    pub fn new(doc: &Document, problems: Vec<Problem>, at: Option<(u16, u16)>) -> State {
        let rows = problems.iter().map(|p| Row::new(doc, p)).collect();
        State { rows, sel: 0, at }
    }

    /// Where it draws: at `self.at` if it opened on a click, or just under the badge — or,
    /// failing that (opened from the keyboard before the badge has ever drawn a frame), the
    /// header's own corner.
    pub fn area(&self, bound: Rect, badge: Option<Rect>) -> Rect {
        let at = self.at.or_else(|| badge.map(|b| (b.x, b.bottom()))).unwrap_or((bound.x, bound.y));
        chrome::near(bound, at, WIDTH, height(&self.rows, bound.height))
    }

    /// Which row a screen point lands on — not the title bar, not the hint line, not a blank
    /// separator between rows, not past the last one. `area` is this panel's own rect, from
    /// [`State::area`].
    pub fn row_at(&self, area: Rect, col: u16, row: u16) -> Option<usize> {
        let body = chrome::panel_body(area, true);
        if col < body.x || col >= body.right() || row < body.y || row >= body.bottom() {
            return None;
        }
        let flat = flatten(&self.rows);
        let view = body.height as usize;
        let off = scroll(&flat, self.sel, view);
        match flat.get(off + (row - body.y) as usize) {
            Some((_, Piece::Blank)) | None => None,
            Some((i, _)) => Some(*i),
        }
    }

    pub fn key(&mut self, k: KeyEvent) -> Outcome {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => Outcome::Cancel,
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.rows.is_empty() {
                    self.sel = (self.sel + 1) % self.rows.len();
                }
                Outcome::Nothing
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !self.rows.is_empty() {
                    self.sel = (self.sel + self.rows.len() - 1) % self.rows.len();
                }
                Outcome::Nothing
            }
            KeyCode::Enter if !self.rows.is_empty() => Outcome::Jump(self.rows[self.sel].relation),
            _ => Outcome::Nothing,
        }
    }
}

#[derive(Clone, Copy)]
pub enum Outcome {
    Nothing,
    Cancel,
    Jump(RelationId),
}

/// One buffer line, and which row it belongs to.
enum Piece {
    Header,
    Why(String),
    /// The gap between one row's reason and the next row's header — its own line so a long
    /// reason never reads as running straight into the row after it.
    Blank,
}

/// Every row flattened to the buffer lines it actually draws — a header, its `why` wrapped to
/// [`TEXT_WIDTH`], then a blank line except after the last row. One list, so the widget's own
/// rendering and a mouse hit-test read the identical geometry rather than two arithmetics that
/// could drift apart.
fn flatten(rows: &[Row]) -> Vec<(usize, Piece)> {
    let mut out = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        out.push((i, Piece::Header));
        out.extend(chrome::wrap(r.why, TEXT_WIDTH).into_iter().map(|w| (i, Piece::Why(w))));
        if i + 1 < rows.len() {
            out.push((i, Piece::Blank));
        }
    }
    out
}

/// The first flattened line to show, given how many fit — the same centring [`super::ctxmenu`]
/// does, just in wrapped-line units rather than one-line-per-row ones.
fn scroll(flat: &[(usize, Piece)], sel: usize, view: usize) -> usize {
    if view == 0 {
        return 0;
    }
    let cur = flat.iter().position(|(i, _)| *i == sel).unwrap_or(0);
    cur.saturating_sub(view / 2).min(flat.len().saturating_sub(view))
}

/// How tall the panel should be: every flattened line, plus the title strip and hint line,
/// clamped to what's actually available.
pub fn height(rows: &[Row], avail: u16) -> u16 {
    (flatten(rows).len() as u16 + 2).clamp(3, avail.max(3))
}

pub struct Dialog<'a> {
    pub state: &'a State,
}

impl Widget for Dialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let title = format!("rules refuse — {} relation{}", s.rows.len(), if s.rows.len() == 1 { "" } else { "s" });
        let inner = chrome::panel(buf, area, &title, theme::t().red);
        let hint = " j/k or hover   enter or click jumps to it   esc/q closes";
        let body = chrome::hint(buf, inner, hint);
        if body.height == 0 {
            return;
        }
        if s.rows.is_empty() {
            Paragraph::new(Line::styled(" nothing the rules refuse", Style::new().fg(theme::t().dim))).render(body, buf);
            return;
        }
        let flat = flatten(&s.rows);
        let view = body.height as usize;
        let off = scroll(&flat, s.sel, view);
        let lines: Vec<Line> = flat
            .iter()
            .skip(off)
            .take(view)
            .map(|(i, piece)| {
                let picked = *i == s.sel;
                match piece {
                    Piece::Header => {
                        let text = format!("{}{}", chrome::marker(picked), s.rows[*i].header);
                        let style = if picked {
                            Style::new().fg(theme::t().inverse).bg(theme::t().red).bold()
                        } else {
                            Style::new().fg(theme::t().bright).bold()
                        };
                        Line::styled(text, style)
                    }
                    Piece::Why(w) => {
                        let style = if picked { Style::new().fg(theme::t().inverse).bg(theme::t().red) } else { Style::new().fg(theme::t().dim) };
                        Line::styled(format!("  {w}"), style)
                    }
                    Piece::Blank => Line::raw(""),
                }
            })
            .collect();
        Paragraph::new(lines).render(body, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::{RelationKind, ShapeKind};

    fn sample() -> (Document, Vec<Problem>) {
        let mut doc = Document::default();
        let p = doc.add(ShapeKind::BusinessProcess, "Pay", 0.0, 0.0);
        let n = doc.add(ShapeKind::Node, "db", 40.0, 0.0);
        doc.connect(RelationKind::Composition, p, n).unwrap();
        let problems = doc.lint();
        (doc, problems)
    }

    #[test]
    fn every_problem_becomes_a_row_naming_its_relation_and_carrying_its_reason_whole() {
        let (doc, problems) = sample();
        let st = State::new(&doc, problems.clone(), None);
        assert_eq!(st.rows.len(), 1);
        assert_eq!(st.rows[0].relation, problems[0].relation);
        assert_eq!(st.rows[0].why, problems[0].why);
        assert!(st.rows[0].header.contains("Pay") && st.rows[0].header.contains("db"), "{}", st.rows[0].header);
    }

    #[test]
    fn enter_jumps_to_the_selected_row_and_esc_or_q_cancels() {
        let (doc, problems) = sample();
        let rid = problems[0].relation;
        let mut st = State::new(&doc, problems, None);
        match st.key(KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE)) {
            Outcome::Jump(r) => assert_eq!(r, rid),
            _ => panic!("expected a jump"),
        }
        assert!(matches!(st.key(KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE)), Outcome::Cancel));
        assert!(matches!(st.key(KeyEvent::new(KeyCode::Char('q'), crossterm::event::KeyModifiers::NONE)), Outcome::Cancel));
    }

    #[test]
    fn a_long_reason_wraps_rather_than_clips_and_a_click_on_any_of_its_lines_still_finds_the_row() {
        let mut doc = Document::default();
        // Same layer, different category — the composition rule's longest reason, well past
        // `TEXT_WIDTH`, is the one that actually exercises wrapping.
        let a = doc.add(ShapeKind::BusinessActor, "A", 0.0, 0.0);
        let b = doc.add(ShapeKind::BusinessProcess, "B", 40.0, 0.0);
        doc.connect(RelationKind::Composition, a, b).unwrap();
        let problems = doc.lint();
        let why = problems[0].why;
        assert!(why.chars().count() > TEXT_WIDTH, "the fixture needs a reason long enough to wrap");
        let st = State::new(&doc, problems, None);
        let full = Rect::new(0, 0, 100, 30);
        let area = st.area(full, None);
        assert!(area.height >= 5, "a title strip, a wrapped header block, and a hint line: {area:?}");
        // Every buffer row inside the body that isn't the blank separator must resolve back
        // to row 0 — there is only the one row.
        let body = chrome::panel_body(area, true);
        for row in body.y..body.bottom() {
            if let Some(i) = st.row_at(area, body.x, row) {
                assert_eq!(i, 0);
            }
        }
    }

    #[test]
    fn it_opens_at_a_click_point_but_under_the_badge_from_the_keyboard() {
        let (doc, problems) = sample();
        let full = Rect::new(0, 0, 200, 30);
        let clicked = State::new(&doc, problems.clone(), Some((10, 5)));
        assert_eq!((clicked.area(full, None).x, clicked.area(full, None).y), (10, 5));
        let from_keyboard = State::new(&doc, problems, None);
        let badge = Rect::new(70, 0, 6, 1);
        let area = from_keyboard.area(full, Some(badge));
        assert_eq!((area.x, area.y), (badge.x, badge.bottom()), "anchored just under the badge, not centred");
    }
}
