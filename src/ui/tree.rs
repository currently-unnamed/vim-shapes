//! The model tree — `:tree` — a fold-and-search browser over a coArchi import's own folder
//! organization, since a real repository can turn into hundreds of tabs and neither the
//! header nor `:tabs`/`:tab N` scale to that: the header just runs out of columns with no
//! sign anything is missing, and `:tab N` wants a number nobody has memorized.
//!
//! Every letter typed is a query, same as the add palette — the arrows move the selection,
//! not `j`/`k`, so a folder or a view named after either letter is never unreachable. A
//! folder toggles open with enter or → ; a view opens it with enter and closes the tree. A
//! folder with nothing under it — every element inside it but no view — never appears at
//! all: there is nowhere for it to send you.

use super::{chrome, theme};
use crate::archimate_import::ModelNode;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub const WIDTH: u16 = 64;

pub fn height(avail: u16) -> u16 {
    avail.saturating_sub(2).clamp(8, 40)
}

pub struct State {
    /// Index into `rows(nodes, &self.filter)` — the *shown* list, filter and folds applied,
    /// the same way the add palette's selection indexes its own filtered rows.
    pub sel: usize,
    pub filter: String,
}

impl State {
    pub fn new() -> State {
        State { sel: 0, filter: String::new() }
    }

    pub fn move_by(&mut self, delta: isize, rows: &[Row]) {
        let n = rows.len();
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

impl Default for State {
    fn default() -> State {
        State::new()
    }
}

#[derive(Clone, Debug)]
pub struct Row {
    /// Child indices from the roots down to this node — how `toggle` finds it again to fold
    /// or unfold it, and how a picked view finds its own tab.
    pub path: Vec<usize>,
    pub depth: usize,
    pub label: String,
    pub kind: RowKind,
}

#[derive(Clone, Copy, Debug)]
pub enum RowKind {
    /// Never `false` in the sense of "nothing to show" — a folder that leads to no view at
    /// all is never turned into a row to begin with, so every one shown here has something
    /// under it.
    Folder { expanded: bool },
    View { tab_index: usize },
}

/// The rows a browser shows right now: every folder's own fold state, unless there is a
/// search, in which case everything that leads to a match is shown regardless of fold state —
/// folding is for browsing you already know the shape of, not for a search.
pub fn rows(nodes: &[ModelNode], filter: &str) -> Vec<Row> {
    let f = filter.trim().to_ascii_lowercase();
    let mut out = Vec::new();
    let mut path = Vec::new();
    walk(nodes, &mut path, 0, &f, &mut out);
    out
}

fn walk(nodes: &[ModelNode], path: &mut Vec<usize>, depth: usize, f: &str, out: &mut Vec<Row>) {
    for (i, node) in nodes.iter().enumerate() {
        path.push(i);
        match node {
            ModelNode::Folder { name, children, expanded } => {
                // Shown only if there is a view somewhere underneath — a folder of nothing
                // but elements is not something to browse to, and not a match for anything
                // typed either — and, while searching, only along a path to a match.
                let matches = f.is_empty() || name.to_ascii_lowercase().contains(f) || any_match(children, f);
                if has_view(children) && matches {
                    let open = *expanded || !f.is_empty();
                    out.push(Row { path: path.clone(), depth, label: name.clone(), kind: RowKind::Folder { expanded: open } });
                    if open {
                        walk(children, path, depth + 1, f, out);
                    }
                }
            }
            ModelNode::View { name, tab_index } => {
                if f.is_empty() || name.to_ascii_lowercase().contains(f) {
                    out.push(Row { path: path.clone(), depth, label: name.clone(), kind: RowKind::View { tab_index: *tab_index } });
                }
            }
        }
        path.pop();
    }
}

fn has_view(nodes: &[ModelNode]) -> bool {
    nodes.iter().any(|n| match n {
        ModelNode::Folder { children, .. } => has_view(children),
        ModelNode::View { .. } => true,
    })
}

fn any_match(nodes: &[ModelNode], f: &str) -> bool {
    nodes.iter().any(|n| match n {
        ModelNode::Folder { name, children, .. } => name.to_ascii_lowercase().contains(f) || any_match(children, f),
        ModelNode::View { name, .. } => name.to_ascii_lowercase().contains(f),
    })
}

/// Flip a folder's own fold state. Does nothing to a view — a view has no fold state to flip.
pub fn toggle(nodes: &mut [ModelNode], path: &[usize]) {
    if let Some(ModelNode::Folder { expanded, .. }) = get_mut(nodes, path) {
        *expanded = !*expanded;
    }
}

/// Force a folder open — `→` on a folded one, never on an already-open one or a view.
pub fn expand(nodes: &mut [ModelNode], path: &[usize]) {
    if let Some(ModelNode::Folder { expanded, .. }) = get_mut(nodes, path) {
        *expanded = true;
    }
}

pub fn collapse(nodes: &mut [ModelNode], path: &[usize]) {
    if let Some(ModelNode::Folder { expanded, .. }) = get_mut(nodes, path) {
        *expanded = false;
    }
}

fn get_mut<'a>(nodes: &'a mut [ModelNode], path: &[usize]) -> Option<&'a mut ModelNode> {
    let (&i, rest) = path.split_first()?;
    let node = nodes.get_mut(i)?;
    if rest.is_empty() {
        return Some(node);
    }
    match node {
        ModelNode::Folder { children, .. } => get_mut(children, rest),
        ModelNode::View { .. } => None,
    }
}

