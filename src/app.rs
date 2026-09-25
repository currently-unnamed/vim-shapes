use std::path::PathBuf;

use crate::model::{ArrowHead, Document, Edge, EdgeStyle, Node, NodeId, NodeStyle, ShapeKind};

const MIN_NODE_W: f64 = 4.0;
const MIN_NODE_H: f64 = 2.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    Command { buffer: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Canvas,
}

/// State while aiming a connection: `from` is fixed (the node the connect
/// started on), `target` hops between nodes as the user navigates with
/// hjkl, same as normal canvas navigation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectState {
    pub from: NodeId,
    pub target: NodeId,
}

pub struct App {
    pub document: Document,
    pub mode: Mode,
    pub focus: Focus,
    /// The node currently under keyboard focus on the canvas. Navigation
    /// (hjkl) hops this between existing nodes spatially - there is no
    /// free-roaming cursor.
    pub focused_node: Option<NodeId>,
    pub pending_connect: Option<ConnectState>,
    pub sidebar_index: usize,
    pub pending_count: Option<u32>,
    pub insert_scratch: String,
    pub status_message: Option<String>,
    pub last_saved_path: Option<PathBuf>,
    pub should_quit: bool,
    /// Set after a single Ctrl+W; a second Ctrl+W while this is set toggles
    /// focus (mirroring vim's <C-w><C-w>). Any other key clears it.
    pub ctrl_w_pending: bool,
}

impl Default for App {
    fn default() -> Self {
        App {
            document: Document::default(),
            mode: Mode::Normal,
            focus: Focus::Sidebar,
            focused_node: None,
            pending_connect: None,
            sidebar_index: 0,
            pending_count: None,
            insert_scratch: String::new(),
            status_message: None,
            last_saved_path: None,
            should_quit: false,
            ctrl_w_pending: false,
        }
    }
}

impl App {
    pub fn selected_shape_kind(&self) -> ShapeKind {
        ShapeKind::ALL[self.sidebar_index]
    }

