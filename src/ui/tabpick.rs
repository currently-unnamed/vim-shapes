//! The question `^t` asks: which of the two kinds of diagram the new tab is.
//!
//! Two rows, because there are two kinds and the choice shapes everything after it — what the
//! palette offers, whether the rules have anything to say. Asked up front, once, rather than
//! discovered later by finding the palette full of things you did not want.

use super::{chrome, theme};
use crate::ontology::View;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub struct State {
    pub sel: usize,
}

/// The two kinds, in the order they are offered.
pub const KINDS: [View; 2] = [View::Freeform, View::Free];

impl State {
    pub fn new() -> State {
        State { sel: 0 }
    }

    pub fn picked(&self) -> View {
        KINDS[self.sel.min(KINDS.len() - 1)]
    }

    pub fn move_by(&mut self, delta: isize) {
        self.sel = (self.sel as isize + delta).rem_euclid(KINDS.len() as isize) as usize;
    }
}

pub const WIDTH: u16 = 74;
pub const HEIGHT: u16 = 5;

pub struct Picker<'a> {
    pub state: &'a State,
}

impl Widget for Picker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = chrome::panel(buf, area, "new tab — which kind of diagram?", theme::t().aqua);
        let body = chrome::hint(buf, inner, " j/k pick   enter make it   esc");
        let lines: Vec<Line> = KINDS
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let on = i == self.state.sel;
                let name = format!("{}{:<14}", chrome::marker(on), v.badge());
                let (ns, ts) = if on {
                    (Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold(), Style::new().fg(theme::t().ink))
                } else {
                    (Style::new().fg(theme::t().aqua).bold(), Style::new().fg(theme::t().muted))
                };
                Line::from(vec![Span::styled(name, ns), Span::styled(format!(" {}", v.tagline()), ts)])
            })
            .collect();
        Paragraph::new(lines).render(body, buf);
    }
}
