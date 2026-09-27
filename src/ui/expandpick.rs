//! The expand picker — `e` on an ontology element lists what it is really connected to in
//! the export that is not on this diagram yet.
//!
//! Not `relpick`: that picks a relation *kind* for a line already being drawn between two
//! shapes that already exist. This picks *which real connection* to pull onto the canvas in
//! the first place — the row is the fact, not a choice of how to draw it, since the ontology
//! itself already says the verb, the cardinality, the labels. One purpose, so its own small
//! dialog, the same call `importdlg` made over folding a third mode into `start`.

use super::{chrome, theme};
use crate::foundry_import::Edge;
use crate::ontology::ShapeKind;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub const WIDTH: u16 = 64;

pub fn height(rows: usize) -> u16 {
    (rows as u16 + 4).clamp(6, 20)
}

/// One connection this element could grow by, and what its far end already is (or would
/// become) — enough to render a row without `expandpick` knowing anything about the ontology
/// itself.
pub struct Row {
    pub edge: Edge,
    pub other_kind: ShapeKind,
    pub other_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// Add just the row at this index.
    One(usize),
    /// Add every row shown.
    All,
    Cancel,
}

pub struct State {
    pub rows: Vec<Row>,
    pub sel: usize,
}

impl State {
    pub fn new(rows: Vec<Row>) -> State {
        State { rows, sel: 0 }
    }

    pub fn key(&mut self, k: KeyEvent) -> Outcome {
        match k.code {
            KeyCode::Esc => Outcome::Cancel,
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
            KeyCode::Char('g') => {
                self.sel = 0;
                Outcome::Nothing
            }
            KeyCode::Char('G') => {
                self.sel = self.rows.len().saturating_sub(1);
                Outcome::Nothing
            }
            KeyCode::Enter if !self.rows.is_empty() => Outcome::One(self.sel),
            KeyCode::Char('a') | KeyCode::Char('A') if !self.rows.is_empty() => Outcome::All,
            _ => Outcome::Nothing,
        }
    }
}

/// A row, read aloud: "creates → Order (object type)".
fn describe(row: &Row) -> String {
    format!("{} {} ({})", row.edge.kind.verb(), row.other_label, row.other_kind.name())
}

pub struct Dialog<'a> {
    pub state: &'a State,
}

impl Widget for Dialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let inner = chrome::panel(buf, area, "expand — what this connects to", theme::t().aqua);
        let hint = " j/k pick   enter adds it   a adds all shown   esc";
        let body = chrome::hint(buf, inner, hint);
        if body.height == 0 {
            return;
        }
        if s.rows.is_empty() {
            Paragraph::new(Line::styled(" nothing left to add", Style::new().fg(theme::t().dim))).render(body, buf);
            return;
        }
        let view = body.height as usize;
        let off = s.sel.saturating_sub(view / 2).min(s.rows.len().saturating_sub(view));
        let lines: Vec<Line> = s
            .rows
            .iter()
            .enumerate()
            .skip(off)
            .take(view)
            .map(|(i, row)| {
                let picked = i == s.sel;
                let text = format!("{}{}", chrome::marker(picked), describe(row));
                let style = if picked { Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold() } else { Style::new().fg(theme::t().bright) };
                Line::styled(text, style)
            })
            .collect();
        Paragraph::new(lines).render(body, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::RelationKind;
    use crossterm::event::KeyModifiers;

    fn row(kind: RelationKind, to: &str) -> Row {
        Row { edge: Edge { kind, from: "src".into(), to: to.into(), tail_label: None, head_label: None, center_label: None, many_to_many: false, fk_property_api_name: None }, other_kind: ShapeKind::ObjectType, other_label: to.into() }
    }

    fn press(s: &mut State, code: KeyCode) -> Outcome {
        s.key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn moving_and_picking_one() {
        let mut s = State::new(vec![row(RelationKind::LinkType, "a"), row(RelationKind::Creates, "b")]);
        assert_eq!(press(&mut s, KeyCode::Char('j')), Outcome::Nothing);
        assert_eq!(s.sel, 1);
        assert_eq!(press(&mut s, KeyCode::Enter), Outcome::One(1));
    }

    #[test]
    fn a_adds_everything_shown() {
        let mut s = State::new(vec![row(RelationKind::LinkType, "a"), row(RelationKind::Creates, "b")]);
        assert_eq!(press(&mut s, KeyCode::Char('a')), Outcome::All);
    }

    #[test]
    fn esc_cancels_and_an_empty_list_adds_nothing() {
        let mut s = State::new(vec![]);
        assert_eq!(press(&mut s, KeyCode::Enter), Outcome::Nothing, "nothing to add");
        assert_eq!(press(&mut s, KeyCode::Esc), Outcome::Cancel);
    }
}
