//! The architecture workbench panel — `:workbench <path>` (`:wb`), docked on the *left* the
//! way `sheet.rs` docks on the right. A live folder tree, not a one-shot import: `n`/`N` add a
//! diagram or a folder, `r` renames, `m` grabs a row and `p` puts it in the folder under the
//! cursor, `d` deletes (after the app's own confirm). This file only shows the tree and holds
//! its cursor/fold/editing state; every real filesystem change is `crate::workbench`'s, and
//! every key is handled in `ui::mod`'s `workbench_key`, the same split `props.rs`/`tree.rs`
//! keep between their own state and the app's key handler.
//!
//! This panel's letters are actions (`n`/`N`/`r`/`m`/`p`/`d`), the same grammar `props.rs`
//! uses for its rows — so, unlike `tree.rs`'s one-shot browser, a letter cannot *always* mean
//! "add to the search" without taking one of those away. `/` is the seam: it arms `filtering`,
//! and only then do letters go into `filter` instead of running a command — `tree.rs`'s own
//! rule (ignore fold state, show every path down to a match) applies to what `/` narrows,
//! folders and the registry's own elements section alike, so leaving it engaged is what makes
//! the tree reachable across however deep a team's workbench has grown. `g/` is a second,
//! separate search — a content grep across every file's own elements (`registry::Entry`'s
//! label, kind, `api_name` and properties, not just a row's name on screen) — because "is
//! this folder named right" and "does anything anywhere reference this shape" are different
//! questions, and conflating them into one filter would make the common, cheap one page
//! through the registry's `seen_paths` on every keystroke for no reason.

use std::collections::HashSet;
use std::path::PathBuf;

use super::{chrome, theme};
use crate::ontology::{Layer, ShapeKind};
use crate::registry;
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

/// A registry grouping node's identity — the whole "elements" section, one layer, or one
/// kind within it — for the fold state a `Group` row's `expanded` reads.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GroupId {
    Root,
    Layer(Layer),
    Kind(ShapeKind),
}

pub struct State {
    /// `None` while a folder has never been chosen this run — `display_rows`/`recents`
    /// decide which the browser shows.
    pub root: Option<PathBuf>,
    pub nodes: Vec<wb::Node>,
    /// Every workbench opened before, most-recent first — shown as pickable rows in place of
    /// a tree when `root` is `None`.
    pub recents: Vec<PathBuf>,
    /// Folders open, by path. Reconciled by path on every rescan; a folder that was just
    /// renamed or moved simply loses its fold state, which is not worth tracking through one.
    expanded: HashSet<PathBuf>,
    /// Registry grouping nodes open, by `GroupId` — untouched by a rescan, since layers and
    /// kinds (unlike a folder's path) never change identity under a rebuilt registry.
    groups_expanded: HashSet<GroupId>,
    pub sel: usize,
    pub editing: Option<(Typing, String)>,
    /// `m` on a row grabs it here; `p` on a folder row puts it there.
    pub grabbed: Option<wb::Entry>,
    /// What `/` narrows the tree to — empty means no filter. Kept even after `filtering`
    /// goes back to `false` on `enter`, so browsing resumes on the narrowed list rather than
    /// snapping back to everything the moment typing stops.
    pub filter: String,
    /// `true` only while a `/` search is being typed — letters go into `filter` instead of
    /// running a command. `esc` clears `filter` too (a second `esc`, with nothing left to
    /// clear, closes the panel); `enter` leaves this `false` with `filter` intact.
    pub filtering: bool,
    /// Armed by a bare `g` and consumed by the very next key — `/` opens the content grep,
    /// anything else drops it silently. A one-key memory, not `keymap::Prefix`: that enum is
    /// the main canvas dispatcher's own grammar, and this panel's `g` means nothing outside
    /// this one chord, so it has no business sharing the other's state.
    pub pending_g: bool,
}

