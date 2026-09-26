//! The layer browser — `:layers`.
//!
//! Most diagram tools have layers and show you none of them: "to front" and "to back" and
//! that is all. This is the picture editor's answer instead — the stack, top layer first, each
//! shown or hidden, unlocked or locked, with what sits on it counted; the one new things go on
//! marked; things moved between them. A panel, docked on the right like the sheet, with the
//! keyboard while it is up.

use super::{chrome, theme};
use crate::model::Document;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub const WIDTH: u16 = 40;

pub struct State {
    /// Index into the stack as shown — top first.
    pub sel: usize,
    /// A name being typed, for a new layer or a rename of the selected one.
    pub editing: Option<(Naming, String)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Naming {
    New,
    Rename,
}

impl State {
    pub fn new(doc: &Document) -> State {
        // Start on the current layer.
        let n = doc.layers.len();
        let sel = doc.layers.iter().position(|l| l.id == doc.metadata.layer).map(|i| n - 1 - i).unwrap_or(0);
        State { sel, editing: None }
    }

    /// The layer id at the selection: the stack is shown top first.
    pub fn selected(&self, doc: &Document) -> Option<u32> {
        let n = doc.layers.len();
        if n == 0 {
            return None;
        }
        doc.layers.get(n - 1 - self.sel.min(n - 1)).map(|l| l.id)
    }

    pub fn move_by(&mut self, delta: isize, doc: &Document) {
        let n = doc.layers.len() as isize;
        if n > 0 {
            self.sel = (self.sel as isize + delta).rem_euclid(n) as usize;
        }
    }

    pub fn select(&mut self, id: u32, doc: &Document) {
        let n = doc.layers.len();
        if let Some(i) = doc.layers.iter().position(|l| l.id == id) {
            self.sel = n - 1 - i;
        }
    }
}

pub struct Browser<'a> {
    pub state: &'a State,
    pub doc: &'a Document,
    /// The layer the cursor's shape or relation is on, to mark it.
    pub cursor_layer: Option<u32>,
}

impl Widget for Browser<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = chrome::panel(buf, area, "layers — top first", theme::t().purple);
        let hint = match &self.state.editing {
            Some((Naming::New, _)) => " name the new layer — enter, or esc",
            Some((Naming::Rename, _)) => " rename — enter, or esc",
            None => " j/k  space show/hide  l lock  enter current  m move here  n new  r rename  J/K reorder  d delete  esc",
        };
        let body = chrome::hint(buf, inner, hint);
        let mut lines: Vec<Line> = vec![Line::styled(" ● shown ○ hidden 🔒 locked ▸ current ← here", Style::new().fg(theme::t().dim)), Line::raw("")];
        let n = self.doc.layers.len();
        for (row, l) in self.doc.layers.iter().rev().enumerate() {
            let on = row == self.state.sel;
            let current = l.id == self.doc.metadata.layer;
            let (els, rels) = self.doc.layer_count(l.id);
            let eye = if l.visible { "●" } else { "○" };
            let lock = if l.locked { "🔒" } else { "  " };
            let name = match &self.state.editing {
                Some((Naming::Rename, t)) if on => format!("{t}█"),
                _ => l.name.clone(),
            };
            let here = if self.cursor_layer == Some(l.id) { " ←" } else { "" };
            let text = format!("{}{eye} {lock} {:<16} {els:>3} sh {rels:>3} rel{here}", chrome::marker(current), name);
            let style = match (on, l.visible) {
                (true, _) => Style::new().fg(theme::t().inverse).bg(theme::t().purple).bold(),
                (false, true) => Style::new().fg(theme::t().ink),
                (false, false) => Style::new().fg(theme::t().dim),
            };
            lines.push(Line::styled(fit(&text, body.width as usize), style));
        }
        if let Some((Naming::New, t)) = &self.state.editing {
            lines.push(Line::styled(format!(" new: {t}█"), Style::new().fg(theme::t().green).bold()));
        }
        let _ = n;
        Paragraph::new(lines).render(body, buf);
    }
}

fn fit(s: &str, room: usize) -> String {
    if s.chars().count() <= room { s.to_string() } else { s.chars().take(room.saturating_sub(1)).chain(['…']).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_shows_the_stack_top_first_and_starts_on_the_current_layer() {
        let mut doc = Document::default();
        let top = doc.add_layer("notes");
        let s = State::new(&doc);
        assert_eq!(s.sel, 0, "the current layer is the new one, which is on top");
        assert_eq!(s.selected(&doc), Some(top));
        let mut s = s;
        s.move_by(1, &doc);
        assert_eq!(s.selected(&doc), Some(0));
        s.move_by(1, &doc);
        assert_eq!(s.selected(&doc), Some(top), "wraps");
    }
}
