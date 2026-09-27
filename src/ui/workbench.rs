//! The architecture workbench panel — `:workbench <path>` (`:wb`), docked on the *left* the
//! way `sheet.rs` docks on the right. A live folder tree, not a one-shot import: `n`/`N` add a
//! diagram or a folder, `r` renames, `m` grabs a row and `p` puts it in the folder under the
//! cursor, `d` deletes (after the app's own confirm). This file only shows the tree and holds
//! its cursor/fold/editing state; every real filesystem change is `crate::workbench`'s, and
//! every key is handled in `ui::mod`'s `workbench_key`, the same split `props.rs`/`tree.rs`
//! keep between their own state and the app's key handler.
//!
//! No live filter: unlike `tree.rs`'s one-shot browser, this panel's letters are actions
//! (`n`/`N`/`r`/`m`/`p`/`d`), the same grammar `props.rs` uses for its rows — so, like props,
//! there is no type-to-search here.

use std::collections::HashSet;
use std::path::PathBuf;

use super::{chrome, theme};
use crate::workbench as wb;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub const WIDTH: u16 = 40;

/// What is being typed: a new diagram's or folder's name, going in wherever the cursor is, or
/// the selected row's own new name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Typing {
    NewDiagram,
    NewFolder,
    Rename,
}

pub struct State {
    /// `None` while a folder has never been chosen this run — `rows`/`recents` decide which
    /// the browser shows.
    pub root: Option<PathBuf>,
    pub nodes: Vec<wb::Node>,
    /// Every workbench opened before, most-recent first — shown as pickable rows in place of
    /// a tree when `root` is `None`.
    pub recents: Vec<PathBuf>,
    /// Folders open, by path. Reconciled by path on every rescan; a folder that was just
    /// renamed or moved simply loses its fold state, which is not worth tracking through one.
    expanded: HashSet<PathBuf>,
    pub sel: usize,
    pub editing: Option<(Typing, String)>,
    /// `m` on a row grabs it here; `p` on a folder row puts it there.
    pub grabbed: Option<wb::Entry>,
}

impl State {
    pub fn recents(recents: Vec<PathBuf>) -> State {
        State { root: None, nodes: Vec::new(), recents, expanded: HashSet::new(), sel: 0, editing: None, grabbed: None }
    }

    pub fn opened(root: PathBuf) -> State {
        let nodes = wb::scan(&root);
        State { root: Some(root), nodes, recents: Vec::new(), expanded: HashSet::new(), sel: 0, editing: None, grabbed: None }
    }

    /// Re-read the folder from disk — after every create, rename, move or delete.
    pub fn rescan(&mut self) {
        if let Some(root) = &self.root {
            self.nodes = wb::scan(root);
        }
    }

    pub fn rows(&self) -> Vec<Row> {
        rows(&self.nodes, &self.expanded)
    }

    pub fn selected_row(&self) -> Option<Row> {
        self.rows().into_iter().nth(self.sel)
    }

    /// The folder a new diagram or folder lands in, or `p` puts something into — the row
    /// under the cursor if it is a folder, that row's own parent if it is a diagram, the root
    /// if there is no selection at all.
    pub fn current_dir(&self) -> Option<PathBuf> {
        match self.selected_row() {
            Some(Row { kind: RowKind::Folder { .. }, path, .. }) => Some(path),
            Some(Row { kind: RowKind::Diagram, path, .. }) => path.parent().map(PathBuf::from),
            None => self.root.clone(),
        }
    }

    pub fn toggle_expanded(&mut self, path: &std::path::Path) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_path_buf());
        }
    }

    pub fn expand(&mut self, path: &std::path::Path) {
        self.expanded.insert(path.to_path_buf());
    }

    pub fn move_by(&mut self, delta: isize, n: usize) {
        if n == 0 {
            self.sel = 0;
            return;
        }
        self.sel = (self.sel as isize + delta).rem_euclid(n as isize) as usize;
    }
}

#[derive(Clone, Debug)]
pub struct Row {
    pub path: PathBuf,
    pub depth: usize,
    pub name: String,
    pub kind: RowKind,
}