    pub fn take_count(&mut self) -> u32 {
        self.pending_count.take().unwrap_or(1)
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Sidebar => Focus::Canvas,
            Focus::Canvas => Focus::Sidebar,
        };
        if self.focus == Focus::Canvas {
            self.ensure_focused_node();
        }
    }

    fn ensure_focused_node(&mut self) {
        let still_valid = self.focused_node.is_some_and(|id| self.document.node(id).is_some());
        if !still_valid {
            self.focused_node = self.document.nodes.first().map(|n| n.id);
        }
    }

    fn next_placement_position(&self) -> (f64, f64) {
        let n = self.document.nodes.len();
        let col = (n % 4) as f64;
        let row = (n / 4) as f64;
        (2.0 + col * 18.0, 2.0 + row * 10.0)
    }

    pub fn place_selected_shape(&mut self) {
        let kind = self.selected_shape_kind();
        let (w, h) = kind.default_size();
        let (x, y) = self.next_placement_position();
        let id = self.document.alloc_node_id();
        self.document.nodes.push(Node { id, kind, x, y, w, h, label: String::new(), style: NodeStyle::default() });
        self.focus = Focus::Canvas;
        self.focused_node = Some(id);
        self.status_message = Some(format!("placed {}", kind.name()));
    }

    /// Moves canvas keyboard focus to the nearest node in the given
    /// direction (dx, dy each in {-1, 0, 1}). No-op if there is no node in
    /// that direction, or none focused yet with none to fall back to.
    pub fn navigate(&mut self, dx: f64, dy: f64) {
        match self.focused_node {
            Some(id) => {
                if let Some(next) = nearest_node_in_direction(&self.document, id, dx, dy) {
                    self.focused_node = Some(next);
                }
            }
            None => self.focused_node = self.document.nodes.first().map(|n| n.id),
        }
    }

    /// Repositions the focused node itself (Ctrl+hjkl), as opposed to
    /// `navigate`, which moves focus between existing nodes.
    pub fn move_focused(&mut self, dx: f64, dy: f64) {
        if let Some(id) = self.focused_node
            && let Some(node) = self.document.node_mut(id) {
                node.x = (node.x + dx).max(0.0);
                node.y = (node.y + dy).max(0.0);
            }
    }

    pub fn resize_focused(&mut self, dw: f64, dh: f64) {
        if let Some(id) = self.focused_node
            && let Some(node) = self.document.node_mut(id) {
                node.w = (node.w + dw).max(MIN_NODE_W);
                node.h = (node.h + dh).max(MIN_NODE_H);
            }
    }

    pub fn start_connect(&mut self) {
        if let Some(from) = self.focused_node {
            self.pending_connect = Some(ConnectState { from, target: from });
        }
    }

    pub fn move_connect_target(&mut self, dx: f64, dy: f64) {
        if let Some(state) = &mut self.pending_connect
            && let Some(next) = nearest_node_in_direction(&self.document, state.target, dx, dy) {
                state.target = next;
            }
    }

    pub fn confirm_connect(&mut self) {
        if let Some(state) = self.pending_connect.take()
            && state.target != state.from {
                let id = self.document.alloc_edge_id();
                self.document.edges.push(Edge {
                    id,
                    from: state.from,
                    to: state.target,
                    label: None,
                    style: EdgeStyle { arrow_head: ArrowHead::Filled, line: Default::default() },
                });
                self.status_message = Some("connected".to_string());
            }
    }

    pub fn cancel_connect(&mut self) {
        self.pending_connect = None;
    }

    pub fn start_label_edit(&mut self) {
        if let Some(id) = self.focused_node
            && let Some(node) = self.document.node(id) {
                self.insert_scratch = node.label.clone();
                self.mode = Mode::Insert;
            }
    }

    pub fn commit_label_edit(&mut self) {
        if let Some(id) = self.focused_node
            && let Some(node) = self.document.node_mut(id) {
                node.label = std::mem::take(&mut self.insert_scratch);
            }
        self.mode = Mode::Normal;
    }

    pub fn delete_focused(&mut self) {
        if let Some(id) = self.focused_node {
            self.document.remove_node(id);
            self.focused_node = None;
            self.pending_connect = None;
            self.status_message = Some("deleted".to_string());
        }
    }
}

