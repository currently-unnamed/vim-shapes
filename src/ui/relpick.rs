//! The relation picker — asked the moment a relation is dropped, and again by `r`.
//!
//! It lists every kind, always: the ones the rules allow between these two elements first and
//! in full colour, the rest dimmed with the reason beside them. The rules are advisory, so a
//! dimmed row can still be chosen — the relation is drawn and marked, and `:lint` will list it
//! — but the reason is on screen at the one moment it can change a mind.

use super::{chrome, theme};
use crate::model::{Document, ElementId, RelationId};
use crate::ontology::{self, RelationKind};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub struct State {
    pub from: ElementId,
    pub to: ElementId,
    /// The relation being re-kinded, if this is `r` rather than a drop.
    pub existing: Option<RelationId>,
    pub sel: usize,
    /// Every kind, allowed first, each with the rules' verdict.
    pub rows: Vec<(RelationKind, Result<(), &'static str>)>,
}

impl State {
    pub fn new(doc: &Document, from: ElementId, to: ElementId, existing: Option<RelationId>) -> State {
        let (a, b) = (doc.element(from).map(|e| e.kind), doc.element(to).map(|e| e.kind));
        let mut rows: Vec<(RelationKind, Result<(), &'static str>)> = RelationKind::ALL
            .into_iter()
            .map(|r| {
                let verdict = match (a, b) {
                    (Some(a), Some(b)) => ontology::allowed(r, a, b),
                    _ => Err("no such element"),
                };
                (r, verdict)
            })
            .collect();
        // Stable, so within each half the picker keeps the table's own order.
        rows.sort_by_key(|(_, v)| v.is_err());
        // Re-kinding starts on the kind it already is; a new one starts on the best allowed.
        let sel = existing
            .and_then(|id| doc.relation(id))
            .and_then(|r| rows.iter().position(|(k, _)| *k == r.kind))
            .unwrap_or(0);
        State { from, to, existing, sel, rows }
    }

    pub fn picked(&self) -> Option<(RelationKind, Result<(), &'static str>)> {
        self.rows.get(self.sel).copied()
    }

    pub fn move_by(&mut self, delta: isize) {
        let n = self.rows.len();
        self.sel = (self.sel as isize + delta).rem_euclid(n as isize) as usize;
    }
}

pub const WIDTH: u16 = 96;

pub fn height(avail: u16) -> u16 {
    ((RelationKind::ALL.len() + 3) as u16).min(avail)
}

pub struct Picker<'a> {
    pub state: &'a State,
    pub doc: &'a Document,
}

impl Widget for Picker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let name = |id| self.doc.element(id).map(|e| e.display()).unwrap_or_else(|| "?".into());
        let title = format!("relation — {} → {}", name(self.state.from), name(self.state.to));
        let inner = chrome::panel(buf, area, &title, theme::t().yellow);
        let body = chrome::hint(buf, inner, " j/k pick   enter draw   esc   dimmed: the rules refuse it — drawn anyway, and marked");
        let cols = body.width as usize;
        let lines: Vec<Line> = self
            .state
            .rows
            .iter()
            .enumerate()
            .map(|(i, (k, verdict))| {
                let on = i == self.state.sel;
                let head = format!("{}{:<15}", chrome::marker(on), k.name());
                match verdict {
                    Ok(()) => {
                        let (hs, ts) = if on {
                            (Style::new().fg(theme::t().inverse).bg(theme::t().yellow).bold(), Style::new().fg(theme::t().ink).bold())
                        } else {
                            (Style::new().fg(theme::t().yellow).bold(), Style::new().fg(theme::t().muted))
                        };
                        Line::from(vec![Span::styled(head, hs), Span::styled(format!(" {}", k.tagline()), ts)])
                    }
                    Err(why) => {
                        let hs = if on {
                            Style::new().fg(theme::t().inverse).bg(theme::t().dim)
                        } else {
                            Style::new().fg(theme::t().dim)
                        };
                        let room = cols.saturating_sub(head.chars().count() + 4);
                        let why: String = if why.chars().count() > room {
                            why.chars().take(room.saturating_sub(1)).chain(['…']).collect()
                        } else {
                            why.to_string()
                        };
                        Line::from(vec![
                            Span::styled(head, hs),
                            Span::styled(format!("  · {why}"), Style::new().fg(theme::t().dim).italic()),
                        ])
                    }
                }
            })
            .collect();
        Paragraph::new(lines).render(body, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::ShapeKind;

    #[test]
    fn the_picker_offers_the_allowed_kinds_first_and_starts_on_the_best() {
        let mut doc = Document::default();
        let c = doc.add(ShapeKind::ApplicationComponent, "", 0.0, 0.0);
        let s = doc.add(ShapeKind::ApplicationService, "", 30.0, 0.0);
        let st = State::new(&doc, c, s, None);
        assert_eq!(st.rows.len(), RelationKind::ALL.len(), "every kind is listed");
        let first_refused = st.rows.iter().position(|(_, v)| v.is_err()).unwrap();
        assert!(st.rows[..first_refused].iter().all(|(_, v)| v.is_ok()));
        assert_eq!(st.picked().map(|p| p.0), Some(RelationKind::Realization), "component realizes service is the best line here");
    }

    #[test]
    fn rekinding_starts_on_the_kind_the_relation_already_is() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::BusinessProcess, "", 0.0, 0.0);
        let b = doc.add(ShapeKind::BusinessProcess, "", 30.0, 0.0);
        let r = doc.connect(RelationKind::Flow, a, b).unwrap();
        let st = State::new(&doc, a, b, Some(r));
        assert_eq!(st.picked().map(|p| p.0), Some(RelationKind::Flow));
    }
}