impl State {
    pub fn recents(recents: Vec<PathBuf>) -> State {
        State { root: None, nodes: Vec::new(), recents, expanded: HashSet::new(), groups_expanded: HashSet::new(), sel: 0, editing: None, grabbed: None, filter: String::new(), filtering: false, pending_g: false }
    }

    pub fn opened(root: PathBuf) -> State {
        let nodes = wb::scan(&root);
        State { root: Some(root), nodes, recents: Vec::new(), expanded: HashSet::new(), groups_expanded: HashSet::new(), sel: 0, editing: None, grabbed: None, filter: String::new(), filtering: false, pending_g: false }
    }

    /// Re-read the folder from disk — after every create, rename, move or delete. The
    /// registry is `App`'s own field, not this panel's — it has to survive the panel
    /// closing (placing an element does, right before `e` needs it), so `App::rescan_workbench`
    /// rebuilds it alongside this.
    pub fn rescan(&mut self) {
        if let Some(root) = &self.root {
            self.nodes = wb::scan(root);
        }
    }

    /// The one list `sel` walks: the real folder tree, then — if `registry` has anything in
    /// it — the virtual "elements" section, grouped by layer then kind, each level shown
    /// only when something is actually under it (`tree.rs`'s rule for a *derived* grouping,
    /// unlike a real folder above, which is shown even empty). `registry` lives on `App`,
    /// not here — see `rescan`'s doc comment for why. While `filter` is set, both halves
    /// apply it — the fold state of a folder or a layer/kind group is ignored in favour of
    /// showing every path down to a match, `tree.rs`'s own rule, so search always reaches
    /// the whole tree rather than whatever happened to already be open.
    pub fn display_rows(&self, registry: Option<&registry::Registry>) -> Vec<DisplayRow> {
        let f = self.filter.trim().to_ascii_lowercase();
        let mut out: Vec<DisplayRow> = rows(&self.nodes, &self.expanded, &f).into_iter().map(DisplayRow::Fs).collect();
        let Some(reg) = registry else { return out };
        if reg.entries.is_empty() {
            return out;
        }
        let matches = |e: &registry::Entry| f.is_empty() || e.name.to_ascii_lowercase().contains(&f);
        if !reg.entries.values().any(matches) {
            return out;
        }
        let root_open = self.groups_expanded.contains(&GroupId::Root) || !f.is_empty();
        out.push(DisplayRow::Heading);
        out.push(DisplayRow::Group { id: GroupId::Root, label: "elements".into(), depth: 0, expanded: root_open });
        if !root_open {
            return out;
        }
        for layer in Layer::ALL {
            let kinds: Vec<ShapeKind> = ShapeKind::ALL.into_iter().filter(|k| k.layer() == layer && reg.entries.values().any(|e| e.key.kind == *k && matches(e))).collect();
            if kinds.is_empty() {
                continue;
            }
            let layer_open = self.groups_expanded.contains(&GroupId::Layer(layer)) || !f.is_empty();
            out.push(DisplayRow::Group { id: GroupId::Layer(layer), label: layer.name().to_string(), depth: 1, expanded: layer_open });
            if !layer_open {
                continue;
            }
            for kind in kinds {
                let kind_open = self.groups_expanded.contains(&GroupId::Kind(kind)) || !f.is_empty();
                out.push(DisplayRow::Group { id: GroupId::Kind(kind), label: kind.name().to_string(), depth: 2, expanded: kind_open });
                if !kind_open {
                    continue;
                }
                let mut entries: Vec<&registry::Entry> = reg.entries.values().filter(|e| e.key.kind == kind && matches(e)).collect();
                entries.sort_by(|a, b| a.name.cmp(&b.name));
                for e in entries {
                    out.push(DisplayRow::Element { key: e.key.clone(), name: e.name.clone(), seen_in: e.seen_in, depth: 3 });
                }
            }
        }
        out
    }

    pub fn selected(&self, registry: Option<&registry::Registry>) -> Option<DisplayRow> {
        self.display_rows(registry).into_iter().nth(self.sel)
    }