pub struct Browser<'a> {
    pub state: &'a State,
    pub nodes: &'a [ModelNode],
}

impl Widget for Browser<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = chrome::panel(buf, area, "model tree — a coArchi import's own folders", theme::t().aqua);
        let hint = " type to search   ↑/↓ pick   enter/→ open   ← fold   esc";
        let body = chrome::hint(buf, inner, hint);
        if body.height < 2 {
            return;
        }
        let query = Line::from(vec![
            Span::styled(" > ", Style::new().fg(theme::t().aqua).bold()),
            Span::styled(self.state.filter.clone(), Style::new().fg(theme::t().ink)),
            Span::styled("█", Style::new().fg(theme::t().aqua)),
        ]);
        Paragraph::new(query).render(Rect { height: 1, ..body }, buf);
        let body = Rect { y: body.y + 1, height: body.height.saturating_sub(1), ..body };

        let all = rows(self.nodes, &self.state.filter);
        if all.is_empty() {
            let msg = if self.state.filter.is_empty() { " nothing imported a folder tree yet".to_string() } else { format!(" nothing matches {:?}", self.state.filter) };
            Paragraph::new(Line::styled(msg, Style::new().fg(theme::t().red))).render(body, buf);
            return;
        }
        let view = body.height as usize;
        let cur = self.state.sel.min(all.len() - 1);
        let max_off = all.len().saturating_sub(view);
        let off = cur.saturating_sub(view / 2).min(max_off);
        let mut lines: Vec<Line> = Vec::new();
        for (i, row) in all.iter().enumerate().skip(off).take(view) {
            let on = i == cur;
            let indent = "  ".repeat(row.depth);
            let glyph = match row.kind {
                RowKind::Folder { expanded: true, .. } => "▾",
                RowKind::Folder { expanded: false, .. } => "▸",
                RowKind::View { .. } => "◆",
            };
            let text = format!("{indent}{glyph} {}", row.label);
            let base = match row.kind {
                RowKind::Folder { .. } => theme::t().yellow,
                RowKind::View { .. } => theme::t().ink,
            };
            let style = if on { Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold() } else { Style::new().fg(base) };
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

    fn sample() -> Vec<ModelNode> {
        vec![
            ModelNode::Folder {
                name: "Business".into(),
                expanded: false,
                children: vec![
                    ModelNode::Folder { name: "Customer Views".into(), expanded: false, children: vec![ModelNode::View { name: "Onboarding".into(), tab_index: 0 }] },
                    ModelNode::View { name: "Support Journey".into(), tab_index: 1 },
                ],
            },
            ModelNode::Folder { name: "Application".into(), expanded: true, children: vec![ModelNode::View { name: "Billing Overview".into(), tab_index: 2 }] },
        ]
    }

    #[test]
    fn a_folded_folder_hides_its_children_until_toggled() {
        let mut tree = sample();
        let closed = rows(&tree, "");
        assert_eq!(closed.len(), 3, "Business folded (1 row), Application open with its one view (2 rows)");
        assert_eq!(closed[0].label, "Business");
        assert!(matches!(closed[0].kind, RowKind::Folder { expanded: false }));
        assert_eq!(closed[1].label, "Application");
        assert_eq!(closed[2].label, "Billing Overview");

        toggle(&mut tree, &[0]);
        let opened = rows(&tree, "");
        assert_eq!(opened.len(), 5, "Business open now reveals its two children");
        assert_eq!(opened[1].label, "Customer Views");
        // Customer Views is itself still folded — its own view stays hidden.
        assert!(!opened.iter().any(|r| r.label == "Onboarding"));
        assert_eq!(opened[2].label, "Support Journey");
    }

    #[test]
    fn searching_ignores_fold_state_and_shows_every_matching_path() {
        let tree = sample();
        let rows = rows(&tree, "onboard");
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        // Business is folded and Customer Views is folded, but the search still surfaces the
        // whole path down to the match.
        assert_eq!(labels, vec!["Business", "Customer Views", "Onboarding"]);
    }

    #[test]
    fn a_folder_with_no_view_anywhere_under_it_never_appears() {
        let tree = vec![ModelNode::Folder { name: "Empty Category".into(), expanded: true, children: Vec::new() }];
        assert!(rows(&tree, "").is_empty());
    }

    #[test]
    fn selection_wraps_and_a_search_resets_it_to_the_top() {
        let mut s = State::new();
        let tree = sample();
        let rows = rows(&tree, "");
        s.move_by(-1, &rows);
        assert_eq!(s.sel, rows.len() - 1);
        s.retype(|f| f.push_str("billing"));
        assert_eq!(s.sel, 0);
    }
}