impl Row {
    /// This row's identity without its subtree — what `wb::rename`/`move_to`/`delete` take.
    pub fn entry(&self) -> wb::Entry {
        wb::Entry {
            path: self.path.clone(),
            name: self.name.clone(),
            kind: match self.kind {
                RowKind::Folder { .. } => wb::NodeKind::Folder,
                RowKind::Diagram => wb::NodeKind::Diagram,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    Folder { expanded: bool },
    Diagram,
}

pub fn rows(nodes: &[wb::Node], expanded: &HashSet<PathBuf>) -> Vec<Row> {
    let mut out = Vec::new();
    walk(nodes, 0, expanded, &mut out);
    out
}

fn walk(nodes: &[wb::Node], depth: usize, expanded: &HashSet<PathBuf>, out: &mut Vec<Row>) {
    for n in nodes {
        match n {
            wb::Node::Folder { path, name, children } => {
                let open = expanded.contains(path);
                out.push(Row { path: path.clone(), depth, name: name.clone(), kind: RowKind::Folder { expanded: open } });
                if open {
                    walk(children, depth + 1, expanded, out);
                }
            }
            wb::Node::Diagram { path, name } => {
                out.push(Row { path: path.clone(), depth, name: name.clone(), kind: RowKind::Diagram });
            }
        }
    }
}

pub struct Browser<'a> {
    pub state: &'a State,
}

impl Widget for Browser<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let title = match &s.root {
            Some(root) => format!("workbench — {}", root.display()),
            None => "workbench — pick one".to_string(),
        };
        let inner = chrome::panel(buf, area, &title, theme::t().aqua);
        let hint = match (&s.editing, &s.grabbed, &s.root) {
            (Some(_), _, _) => " type a name — enter, or esc",
            (None, Some(_), _) => " navigate, then p to put it here — esc cancels the move",
            (None, None, Some(_)) => " j/k move  enter open/toggle  n diagram  N folder  r rename  m move  d delete  esc",
            (None, None, None) => " j/k move  enter opens it  esc closes",
        };
        let body = chrome::hint(buf, inner, hint);
        if body.height < 2 {
            return;
        }

        let mut lines: Vec<Line> = Vec::new();
        if s.root.is_none() {
            if s.recents.is_empty() {
                lines.push(Line::styled(" no workbench opened yet — :workbench <path>", Style::new().fg(theme::t().dim)));
            }
            for (i, p) in s.recents.iter().enumerate() {
                let on = i == s.sel;
                let style = if on { Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold() } else { Style::new().fg(theme::t().ink) };
                lines.push(Line::styled(format!("{}{}", chrome::marker(on), p.display()), style));
            }
            Paragraph::new(lines).render(body, buf);
            return;
        }

        let all = s.rows();
        if all.is_empty() && s.editing.is_none() {
            lines.push(Line::styled(" empty — n adds a diagram, N a folder", Style::new().fg(theme::t().dim)));
        }
        for (i, row) in all.iter().enumerate() {
            let on = i == s.sel;
            let grabbed = s.grabbed.as_ref().is_some_and(|g| g.path == row.path);
            let indent = "  ".repeat(row.depth);
            let glyph = match row.kind {
                RowKind::Folder { expanded: true } => "▾",
                RowKind::Folder { expanded: false } => "▸",
                RowKind::Diagram => "▪",
            };
            let text = match (&s.editing, on) {
                (Some((Typing::Rename, t)), true) => format!(" {indent}{glyph} {t}█"),
                _ => format!(" {indent}{glyph} {}", row.name),
            };
            let base = match row.kind {
                RowKind::Folder { .. } => theme::t().yellow,
                RowKind::Diagram => theme::t().ink,
            };
            let style = match (on, grabbed) {
                (true, _) => Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold(),
                (false, true) => Style::new().fg(theme::t().green).italic(),
                (false, false) => Style::new().fg(base),
            };
            lines.push(Line::styled(fit(&text, body.width as usize), style));
        }
        if let Some((Typing::NewDiagram | Typing::NewFolder, t)) = &s.editing {
            lines.push(Line::styled(format!(" new: {t}█"), Style::new().fg(theme::t().green).bold()));
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

    fn sample() -> Vec<wb::Node> {
        vec![
            wb::Node::Folder {
                path: "/root/Billing".into(),
                name: "Billing".into(),
                children: vec![wb::Node::Diagram { path: "/root/Billing/Overview.json".into(), name: "Overview.json".into() }],
            },
            wb::Node::Diagram { path: "/root/aaa.json".into(), name: "aaa.json".into() },
        ]
    }

    #[test]
    fn a_folded_folder_hides_its_children_until_expanded() {
        let expanded = HashSet::new();
        let closed = rows(&sample(), &expanded);
        assert_eq!(closed.len(), 2, "Billing folded, plus the top-level diagram");
        assert!(matches!(closed[0].kind, RowKind::Folder { expanded: false }));

        let mut expanded = HashSet::new();
        expanded.insert(PathBuf::from("/root/Billing"));
        let opened = rows(&sample(), &expanded);
        assert_eq!(opened.len(), 3);
        assert_eq!(opened[1].name, "Overview.json");
        assert_eq!(opened[1].depth, 1);
    }

    #[test]
    fn current_dir_is_the_selected_folder_or_a_diagram_s_own_parent_or_the_root() {
        let mut st = State::opened(PathBuf::from("/root"));
        st.nodes = sample();
        st.sel = 0;
        assert_eq!(st.current_dir(), Some(PathBuf::from("/root/Billing")), "a folder row: itself");
        st.sel = 1;
        assert_eq!(st.current_dir(), Some(PathBuf::from("/root")), "a diagram row: its own parent");
        st.nodes = Vec::new();
        assert_eq!(st.current_dir(), Some(PathBuf::from("/root")), "nothing selected: the root");
    }

    #[test]
    fn selection_wraps() {
        let mut s = State::opened(PathBuf::from("/root"));
        s.move_by(-1, 3);
        assert_eq!(s.sel, 2);
        s.move_by(1, 3);
        assert_eq!(s.sel, 0);
    }
}