    /// The folder a new diagram or folder lands in, or `p` puts something into — the row
    /// under the cursor if it is a folder, that row's own parent if it is a diagram, the
    /// root otherwise (nothing selected, or standing in the virtual elements section, which
    /// has no folder of its own).
    pub fn current_dir(&self, registry: Option<&registry::Registry>) -> Option<PathBuf> {
        match self.selected(registry) {
            Some(DisplayRow::Fs(Row { kind: RowKind::Folder { .. }, path, .. })) => Some(path),
            Some(DisplayRow::Fs(Row { kind: RowKind::Diagram, path, .. })) => path.parent().map(PathBuf::from),
            _ => self.root.clone(),
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

    pub fn toggle_group(&mut self, id: GroupId) {
        if !self.groups_expanded.remove(&id) {
            self.groups_expanded.insert(id);
        }
    }

    pub fn expand_group(&mut self, id: GroupId) {
        self.groups_expanded.insert(id);
    }

    pub fn move_by(&mut self, delta: isize, n: usize) {
        if n == 0 {
            self.sel = 0;
            return;
        }
        self.sel = (self.sel as isize + delta).rem_euclid(n as isize) as usize;
    }

    /// `^d`/`^u`'s own jump — clamped, not wrapped like `move_by`: landing past either end
    /// and wrapping around would leap clear across the tree instead of paging through it.
    pub fn page_by(&mut self, delta: isize, n: usize) {
        if n == 0 {
            self.sel = 0;
            return;
        }
        self.sel = (self.sel as isize + delta).clamp(0, n as isize - 1) as usize;
    }

    /// How many rows of the list fit on screen right now — a page, for `page_by` — the same
    /// arithmetic `render` and `row_at` already use to keep the widget and its hit-test in
    /// step.
    pub fn page_size(&self, area: Rect) -> usize {
        let body = chrome::panel_body(area, true);
        (body.height.saturating_sub(self.header_lines())) as usize
    }

    /// `/`'s own typing — same shape as `tree::State::retype`, resetting `sel` so a narrower
    /// (or wider) list never leaves the cursor stranded past its end or on an unrelated row.
    pub fn retype(&mut self, f: impl FnOnce(&mut String)) {
        f(&mut self.filter);
        self.sel = 0;
    }

    /// How many lines sit above the row list itself — the `/` filter line, when it's shown.
    /// One function, so the widget's own offset and a mouse hit-test's can never disagree.
    fn header_lines(&self) -> u16 {
        if self.root.is_some() && (self.filtering || !self.filter.is_empty()) { 1 } else { 0 }
    }

    /// The first row shown, given how many rows there are and how many fit — `ctxmenu::
    /// State::scroll`'s own reasoning, so a long tree keeps `sel` roughly centred rather than
    /// pinned to whichever edge it walked off of.
    fn scroll(&self, n: usize, view: usize) -> usize {
        if view == 0 {
            return 0;
        }
        self.sel.saturating_sub(view / 2).min(n.saturating_sub(view))
    }

    /// Which row a screen point lands on — not the title bar, the hint line, the `/` filter
    /// line, or a "new:" line being typed, none of which are `sel`-addressable. `area` is the
    /// panel's own rect, from `App::workbench_area`; `n` is how many rows are in the list
    /// right now (`display_rows`'s length, or `recents`' while no root is chosen) — not
    /// available from `State` alone without the registry `display_rows` itself needs.
    pub fn row_at(&self, area: Rect, n: usize, col: u16, row: u16) -> Option<usize> {
        let body = chrome::panel_body(area, true);
        let header = self.header_lines();
        if col < body.x || col >= body.right() || row < body.y + header || row >= body.bottom() {
            return None;
        }
        let view = (body.height.saturating_sub(header)) as usize;
        let i = self.scroll(n, view) + (row - body.y - header) as usize;
        (i < n).then_some(i)
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

/// One row of the combined list `State::display_rows` builds — a real filesystem row, or one
/// of the virtual "elements" section's rows. Kept separate from `Row` rather than folded into
/// it: `Row` is real-`PathBuf`-shaped, and every filesystem operation (`n`/`N`/`r`/`m`/`d`)
/// takes a `Row`'s own `entry()` — a `Group`/`Element` row has no path to give it.
pub enum DisplayRow {
    Fs(Row),
    /// A non-interactive divider between the folder tree and the elements section.
    Heading,
    Group { id: GroupId, label: String, depth: usize, expanded: bool },
    Element { key: registry::Key, name: String, seen_in: usize, depth: usize },
}

#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    Folder { expanded: bool },
    Diagram,
}

/// The tree's own rows, folded per `expanded` — or, while `filter` (already lowercased and
/// trimmed) is non-empty, `tree.rs`'s rule instead: fold state is ignored, and only a folder
/// on a path to a match, or a diagram matching itself, is shown at all.
pub fn rows(nodes: &[wb::Node], expanded: &HashSet<PathBuf>, filter: &str) -> Vec<Row> {
    let mut out = Vec::new();
    walk(nodes, 0, expanded, filter, &mut out);
    out
}

fn walk(nodes: &[wb::Node], depth: usize, expanded: &HashSet<PathBuf>, filter: &str, out: &mut Vec<Row>) {
    for n in nodes {
        match n {
            wb::Node::Folder { path, name, children } => {
                let matches = filter.is_empty() || name.to_ascii_lowercase().contains(filter) || any_match(children, filter);
                if !matches {
                    continue;
                }
                let open = expanded.contains(path) || !filter.is_empty();
                out.push(Row { path: path.clone(), depth, name: name.clone(), kind: RowKind::Folder { expanded: open } });
                if open {
                    walk(children, depth + 1, expanded, filter, out);
                }
            }
            wb::Node::Diagram { path, name } => {
                if filter.is_empty() || name.to_ascii_lowercase().contains(filter) {
                    out.push(Row { path: path.clone(), depth, name: name.clone(), kind: RowKind::Diagram });
                }
            }
        }
    }
}

fn any_match(nodes: &[wb::Node], filter: &str) -> bool {
    nodes.iter().any(|n| match n {
        wb::Node::Folder { name, children, .. } => name.to_ascii_lowercase().contains(filter) || any_match(children, filter),
        wb::Node::Diagram { name, .. } => name.to_ascii_lowercase().contains(filter),
    })
}

pub struct Browser<'a> {
    pub state: &'a State,
    /// `App`'s own field, not the panel's — see `State::rescan`'s doc comment.
    pub registry: Option<&'a registry::Registry>,
}

impl Widget for Browser<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let title = match &s.root {
            Some(root) => format!("workbench — {}", root.display()),
            None => "workbench — pick one".to_string(),
        };
        let inner = chrome::panel(buf, area, &title, theme::t().aqua);
        let hint = match (s.filtering, &s.editing, &s.grabbed, &s.root) {
            (true, _, _, _) => " type to search — enter keeps it, esc clears it",
            (false, Some(_), _, _) => " type a name — enter, or esc",
            (false, None, Some(_), _) => " navigate, then p to put it here — esc cancels the move",
            (false, None, None, Some(_)) => " j/k move  ^d/^u page  enter open  i insert  / search  g/ grep  n diagram  N folder  r rename  m move  d delete  esc",
            (false, None, None, None) => " j/k move  ^d/^u page  enter opens it  esc closes",
        };
        let body = chrome::hint(buf, inner, hint);
        if body.height < 2 {
            return;
        }

        let mut lines: Vec<Line> = Vec::new();
        if s.root.is_some() && (s.filtering || !s.filter.is_empty()) {
            let cursor = if s.filtering { "█" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(" /", Style::new().fg(theme::t().aqua).bold()),
                Span::styled(s.filter.clone(), Style::new().fg(theme::t().ink)),
                Span::styled(cursor, Style::new().fg(theme::t().aqua)),
            ]));
        }
        if s.root.is_none() {
            if s.recents.is_empty() {
                lines.push(Line::styled(" no workbench opened yet — :workbench <path>", Style::new().fg(theme::t().dim)));
            }
            let view = body.height as usize;
            let off = s.scroll(s.recents.len(), view);
            for (i, p) in s.recents.iter().enumerate().skip(off).take(view) {
                let on = i == s.sel;
                let style = if on { Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold() } else { Style::new().fg(theme::t().ink) };
                lines.push(Line::styled(format!("{}{}", chrome::marker(on), p.display()), style));
            }
            Paragraph::new(lines).render(body, buf);
            return;
        }

        let all = s.display_rows(self.registry);
        if all.is_empty() && s.editing.is_none() && !s.filter.is_empty() {
            lines.push(Line::styled(format!(" nothing matches {:?}", s.filter), Style::new().fg(theme::t().red)));
        } else if all.is_empty() && s.editing.is_none() {
            lines.push(Line::styled(" empty — n adds a diagram, N a folder", Style::new().fg(theme::t().dim)));
        }
        // Scrolled the same way `ctxmenu`'s own list is: `row_at`'s hit-test reads the exact
        // same `scroll` a long tree needs to keep `sel` in view at all, rather than every row
        // past whatever fits just running off the bottom of the panel.
        let view = (body.height as usize).saturating_sub(s.header_lines() as usize);
        let off = s.scroll(all.len(), view);
        for (i, row) in all.iter().enumerate().skip(off).take(view) {
            let on = i == s.sel;
            match row {
                DisplayRow::Fs(row) => {
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
                DisplayRow::Heading => {
                    lines.push(Line::styled(fit(" ──────────", body.width as usize), Style::new().fg(theme::t().dim)));
                }
                DisplayRow::Group { label, depth, expanded, .. } => {
                    let indent = "  ".repeat(*depth);
                    let glyph = if *expanded { "▾" } else { "▸" };
                    let text = format!(" {indent}{glyph} {label}");
                    let style = if on { Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold() } else { Style::new().fg(theme::t().yellow) };
                    lines.push(Line::styled(fit(&text, body.width as usize), style));
                }
                DisplayRow::Element { name, depth, seen_in, .. } => {
                    let indent = "  ".repeat(*depth);
                    let text = if *seen_in > 1 { format!(" {indent}◆ {name} ({seen_in})") } else { format!(" {indent}◆ {name}") };
                    let style = if on { Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold() } else { Style::new().fg(theme::t().ink) };
                    lines.push(Line::styled(fit(&text, body.width as usize), style));
                }
            }
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
    use std::collections::HashMap;

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
        let closed = rows(&sample(), &expanded, "");
        assert_eq!(closed.len(), 2, "Billing folded, plus the top-level diagram");
        assert!(matches!(closed[0].kind, RowKind::Folder { expanded: false }));

        let mut expanded = HashSet::new();
        expanded.insert(PathBuf::from("/root/Billing"));
        let opened = rows(&sample(), &expanded, "");
        assert_eq!(opened.len(), 3);
        assert_eq!(opened[1].name, "Overview.json");
        assert_eq!(opened[1].depth, 1);
    }

    #[test]
    fn a_search_ignores_fold_state_and_shows_every_matching_path() {
        // Billing is folded, but searching for its own child must still reveal it — the
        // same rule tree.rs's search already uses.
        let rows = rows(&sample(), &HashSet::new(), "overview");
        assert_eq!(rows.len(), 2, "Billing (a path to the match) plus the match itself");
        assert!(matches!(rows[0].kind, RowKind::Folder { expanded: true }), "forced open by the search");
        assert_eq!(rows[1].name, "Overview.json");

        // A query matching nothing at all shows nothing, not everything.
        assert!(rows_for("nothing-like-this").is_empty());
    }

    fn rows_for(filter: &str) -> Vec<Row> {
        rows(&sample(), &HashSet::new(), filter)
    }

    #[test]
    fn current_dir_is_the_selected_folder_or_a_diagram_s_own_parent_or_the_root() {
        let mut st = State::opened(PathBuf::from("/root"));
        st.nodes = sample();
        st.sel = 0;
        assert_eq!(st.current_dir(None), Some(PathBuf::from("/root/Billing")), "a folder row: itself");
        st.sel = 1;
        assert_eq!(st.current_dir(None), Some(PathBuf::from("/root")), "a diagram row: its own parent");
        st.nodes = Vec::new();
        assert_eq!(st.current_dir(None), Some(PathBuf::from("/root")), "nothing selected: the root");
    }

    #[test]
    fn selection_wraps() {
        let mut s = State::opened(PathBuf::from("/root"));
        s.move_by(-1, 3);
        assert_eq!(s.sel, 2);
        s.move_by(1, 3);
        assert_eq!(s.sel, 0);
        s.move_by(1, 0);
        assert_eq!(s.sel, 0, "nothing to walk: stays put rather than dividing by zero");
    }

    #[test]
    fn recents_starts_rootless_and_expand_toggle_group_each_flip_their_own_set() {
        let st = State::recents(vec![PathBuf::from("/a")]);
        assert!(st.root.is_none(), "no folder chosen yet");
        assert_eq!(st.recents, vec![PathBuf::from("/a")]);

        let mut st = State::opened(PathBuf::from("/root"));
        let p = PathBuf::from("/root/Billing");
        assert!(!st.expanded.contains(&p));
        st.expand(&p);
        assert!(st.expanded.contains(&p));
        st.toggle_expanded(&p);
        assert!(!st.expanded.contains(&p), "toggle closes what expand opened");
        st.toggle_expanded(&p);
        assert!(st.expanded.contains(&p), "toggle opens it again");

        assert!(!st.groups_expanded.contains(&GroupId::Root));
        st.toggle_group(GroupId::Root);
        assert!(st.groups_expanded.contains(&GroupId::Root));
        st.toggle_group(GroupId::Root);
        assert!(!st.groups_expanded.contains(&GroupId::Root), "toggle again closes it");
    }

    fn entry(kind: ShapeKind, name: &str) -> registry::Entry {
        registry::Entry {
            key: registry::Key { kind, ident: registry::Ident::Name(name.into()) },
            name: name.into(),
            api_name: None,
            properties: Vec::new(),
            status: crate::model::Status::Active,
            seen_in: 1,
            seen_paths: Vec::new(),
        }
    }

    #[test]
    fn a_row_s_own_entry_is_what_move_and_delete_take() {
        let rows = rows(&sample(), &HashSet::new(), "");
        let e = rows[0].entry();
        assert_eq!(e.path, PathBuf::from("/root/Billing"));
        assert_eq!(e.kind, wb::NodeKind::Folder);
    }

    #[test]
    fn the_browser_draws_recents_a_tree_being_edited_and_the_registry_s_groups() {
        let render = |w: u16, h: u16, st: &State, registry: Option<&registry::Registry>| -> Vec<String> {
            let area = Rect::new(0, 0, w, h);
            let mut buf = Buffer::empty(area);
            Browser { state: st, registry }.render(area, &mut buf);
            (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>()).collect()
        };
        let dump = |out: &[String]| out.concat();

        // No folder chosen yet: the recents list, or the reason there is none.
        let empty = State::recents(Vec::new());
        let out = dump(&render(50, 8, &empty, None));
        assert!(out.contains("no workbench opened yet"), "{out}");
        let picked = State::recents(vec![PathBuf::from("/a"), PathBuf::from("/b")]);
        let out = dump(&render(50, 8, &picked, None));
        assert!(out.contains("/a") && out.contains("/b"), "{out}");

        // A real tree, typing a rename, then typing a brand new name.
        let mut st = State::opened(PathBuf::from("/root"));
        st.nodes = sample();
        st.sel = 0;
        st.editing = Some((Typing::Rename, "renamed".into()));
        let out = dump(&render(50, 8, &st, None));
        assert!(out.contains("renamed"), "typing a rename shows the buffer in place: {out}");
        st.editing = Some((Typing::NewFolder, "fresh".into()));
        let out = dump(&render(50, 8, &st, None));
        assert!(out.contains("new:") && out.contains("fresh"), "{out}");
        st.editing = None;

        // A row grabbed for a move reads differently, and asks for `p` instead of the usual keys.
        st.grabbed = Some(rows(&sample(), &HashSet::new(), "")[1].entry());
        let out = dump(&render(50, 8, &st, None));
        assert!(out.contains("p to put it here"), "{out}");
        st.grabbed = None;

        // The registry's own section: closed root, an open layer with a closed kind under
        // it, and a second layer left closed — both branches `display_rows` skips over.
        let reg = registry::Registry {
            entries: HashMap::from([
                (registry::Key { kind: ShapeKind::ApplicationComponent, ident: registry::Ident::Name("CRM".into()) }, entry(ShapeKind::ApplicationComponent, "CRM")),
                (registry::Key { kind: ShapeKind::BusinessActor, ident: registry::Ident::Name("Ops".into()) }, entry(ShapeKind::BusinessActor, "Ops")),
            ]),
            edges: Vec::new(),
        };
        let out = dump(&render(50, 12, &st, Some(&reg)));
        assert!(out.contains("elements"), "the section itself always shows, once the registry has anything: {out}");
        assert!(!out.contains("CRM") && !out.contains("Ops"), "but nothing under it while the root is closed: {out}");
        st.toggle_group(GroupId::Root);
        st.toggle_group(GroupId::Layer(ShapeKind::ApplicationComponent.layer()));
        let out = dump(&render(50, 12, &st, Some(&reg)));
        assert!(out.contains("elements"), "the root section, opened: {out}");
        assert!(!out.contains("CRM"), "its kind group is still closed: {out}");
        st.toggle_group(GroupId::Kind(ShapeKind::ApplicationComponent));
        let out = dump(&render(50, 12, &st, Some(&reg)));
        assert!(out.contains("CRM"), "opened all the way down to the element: {out}");
    }

    #[test]
    fn row_at_finds_the_row_under_the_title_bar_and_hint_line_excluded() {
        let mut st = State::opened(PathBuf::from("/root"));
        st.nodes = sample();
        let area = Rect::new(0, 0, 50, 10);
        let n = st.display_rows(None).len();
        assert_eq!(st.row_at(area, n, 5, 0), None, "the title bar");
        assert_eq!(st.row_at(area, n, 5, 1), Some(0), "the first row, right under it");
        assert_eq!(st.row_at(area, n, 0, 1), None, "off the left edge");
        assert_eq!(st.row_at(area, n, 5, 100), None, "well past the last visible row");
    }

    #[test]
    fn row_at_skips_the_filter_line_when_one_is_showing() {
        let mut st = State::opened(PathBuf::from("/root"));
        st.nodes = sample();
        st.filtering = true;
        let area = Rect::new(0, 0, 50, 10);
        let n = st.display_rows(None).len();
        assert_eq!(st.row_at(area, n, 5, 1), None, "the / line itself, not a row");
        assert_eq!(st.row_at(area, n, 5, 2), Some(0), "the first real row, one line lower");
    }

    #[test]
    fn row_at_agrees_with_a_long_list_s_own_scroll() {
        let recents: Vec<PathBuf> = (0..20).map(|i| PathBuf::from(format!("/w{i}"))).collect();
        let mut st = State::recents(recents);
        st.sel = 15;
        let area = Rect::new(0, 0, 50, 10);
        // The title row and the hint row are the only two `panel`/`hint` spend here, so the
        // body is 8 rows tall — `scroll` keeps `sel` centred rather than pinned to an edge.
        let off = st.scroll(20, 8);
        assert_eq!(st.row_at(area, 20, 5, 1), Some(off), "the first visible row is wherever scroll put it");
        assert_eq!(st.row_at(area, 20, 5, 1 + (15 - off) as u16), Some(15), "sel itself is still findable, scrolled into view");
    }
}