/// Among nodes other than `from`, picks the one whose center lies most
/// directly in the (dx, dy) direction: strictly on that side (a positive
/// projection onto the direction vector), minimizing primary-axis distance
/// plus a penalty for drifting off-axis. Pure and independent of rendering.
pub fn nearest_node_in_direction(document: &Document, from: NodeId, dx: f64, dy: f64) -> Option<NodeId> {
    let from_center = document.node(from)?.center();
    document
        .nodes
        .iter()
        .filter(|n| n.id != from)
        .filter_map(|n| {
            let center = n.center();
            let (ddx, ddy) = (center.0 - from_center.0, center.1 - from_center.1);
            let primary = ddx * dx + ddy * dy;
            if primary <= 0.0 {
                return None;
            }
            let secondary = (ddx * dy - ddy * dx).abs();
            Some((n.id, primary + secondary * 2.0))
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(doc: &mut Document, kind: ShapeKind, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        let id = doc.alloc_node_id();
        doc.nodes.push(Node { id, kind, x, y, w, h, label: String::new(), style: NodeStyle::default() });
        id
    }

    #[test]
    fn navigate_moves_focus_to_nearest_node_to_the_right() {
        let mut app = App::default();
        let a = place(&mut app.document, ShapeKind::RoundedRectangle, 0.0, 0.0, 4.0, 4.0);
        let b = place(&mut app.document, ShapeKind::RoundedRectangle, 20.0, 0.0, 4.0, 4.0);
        app.focused_node = Some(a);
        app.navigate(1.0, 0.0);
        assert_eq!(app.focused_node, Some(b));
    }

    #[test]
    fn navigate_prefers_axis_aligned_node_at_equal_distance_over_an_off_axis_one() {
        let mut app = App::default();
        let a = place(&mut app.document, ShapeKind::RoundedRectangle, 0.0, 0.0, 4.0, 4.0);
        // Same primary (x) distance from `a` as `aligned`, but offset on the
        // y axis - the off-axis penalty should make it lose even though its
        // straight-line distance from `a` is comparable.
        let _off_axis = place(&mut app.document, ShapeKind::RoundedRectangle, 15.0, 3.0, 4.0, 4.0);
        let aligned = place(&mut app.document, ShapeKind::RoundedRectangle, 15.0, 0.0, 4.0, 4.0);
        app.focused_node = Some(a);
        app.navigate(1.0, 0.0);
        assert_eq!(app.focused_node, Some(aligned));
    }

    #[test]
    fn navigate_is_noop_when_nothing_lies_in_that_direction() {
        let mut app = App::default();
        let a = place(&mut app.document, ShapeKind::RoundedRectangle, 10.0, 10.0, 4.0, 4.0);
        app.focused_node = Some(a);
        app.navigate(-1.0, 0.0);
        assert_eq!(app.focused_node, Some(a));
    }

    #[test]
    fn resize_floors_at_minimum_size() {
        let mut app = App::default();
        let id = place(&mut app.document, ShapeKind::RoundedRectangle, 0.0, 0.0, MIN_NODE_W, MIN_NODE_H);
        app.focused_node = Some(id);
        app.resize_focused(-100.0, -100.0);
        let node = app.document.node(id).unwrap();
        assert_eq!(node.w, MIN_NODE_W);
        assert_eq!(node.h, MIN_NODE_H);
    }

    #[test]
    fn confirm_connect_creates_edge_between_distinct_nodes() {
        let mut app = App::default();
        let a = place(&mut app.document, ShapeKind::RoundedRectangle, 0.0, 0.0, 4.0, 4.0);
        let b = place(&mut app.document, ShapeKind::RoundedRectangle, 20.0, 0.0, 4.0, 4.0);
        app.focused_node = Some(a);
        app.start_connect();
        app.move_connect_target(1.0, 0.0);
        app.confirm_connect();
        assert_eq!(app.document.edges.len(), 1);
        assert_eq!(app.document.edges[0].from, a);
        assert_eq!(app.document.edges[0].to, b);
    }

    #[test]
    fn confirm_connect_is_noop_when_target_never_moved_off_the_source() {
        let mut app = App::default();
        let a = place(&mut app.document, ShapeKind::RoundedRectangle, 0.0, 0.0, 4.0, 4.0);
        app.focused_node = Some(a);
        app.start_connect();
        app.confirm_connect();
        assert!(app.document.edges.is_empty());
    }

    #[test]
    fn delete_focused_clears_focus() {
        let mut app = App::default();
        let a = place(&mut app.document, ShapeKind::RoundedRectangle, 0.0, 0.0, 4.0, 4.0);
        app.focused_node = Some(a);
        app.delete_focused();
        assert_eq!(app.focused_node, None);
        assert!(app.document.nodes.is_empty());
    }

    #[test]
    fn toggle_focus_into_canvas_selects_first_node_when_none_focused() {
        let mut app = App::default();
        let a = place(&mut app.document, ShapeKind::RoundedRectangle, 0.0, 0.0, 4.0, 4.0);
        app.focus = Focus::Sidebar;
        app.toggle_focus();
        assert_eq!(app.focus, Focus::Canvas);
        assert_eq!(app.focused_node, Some(a));
    }

    #[test]
    fn toggle_focus_into_canvas_falls_back_when_previously_focused_node_was_deleted() {
        let mut app = App::default();
        let a = place(&mut app.document, ShapeKind::RoundedRectangle, 0.0, 0.0, 4.0, 4.0);
        app.focused_node = Some(999); // stale id, e.g. after a delete
        let _ = a;
        app.focus = Focus::Sidebar;
        app.toggle_focus();
        assert_eq!(app.focused_node, Some(a));
    }
}
