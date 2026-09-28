//! `g/` from the workbench — a content search over every diagram under its root, not just the
//! names already on screen. `workbench.rs`'s own `/` only ever sees a row's name; this reads
//! `registry::Entry` instead, so a query matches a shape's label, its `ElementKind`, its
//! `api_name`, or a property's — "which diagrams touch `orderId`" answers with the object
//! type that has it, wherever it is drawn, not with a row literally called that. One row per
//! `(entry, file)` pair, grep's own grain: a shape seen in three files is three rows, each
//! one `enter` away from opening that file, so a match is never a name you still have to go
//! find a file for by hand.

use std::path::PathBuf;

use super::{chrome, theme};
use crate::registry;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub const WIDTH: u16 = 72;

pub fn height(avail: u16) -> u16 {
    avail.saturating_sub(2).clamp(8, 30)
}

pub struct State {
    pub query: String,
    pub sel: usize,
}

impl State {
    pub fn new() -> State {
        State { query: String::new(), sel: 0 }
    }

    pub fn retype(&mut self, f: impl FnOnce(&mut String)) {
        f(&mut self.query);
        self.sel = 0;
    }

    pub fn move_by(&mut self, delta: isize, n: usize) {
        if n == 0 {
            self.sel = 0;
            return;
        }
        self.sel = (self.sel as isize + delta).rem_euclid(n as isize) as usize;
    }
}

impl Default for State {
    fn default() -> State {
        State::new()
    }
}

#[derive(Clone)]
pub struct Hit {
    pub name: String,
    pub kind: crate::ontology::ShapeKind,
    pub path: PathBuf,
}

/// Every `(matching entry, file it is in)` pair, sorted by name then path — empty for an
/// empty query, the same as a real `grep` with nothing to look for finds nothing rather than
/// everything.
pub fn results(query: &str, registry: &registry::Registry) -> Vec<Hit> {
    let mut entries: Vec<&registry::Entry> = registry.entries.values().filter(|e| e.matches(query)).collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = Vec::new();
    for e in entries {
        for p in &e.seen_paths {
            out.push(Hit { name: e.name.clone(), kind: e.key.kind, path: p.clone() });
        }
    }
    out
}

pub struct Browser<'a> {
    pub state: &'a State,
    pub registry: Option<&'a registry::Registry>,
}

impl Widget for Browser<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = chrome::panel(buf, area, "grep — every diagram, not just this folder's names", theme::t().aqua);
        let hint = " type to search   \u{2191}/\u{2193} pick   enter opens the file   esc";
        let body = chrome::hint(buf, inner, hint);
        if body.height < 2 {
            return;
        }
        let query = Line::from(vec![
            Span::styled(" g/", Style::new().fg(theme::t().aqua).bold()),
            Span::styled(self.state.query.clone(), Style::new().fg(theme::t().ink)),
            Span::styled("█", Style::new().fg(theme::t().aqua)),
        ]);
        Paragraph::new(query).render(Rect { height: 1, ..body }, buf);
        let body = Rect { y: body.y + 1, height: body.height.saturating_sub(1), ..body };

        let Some(reg) = self.registry else {
            Paragraph::new(Line::styled(" no workbench open — grep has nothing to search", Style::new().fg(theme::t().red))).render(body, buf);
            return;
        };
        let all = results(&self.state.query, reg);
        if all.is_empty() {
            let msg = if self.state.query.trim().is_empty() { " type a label, kind, api name or property".to_string() } else { format!(" nothing matches {:?}", self.state.query) };
            Paragraph::new(Line::styled(msg, Style::new().fg(theme::t().red))).render(body, buf);
            return;
        }
        let view = body.height as usize;
        let cur = self.state.sel.min(all.len() - 1);
        let max_off = all.len().saturating_sub(view);
        let off = cur.saturating_sub(view / 2).min(max_off);
        let mut lines: Vec<Line> = Vec::new();
        for (i, hit) in all.iter().enumerate().skip(off).take(view) {
            let on = i == cur;
            let text = format!(" {} — {} — {}", hit.name, hit.kind.name(), hit.path.display());
            let style = if on { Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold() } else { Style::new().fg(theme::t().ink) };
            lines.push(Line::styled(fit(&text, body.width as usize), style));
        }
        Paragraph::new(lines).render(body, buf);
    }
}

fn fit(s: &str, room: usize) -> String {
    if s.chars().count() <= room { s.to_string() } else { s.chars().take(room.saturating_sub(1)).chain(['…']).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use crate::ontology::ShapeKind;
    use crate::registry::{Entry, Ident, Key};
    use std::collections::HashMap;

    fn reg() -> registry::Registry {
        let mut entries = HashMap::new();
        entries.insert(
            Key { kind: ShapeKind::ObjectType, ident: Ident::Api("customer".into()) },
            Entry {
                key: Key { kind: ShapeKind::ObjectType, ident: Ident::Api("customer".into()) },
                name: "Customer".into(),
                api_name: Some("customer".into()),
                properties: Vec::new(),
                status: Status::Active,
                seen_in: 2,
                seen_paths: vec![PathBuf::from("/root/a.json"), PathBuf::from("/root/sub/b.json")],
            },
        );
        entries.insert(
            Key { kind: ShapeKind::BusinessActor, ident: Ident::Name("Ops".into()) },
            Entry {
                key: Key { kind: ShapeKind::BusinessActor, ident: Ident::Name("Ops".into()) },
                name: "Ops".into(),
                api_name: None,
                properties: Vec::new(),
                status: Status::Active,
                seen_in: 1,
                seen_paths: vec![PathBuf::from("/root/c.json")],
            },
        );
        registry::Registry { entries, edges: Vec::new() }
    }

    #[test]
    fn a_shape_in_several_files_is_one_row_per_file() {
        let hits = results("customer", &reg());
        assert_eq!(hits.len(), 2, "seen in two files — two rows, each openable on its own");
        let mut paths: Vec<&PathBuf> = hits.iter().map(|h| &h.path).collect();
        paths.sort();
        assert_eq!(paths, vec![&PathBuf::from("/root/a.json"), &PathBuf::from("/root/sub/b.json")]);
    }

    #[test]
    fn an_empty_query_finds_nothing_and_a_real_one_ignores_case() {
        assert!(results("", &reg()).is_empty(), "nothing typed, nothing found — not everything");
        assert_eq!(results("OPS", &reg()).len(), 1);
    }
}
