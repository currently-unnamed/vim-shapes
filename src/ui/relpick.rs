//! The relation picker — asked the moment a relation is dropped, and again by `r`.
//!
//! It lists every kind, always: the ones the rules allow between these two elements first and
//! in full colour, the rest dimmed with the reason beside them. The rules are advisory, so a
//! dimmed row can still be chosen — the relation is drawn and marked, and `:lint` will list it
//! — but the reason is on screen at the one moment it can change a mind.
//!
//! Both halves are grouped by [`Family`] — structural, dependency, dynamic, the ontology's
//! schema and its action rules, other — so the lit half reads as the sorts of thing you could
//! be saying here, and from an ontology shape the architecture's families sink, whole, into
//! the refused half.

use super::{chrome, theme};
use crate::model::{Document, ElementId, RelationId};
use crate::ontology::{self, Family, RelationKind};
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

pub fn height(state: &State, avail: u16) -> u16 {
    ((lines(state).len() + 3) as u16).min(avail)
}

enum Row {
    /// Where the lit half ends and the refused half begins.
    Refused,
    Family(Family, bool),
    Item(usize),
}

/// The rows with their headings: a family heading wherever the family changes, and once
/// more after the refused divider, since both halves run in family order.
fn lines(state: &State) -> Vec<Row> {
    let mut out = Vec::new();
    let mut last: Option<(Family, bool)> = None;
    for (i, (k, verdict)) in state.rows.iter().enumerate() {
        let lit = verdict.is_ok();
        if !lit && last.is_none_or(|(_, l)| l) {
            out.push(Row::Refused);
        }
        if last != Some((k.family(), lit)) {
            out.push(Row::Family(k.family(), lit));
        }
        last = Some((k.family(), lit));
        out.push(Row::Item(i));
    }
    out
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
        let all = lines(self.state);
        // Keep the selection in view: the refused half runs past the bottom of most screens.
        let view = body.height as usize;
        let cur = all.iter().position(|r| matches!(r, Row::Item(i) if *i == self.state.sel)).unwrap_or(0);
        let off = cur.saturating_sub(view.saturating_sub(2)).min(all.len().saturating_sub(view));
        let lines: Vec<Line> = all
            .iter()
            .skip(off)
            .take(view)
            .map(|row| {
                let i = match row {
                    Row::Refused => {
                        return Line::styled(" refused here — drawn anyway, and marked", Style::new().fg(theme::t().dim).bold());
                    }
                    Row::Family(f, lit) => {
                        let fs = if *lit { Style::new().fg(theme::t().yellow).italic() } else { Style::new().fg(theme::t().dim).italic() };
                        return Line::from(vec![
                            Span::styled(format!(" ── {} ", f.name()), fs),
                            Span::styled(format!("· {}", f.tagline()), Style::new().fg(theme::t().dim).italic()),
                        ]);
                    }
                    Row::Item(i) => *i,
                };
                let (k, verdict) = &self.state.rows[i];
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
    fn from_an_object_type_the_schema_is_lit_and_the_architecture_sinks_whole() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::ObjectType, "", 0.0, 0.0);
        let b = doc.add(ShapeKind::Interface, "", 30.0, 0.0);
        let st = State::new(&doc, a, b, None);
        assert_eq!(st.picked().map(|p| p.0), Some(RelationKind::LinkType), "the schema leads");
        let heads: Vec<(Family, bool)> = lines(&st).iter().filter_map(|r| if let Row::Family(f, l) = r { Some((*f, *l)) } else { None }).collect();
        assert_eq!(heads[0], (Family::Schema, true));
        assert!(heads.contains(&(Family::Structural, false)), "structural is refused here, as a family");
        assert_eq!(heads.iter().filter(|h| h.0 == Family::Structural).count(), 1, "one heading per family per half: {heads:?}");
    }

    #[test]
    fn the_selection_stays_on_screen_in_a_short_picker() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::ObjectType, "", 0.0, 0.0);
        let b = doc.add(ShapeKind::Interface, "", 30.0, 0.0);
        let mut st = State::new(&doc, a, b, None);
        st.move_by(-1);
        let last = st.picked().unwrap().0.name();
        let mut term = Terminal::new(TestBackend::new(WIDTH, 12)).unwrap();
        term.draw(|f| f.render_widget(Picker { state: &st, doc: &doc }, f.area())).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains(chrome::marker(true)) && out.contains(last), "the last row, {last}, is scrolled to");
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
