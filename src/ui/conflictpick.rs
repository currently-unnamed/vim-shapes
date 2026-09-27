//! `:import`'s conflict resolution — one at a time, when a fresh Foundry ontology redefines
//! an object type, interface or action type the open workbench's registry already knows
//! differently (`registry::Registry::conflicts_with`). Modelled on `importdlg.rs`'s split of
//! state from key handling, not `expandpick.rs`'s: a conflict is a two-way decision with a
//! diff to show, not a plain pick-from-a-list.
//!
//! `k` keeps the registry's existing definition, `i` takes the import's incoming one; `K`/`I`
//! apply that same choice to every conflict still queued; `esc`/`q` stops early. Whatever is
//! left undecided defaults to **keep** — the non-destructive default, since resolving a
//! conflict only ever changes the registry's own in-memory idea of the element, never
//! rewrites a diagram file that disagrees with it.

use super::{chrome, theme};
use crate::registry::Conflict;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub const WIDTH: u16 = 70;
pub const HEIGHT: u16 = 20;

/// What a key means — `App` does the actual applying, reading `State::current`/`conflicts`
/// before it calls `State::advance`, the same table-describes-mod-acts split every dialog
/// here keeps.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Nothing,
    Resolve { take_incoming: bool },
    ResolveAll { take_incoming: bool },
    Done,
}

pub struct State {
    pub conflicts: Vec<Conflict>,
    pub i: usize,
}

impl State {
    pub fn new(conflicts: Vec<Conflict>) -> State {
        State { conflicts, i: 0 }
    }

    pub fn current(&self) -> Option<&Conflict> {
        self.conflicts.get(self.i)
    }

    pub fn advance(&mut self) {
        self.i += 1;
    }

    pub fn key(&self, k: KeyEvent) -> Outcome {
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => Outcome::Done,
            KeyCode::Char('k') => Outcome::Resolve { take_incoming: false },
            KeyCode::Char('i') => Outcome::Resolve { take_incoming: true },
            KeyCode::Char('K') => Outcome::ResolveAll { take_incoming: false },
            KeyCode::Char('I') => Outcome::ResolveAll { take_incoming: true },
            _ => Outcome::Nothing,
        }
    }
}

pub struct Dialog<'a> {
    pub state: &'a State,
}

impl Widget for Dialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let title = format!("vim-shapes — resolve conflict {} of {}", (s.i + 1).min(s.conflicts.len().max(1)), s.conflicts.len());
        let inner = chrome::panel(buf, area, &title, theme::t().yellow);
        let hint = " k keep existing   i take incoming   K/I same, for every conflict left   esc/q stops — the rest default to keep";
        let body = chrome::hint(buf, inner, hint);
        if body.height < 4 {
            return;
        }
        let Some(c) = s.current() else {
            Paragraph::new(Line::styled(" nothing left to resolve", Style::new().fg(theme::t().dim))).render(body, buf);
            return;
        };
        let mut lines = vec![
            Line::styled(format!(" {} · {}", c.name, c.key.kind.name()), Style::new().fg(theme::t().bright).bold()),
            Line::raw(""),
            Line::styled(" existing → incoming", Style::new().fg(theme::t().dim)),
        ];
        let mut names: Vec<&str> = c.existing.iter().chain(c.incoming.iter()).map(|p| p.name.as_str()).collect();
        names.sort();
        names.dedup();
        for name in names {
            let old = c.existing.iter().find(|p| p.name == name);
            let new = c.incoming.iter().find(|p| p.name == name);
            let (mark, text, colour) = match (old, new) {
                (Some(o), Some(n)) if o == n => (' ', format!("{name}: {}", n.base_type.name()), theme::t().dim),
                (Some(o), Some(n)) => ('~', format!("{name}: {} → {}", o.base_type.name(), n.base_type.name()), theme::t().yellow),
                (None, Some(n)) => ('+', format!("{name}: {}", n.base_type.name()), theme::t().green),
                (Some(o), None) => ('-', format!("{name}: {}", o.base_type.name()), theme::t().red),
                (None, None) => unreachable!("named from one side or the other"),
            };
            lines.push(Line::styled(format!(" {mark} {text}"), Style::new().fg(colour)));
        }
        Paragraph::new(lines).render(body, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Property;
    use crate::ontology::ShapeKind;
    use crate::registry::{Ident, Key};
    use crossterm::event::KeyModifiers;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
    }

    fn sample() -> Vec<Conflict> {
        let key = |api: &str| Key { kind: ShapeKind::ObjectType, ident: Ident::Api(api.into()) };
        vec![
            Conflict { key: key("a"), name: "A".into(), existing: vec![Property::new("id")], incoming: vec![Property::new("id"), Property::new("name")] },
            Conflict { key: key("b"), name: "B".into(), existing: vec![Property::new("id")], incoming: vec![Property::new("id"), Property::new("email")] },
        ]
    }

    #[test]
    fn k_and_i_answer_one_conflict_and_esc_stops() {
        let s = State::new(sample());
        assert_eq!(s.key(press(KeyCode::Char('k'))), Outcome::Resolve { take_incoming: false });
        assert_eq!(s.key(press(KeyCode::Char('i'))), Outcome::Resolve { take_incoming: true });
        assert_eq!(s.key(press(KeyCode::Esc)), Outcome::Done);
        assert_eq!(s.key(press(KeyCode::Char('q'))), Outcome::Done);
    }

    #[test]
    fn shift_k_and_i_answer_every_conflict_left() {
        let s = State::new(sample());
        assert_eq!(s.key(shift(KeyCode::Char('K'))), Outcome::ResolveAll { take_incoming: false });
        assert_eq!(s.key(shift(KeyCode::Char('I'))), Outcome::ResolveAll { take_incoming: true });
    }

    #[test]
    fn advance_walks_the_queue_and_current_ends_empty() {
        let mut s = State::new(sample());
        assert_eq!(s.current().unwrap().name, "A");
        s.advance();
        assert_eq!(s.current().unwrap().name, "B");
        s.advance();
        assert!(s.current().is_none());
    }
}
