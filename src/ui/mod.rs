//! The app: state, the event loop's two halves — `on_key` and `draw` — and what each
//! ex-command does.
//!
//! Tables describe; this file acts. `keymap::COMMANDS` says what a key can do and when;
//! `excmd::COMMANDS` says what a typed word names; the ontology says what a relation may join.
//! None of them can change anything. Everything that changes the document goes through a
//! method here, after a `checkpoint`, so that every edit is one undo step.

pub mod canvas;
pub mod chrome;
pub mod cmdline;
pub mod colour;
pub mod ctxmenu;
pub mod debug;
pub mod excmd;
pub mod expandpick;
pub mod exportdlg;
pub mod form;
pub mod help;
pub mod importdlg;
pub mod keymap;
pub mod layers;
pub mod manual;
pub mod palette;
pub mod props;
pub mod relpick;
pub mod sheet;
pub mod splash;
pub mod start;
pub mod tabpick;
pub mod theme;
pub mod tree;
pub mod wildmenu;
pub mod wire;
pub mod workbench;

use std::collections::HashMap;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::layout;
use crate::model::{Document, Element, ElementId, Node, Relation, RelationId, Tab, Workspace};
use crate::ontology::{idiom, Colour, RelationKind, ShapeKind, View};
use crate::persistence;

/// A tab's name as a scratch file's stem.
fn exportdlg_slug(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect()
}

/// Which of the file `:import` formats a file is, by its content rather than its name — a
/// draw.io file's root is always `<mxfile`, and a peek at the first few KB is enough to see
/// it without reading a large ArchiMate model (or, here, a much larger Foundry ontology
/// export) twice over.
fn sniff_is_drawio(path: &std::path::Path) -> Result<bool, String> {
    Ok(sniff_prefix(path)?.contains("<mxfile"))
}

/// A Foundry ontology export's own top-level shape: JSON, with `"objectTypes"` — the second
/// key Foundry itself writes, right after `"version"` — inside a 4 KB prefix. `"actionTypes"`
/// is a later top-level key, written only after the entire (often many-megabyte) object type
/// array, so it is never in reach of a cheap prefix read and cannot be part of this check.
fn sniff_is_foundry_ontology(path: &std::path::Path) -> Result<bool, String> {
    let prefix = sniff_prefix(path)?;
    Ok(prefix.trim_start().starts_with('{') && prefix.contains("\"objectTypes\""))
}

fn sniff_prefix(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| format!("could not read file: {e}"))?;
    let mut buf = [0u8; 4096];
    let n = f.read(&mut buf).map_err(|e| format!("could not read file: {e}"))?;
    Ok(String::from_utf8_lossy(&buf[..n]).into_owned())
}
use canvas::Target;
use keymap::{Avail, Focus, Mode, Prefix, Resolved, Spot, Where};

/// How many undo steps are kept.
const UNDO_DEPTH: usize = 200;

/// What a footer question is asking, and what happens on `y`.
#[derive(Clone, Debug, PartialEq)]
enum Confirm {
    DeleteElement(ElementId),
    DeletePicked,
    /// Close a tab whose diagram is not on disk.
    CloseTab,
    /// Throw away unsaved work, and then…
    Discard(Pending),
    /// `d` in the workbench panel — a real `rm`, on a file or a whole folder.
    DeleteWorkbenchEntry(crate::workbench::Entry),
}

#[derive(Clone, Debug, PartialEq)]
enum Pending {
    Quit,
    New,
    Open(PathBuf),
    Import(PathBuf),
}

impl Pending {
    fn verb(&self) -> &'static str {
        match self {
            Pending::Quit => "quit",
            Pending::New => "start a new diagram",
            Pending::Open(_) => "open another",
            Pending::Import(_) => "import a draw.io file",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tone {
    Good,
    Bad,
    Note,
}

struct Insert {
    target: Target,
    buf: String,
}

/// A stop on the walk: a shape, or a relation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stop {
    Shape(ElementId),
    Relation(RelationId),
}

/// Stepped into the cursor's shape: which handle the cursor is on, whether it is in hand for
/// dragging, and whether a relation's end has been picked up off a handle to move.
#[derive(Clone, Copy, Debug)]
struct Reshape {
    /// Index into `Element::handles`: clockwise from the top-left corner.
    handle: usize,
    held: bool,
    /// `(relation, this end is its `from`, the handle it came from)`.
    moving: Option<(RelationId, bool, usize)>,
}

/// Which way each handle faces: `(dx, dy)` in {-1, 0, 1}, clockwise from the top-left.
const HANDLE_DIR: [(f64, f64); 8] = [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0)];

/// How far a handle moves per keypress with a modifier held.
const BIG_STEP: f64 = 4.0;
/// How far one pan step moves the view: a few cells, square-ish on the screen.
const PAN_X: f64 = 8.0;
const PAN_Y: f64 = 4.0;
const MIN_W: f64 = 4.0;
const MIN_H: f64 = 2.0;

/// What a mouse `Down` landed on, kept until the matching `Up` (or the first `Drag`) decides
/// what the gesture meant — a click that never drags must look, to the document, exactly like
/// nothing happened at all.
#[derive(Clone, Debug, PartialEq)]
enum MouseGesture {
    /// An element's body: the ids a drag of it would move (locked ones already filtered out).
    Body(Vec<ElementId>),
    /// A handle of the cursor's element. `self.reshape`, set at the same moment, already says
    /// whether it is a resize or a reroute — the mouse just triggers the same grab `Enter`
    /// would, at `Down` instead of a keystroke.
    Handle,
    /// Empty ground, at this world point — a drag turns it into a marquee.
    Ground((f64, f64)),
}

/// A handle's hit radius, in cells — generous, since a handle is a single point and a mouse
/// arrives at cell resolution.
const HANDLE_HIT: f64 = 0.75;
/// How close a click has to land on a relation's route to focus it rather than miss it.
const RELATION_HIT: f64 = 0.6;
/// How far a hover arrow sits outside its handle — far enough that the two hit-zones
/// (`HANDLE_HIT` around the handle, `ARROW_HIT` around the arrow) never overlap, so a click
/// is never ambiguous between "resize" and "open a new connected shape this way."
const ARROW_GAP: f64 = 1.6;
const ARROW_HIT: f64 = 0.7;

/// A tab that is not the current one: its name, and everything about it that the app keeps
/// per diagram, parked until you switch back. The current tab's state lives in `App`'s own
/// fields, and its slot holds only the name — so every method that says `self.doc` keeps
/// meaning "the diagram you are looking at", and switching is a swap.
#[derive(Default)]
struct TabSlot {
    name: String,
    doc: Document,
    cursor: Option<ElementId>,
    focus: usize,
    camera: (f64, f64),
    undo: Vec<Document>,
    redo: Vec<Document>,
    back: Vec<ElementId>,
    forward: Vec<ElementId>,
    /// Which of this tab's elements came from the resident `ontology` index, and under what
    /// id — how `e` (expand) knows what a placed element really is in the export, and how
    /// far it has already grown. A hand-drawn element, or one from any other import, simply
    /// has no entry.
    ontology_ids: HashMap<ElementId, String>,
}

pub struct App {
    /// The tabs, the current one's slot holding only its name.
    tabs: Vec<TabSlot>,
    pub tab: usize,
    pub doc: Document,
    /// The document as last saved or opened, serialized — dirtiness is a comparison against
    /// this, so undoing back to the saved state correctly reports clean.
    saved: String,
    pub path: Option<PathBuf>,
    pub cursor: Option<ElementId>,
    /// 0 is the element itself; n is its n-th relation, in `Document::incident` order.
    focus: usize,
    /// On a relation, which of its three nodes the cursor is on.
    node: Node,
    /// A shape just opened off another with `o`: once its relation is picked, its label is
    /// the next thing to type.
    pending_label: Option<ElementId>,
    /// `o` or a `^`-direction from inside a shape: which handle the new shape opens out of.
    pending_port: Option<usize>,
    visual: bool,
    selection: Vec<ElementId>,
    holding: Option<ElementId>,
    insert: Option<Insert>,
    reshape: Option<Reshape>,
    /// Presenting: the diagram alone, framed by its bounds.
    present: bool,
    /// The start dialog — up after the title screen when no file was asked for.
    start: Option<start::State>,
    /// The debugging panel is up.
    debug: bool,
    /// The last keys pressed, newest last, for the debugging panel.
    keys: Vec<String>,
    /// What the command table said of the last key: allowed, refused and why, or unknown.
    last_resolved: String,
    pending_prefix: Option<Prefix>,
    count: Option<usize>,
    undo: Vec<Document>,
    redo: Vec<Document>,
    clip: Option<(Vec<Element>, Vec<Relation>)>,
    cmdline: Option<cmdline::State>,
    cmd_history: Vec<String>,
    palette: Option<palette::State>,
    help: Option<help::State>,
    manual: Option<manual::State>,
    relpick: Option<relpick::State>,
    tabpick: Option<tabpick::State>,
    sheet: Option<sheet::State>,
    layers: Option<layers::State>,
    /// The model tree — the folder organization a coArchi import came from, if the workspace
    /// is one; empty otherwise. Never saved: `load_workspace` clears it, since a file this app
    /// wrote back out has no folders of its own to remember.
    model_tree: Vec<crate::archimate_import::ModelNode>,
    /// The whole of a Foundry ontology export, resident but never drawn in full — same
    /// lifecycle as `model_tree`, and set alongside it. `None` outside a Foundry import.
    ontology: Option<crate::foundry_import::Index>,
    /// The current tab's own `TabSlot::ontology_ids`, live — parked and restored on
    /// `switch_tab` exactly like `cursor` or `camera`.
    ontology_ids: HashMap<ElementId, String>,
    tree: Option<tree::State>,
    /// The property browser, on an ontology type's rows, while one is up.
    props: Option<props::State>,
    /// The architecture workbench — a live folder of diagrams, docked on the left, while one
    /// is up. Never saved: it is a window onto a folder, not part of any one diagram.
    workbench: Option<workbench::State>,
    /// The colour picker, over the sheet, while one is up.
    colour: Option<colour::State>,
    /// Colours picked this session, newest first — the picker's recent row.
    recent_colours: Vec<Colour>,
    export: Option<exportdlg::State>,
    /// The import dialog — up when `:import` is typed with no path.
    importdlg: Option<importdlg::State>,
    /// `e` (expand) — up while its list is showing.
    expandpick: Option<expandpick::State>,
    /// The right-click menu — up from an idle right-click until a row runs, or esc.
    ctxmenu: Option<ctxmenu::State>,
    confirm: Option<Confirm>,
    status: Option<(String, Tone)>,
    /// World coordinates of the top-left cell of the view.
    camera: (f64, f64),
    /// The body's size at the last draw, for the camera to keep the cursor inside.
    view_size: (u16, u16),
    /// The diagram body's screen rectangle at the last draw, in the terminal's own
    /// coordinates — the same space a `MouseEvent`'s `column`/`row` arrive in, so a click can
    /// be turned into a world point the same way the camera already turns cells into world.
    body: Rect,
    /// The element under the mouse, updated on every `Moved` — purely so its handles can be
    /// painted before anything is clicked. `self.reshape`'s handles are the ones in hand;
    /// these are the ones a mouse could reach for.
    hover: Option<ElementId>,
    /// What a mouse `Down` is in the middle of, until its `Up`.
    mouse_gesture: Option<MouseGesture>,
    /// Whether at least one `Drag` has arrived since the last `Down` — the one bit of state
    /// that tells a click from a drag, since a terminal reports them as distinct event kinds.
    mouse_dragging: bool,
    /// The last mouse position translated to world coordinates, so a `Drag` event (an
    /// absolute position) can be turned into the delta `move_elements`/`drag_handle` want.
    mouse_pos: Option<(f64, f64)>,
    /// A marquee in progress: its anchor, and where the mouse is now.
    marquee: Option<((f64, f64), (f64, f64))>,
    /// The right button's candidate element for a connection, from `Down` until `Up`.
    connect_from: Option<ElementId>,
    /// A right-drag's own anchor, in raw screen cells (not world ones, which move under it
    /// as the camera does) — set on a right `Down` that starts on empty ground, cleared on
    /// `Up`. `Some` is what tells `mouse_right_drag` to pan instead of trying to connect.
    pan_from: Option<(u16, u16)>,
    /// What the debugging panel's "last mouse" row shows.
    last_mouse: String,
    search: Option<String>,
    back: Vec<ElementId>,
    forward: Vec<ElementId>,
    /// The title screen is up. Any key takes it down.
    pub loading: bool,
    pub should_quit: bool,
    /// The terminal speaks the kitty keyboard protocol, so shift+ctrl+h and ctrl+h arrive
    /// as different keys. Without it they are one byte, and the app has to pick.
    pub enhanced_keys: bool,
    /// Moving the cursor's label (`T`): hjkl drag it, the cursor stays.
    placing: bool,
    /// Panning (`zv`): hjkl move the view, the cursor stays.
    viewing: bool,
    /// The view has been panned away from the cursor on purpose, and stays there until the
    /// cursor moves again.
    panned: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> App {
        let doc = Document::default();
        let mut app = App {
            tabs: vec![TabSlot { name: "diagram 1".into(), ..Default::default() }],
            tab: 0,
            doc,
            saved: String::new(),
            path: None,
            cursor: None,
            focus: 0,
            node: Node::Centre,
            pending_label: None,
            pending_port: None,
            visual: false,
            selection: Vec::new(),
            holding: None,
            insert: None,
            reshape: None,
            present: false,
            start: None,
            debug: false,
            keys: Vec::new(),
            last_resolved: String::new(),
            pending_prefix: None,
            count: None,
            undo: Vec::new(),
            redo: Vec::new(),
            clip: None,
            cmdline: None,
            cmd_history: Vec::new(),
            palette: None,
            help: None,
            manual: None,
            relpick: None,
            tabpick: None,
            sheet: None,
            layers: None,
            model_tree: Vec::new(),
            ontology: None,
            ontology_ids: HashMap::new(),
            tree: None,
            props: None,
            workbench: None,
            colour: None,
            recent_colours: Vec::new(),
            enhanced_keys: false,
            placing: false,
            viewing: false,
            panned: false,
            export: None,
            importdlg: None,
            expandpick: None,
            ctxmenu: None,
            confirm: None,
            status: None,
            camera: (0.0, 0.0),
            view_size: (80, 22),
            body: Rect::new(0, 0, 80, 22),
            hover: None,
            mouse_gesture: None,
            mouse_dragging: false,
            mouse_pos: None,
            marquee: None,
            connect_from: None,
            pan_from: None,
            last_mouse: String::new(),
            search: None,
            back: Vec::new(),
            forward: Vec::new(),
            loading: true,
            should_quit: false,
        };
        app.saved = app.serialized();
        app
    }

    // ─── the workspace, and its tabs ────────────────────────────────────────

    /// The whole workspace as the file would hold it: every parked tab, and the current one
    /// assembled from the app's own fields.
    fn workspace(&self) -> Workspace {
        let tabs = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| Tab {
                name: t.name.clone(),
                diagram: if i == self.tab { self.doc.clone() } else { t.doc.clone() },
            })
            .collect();
        Workspace { version: crate::model::WORKSPACE_VERSION, grid: true, current: self.tab, tabs }
    }

    fn serialized(&self) -> String {
        serde_json::to_string(&self.workspace()).unwrap_or_default()
    }

    /// Replace everything with a workspace — what opening a file and `:new` both do.
    fn load_workspace(&mut self, ws: Workspace) {
        // A model tree only means anything for the import that just made it — the caller
        // sets `model_tree` (and, for a Foundry ontology, `ontology`) again immediately
        // after, if this workspace is one.
        self.model_tree = Vec::new();
        self.ontology = None;
        self.tree = None;
        self.tabs = ws
            .tabs
            .into_iter()
            .map(|t| TabSlot { name: t.name, doc: t.diagram, ..Default::default() })
            .collect();
        // An older file kept one grid switch for the whole workspace: honour it on every tab.
        if !ws.grid {
            for t in &mut self.tabs {
                t.doc.metadata.page.grid = false;
            }
        }
        self.holding = None;
        self.visual = false;
        self.selection.clear();
        self.insert = None;
        // Park nothing: the app's fields are about to be filled from the slot we switch to.
        self.tab = usize::MAX;
        self.switch_tab(ws.current.min(self.tabs.len() - 1));
    }

    /// Park the current tab's state in its slot, and take the other's out.
    fn switch_tab(&mut self, to: usize) {
        if to >= self.tabs.len() {
            return;
        }
        if self.tab < self.tabs.len() {
            let slot = &mut self.tabs[self.tab];
            slot.doc = std::mem::take(&mut self.doc);
            slot.cursor = self.cursor.take();
            slot.focus = std::mem::take(&mut self.focus);
            slot.camera = self.camera;
            slot.undo = std::mem::take(&mut self.undo);
            slot.redo = std::mem::take(&mut self.redo);
            slot.back = std::mem::take(&mut self.back);
            slot.forward = std::mem::take(&mut self.forward);
            slot.ontology_ids = std::mem::take(&mut self.ontology_ids);
        }
        // Leaving a tab drops whatever was half done in it: a relation in hand, a selection.
        self.holding = None;
        self.visual = false;
        self.selection.clear();
        self.tab = to;
        let slot = &mut self.tabs[to];
        self.doc = std::mem::take(&mut slot.doc);
        self.cursor = slot.cursor.take();
        self.focus = std::mem::take(&mut slot.focus);
        self.camera = slot.camera;
        self.undo = std::mem::take(&mut slot.undo);
        self.redo = std::mem::take(&mut slot.redo);
        self.back = std::mem::take(&mut slot.back);
        self.forward = std::mem::take(&mut slot.forward);
        self.ontology_ids = std::mem::take(&mut slot.ontology_ids);
        self.ensure_cursor();
        self.follow_camera();
    }

    pub fn tab_name(&self) -> &str {
        &self.tabs[self.tab].name
    }

    /// A new tab of the given kind, named after its number, and switched to.
    fn new_tab(&mut self, view: View) {
        let n = self.tabs.len() + 1;
        let mut doc = Document::default();
        doc.metadata.view = view;
        self.tabs.push(TabSlot { name: format!("diagram {n}"), doc, ..Default::default() });
        self.switch_tab(self.tabs.len() - 1);
    }

    /// `:tabnew <path>` — a diagram `:export`ed with `Format::Diagram` (or any other single-
    /// tab file this app itself could open), added as a new tab rather than replacing the
    /// workspace the way `:import`/`:open` do. Purely additive — every other tab is
    /// untouched — so, unlike those, this never needs to ask about unsaved work first.
    fn tabnew_from_path(&mut self, path: PathBuf) {
        match persistence::load(&path) {
            Ok(ws) if ws.tabs.len() == 1 => {
                let Tab { name, diagram } = ws.tabs.into_iter().next().expect("checked len == 1");
                self.tabs.push(TabSlot { name, doc: diagram, ..Default::default() });
                self.switch_tab(self.tabs.len() - 1);
                self.say(format!("opened {} as a new tab", path.display()), Tone::Good);
            }
            Ok(ws) => self.say(format!("{} has {} tabs — :open it instead, or :export just one tab and :tabnew that", path.display(), ws.tabs.len()), Tone::Bad),
            Err(e) => self.say(e.to_string(), Tone::Bad),
        }
    }

    /// A `tree::RowKind::Resource` picked: a fresh tab, holding just this one element from
    /// the resident ontology index, ready to `e`xpand. Does nothing if the id is somehow not
    /// (or no longer) in the index — the tree is only ever built from it, so that should not
    /// happen, but a stale row is not worth a panic over.
    fn place_ontology_resource(&mut self, id: &str) {
        self.new_tab(View::Ontology);
        self.checkpoint();
        let Some(index) = &self.ontology else { return };
        let new_id = if let Some(o) = index.object_type(id) {
            crate::foundry_import::add_object_type(&mut self.doc, o)
        } else if let Some(i) = index.interface(id) {
            crate::foundry_import::add_interface(&mut self.doc, i)
        } else if let Some(a) = index.action(id) {
            crate::foundry_import::add_action_type(&mut self.doc, a)
        } else {
            return;
        };
        self.ontology_ids.insert(new_id, id.to_string());
        self.cursor = Some(new_id);
        let name = self.doc.element(new_id).map(|e| e.display()).unwrap_or_default();
        self.tabs[self.tab].name = name.clone();
        self.say(format!("{name} — e to expand what it connects to"), Tone::Good);
    }

    /// A `tree::RowKind::Link` picked: the tree's shortcut for what picking `from`, then `e`,
    /// then this same connection, would otherwise take three moves to do — one undo step,
    /// since `place_ontology_resource`'s own checkpoint already covers everything after it.
    fn place_ontology_link(&mut self, from: &str, to: &str) {
        self.place_ontology_resource(from);
        let Some(source) = self.cursor else { return };
        let Some(edge) = self.ontology.as_ref().and_then(|idx| idx.find_link(from, to)) else { return };
        self.add_ontology_connection(source, &edge);
    }

    /// The other end of an edge from `id`, as an `ElementId` on the current tab — already
    /// placed (found by its raw id in `ontology_ids`), or placed for the first time now. The
    /// edge's own kind tells an opaque target (never in the index by itself) a `Calls`
    /// target is a function and a `BackedBy` target is a datasource.
    fn place_ontology_connection(&mut self, other_id: &str, via: RelationKind) -> ElementId {
        if let Some(&existing) = self.ontology_ids.iter().find(|(_, raw)| raw.as_str() == other_id).map(|(el, _)| el) {
            return existing;
        }
        let Some(index) = &self.ontology else { unreachable!("expand only runs with an ontology resident") };
        let new_id = if let Some(o) = index.object_type(other_id) {
            crate::foundry_import::add_object_type(&mut self.doc, o)
        } else if let Some(i) = index.interface(other_id) {
            crate::foundry_import::add_interface(&mut self.doc, i)
        } else if let Some(a) = index.action(other_id) {
            crate::foundry_import::add_action_type(&mut self.doc, a)
        } else if via == RelationKind::Calls {
            crate::foundry_import::add_function(&mut self.doc, other_id)
        } else {
            crate::foundry_import::add_datasource(&mut self.doc, other_id)
        };
        self.ontology_ids.insert(new_id, other_id.to_string());
        new_id
    }

    /// `e` — expand: every real connection `source` has in the ontology that is not already
    /// on this tab. `Vec::new()` if `source` was never placed from the index, or has none
    /// left — `Where::can_expand` already refused the key in that case.
    fn ontology_connections(&self, source: ElementId) -> Vec<crate::foundry_import::Edge> {
        let (Some(raw_id), Some(index)) = (self.ontology_ids.get(&source), &self.ontology) else { return Vec::new() };
        let placed: std::collections::HashSet<&str> = self.ontology_ids.values().map(String::as_str).collect();
        index.connections(raw_id, &placed).into_iter().cloned().collect()
    }

    /// The link a foreign-key row in the property browser backs, if any — `e` there expands
    /// it, the same connection `e` on the canvas would offer for the object type it belongs
    /// to. `None` for a row that is not a foreign key, or an element the index does not know.
    fn props_fk_edge(&self) -> Option<crate::foundry_import::Edge> {
        let st = self.props.as_ref()?;
        let raw_id = self.ontology_ids.get(&st.target)?;
        let api_name = self.doc.element(st.target)?.properties.get(st.sel)?.api_name.as_deref()?;
        self.ontology.as_ref()?.foreign_key_edge(raw_id, api_name)
    }

    /// Add one connection `source` has in the ontology: place the far end (or find it, if
    /// something else already placed it this session), draw the edge in the direction the
    /// export itself means, and — for a link type — its cardinality and labels too. One
    /// undo step, the same whether this is called once or, from "expand all", many times.
    /// Adds one real connection and returns the far end's `ElementId` — already on this tab,
    /// or placed just now — so a caller can move the cursor there (`e` in the property
    /// browser does; `expandpick`'s own outcome handling does not need to).
    fn add_ontology_connection(&mut self, source: ElementId, edge: &crate::foundry_import::Edge) -> Option<ElementId> {
        let source_raw = self.ontology_ids.get(&source).cloned()?;
        let other_raw = if edge.from == source_raw { &edge.to } else { &edge.from };
        let other_id = self.place_ontology_connection(other_raw, edge.kind);
        let (from_id, to_id) = if edge.from == source_raw { (source, other_id) } else { (other_id, source) };
        let Ok(rel_id) = self.doc.connect(edge.kind, from_id, to_id) else { return Some(other_id) };
        if edge.kind == RelationKind::LinkType {
            crate::foundry_import::apply_link_notation(&mut self.doc, rel_id, edge);
        }
        self.place_expanded(source, other_id, edge.prefers_vertical());
        Some(other_id)
    }

    /// Where a freshly expanded neighbour lands. Side by side by default — a column to the
    /// right of the element being expanded, stacked downward if more than one lands there.
    /// But a line drawn straight across to it carries this edge's own label, and a label too
    /// long for that gutter (`Edge::prefers_vertical`) bleeds into the boxes on either side of
    /// it, or — once elements stop lining up — smears across a diagonal. So a long label gets
    /// a straight line under the element instead, where it has the room a sentence needs.
    /// Neither is a general layout, just enough that expanding one element's neighbourhood
    /// never has to fight the rest of the diagram for space.
    fn place_expanded(&mut self, source: ElementId, new_el: ElementId, vertical: bool) {
        let Some(src) = self.doc.element(source) else { return };
        let (sx, sy, sw, sh) = (src.x, src.y, src.w, src.h);
        let (x, y) = if vertical {
            let below: f64 = self
                .ontology_ids
                .keys()
                .filter_map(|&id| self.doc.element(id))
                .filter(|e| e.id != new_el && e.x < sx + sw && e.x + e.w > sx)
                .map(|e| e.y + e.h + layout::GUT_Y)
                .fold(sy + sh + layout::GUT_Y, f64::max);
            (sx, below)
        } else {
            let below: f64 = self
                .ontology_ids
                .keys()
                .filter_map(|&id| self.doc.element(id))
                .filter(|e| e.id != new_el && e.x >= sx + sw)
                .map(|e| e.y + e.h + layout::GUT_Y)
                .fold(sy, f64::max);
            (sx + sw + layout::GUT_X, below)
        };
        if let Some(e) = self.doc.element_mut(new_el) {
            e.x = x;
            e.y = y;
        }
    }

    /// `e`: open the list of what the cursor's element really connects to. `Where::can_expand`
    /// already refused the key if there is nothing to show, so this is never called empty.
    fn open_expand(&mut self) {
        let Some(source) = self.cursor else { return };
        let edges = self.ontology_connections(source);
        let Some(index) = &self.ontology else { return };
        let source_raw = self.ontology_ids.get(&source).cloned().unwrap_or_default();
        let rows: Vec<expandpick::Row> = edges
            .into_iter()
            .map(|edge| {
                let other = if edge.from == source_raw { &edge.to } else { &edge.from };
                let other_kind = index.kind_of(other, edge.kind);
                let other_label = index.display_name(other, edge.kind);
                expandpick::Row { edge, other_kind, other_label }
            })
            .collect();
        self.expandpick = Some(expandpick::State::new(rows));
    }

    /// Close the current tab. The last tab cannot be closed — `:new` is how you start over.
    fn close_tab(&mut self) {
        if self.tabs.len() == 1 {
            self.say("the only tab — :new starts over, :tabnew adds another", Tone::Bad);
            return;
        }
        let closing = self.tab;
        let name = self.tab_name().to_string();
        self.switch_tab(if closing == 0 { 1 } else { closing - 1 });
        self.tabs.remove(closing);
        if closing < self.tab {
            self.tab -= 1;
        }
        self.say(format!("closed {name}"), Tone::Note);
    }

    // ─── the document ───────────────────────────────────────────────────────

    /// Open a file, replacing whatever is here. The caller has already decided that is fine.
    pub fn open_path(&mut self, path: PathBuf) -> Result<(), String> {
        let ws = persistence::load(&path).map_err(|e| e.to_string())?;
        self.load_workspace(ws);
        self.saved = self.serialized();
        self.path = Some(path.clone());
        self.say(format!("opened {}", path.display()), Tone::Good);
        Ok(())
    }

    /// Import a draw.io file, an ArchiMate exchange file, a coArchi model folder, or a
    /// Foundry ontology export, replacing whatever is here. The caller has already decided
    /// that is fine. A directory is coArchi; otherwise the formats are told apart by content,
    /// not extension — a draw.io file's root is always `<mxfile`, a Foundry export is JSON
    /// with `objectTypes` and `actionTypes` at its top level, and anything else is handed to
    /// the ArchiMate reader, which refuses outright if it isn't one either. Unlike
    /// `open_path`, this never sets `self.path` — none of the four is what `:w` would save
    /// back to, so a bare `:w` afterward asks for a name rather than silently writing JSON
    /// over whatever was just read.
    pub fn import_path(&mut self, path: PathBuf) -> Result<(), String> {
        let mut ontology = None;
        let mut open_tree = false;
        let (ws, tree) = if path.is_dir() {
            let (ws, tree) = crate::archimate_import::import_coarchi(&path).map_err(|e| e.to_string())?;
            persistence::validate_workspace(&ws).map_err(|why| format!("{}: {why}", path.display()))?;
            (ws, tree)
        } else if sniff_is_drawio(&path)? {
            let doc = crate::drawio_import::import(&path).map_err(|e| e.to_string())?;
            persistence::validate(&doc).map_err(|why| format!("{}: {why}", path.display()))?;
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "diagram 1".into());
            (Workspace::single(name, doc), Vec::new())
        } else if sniff_is_foundry_ontology(&path)? {
            // Everything a real export holds is a tangle if drawn at once — so it starts
            // empty, and the tree opens on its own: there is nothing else to do yet but pick
            // the first resource to grow a diagram from.
            let index = crate::foundry_import::import(&path).map_err(|e| e.to_string())?;
            let tree = index.tree();
            ontology = Some(index);
            open_tree = true;
            (Workspace::new(), tree)
        } else {
            let ws = crate::archimate_import::import(&path).map_err(|e| e.to_string())?;
            persistence::validate_workspace(&ws).map_err(|why| format!("{}: {why}", path.display()))?;
            (ws, Vec::new())
        };
        self.load_workspace(ws);
        self.model_tree = tree;
        self.ontology = ontology;
        self.saved = self.serialized();
        self.path = None;
        self.say(format!("imported {}", path.display()), Tone::Good);
        if open_tree {
            self.tree = Some(tree::State::new());
        }
        Ok(())
    }

    fn new_document(&mut self) {
        self.load_workspace(Workspace::new());
        self.saved = self.serialized();
        self.path = None;
    }

    pub fn dirty(&self) -> bool {
        self.serialized() != self.saved
    }

    fn checkpoint(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            let now = std::mem::replace(&mut self.doc, prev);
            self.redo.push(now);
            self.after_document_change();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            let now = std::mem::replace(&mut self.doc, next);
            self.undo.push(now);
            self.after_document_change();
        }
    }

    fn after_document_change(&mut self) {
        self.holding = None;
        self.reshape = None;
        // A pick of something an undo took away is no pick.
        self.selection.retain(|id| self.doc.element(*id).is_some());
        // The sheet reads the document; after an undo it must read it again.
        if let Some(sh) = &mut self.sheet {
            sh.refresh(&self.doc);
        }
        self.selection.retain(|id| self.doc.element(*id).is_some());
        self.ensure_cursor();
    }

    fn say(&mut self, msg: impl Into<String>, tone: Tone) {
        self.status = Some((msg.into(), tone));
    }

    // ─── where the cursor is ────────────────────────────────────────────────

    /// Elements in reading order: top to bottom, then left to right.
    fn ordered(&self) -> Vec<ElementId> {
        let mut v: Vec<&Element> = self.doc.elements.iter().filter(|e| self.doc.element_visible(e.id)).collect();
        v.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap().then(a.x.partial_cmp(&b.x).unwrap()).then(a.id.cmp(&b.id)));
        v.into_iter().map(|e| e.id).collect()
    }

    fn ensure_cursor(&mut self) {
        let valid = self.cursor.is_some_and(|id| self.doc.element(id).is_some() && self.doc.element_visible(id));
        if !valid {
            self.cursor = self.ordered().first().copied();
            self.focus = 0;
        }
        let n = self.cursor.map_or(0, |id| self.doc.incident(id).len());
        if self.focus > n {
            self.focus = 0;
        }
    }

    fn set_cursor(&mut self, id: ElementId) {
        self.cursor = Some(id);
        self.focus = 0;
        self.node = Node::Centre;
        self.panned = false;
    }

    /// Move the view by `(dx, dy)` cells, and keep it there: a pan is on purpose, and the
    /// camera stops following the cursor until the cursor next moves.
    fn pan(&mut self, dx: f64, dy: f64) {
        self.camera = (self.camera.0 + dx, self.camera.1 + dy);
        self.panned = true;
    }

    /// A pan's steps: a few cells, or half the view.
    fn pan_step(&self, c: char, big: bool, count: usize) -> Option<(f64, f64)> {
        let (sx, sy) = if big { ((self.view_size.0 / 2) as f64, (self.view_size.1 / 2) as f64) } else { (PAN_X, PAN_Y) };
        let n = count as f64;
        Some(match c.to_ascii_lowercase() {
            'h' => (-sx * n, 0.0),
            'l' => (sx * n, 0.0),
            'k' => (0.0, -sy * n),
            'j' => (0.0, sy * n),
            _ => return None,
        })
    }

    /// `T`: start dragging the cursor's label — one undo step for the whole drag.
    fn start_placing(&mut self) {
        let Some(target) = self.target() else { return };
        if target == Target::Diagram {
            return;
        }
        if form::locked(&self.doc, target) {
            self.say(form::LOCKED, Tone::Bad);
            return;
        }
        self.checkpoint();
        self.placing = true;
        self.say("moving the label — hjkl drag it anywhere, HJKL four cells, 0 puts it back, esc leaves it", Tone::Note);
    }

    fn nudge_label(&mut self, dx: f64, dy: f64) {
        match self.target() {
            Some(Target::Element(id)) => {
                if let Some(e) = self.doc.element_mut(id) {
                    e.text.offset = (e.text.offset.0 + dx, e.text.offset.1 + dy);
                }
            }
            Some(Target::Relation(id, node)) => {
                if let Some(r) = self.doc.relation_mut(id) {
                    let o = r.offset_mut(node);
                    *o = (o.0 + dx, o.1 + dy);
                }
            }
            _ => {}
        }
    }

    fn reset_label(&mut self) {
        match self.target() {
            Some(Target::Element(id)) => {
                if let Some(e) = self.doc.element_mut(id) {
                    e.text.offset = (0.0, 0.0);
                }
            }
            Some(Target::Relation(id, node)) => {
                if let Some(r) = self.doc.relation_mut(id) {
                    *r.offset_mut(node) = (0.0, 0.0);
                }
            }
            _ => {}
        }
    }

    /// The keys while a label is being moved: the reshape grammar, on the text.
    fn place_key(&mut self, k: KeyEvent, count: usize) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let n = count as f64;
        let step = |c: char, big: bool| -> Option<(f64, f64)> {
            let s = if big { BIG_STEP } else { 1.0 } * n;
            Some(match c {
                'h' => (-s, 0.0),
                'l' => (s, 0.0),
                'k' => (0.0, -s),
                'j' => (0.0, s),
                _ => return None,
            })
        };
        match k.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('T') => self.placing = false,
            KeyCode::Char('0') => self.reset_label(),
            KeyCode::Char(':') => {
                self.placing = false;
                self.cmdline = Some(cmdline::State::new(':'));
            }
            KeyCode::Char('?') => self.help = Some(help::State::new()),
            KeyCode::Left => self.nudge_label(-n, 0.0),
            KeyCode::Right => self.nudge_label(n, 0.0),
            KeyCode::Up => self.nudge_label(0.0, -n),
            KeyCode::Down => self.nudge_label(0.0, n),
            KeyCode::Backspace => self.nudge_label(-BIG_STEP * n, 0.0),
            KeyCode::Char(c) => {
                let big = ctrl || c.is_ascii_uppercase();
                if let Some((dx, dy)) = step(c.to_ascii_lowercase(), big) {
                    self.nudge_label(dx, dy);
                }
            }
            _ => {}
        }
    }

    /// The keys while panning: hjkl the view, HJKL half a screen, esc back.
    fn view_key(&mut self, k: KeyEvent, count: usize) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('v') => self.viewing = false,
            KeyCode::Char(':') => {
                self.viewing = false;
                self.cmdline = Some(cmdline::State::new(':'));
            }
            KeyCode::Char('?') => self.help = Some(help::State::new()),
            KeyCode::Left => self.pan(-PAN_X * count as f64, 0.0),
            KeyCode::Right => self.pan(PAN_X * count as f64, 0.0),
            KeyCode::Up => self.pan(0.0, -PAN_Y * count as f64),
            KeyCode::Down => self.pan(0.0, PAN_Y * count as f64),
            KeyCode::Char(c) => {
                if let Some((dx, dy)) = self.pan_step(c, ctrl || c.is_ascii_uppercase(), count) {
                    self.pan(dx, dy);
                }
            }
            _ => {}
        }
    }

    fn focused_relation(&self) -> Option<RelationId> {
        if self.focus == 0 {
            return None;
        }
        self.doc.incident(self.cursor?).get(self.focus - 1).copied()
    }

    fn cursor_element(&self) -> Option<&Element> {
        self.doc.element(self.cursor?)
    }

    fn mode(&self) -> Mode {
        if self.manual.is_some() {
            Mode::Manual
        } else if self.insert.is_some() {
            Mode::Insert
        } else if self.sheet.as_ref().is_some_and(|s| s.focused) {
            Mode::Sheet
        } else if self.present {
            Mode::Present
        } else if self.placing {
            Mode::Text
        } else if self.viewing {
            Mode::View
        } else if self.reshape.is_some() {
            Mode::Reshape
        } else if self.visual {
            Mode::Visual
        } else {
            Mode::Normal
        }
    }

    /// Where we are, as the command table sees it. A plain snapshot on purpose.
    fn whereami(&self) -> Where {
        let on = self.cursor_element().map(|e| {
            let rels = self.doc.incident(e.id).len();
            let (focus, refused) = match self.focused_relation() {
                Some(r) => (Focus::Relation, self.doc.relation(r).is_some_and(|r| self.doc.check(r).is_err())),
                None => (Focus::Body, false),
            };
            Spot { focus, composite: e.kind.is_composite(), relations: rels, refused }
        });
        Where {
            mode: self.mode(),
            on,
            holding: self.holding.is_some(),
            carrying: self.clip.is_some(),
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            can_back: !self.back.is_empty(),
            can_fwd: !self.forward.is_empty(),
            picked: self.selection.len(),
            searched: self.search.is_some(),
            elements: self.doc.elements.len(),
            dirty: self.dirty(),
            tabs: self.tabs.len(),
            plain_keys: !self.enhanced_keys,
            in_group: self.cursor.is_some_and(|id| self.parent_group(id).is_some()),
            held: self.reshape.is_some_and(|r| r.held),
            patched: self.reshape.zip(self.cursor).is_some_and(|(r, id)| !self.doc.at_port(id, r.handle).is_empty()),
            moving_end: self.reshape.is_some_and(|r| r.moving.is_some()),
            can_expand: self.cursor.is_some_and(|id| !self.ontology_connections(id).is_empty()),
        }
    }

    /// Where the cursor is, in words, for the menu's title.
    fn here_label(&self) -> String {
        if let Some(r) = self.reshape {
            const NAMES: [&str; 8] = ["top-left", "top", "top-right", "right", "bottom-right", "bottom", "bottom-left", "left"];
            let name = self.cursor_element().map(|e| e.display()).unwrap_or_default();
            return format!("reshaping {name} · {} handle{}", NAMES[r.handle.min(7)], if r.held { " in hand" } else { "" });
        }
        if self.visual {
            return format!("picking · {} picked", self.selection.len());
        }
        let Some(e) = self.cursor_element() else { return "empty diagram".into() };
        match self.focused_relation().and_then(|r| self.doc.relation(r)) {
            Some(r) => {
                let other = self.doc.other_end(r.id, e.id).and_then(|o| self.doc.element(o)).map(|o| o.display()).unwrap_or_default();
                let line = if r.from == e.id {
                    format!("{} {} {}", e.display(), r.kind.verb(), other)
                } else {
                    format!("{} {} {}", other, r.kind.verb(), e.display())
                };
                format!("{line} · {}", self.node.name())
            }
            None if self.holding.is_some() => format!("{} · relation in hand", e.display()),
            None => format!("{} · {}", e.display(), e.kind.name().to_ascii_lowercase()),
        }
    }

    // ─── moving ─────────────────────────────────────────────────────────────

    /// The point hjkl measures from. A composite's is its label, up on the top edge: its
    /// centre would coincide with whatever is drawn inside it, and the box would then be
    /// impossible to hop to — or away from.
    fn nav_point(e: &Element) -> (f64, f64) {
        if e.kind.is_composite() { (e.x + 3.0, e.y) } else { e.center() }
    }

    /// Where a relation is, for the walk: the middle of its drawn line.
    fn rel_point(&self, r: &Relation) -> Option<(f64, f64)> {
        let (p1, p2) = self.doc.end_points(r)?;
        Some(((p1.0 + p2.0) / 2.0, (p1.1 + p2.1) / 2.0))
    }

    /// Where the cursor is, for the walk: on a relation, its middle; else the element.
    fn here_point(&self) -> Option<(f64, f64)> {
        match self.focused_relation().and_then(|r| self.doc.relation(r)) {
            Some(r) => self.rel_point(r),
            None => self.cursor_element().map(Self::nav_point),
        }
    }

    /// Among every shape and relation other than the one the cursor is on, the one most
    /// directly in the (dx, dy) direction: strictly on that side, minimizing distance along
    /// the axis plus a penalty off it. A relation is a stop on the walk like any shape — the
    /// two are one concept, and the walk treats them as one.
    fn nearest(&self, dx: f64, dy: f64) -> Option<Stop> {
        let origin = self.here_point()?;
        let on_rel = self.focused_relation();
        let on_el = if on_rel.is_none() { self.cursor } else { None };
        let score = |p: (f64, f64)| -> Option<f64> {
            let (ddx, ddy) = (p.0 - origin.0, (p.1 - origin.1) * 2.0);
            let primary = ddx * dx + ddy * dy;
            if primary <= 0.0 {
                return None;
            }
            Some(primary + (ddx * dy - ddy * dx).abs() * 2.0)
        };
        let shapes = self
            .doc
            .elements
            .iter()
            .filter(|e| Some(e.id) != on_el && self.doc.element_visible(e.id))
            .filter_map(|e| score(Self::nav_point(e)).map(|s| (Stop::Shape(e.id), s)));
        let rels = self
            .doc
            .relations
            .iter()
            .filter(|r| Some(r.id) != on_rel && self.doc.relation_visible(r.id))
            .filter_map(|r| self.rel_point(r).and_then(&score).map(|s| (Stop::Relation(r.id), s)));
        shapes.chain(rels).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|(stop, _)| stop)
    }

    /// Put the cursor on a stop. A relation is stood on from the element it starts at, so
    /// everything that reads "the cursor's element and its focused relation" keeps working.
    fn go_to(&mut self, stop: Stop) {
        match stop {
            Stop::Shape(id) => self.set_cursor(id),
            Stop::Relation(rid) => {
                let Some(r) = self.doc.relation(rid) else { return };
                let from = r.from;
                self.cursor = Some(from);
                self.focus = self.doc.incident(from).iter().position(|&x| x == rid).map_or(0, |i| i + 1);
                self.node = Node::Centre;
            }
        }
    }

    fn navigate(&mut self, dx: f64, dy: f64, count: usize) {
        for _ in 0..count.max(1) {
            match self.nearest(dx, dy) {
                Some(next) => self.go_to(next),
                None => break,
            }
        }
    }

    fn push_jump(&mut self) {
        if let Some(id) = self.cursor {
            self.back.push(id);
            self.forward.clear();
        }
    }

    fn jump_back(&mut self) {
        while let Some(id) = self.back.pop() {
            if self.doc.element(id).is_some() {
                if let Some(cur) = self.cursor {
                    self.forward.push(cur);
                }
                self.set_cursor(id);
                return;
            }
        }
    }

    fn jump_forward(&mut self) {
        while let Some(id) = self.forward.pop() {
            if self.doc.element(id).is_some() {
                if let Some(cur) = self.cursor {
                    self.back.push(cur);
                }
                self.set_cursor(id);
                return;
            }
        }
    }

    /// The letter every element wears while `f` is pending.
    fn letters(&self) -> Vec<(ElementId, char)> {
        let alphabet: Vec<char> = ('a'..='z').chain('A'..='Z').collect();
        self.ordered().into_iter().zip(alphabet).collect()
    }

    fn search_step(&mut self, forward: bool) {
        let Some(q) = self.search.clone() else { return };
        let q = q.to_ascii_lowercase();
        let order = self.ordered();
        if order.is_empty() {
            return;
        }
        let n = order.len();
        let start = self.cursor.and_then(|c| order.iter().position(|&i| i == c)).unwrap_or(0);
        for step in 1..=n {
            let i = if forward { (start + step) % n } else { (start + n - step % n) % n };
            let id = order[i];
            if self.doc.element(id).is_some_and(|e| e.label.to_ascii_lowercase().contains(&q)) {
                self.push_jump();
                self.set_cursor(id);
                return;
            }
        }
        self.say(format!("no label matches {q:?}"), Tone::Bad);
    }

    // ─── the camera ─────────────────────────────────────────────────────────

    fn follow_camera(&mut self) {
        if self.present || self.panned {
            return;
        }
        let Some(e) = self.cursor_element() else { return };
        let (w, h) = (self.view_size.0 as f64, self.view_size.1 as f64);
        let margin = 2.0;
        let (ex, ey, er, eb) = (e.x - margin, e.y - margin, e.right() + margin, e.bottom() + margin);
        let (mut cx, mut cy) = self.camera;
        if er > cx + w {
            cx = er - w;
        }
        if ex < cx {
            cx = ex;
        }
        if eb > cy + h {
            cy = eb - h;
        }
        if ey < cy {
            cy = ey;
        }
        self.camera = (cx.round(), cy.round());
    }

    fn center_camera(&mut self) {
        self.panned = false;
        let Some(e) = self.cursor_element() else { return };
        let (cx, cy) = e.center();
        let (w, h) = (self.view_size.0 as f64, self.view_size.1 as f64);
        self.camera = ((cx - w / 2.0).round(), (cy - h / 2.0).round());
    }

    // ─── editing ────────────────────────────────────────────────────────────

    fn overlaps_any(&self, x: f64, y: f64, w: f64, h: f64, ignore: &[ElementId]) -> bool {
        self.doc.elements.iter().filter(|e| !e.kind.is_composite() && !ignore.contains(&e.id)).any(|e| {
            !(x + w + 1.0 <= e.x || e.right() + 1.0 <= x || y + h + 1.0 <= e.y || e.bottom() + 1.0 <= y)
        })
    }

    /// Somewhere a `w × h` box fits: beside the cursor, then below it, then further along.
    fn free_spot(&self, w: f64, h: f64) -> (f64, f64) {
        let (sx, sy) = match self.cursor_element() {
            Some(e) if e.kind.is_composite() => (e.x + 2.0, e.y + 2.0),
            Some(e) => (e.right() + layout::GUT_X, e.y),
            None => match self.doc.bounds() {
                Some((bx, _, _, bb)) => (bx, bb + layout::GUT_Y),
                None => (self.camera.0 + 2.0, self.camera.1 + 2.0),
            },
        };
        for col in 0..8 {
            for row in 0..12 {
                let (x, y) = (sx + col as f64 * (w + layout::GUT_X), sy + row as f64 * (h + layout::GUT_Y));
                if !self.overlaps_any(x, y, w, h, &[]) {
                    return (x, y);
                }
            }
        }
        (sx, sy)
    }

    fn add_kind(&mut self, kind: ShapeKind) {
        let (w, h) = kind.default_size();
        let (x, y) = self.free_spot(w, h);
        self.checkpoint();
        let id = self.doc.add(kind, "", x, y);
        self.set_cursor(id);
        self.follow_camera();
        let outside = !self.doc.metadata.view.shows(kind);
        self.start_insert(Target::Element(id));
        if outside {
            self.say(
                format!("added {} — outside a {} view; type its label, esc when done", kind.name(), self.doc.metadata.view.name()),
                Tone::Note,
            );
        } else {
            self.say(format!("added {} — type its label, esc when done", kind.name()), Tone::Note);
        }
    }

    fn start_insert(&mut self, target: Target) {
        let buf = match target {
            Target::Element(id) => self.doc.element(id).map(|e| e.label.clone()).unwrap_or_default(),
            Target::Relation(id, node) => self.doc.relation(id).and_then(|r| r.label_at(node)).unwrap_or_default().to_string(),
            // The diagram's text is its title, and the sheet's text tab has it.
            Target::Diagram => self.doc.metadata.title.clone().unwrap_or_default(),
            Target::Picked => String::new(),
        };
        self.insert = Some(Insert { target, buf });
    }

    fn commit_insert(&mut self) {
        let Some(ins) = self.insert.take() else { return };
        let text = ins.buf.trim().to_string();
        let unchanged = match ins.target {
            Target::Element(id) => self.doc.element(id).is_some_and(|e| e.label == text),
            Target::Relation(id, node) => self.doc.relation(id).is_some_and(|r| r.label_at(node).unwrap_or("") == text),
            Target::Diagram => self.doc.metadata.title.clone().unwrap_or_default() == text,
            Target::Picked => true,
        };
        if unchanged {
            return;
        }
        self.checkpoint();
        match ins.target {
            Target::Element(id) => {
                if let Some(e) = self.doc.element_mut(id) {
                    e.label = text;
                }
            }
            Target::Relation(id, node) => {
                if let Some(r) = self.doc.relation_mut(id) {
                    r.set_label_at(node, &text);
                }
            }
            Target::Diagram => self.doc.metadata.title = if text.is_empty() { None } else { Some(text) },
            Target::Picked => {}
        }
    }

    /// `o`: a new shape off the cursor's, related to it. The palette asks the kind of shape;
    /// what happens after is `open_off_finish`. From inside a shape, `port` is the handle the
    /// new shape opens out of, and decides where it lands and where the relation attaches.
    fn open_off(&mut self, port: Option<usize>) {
        let Some(kind) = self.cursor_element().map(|e| e.kind) else { return };
        self.pending_port = port;
        self.palette = Some(palette::State::relating(self.doc.metadata.view, kind, port));
    }

    /// Where a shape of `w × h` opened out of handle `port` of `e` lands: beside that side,
    /// or off that corner, and further along the same way if something is already there.
    fn spot_from_port(&self, e: &Element, port: usize, w: f64, h: f64) -> (f64, f64) {
        let (dx, dy) = HANDLE_DIR[port.min(7)];
        let place = |k: f64| {
            let x = match dx {
                d if d > 0.0 => e.right() + layout::GUT_X + (k - 1.0) * (w + layout::GUT_X),
                d if d < 0.0 => e.x - layout::GUT_X - w - (k - 1.0) * (w + layout::GUT_X),
                _ => e.x + (e.w - w) / 2.0,
            };
            let y = match dy {
                d if d > 0.0 => e.bottom() + layout::GUT_Y + (k - 1.0) * (h + layout::GUT_Y),
                d if d < 0.0 => e.y - layout::GUT_Y - h - (k - 1.0) * (h + layout::GUT_Y),
                _ => e.y + (e.h - h) / 2.0,
            };
            (x, y)
        };
        (1..=10)
            .map(|k| place(k as f64))
            .find(|&(x, y)| !self.overlaps_any(x, y, w, h, &[e.id]))
            .unwrap_or_else(|| place(1.0))
    }

    /// The palette has picked a kind: place it, relate it, and go on to its label. On a
    /// freeform tab the relation is a plain link and needs no picking; on an architecture
    /// tab the picker asks which kind, and the label waits for its answer — unless the row
    /// picked was a suggested line, `via`, which has answered already.
    fn open_off_finish(&mut self, from: ElementId, kind: ShapeKind, via: Option<RelationKind>) {
        let (w, h) = kind.default_size();
        let port = self.pending_port.take();
        let (x, y) = match (port, self.doc.element(from)) {
            (Some(p), Some(e)) => self.spot_from_port(e, p, w, h),
            _ => self.free_spot(w, h),
        };
        self.checkpoint();
        let id = self.doc.add(kind, "", x, y);
        // Opened out of a handle, the relation is anchored there — and at the facing handle
        // of the new shape — so it stays put whichever way the shapes are later moved.
        let anchor = |doc: &mut Document, rid: RelationId| {
            if let (Some(p), Some(r)) = (port, doc.relation_mut(rid)) {
                r.from_port = Some(p as u8);
                r.to_port = Some(((p + 4) % 8) as u8);
            }
        };
        self.reshape = None;
        if self.doc.metadata.view == View::Freeform || (kind.is_sketch() && self.doc.element(from).is_some_and(|e| e.kind.is_sketch())) {
            if let Ok(rid) = self.doc.connect(RelationKind::Link, from, id) {
                anchor(&mut self.doc, rid);
            }
            self.set_cursor(id);
            self.follow_camera();
            self.start_insert(Target::Element(id));
            self.say(format!("added {} linked from here — type its label, esc when done", kind.name()), Tone::Note);
        } else {
            self.set_cursor(id);
            self.follow_camera();
            self.pending_label = Some(id);
            self.pending_port = port;
            self.relpick = Some(relpick::State::new(&self.doc, from, id, None));
            // A suggested line is the picker answered in advance: the same path, so the
            // anchoring, the landing and the label all happen the one way they always do.
            if let Some(r) = via {
                let verdict = crate::ontology::allowed(r, self.doc.element(from).map_or(kind, |e| e.kind), kind);
                self.apply_relpick(r, verdict);
            }
        }
    }

    fn drop_relation(&mut self) {
        let (Some(from), Some(to)) = (self.holding, self.cursor) else { return };
        if from == to {
            self.say("carry it to another element first — hjkl", Tone::Bad);
            return;
        }
        self.relpick = Some(relpick::State::new(&self.doc, from, to, None));
    }

    fn apply_relpick(&mut self, kind: RelationKind, verdict: Result<(), &'static str>) {
        let Some(pick) = self.relpick.take() else { return };
        self.checkpoint();
        match pick.existing {
            Some(id) => {
                if let Some(r) = self.doc.relation_mut(id) {
                    r.kind = kind;
                }
            }
            None => match self.doc.connect(kind, pick.from, pick.to) {
                Ok(id) => {
                    if let (Some(p), Some(r)) = (self.pending_port.take(), self.doc.relation_mut(id)) {
                        r.from_port = Some(p as u8);
                        r.to_port = Some(((p + 4) % 8) as u8);
                    }
                    self.holding = None;
                    // Land on the new relation, from the element it was drawn to.
                    self.set_cursor(pick.to);
                    let rels = self.doc.incident(pick.to);
                    self.focus = rels.iter().position(|&r| r == id).map_or(0, |i| i + 1);
                }
                Err(why) => {
                    self.say(why, Tone::Bad);
                    return;
                }
            },
        }
        match verdict {
            Ok(()) => self.say(format!("{} — drawn", kind.name()), Tone::Good),
            Err(why) => self.say(format!("{} — drawn, but the rules refuse it: {why}", kind.name()), Tone::Bad),
        }
        if let Some(id) = self.pending_label.take() {
            self.set_cursor(id);
            self.start_insert(Target::Element(id));
            self.say("related — now type the new shape's label, esc when done", Tone::Note);
        }
    }

    fn delete_element(&mut self, id: ElementId) {
        self.checkpoint();
        self.doc.remove_element(id);
        self.after_document_change();
        self.say("deleted", Tone::Note);
    }

    fn delete_picked(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        for id in std::mem::take(&mut self.selection) {
            self.doc.remove_element(id);
        }
        self.visual = false;
        self.after_document_change();
        self.say("deleted", Tone::Note);
    }

    /// The elements a move applies to: the picked set, or the cursor's element and — for a
    /// composite — everything drawn inside it.
    /// The grouping the cursor's element sits inside — the smallest, when they nest.
    fn parent_group(&self, id: ElementId) -> Option<ElementId> {
        let e = self.doc.element(id)?;
        self.doc
            .elements
            .iter()
            .filter(|g| g.id != id && g.kind.is_composite() && g.contains(e.center()))
            .min_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)))
            .map(|g| g.id)
    }

    /// `g` in visual mode: a box around what is picked, named. Membership is where things
    /// sit, so the box is all there is to make — and the name is what it is for.
    fn group_picked(&mut self) {
        if self.selection.is_empty() {
            self.say("nothing picked yet — space", Tone::Bad);
            return;
        }
        let es: Vec<&Element> = self.selection.iter().filter_map(|&id| self.doc.element(id)).collect();
        if es.is_empty() {
            return;
        }
        let x = es.iter().map(|e| e.x).fold(f64::INFINITY, f64::min) - 2.0;
        let y = es.iter().map(|e| e.y).fold(f64::INFINITY, f64::min) - 2.0;
        let r = es.iter().map(|e| e.right()).fold(f64::NEG_INFINITY, f64::max) + 2.0;
        let b = es.iter().map(|e| e.bottom()).fold(f64::NEG_INFINITY, f64::max) + 1.0;
        // In an ontology, a box around object types is an object type group; elsewhere it is
        // the architecture's own grouping.
        let ontology = es.iter().all(|e| e.kind.layer() == crate::ontology::Layer::Ontology);
        let kind = if ontology { ShapeKind::ObjectTypeGroup } else { ShapeKind::Grouping };
        self.checkpoint();
        let id = self.doc.add(kind, "", x, y);
        if let Some(g) = self.doc.element_mut(id) {
            g.w = r - x;
            g.h = b - y;
        }
        // The box goes behind what it holds, or it would cover them.
        self.doc.raise_element(id, crate::model::Raise::Back);
        let n = self.selection.len();
        self.visual = false;
        self.selection.clear();
        self.set_cursor(id);
        self.say(format!("a grouping round {n} shape{} — type its name, esc when done", if n == 1 { "" } else { "s" }), Tone::Good);
        self.start_insert(Target::Element(id));
    }

    /// `gu`: the box goes; what was inside stays where it is, since membership is position.
    fn dissolve_group(&mut self) {
        let Some(id) = self.cursor else { return };
        let Some(e) = self.doc.element(id) else { return };
        if !e.kind.is_composite() {
            self.say("put the cursor on a grouping to dissolve it", Tone::Bad);
            return;
        }
        if self.doc.element_locked(id) {
            self.say(form::LOCKED, Tone::Bad);
            return;
        }
        let n = self.doc.members(id).len();
        let name = e.display();
        self.checkpoint();
        self.doc.remove_element(id);
        self.after_document_change();
        self.say(format!("{name} dissolved — {n} shape{} stay where they are", if n == 1 { "" } else { "s" }), Tone::Note);
    }

    fn movable(&self) -> Vec<ElementId> {
        if self.visual {
            return self.selection.clone();
        }
        let Some(e) = self.cursor_element() else { return Vec::new() };
        let mut v = vec![e.id];
        v.extend(self.doc.members(e.id));
        v
    }

    fn nudge(&mut self, dx: f64, dy: f64) {
        let ids: Vec<ElementId> = self.movable().into_iter().filter(|id| !self.doc.element_locked(*id)).collect();
        if ids.is_empty() {
            if self.cursor.is_some_and(|id| self.doc.element_locked(id)) {
                self.say(form::LOCKED, Tone::Bad);
            }
            return;
        }
        self.checkpoint();
        self.move_elements(&ids, dx, dy);
    }

    /// Move each element in `ids` by `(dx, dy)` — the one place "moving these shapes" happens.
    /// `nudge` checkpoints before every call, one keypress at a time; a mouse drag checkpoints
    /// once and calls this once per `Drag` event, so the whole drag is one undo step.
    fn move_elements(&mut self, ids: &[ElementId], dx: f64, dy: f64) {
        for &id in ids {
            if let Some(e) = self.doc.element_mut(id) {
                e.x += dx;
                e.y += dy;
            }
        }
    }

    fn resize(&mut self, dw: f64, dh: f64) {
        let Some(id) = self.cursor else { return };
        if self.doc.element_locked(id) {
            self.say(form::LOCKED, Tone::Bad);
            return;
        }
        self.checkpoint();
        if let Some(e) = self.doc.element_mut(id) {
            e.w = (e.w + dw).max(4.0);
            e.h = (e.h + dh).max(2.0);
        }
    }

    /// In visual mode `- = _ +` stretch the picked set: its bounding box grows or shrinks by
    /// `(dw, dh)` cells (a count: that many), and every picked shape's place and size scale
    /// with it from the box's top-left, so the set keeps its layout at the new size. What
    /// dragging a corner of a desktop tool's group selection does.
    fn stretch(&mut self, dw: f64, dh: f64) {
        let ids: Vec<ElementId> = self.selection.iter().copied().filter(|&id| !self.doc.element_locked(id)).collect();
        if ids.is_empty() {
            if !self.selection.is_empty() {
                self.say(form::LOCKED, Tone::Bad);
            }
            return;
        }
        let es: Vec<&Element> = ids.iter().filter_map(|&id| self.doc.element(id)).collect();
        let gx = es.iter().map(|e| e.x).fold(f64::INFINITY, f64::min);
        let gy = es.iter().map(|e| e.y).fold(f64::INFINITY, f64::min);
        let gw = es.iter().map(|e| e.right()).fold(f64::NEG_INFINITY, f64::max) - gx;
        let gh = es.iter().map(|e| e.bottom()).fold(f64::NEG_INFINITY, f64::max) - gy;
        // Never below the smallest shape's own minimum, or the set folds onto itself.
        if gw + dw < 4.0 || gh + dh < 2.0 {
            self.say("as small as it goes", Tone::Note);
            return;
        }
        let (fx, fy) = ((gw + dw) / gw, (gh + dh) / gh);
        self.checkpoint();
        for id in ids {
            if let Some(e) = self.doc.element_mut(id) {
                e.x = (gx + (e.x - gx) * fx).round();
                e.y = (gy + (e.y - gy) * fy).round();
                e.w = (e.w * fx).round().max(4.0);
                e.h = (e.h * fy).round().max(2.0);
            }
        }
    }

    /// `<` / `>` lean the cursor's shape a cell to the left or the right; `{` / `}` a cell
    /// up or down.
    fn skew(&mut self, dx: f64, dy: f64) {
        let Some(id) = self.cursor else { return };
        if self.doc.element_locked(id) {
            self.say(form::LOCKED, Tone::Bad);
            return;
        }
        self.checkpoint();
        if let Some(e) = self.doc.element_mut(id) {
            e.skew = (e.skew + dx).clamp(-e.w, e.w);
            e.skew_y = (e.skew_y + dy).clamp(-e.h, e.h);
        }
    }

    fn yank(&mut self) {
        let ids: Vec<ElementId> = if self.visual { self.selection.clone() } else { self.cursor.into_iter().collect() };
        if ids.is_empty() {
            return;
        }
        let elements: Vec<Element> = self.doc.elements.iter().filter(|e| ids.contains(&e.id)).cloned().collect();
        let relations: Vec<Relation> =
            self.doc.relations.iter().filter(|r| ids.contains(&r.from) && ids.contains(&r.to)).cloned().collect();
        let n = elements.len();
        self.clip = Some((elements, relations));
        if self.visual {
            self.visual = false;
            self.selection.clear();
        }
        self.say(format!("copied {n} element{}", if n == 1 { "" } else { "s" }), Tone::Note);
    }

    fn paste(&mut self) {
        let Some((elements, relations)) = self.clip.clone() else { return };
        let Some(first) = elements.first() else { return };
        let (bx, by) = elements.iter().fold((first.x, first.y), |(x, y), e| (x.min(e.x), y.min(e.y)));
        let (br, bb) = elements.iter().fold((first.right(), first.bottom()), |(r, b), e| (r.max(e.right()), b.max(e.bottom())));
        let (x, y) = self.free_spot(br - bx, bb - by);
        self.checkpoint();
        let mut map: Vec<(ElementId, ElementId)> = Vec::new();
        for e in &elements {
            let id = self.doc.alloc_element_id();
            self.doc.elements.push(Element { id, x: e.x - bx + x, y: e.y - by + y, ..e.clone() });
            map.push((e.id, id));
        }
        let new = |old| map.iter().find(|(o, _)| *o == old).map(|(_, n)| *n);
        for r in &relations {
            if let (Some(f), Some(t)) = (new(r.from), new(r.to)) {
                let id = self.doc.alloc_relation_id();
                self.doc.relations.push(Relation { id, from: f, to: t, ..r.clone() });
            }
        }
        if let Some((_, id)) = map.first() {
            self.set_cursor(*id);
        }
        self.follow_camera();
        self.say(format!("pasted {}", elements.len()), Tone::Note);
    }

    fn apply_layout(&mut self, positions: Vec<(ElementId, f64, f64)>) {
        self.checkpoint();
        for (id, x, y) in positions {
            if let Some(e) = self.doc.element_mut(id) {
                e.x = x;
                e.y = y;
            }
        }
        self.camera = (0.0, 0.0);
        self.follow_camera();
    }

    fn lint(&mut self) {
        let problems = self.doc.lint();
        let outside = self.doc.out_of_view().len();
        if problems.is_empty() {
            // An ontology has more to get right than its lines: the schema's own checks.
            let schema = crate::ontology::doc::check(&self.doc);
            if let Some(first) = schema.first() {
                self.say(format!("{first}{}", if schema.len() > 1 { format!(" — and {} more (--check lists them)", schema.len() - 1) } else { String::new() }), Tone::Bad);
                return;
            }
            let msg = match outside {
                0 => format!("nothing the rules refuse — {} elements, {} relations", self.doc.elements.len(), self.doc.relations.len()),
                n => format!("nothing the rules refuse; {n} element{} outside a {} view", if n == 1 { "" } else { "s" }, self.doc.metadata.view.name()),
            };
            self.say(msg, Tone::Good);
            return;
        }
        let first = &problems[0];
        if let Some(r) = self.doc.relation(first.relation).cloned() {
            self.push_jump();
            self.set_cursor(r.from);
            let rels = self.doc.incident(r.from);
            self.focus = rels.iter().position(|&x| x == r.id).map_or(0, |i| i + 1);
            self.follow_camera();
        }
        let n = problems.len();
        self.say(format!("{n} refused — {}", first.why), Tone::Bad);
    }

    // ─── reshaping ──────────────────────────────────────────────────────────

    /// Move between handles: the nearest one in that direction, measured with the cell
    /// aspect corrected so "up" from the right handle finds the top-right corner and not
    /// the far top-left.
    fn next_handle(&self, from: usize, dx: f64, dy: f64) -> Option<usize> {
        let e = self.cursor_element()?;
        let hs = e.handles();
        let (ox, oy) = hs[from];
        hs.iter()
            .enumerate()
            .filter(|(i, _)| *i != from)
            .filter_map(|(i, (hx, hy))| {
                let (ddx, ddy) = (hx - ox, (hy - oy) * 2.0);
                let primary = ddx * dx + ddy * dy;
                if primary <= 0.0 {
                    return None;
                }
                Some((i, primary + (ddx * dy - ddy * dx).abs() * 2.0))
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(i, _)| i)
    }

    /// Drag the handle in hand by `(dx, dy)`: a side handle moves one edge, a corner two,
    /// and the far edges stay where they are. The box never shrinks past the minimum.
    fn drag_handle(&mut self, handle: usize, dx: f64, dy: f64) {
        let Some(id) = self.cursor else { return };
        if self.doc.element_locked(id) {
            self.say(form::LOCKED, Tone::Bad);
            return;
        }
        let Some(e) = self.doc.element_mut(id) else { return };
        let (top, right, bottom, left) = (
            matches!(handle, 0..=2),
            matches!(handle, 2..=4),
            matches!(handle, 4..=6),
            matches!(handle, 6 | 7 | 0),
        );
        if left && dx != 0.0 {
            let nx = (e.x + dx).min(e.right() - MIN_W);
            e.w = e.right() - nx;
            e.x = nx;
        }
        if right && dx != 0.0 {
            e.w = (e.w + dx).max(MIN_W);
        }
        if top && dy != 0.0 {
            let ny = (e.y + dy).min(e.bottom() - MIN_H);
            e.h = e.bottom() - ny;
            e.y = ny;
        }
        if bottom && dy != 0.0 {
            e.h = (e.h + dy).max(MIN_H);
        }
    }

    fn reshape_key(&mut self, k: KeyEvent, rs: Reshape) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let Some(id) = self.cursor else { return };
        // A ^direction with nothing in hand opens a linked shape that way: the four sides
        // on ^hjkl, the four corners on ^yubn — the rogue-like's diagonals.
        if ctrl && !rs.held && rs.moving.is_none() {
            let port = match k.code {
                KeyCode::Char('h') | KeyCode::Char('H') => Some(7),
                KeyCode::Char('l') | KeyCode::Char('L') => Some(3),
                KeyCode::Char('k') | KeyCode::Char('K') => Some(1),
                KeyCode::Char('j') | KeyCode::Char('J') => Some(5),
                KeyCode::Char('y') => Some(0),
                KeyCode::Char('u') => Some(2),
                KeyCode::Char('b') => Some(6),
                KeyCode::Char('n') => Some(4),
                _ => None,
            };
            if let Some(p) = port {
                self.open_off(Some(p));
                return;
            }
        }
        match k.code {
            // Out of this very handle — a corner handle opens at that corner.
            KeyCode::Char('o') if !rs.held && rs.moving.is_none() => {
                self.open_off(Some(rs.handle));
                return;
            }
            // Disconnect what is attached here.
            KeyCode::Char('x') if !rs.held && rs.moving.is_none() => {
                if let Some(&rid) = self.doc.at_port(id, rs.handle).first() {
                    self.checkpoint();
                    self.doc.remove_relation(rid);
                    self.say("disconnected", Tone::Note);
                }
                return;
            }
            KeyCode::Char(c @ ('<' | '>' | '{' | '}')) if !rs.held && rs.moving.is_none() => {
                match c {
                    '<' => self.skew(-1.0, 0.0),
                    '>' => self.skew(1.0, 0.0),
                    '{' => self.skew(0.0, -1.0),
                    _ => self.skew(0.0, 1.0),
                }
                return;
            }
            _ => {}
        }
        let (dir, big) = match k.code {
            KeyCode::Char('h') | KeyCode::Left => (Some((-1.0, 0.0)), ctrl),
            KeyCode::Char('l') | KeyCode::Right => (Some((1.0, 0.0)), ctrl),
            KeyCode::Char('k') | KeyCode::Up => (Some((0.0, -1.0)), ctrl),
            KeyCode::Char('j') | KeyCode::Down => (Some((0.0, 1.0)), ctrl),
            // Shift as well as control: a terminal without the kitty protocol delivers ^h as
            // Backspace and ^j as a bare newline, and a drag that only sometimes works is
            // worse than one with two spellings.
            KeyCode::Char('H') | KeyCode::Backspace => (Some((-1.0, 0.0)), true),
            KeyCode::Char('L') => (Some((1.0, 0.0)), true),
            KeyCode::Char('K') => (Some((0.0, -1.0)), true),
            KeyCode::Char('J') => (Some((0.0, 1.0)), true),
            _ => (None, false),
        };
        match (k.code, dir) {
            (_, Some((dx, dy))) if rs.held => {
                let step = if big { BIG_STEP } else { 1.0 };
                self.drag_handle(rs.handle, dx * step, dy * step);
            }
            (_, Some((dx, dy))) => {
                if let Some(next) = self.next_handle(rs.handle, dx, dy) {
                    self.reshape = Some(Reshape { handle: next, ..rs });
                }
            }
            // Enter, in order of what is in hand: place a moving end on an open handle; let
            // go of a held handle; pick up the relation attached here; take hold to drag.
            (KeyCode::Enter, _) => match rs.moving {
                Some((rid, is_from, _)) => {
                    if !self.doc.at_port(id, rs.handle).is_empty() {
                        self.say("this handle is taken — pick an open one", Tone::Bad);
                        return;
                    }
                    self.checkpoint();
                    if let Some(r) = self.doc.relation_mut(rid) {
                        if is_from { r.from_port = Some(rs.handle as u8) } else { r.to_port = Some(rs.handle as u8) }
                    }
                    self.reshape = Some(Reshape { handle: rs.handle, held: false, moving: None });
                    self.say("moved", Tone::Note);
                }
                None if rs.held => self.reshape = Some(Reshape { handle: rs.handle, held: false, moving: None }),
                None => match self.doc.at_port(id, rs.handle).first().copied() {
                    Some(rid) => {
                        let is_from = self.doc.relation(rid).is_some_and(|r| r.from == id);
                        self.reshape = Some(Reshape { handle: rs.handle, held: false, moving: Some((rid, is_from, rs.handle)) });
                        self.say("end in hand — hjkl to an open handle, enter places it, esc puts it back", Tone::Note);
                    }
                    None => {
                        // One checkpoint per grab: the whole drag is one undo step.
                        self.checkpoint();
                        self.reshape = Some(Reshape { handle: rs.handle, held: true, moving: None });
                    }
                },
            },
            (KeyCode::Esc, _) => {
                self.reshape = match (rs.moving, rs.held) {
                    (Some((_, _, back)), _) => Some(Reshape { handle: back, held: false, moving: None }),
                    (None, true) => Some(Reshape { handle: rs.handle, held: false, moving: None }),
                    (None, false) => None,
                };
            }
            _ => {}
        }
    }

    // ─── keys ───────────────────────────────────────────────────────────────

    pub fn on_key(&mut self, k: KeyEvent) {
        // With the kitty protocol, shift+ctrl+h arrives as `h` with both modifiers; the
        // table spells it `^H`. One spelling, whatever the terminal.
        let k = match k.code {
            KeyCode::Char(c) if c.is_ascii_lowercase() && k.modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                KeyEvent::new(KeyCode::Char(c.to_ascii_uppercase()), k.modifiers - KeyModifiers::SHIFT)
            }
            _ => k,
        };
        self.keys.push(debug::describe(&k));
        if self.keys.len() > debug::KEEP {
            self.keys.remove(0);
        }
        if self.loading {
            self.loading = false;
            // Nothing was asked for, so ask: a new diagram, or an old one.
            if self.path.is_none() {
                self.start = Some(start::State::new());
            }
            return;
        }
        self.status = None;
        if let Some(st) = &mut self.start {
            let typing = st.side == start::Side::Right && st.choice == start::Choice::New && st.new_focus == start::NewFocus::Name;
            if k.code == KeyCode::Char(':') && !typing {
                self.start = None;
                self.cmdline = Some(cmdline::State::new(':'));
                return;
            }
            match st.key(k) {
                start::Outcome::Nothing => {}
                start::Outcome::Dismiss => self.start = None,
                start::Outcome::New { path, view } => {
                    self.start = None;
                    self.new_document();
                    self.doc.metadata.view = view;
                    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "diagram 1".into());
                    self.tabs[self.tab].name = stem;
                    self.saved = self.serialized();
                    self.path = Some(path.clone());
                    self.say(format!("a new {} diagram — :w saves it to {}", view.badge(), path.display()), Tone::Good);
                }
                start::Outcome::Open(path) => {
                    self.start = None;
                    if let Err(e) = self.open_path(path) {
                        self.say(e, Tone::Bad);
                    }
                }
                start::Outcome::Workbench(path) => {
                    self.start = None;
                    self.open_workbench(path);
                }
            }
            return;
        }
        if let Some(st) = &mut self.importdlg {
            match st.key(k) {
                importdlg::Outcome::Nothing => {}
                importdlg::Outcome::Cancel => self.importdlg = None,
                importdlg::Outcome::Import(path) => {
                    self.importdlg = None;
                    if self.dirty() {
                        self.confirm = Some(Confirm::Discard(Pending::Import(path)));
                    } else if let Err(e) = self.import_path(path) {
                        self.say(e, Tone::Bad);
                    }
                }
            }
            return;
        }
        if let Some(st) = &mut self.expandpick {
            let outcome = st.key(k);
            let rows: Vec<crate::foundry_import::Edge> = st.rows.iter().map(|r| r.edge.clone()).collect();
            match outcome {
                expandpick::Outcome::Nothing => {}
                expandpick::Outcome::Cancel => self.expandpick = None,
                expandpick::Outcome::One(i) => {
                    self.expandpick = None;
                    if let Some(source) = self.cursor {
                        self.checkpoint();
                        self.add_ontology_connection(source, &rows[i]);
                    }
                }
                expandpick::Outcome::All => {
                    self.expandpick = None;
                    if let Some(source) = self.cursor {
                        self.checkpoint();
                        for edge in &rows {
                            self.add_ontology_connection(source, edge);
                        }
                    }
                }
            }
            return;
        }
        if let Some(st) = &mut self.ctxmenu {
            match st.key(k) {
                ctxmenu::Outcome::Nothing => {}
                ctxmenu::Outcome::Cancel => self.ctxmenu = None,
                ctxmenu::Outcome::Run(cmd) => self.run_cmd(cmd, Avail::Yes),
            }
            return;
        }

        if self.manual.is_some() {
            if k.code == KeyCode::Char(':') {
                self.manual = None;
                self.cmdline = Some(cmdline::State::new(':'));
                return;
            }
            if self.manual.as_mut().is_some_and(|m| m.key(k)) {
                self.manual = None;
            }
            return;
        }
        if self.help.is_some() {
            self.help_key(k);
            return;
        }
        if self.palette.is_some() {
            self.palette_key(k);
            return;
        }
        if self.relpick.is_some() {
            self.relpick_key(k);
            return;
        }
        if self.tabpick.is_some() {
            self.tabpick_key(k);
            return;
        }
        if self.layers.is_some() {
            self.layers_key(k);
            return;
        }
        if self.tree.is_some() {
            self.tree_key(k);
            return;
        }
        if self.props.is_some() {
            self.props_key(k);
            return;
        }
        if self.workbench.is_some() {
            // Deleting a row arms the app's own confirm while the panel stays open — check
            // it here rather than letting the later, general `confirm` check ever see it,
            // since this branch would otherwise always win first.
            if self.confirm.is_some() {
                self.confirm_key(k);
            } else {
                self.workbench_key(k);
            }
            return;
        }
        if self.colour.is_some() {
            self.colour_key(k);
            return;
        }
        if self.sheet.as_ref().is_some_and(|s| s.focused) {
            self.sheet_key(k);
            self.follow_camera();
            return;
        }
        if self.export.is_some() {
            self.export_key(k);
            return;
        }
        if self.cmdline.is_some() {
            self.cmdline_key(k);
            return;
        }
        if self.confirm.is_some() {
            self.confirm_key(k);
            return;
        }
        if self.insert.is_some() {
            self.insert_key(k);
            return;
        }

        // `f` is waiting for a letter — the next key is that letter and nothing else.
        if self.pending_prefix == Some(Prefix::F) {
            self.pending_prefix = None;
            if let KeyCode::Char(c) = k.code
                && let Some((id, _)) = self.letters().into_iter().find(|(_, l)| *l == c)
            {
                self.push_jump();
                self.set_cursor(id);
                self.follow_camera();
            }
            return;
        }

        // A count belongs to the very next motion. Digits extend it; anything else spends it.
        if let KeyCode::Char(c) = k.code
            && c.is_ascii_digit()
            && (c != '0' || self.count.is_some())
            && !k.modifiers.contains(KeyModifiers::CONTROL)
        {
            let d = c as usize - '0' as usize;
            self.count = Some(self.count.unwrap_or(0).saturating_mul(10).saturating_add(d).min(999));
            return;
        }
        let given = self.count;
        let count = self.count.take().unwrap_or(1);

        // A terminal without the kitty protocol sends shift+ctrl+h as ctrl+h — and ctrl+h as
        // Backspace. On the diagram that has to be the MOVE: it is the foundational control,
        // and a linked shape still has o, the corners, and i then a side.
        let k = if !self.enhanced_keys && self.reshape.is_none() {
            let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
            match k.code {
                KeyCode::Backspace => KeyEvent::new(KeyCode::Char('H'), KeyModifiers::CONTROL),
                KeyCode::Char(c @ ('h' | 'j' | 'k' | 'l')) if ctrl => KeyEvent::new(KeyCode::Char(c.to_ascii_uppercase()), KeyModifiers::CONTROL),
                _ => k,
            }
        } else {
            k
        };
        let w = self.whereami();
        let resolved = keymap::resolve(&k, self.pending_prefix, &w);
        self.last_resolved = match &resolved {
            Resolved::Allow => "allowed".into(),
            Resolved::Refuse(why) => format!("refused: {why}"),
            Resolved::Unknown => "not a command".into(),
        };
        match resolved {
            Resolved::Refuse(why) => {
                self.pending_prefix = None;
                if k.code != KeyCode::Esc {
                    self.say(why, Tone::Bad);
                }
                return;
            }
            Resolved::Unknown => {
                // Not a command here — but it may be arming a prefix.
                let pending = self.pending_prefix.take();
                match (pending, k.code) {
                    (None, KeyCode::Char('g')) => self.pending_prefix = Some(Prefix::G),
                    (None, KeyCode::Char('Z')) => self.pending_prefix = Some(Prefix::Z),
                    (None, KeyCode::Char('z')) => self.pending_prefix = Some(Prefix::Zz),
                    _ => {}
                }
                // A count typed before a prefix is for the chord: `2zj` pans twice.
                if self.pending_prefix.is_some() {
                    self.count = given;
                }
                return;
            }
            Resolved::Allow => {}
        }

        let pending = self.pending_prefix.take();
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        if self.placing {
            self.place_key(k, count);
            self.follow_camera();
            return;
        }
        if self.viewing {
            self.view_key(k, count);
            return;
        }
        if let Some(rs) = self.reshape {
            if k.code == KeyCode::Char(':') {
                self.reshape = None;
                self.cmdline = Some(cmdline::State::new(':'));
                return;
            }
            self.reshape_key(k, rs);
            self.follow_camera();
            return;
        }
        // Presenting draws nothing but the diagram, so the line and the menu bring the chrome
        // back first: they must be usable, and they must be visible to be used.
        if self.present && matches!(k.code, KeyCode::Char(':') | KeyCode::Char('?')) {
            self.present = false;
        }
        match (pending, k.code) {
            (Some(Prefix::G), KeyCode::Char('g')) => {
                if let Some(first) = self.ordered().first().copied() {
                    self.set_cursor(first);
                }
            }
            (Some(Prefix::G), KeyCode::Char('d')) => {
                if let Some(rid) = self.focused_relation()
                    && let Some(r) = self.doc.relation(rid).cloned()
                {
                    let here = self.cursor.unwrap_or(0);
                    // From an end, that end; from the centre, the far end.
                    let to = match self.node {
                        Node::Tail => r.from,
                        Node::Head => r.to,
                        Node::Centre => if r.from == here { r.to } else { r.from },
                    };
                    self.push_jump();
                    self.set_cursor(to);
                }
            }
            (_, KeyCode::Char('a')) => self.palette = Some(palette::State::new(self.doc.metadata.view)),
            (Some(Prefix::G), KeyCode::Char('p')) => {
                if let Some(g) = self.cursor.and_then(|id| self.parent_group(id)) {
                    self.push_jump();
                    self.set_cursor(g);
                }
            }
            (Some(Prefix::G), KeyCode::Char('u')) => self.dissolve_group(),
            (_, KeyCode::Char('g')) if self.visual => self.group_picked(),
            // A bare `g` outside visual mode is the prefix: gg, gd, gp, gu, gt, gT.
            (None, KeyCode::Char('g')) => self.pending_prefix = Some(Prefix::G),
            (Some(Prefix::G), KeyCode::Char('t')) => self.switch_tab((self.tab + 1) % self.tabs.len()),
            (Some(Prefix::G), KeyCode::Char('T')) => self.switch_tab((self.tab + self.tabs.len() - 1) % self.tabs.len()),
            (_, KeyCode::Char('t')) if ctrl => self.tabpick = Some(tabpick::State::new()),
            (Some(Prefix::Z), KeyCode::Char('Z')) => self.run_excmd("wq".into()),
            (Some(Prefix::Z), KeyCode::Char('Q')) => self.run_excmd("q!".into()),
            (Some(Prefix::Zz), KeyCode::Char('z')) => self.center_camera(),
            (Some(Prefix::Zz), KeyCode::Char('v')) => self.viewing = true,
            (Some(Prefix::Zz), KeyCode::Char(c @ ('h' | 'j' | 'k' | 'l' | 'H' | 'J' | 'K' | 'L'))) => {
                if let Some((dx, dy)) = self.pan_step(c, c.is_ascii_uppercase(), count) {
                    self.pan(dx, dy);
                }
            }
            (_, KeyCode::Left) if shift => self.pan(-PAN_X * count as f64, 0.0),
            (_, KeyCode::Right) if shift => self.pan(PAN_X * count as f64, 0.0),
            (_, KeyCode::Up) if shift => self.pan(0.0, -PAN_Y * count as f64),
            (_, KeyCode::Down) if shift => self.pan(0.0, PAN_Y * count as f64),
            (_, KeyCode::Char('T')) => self.start_placing(),
            (_, KeyCode::Char('P')) => self.open_props(),
            // ^-direction on a shape: a linked shape that way, exactly as from inside it.
            (_, KeyCode::Char(c @ ('h' | 'j' | 'k' | 'l' | 'y' | 'u' | 'b' | 'n'))) if ctrl => {
                if let Some(port) = palette::dir_of(c) {
                    self.open_off(Some(port));
                }
            }
            (_, KeyCode::Char('h')) | (_, KeyCode::Left) if !ctrl => self.navigate(-1.0, 0.0, count),
            (_, KeyCode::Char('l')) | (_, KeyCode::Right) if !ctrl => self.navigate(1.0, 0.0, count),
            (_, KeyCode::Char('k')) | (_, KeyCode::Up) if !ctrl => self.navigate(0.0, -1.0, count),
            (_, KeyCode::Char('j')) | (_, KeyCode::Down) if !ctrl => self.navigate(0.0, 1.0, count),
            (_, KeyCode::Char('G')) => {
                if let Some(last) = self.ordered().last().copied() {
                    self.set_cursor(last);
                }
            }
            (_, KeyCode::Char('f')) => self.pending_prefix = Some(Prefix::F),
            (_, KeyCode::Char('/')) => self.cmdline = Some(cmdline::State::new('/')),
            (_, KeyCode::Char('n')) => self.search_step(true),
            (_, KeyCode::Char('N')) => self.search_step(false),
            // On a relation, tab walks its three nodes; on a shape, it steps onto its
            // relations one by one — the way to reach one the walk would not land on.
            (_, KeyCode::Tab) | (_, KeyCode::BackTab) if self.focus > 0 => {
                let fwd = k.code == KeyCode::Tab;
                self.node = match (self.node, fwd) {
                    (Node::Tail, true) | (Node::Head, false) => Node::Centre,
                    (Node::Centre, true) => Node::Head,
                    (Node::Centre, false) => Node::Tail,
                    (Node::Head, true) => Node::Tail,
                    (Node::Tail, false) => Node::Head,
                };
            }
            (_, KeyCode::Tab) | (_, KeyCode::BackTab) => {
                let n = self.cursor.map_or(0, |id| self.doc.incident(id).len());
                if n > 0 {
                    self.focus = if k.code == KeyCode::Tab { 1 } else { n };
                    self.node = Node::Centre;
                }
            }
            (_, KeyCode::Char('o')) if ctrl => self.jump_back(),
            (_, KeyCode::Char('i')) if ctrl => self.jump_forward(),
            (_, KeyCode::Enter) => {
                if self.holding.is_some() {
                    self.drop_relation();
                } else if let Some(r) = self.focused_relation() {
                    if let Some(rel) = self.doc.relation(r).cloned() {
                        self.relpick = Some(relpick::State::new(&self.doc, rel.from, rel.to, Some(r)));
                    }
                } else if let Some(id) = self.cursor {
                    self.holding = Some(id);
                }
            }
            (_, KeyCode::Char('r')) if ctrl => self.redo(),
            (_, KeyCode::Char('r')) => {
                if let Some(r) = self.focused_relation()
                    && let Some(rel) = self.doc.relation(r).cloned()
                {
                    self.relpick = Some(relpick::State::new(&self.doc, rel.from, rel.to, Some(r)));
                }
            }
            (_, KeyCode::Char('x')) => {
                if let Some(r) = self.focused_relation() {
                    self.checkpoint();
                    self.doc.remove_relation(r);
                    self.focus = 0;
                    self.say("removed", Tone::Note);
                }
            }
            (_, KeyCode::Char('t')) => {
                if let Some(target) = self.target() {
                    if form::locked(&self.doc, target) {
                        self.say(form::LOCKED, Tone::Bad);
                    } else {
                        self.start_insert(target);
                    }
                }
            }
            (_, KeyCode::Char('c')) => match &mut self.sheet {
                Some(sh) => sh.focused = true,
                None => self.sheet = Some(sheet::State::open(&self.doc, self.sheet_target(), &self.selection)),
            },
            (_, KeyCode::Char('d')) => {
                if self.visual {
                    self.confirm = Some(Confirm::DeletePicked);
                } else if let Some(id) = self.cursor {
                    if self.doc.element_locked(id) {
                        self.say(form::LOCKED, Tone::Bad);
                    } else {
                        self.confirm = Some(Confirm::DeleteElement(id));
                    }
                }
            }
            // Shift+ctrl: the same move, four cells at a time.
            (_, KeyCode::Char('H')) if ctrl => self.nudge(-(count as f64) * BIG_STEP, 0.0),
            (_, KeyCode::Char('L')) if ctrl => self.nudge(count as f64 * BIG_STEP, 0.0),
            (_, KeyCode::Char('K')) if ctrl => self.nudge(0.0, -(count as f64) * BIG_STEP),
            (_, KeyCode::Char('J')) if ctrl => self.nudge(0.0, count as f64 * BIG_STEP),
            (_, KeyCode::Char('H')) => self.nudge(-(count as f64), 0.0),
            (_, KeyCode::Char('L')) => self.nudge(count as f64, 0.0),
            (_, KeyCode::Char('K')) => self.nudge(0.0, -(count as f64)),
            (_, KeyCode::Char('J')) => self.nudge(0.0, count as f64),
            (_, KeyCode::Char('<')) => self.skew(-(count as f64), 0.0),
            (_, KeyCode::Char('>')) => self.skew(count as f64, 0.0),
            (_, KeyCode::Char('{')) => self.skew(0.0, -(count as f64)),
            (_, KeyCode::Char('}')) => self.skew(0.0, count as f64),
            // One shape: its own size. A picked set: the whole set, stretched.
            (_, KeyCode::Char('-')) if self.visual => self.stretch(-(count as f64) * BIG_STEP, 0.0),
            (_, KeyCode::Char('=')) if self.visual => self.stretch(count as f64 * BIG_STEP, 0.0),
            (_, KeyCode::Char('_')) if self.visual => self.stretch(0.0, -(count as f64) * BIG_STEP / 2.0),
            (_, KeyCode::Char('+')) if self.visual => self.stretch(0.0, count as f64 * BIG_STEP / 2.0),
            (_, KeyCode::Char('-')) => self.resize(-(count as f64), 0.0),
            (_, KeyCode::Char('=')) => self.resize(count as f64, 0.0),
            (_, KeyCode::Char('_')) => self.resize(0.0, -(count as f64)),
            (_, KeyCode::Char('+')) => self.resize(0.0, count as f64),
            (_, KeyCode::Char('V')) => self.preview(),
            (_, KeyCode::Char('\\')) => {
                self.present = !self.present;
                if self.present {
                    // Framed like the rendering: the diagram's own top-left, a margin in.
                    if let Some((bx, by, _, _)) = self.doc.bounds() {
                        self.camera = ((bx - 2.0).floor(), (by - 2.0).floor());
                    }
                }
            }
            (_, KeyCode::Char('i')) => self.reshape = Some(Reshape { handle: 3, held: false, moving: None }),
            (_, KeyCode::Char('o')) => self.open_off(None),
            (_, KeyCode::Char('e')) => self.open_expand(),
            (_, KeyCode::Char('u')) => self.undo(),
            (_, KeyCode::Char('y')) => self.yank(),
            (_, KeyCode::Char('p')) => self.paste(),
            (_, KeyCode::Char('v')) => {
                if self.visual {
                    self.visual = false;
                    self.selection.clear();
                } else {
                    self.visual = true;
                    self.focus = 0;
                    if let Some(id) = self.cursor {
                        self.selection = vec![id];
                    }
                }
            }
            (_, KeyCode::Char(' ')) => {
                if let Some(id) = self.cursor {
                    match self.selection.iter().position(|&s| s == id) {
                        Some(i) => {
                            self.selection.remove(i);
                        }
                        None => self.selection.push(id),
                    }
                }
            }
            (_, KeyCode::Char(':')) => self.cmdline = Some(cmdline::State::new(':')),
            (_, KeyCode::Char('?')) => self.help = Some(help::State::new()),
            (_, KeyCode::Esc) => {
                if self.present {
                    self.present = false;
                } else if self.holding.is_some() {
                    self.holding = None;
                } else if self.visual {
                    self.visual = false;
                    self.selection.clear();
                } else if self.focus > 0 {
                    self.focus = 0;
                    self.node = Node::Centre;
                }
            }
            _ => {}
        }
        self.follow_camera();
    }

    // ─── the mouse ──────────────────────────────────────────────────────────

    /// A screen point — a terminal's own coordinates, what a `MouseEvent`'s `column`/`row`
    /// are — as a point in the diagram's world, or `None` outside the diagram body: the
    /// header, the footer, a docked panel.
    fn screen_to_world(&self, col: u16, row: u16) -> Option<(f64, f64)> {
        let b = self.body;
        if col < b.x || col >= b.x + b.width || row < b.y || row >= b.y + b.height {
            return None;
        }
        Some((self.camera.0 + (col - b.x) as f64, self.camera.1 + (row - b.y) as f64))
    }

    /// The hover arrow under a point, if one is showing. Checked only against `self.hover`'s
    /// element — the only one currently painting arrows, per `canvas.rs`'s hover-fallback
    /// branch — since an arrow sits outside its box, where `element_at` would miss it.
    fn hit_arrow(&self, p: (f64, f64)) -> Option<(ElementId, usize)> {
        let id = self.hover?;
        let e = self.doc.element(id)?;
        e.arrow_at(p, ARROW_HIT, ARROW_GAP).map(|d| (id, d))
    }

    /// The handle of the frontmost element under a point, if the point is close enough to
    /// one. A click near a box's corner or edge is close enough to it that `element_at` would
    /// answer with the same element anyway, so there is no separate "which element's handles"
    /// question to ask first.
    fn hit_handle(&self, p: (f64, f64)) -> Option<(ElementId, usize)> {
        let id = self.doc.element_at(p)?;
        let e = self.doc.element(id)?;
        e.handle_at(p, HANDLE_HIT).map(|h| (id, h))
    }

    /// Land the cursor on a relation clicked directly: whichever end sits nearer the click,
    /// with the focus already on this relation — the same state `Tab` walks to.
    fn land_on_relation(&mut self, rid: RelationId, p: (f64, f64)) {
        let Some(r) = self.doc.relation(rid) else { return };
        let (from, to) = (r.from, r.to);
        let (Some(a), Some(b)) = (self.doc.element(from), self.doc.element(to)) else { return };
        let d = |c: (f64, f64)| (c.0 - p.0).powi(2) + (c.1 - p.1).powi(2);
        let near = if d(a.center()) <= d(b.center()) { from } else { to };
        self.set_cursor(near);
        let rels = self.doc.incident(near);
        self.focus = rels.iter().position(|&x| x == rid).map_or(0, |i| i + 1);
    }

    /// Whether anything owns the keyboard in a way that should own the mouse too — every
    /// overlay `on_key` already checks first, in the same order (`manual`, `palette`,
    /// `relpick`, and so on). Mouse support in this pass is the diagram body only; clicking an
    /// overlay is a later, separate addition, so while one is up the mouse simply does
    /// nothing rather than acting on the diagram underneath it.
    fn mouse_active(&self) -> bool {
        !self.loading
            && !self.present
            && self.start.is_none()
            && self.manual.is_none()
            && self.help.is_none()
            && self.palette.is_none()
            && self.relpick.is_none()
            && self.tabpick.is_none()
            && self.layers.is_none()
            && self.tree.is_none()
            && self.props.is_none()
            && self.workbench.is_none()
            && self.colour.is_none()
            && !self.sheet.as_ref().is_some_and(|s| s.focused)
            && self.export.is_none()
            && self.importdlg.is_none()
            && self.expandpick.is_none()
            && self.ctxmenu.is_none()
            && self.cmdline.is_none()
            && self.confirm.is_none()
            && self.insert.is_none()
            && !self.placing
            && !self.viewing
    }

    pub fn on_mouse(&mut self, m: MouseEvent) {
        self.last_mouse = debug::describe_mouse(&m);
        if !self.mouse_active() {
            return;
        }
        let Some(p) = self.screen_to_world(m.column, m.row) else {
            // Outside the diagram body — nothing to hit, but a stray `Up` off the edge must
            // still let go of whatever was in hand, or a drag that leaves the body gets stuck
            // open forever.
            if let MouseEventKind::Up(btn) = m.kind {
                let last = self.mouse_pos.unwrap_or((self.camera.0, self.camera.1));
                match btn {
                    MouseButton::Left => self.mouse_left_up(last),
                    MouseButton::Right => self.mouse_right_up(last),
                    MouseButton::Middle => {}
                }
            }
            return;
        };
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => self.mouse_left_down(p),
            MouseEventKind::Drag(MouseButton::Left) => self.mouse_left_drag(p),
            MouseEventKind::Up(MouseButton::Left) => self.mouse_left_up(p),
            MouseEventKind::Down(MouseButton::Right) => self.mouse_right_down(p, (m.column, m.row)),
            MouseEventKind::Drag(MouseButton::Right) => self.mouse_right_drag(p, (m.column, m.row)),
            MouseEventKind::Up(MouseButton::Right) => self.mouse_right_up(p),
            MouseEventKind::Moved => self.hover = self.doc.element_near(p, ARROW_HIT, ARROW_GAP),
            MouseEventKind::ScrollUp => self.pan(0.0, -PAN_Y),
            MouseEventKind::ScrollDown => self.pan(0.0, PAN_Y),
            MouseEventKind::ScrollLeft => self.pan(-PAN_X, 0.0),
            MouseEventKind::ScrollRight => self.pan(PAN_X, 0.0),
            _ => {}
        }
        self.mouse_pos = Some(p);
        self.follow_camera();
    }

    fn mouse_left_down(&mut self, p: (f64, f64)) {
        self.status = None;
        self.mouse_dragging = false;
        // Checked before `hit_handle`: an arrow sits `ARROW_GAP` outside its handle, so the
        // two never overlap, but the arrow is the one meant first when hovering shows both.
        if let Some((id, dir)) = self.hit_arrow(p) {
            self.set_cursor(id);
            self.open_off(Some(dir));
            self.mouse_gesture = None;
            return;
        }
        if let Some((id, handle)) = self.hit_handle(p) {
            self.set_cursor(id);
            match self.doc.at_port(id, handle).first().copied() {
                Some(rid) => {
                    let is_from = self.doc.relation(rid).is_some_and(|r| r.from == id);
                    self.reshape = Some(Reshape { handle, held: false, moving: Some((rid, is_from, handle)) });
                }
                None => {
                    // One checkpoint per grab, exactly as `Enter` takes one — the whole drag,
                    // however it ends, is one undo step.
                    self.checkpoint();
                    self.reshape = Some(Reshape { handle, held: true, moving: None });
                }
            }
            self.mouse_gesture = Some(MouseGesture::Handle);
            return;
        }
        if let Some(id) = self.doc.element_at(p) {
            // Clicking outside the current pick starts a fresh one — dragging a member of it
            // moves the whole group; dragging something else moves just that.
            if self.visual && !self.selection.contains(&id) {
                self.visual = false;
                self.selection.clear();
            }
            self.set_cursor(id);
            let ids: Vec<ElementId> = self.movable().into_iter().filter(|i| !self.doc.element_locked(*i)).collect();
            self.mouse_gesture = Some(MouseGesture::Body(ids));
            return;
        }
        if let Some(rid) = self.doc.relation_at(p, RELATION_HIT) {
            self.land_on_relation(rid, p);
        } else {
            self.cursor = None;
            self.visual = false;
            self.selection.clear();
        }
        self.mouse_gesture = Some(MouseGesture::Ground(p));
    }

    fn mouse_left_drag(&mut self, p: (f64, f64)) {
        let first = !self.mouse_dragging;
        self.mouse_dragging = true;
        let Some(prev) = self.mouse_pos else { return };
        let (dx, dy) = (p.0 - prev.0, p.1 - prev.1);
        match self.mouse_gesture.clone() {
            Some(MouseGesture::Body(ids)) if !ids.is_empty() => {
                if first {
                    self.checkpoint();
                }
                self.move_elements(&ids, dx, dy);
            }
            Some(MouseGesture::Handle) => {
                let Some(rs) = self.reshape else { return };
                if rs.moving.is_some() {
                    // Reroute only ever retargets a relation's end to another port on the
                    // same element — keyboard's own `next_handle` never crosses elements
                    // either — so the live preview is just "which handle is nearest now".
                    if let Some(id) = self.cursor
                        && let Some(e) = self.doc.element(id)
                    {
                        let nearest = e.nearest_handle(p);
                        if nearest != rs.handle {
                            self.reshape = Some(Reshape { handle: nearest, ..rs });
                        }
                    }
                } else if rs.held {
                    self.drag_handle(rs.handle, dx, dy);
                }
            }
            Some(MouseGesture::Ground(anchor)) => {
                self.marquee = Some((anchor, p));
            }
            _ => {}
        }
    }

    fn mouse_left_up(&mut self, p: (f64, f64)) {
        if self.mouse_dragging {
            match self.mouse_gesture.take() {
                Some(MouseGesture::Handle) => {
                    // A mouse handle-drag is a one-shot gesture — it ends when the button
                    // comes up, unlike keyboard's sticky Reshape mode, which needs a second
                    // `Enter` or `Esc` to leave.
                    if let Some(rs) = self.reshape.take()
                        && let Some((rid, is_from, _back)) = rs.moving
                        && let Some(id) = self.cursor
                        && self.doc.at_port(id, rs.handle).is_empty()
                    {
                        self.checkpoint();
                        if let Some(r) = self.doc.relation_mut(rid) {
                            if is_from { r.from_port = Some(rs.handle as u8) } else { r.to_port = Some(rs.handle as u8) }
                        }
                        self.say("moved", Tone::Note);
                    }
                    // Otherwise: a resize, already applied live during the drag, or a reroute
                    // dropped on a taken handle — either way, letting go is all that is left.
                }
                Some(MouseGesture::Ground(anchor)) if self.marquee.take().is_some() => {
                    let (x0, x1) = (anchor.0.min(p.0), anchor.0.max(p.0));
                    let (y0, y1) = (anchor.1.min(p.1), anchor.1.max(p.1));
                    let hit: Vec<ElementId> = self
                        .doc
                        .elements_in_order()
                        .into_iter()
                        .filter(|e| self.doc.element_visible(e.id) && e.x < x1 && e.right() > x0 && e.y < y1 && e.bottom() > y0)
                        .map(|e| e.id)
                        .collect();
                    self.visual = !hit.is_empty();
                    self.selection = hit;
                    if let Some(&first) = self.selection.first() {
                        self.set_cursor(first);
                    }
                }
                _ => {}
            }
        }
        self.marquee = None;
        self.mouse_gesture = None;
        self.mouse_dragging = false;
    }

    fn mouse_right_down(&mut self, p: (f64, f64), raw: (u16, u16)) {
        self.status = None;
        self.mouse_dragging = false;
        self.connect_from = self.doc.element_at(p);
        // Empty ground: this might turn into a pan rather than a connection. Raw screen
        // cells, not `p` — `p` is a world position, and the world moves under the mouse the
        // moment a pan starts, which would make a delta taken from it chase its own tail.
        self.pan_from = self.connect_from.is_none().then_some(raw);
    }

    /// A right-drag starting on empty ground pans: the view moves with the mouse, exactly
    /// as far and in the same direction, like a hand dragging the canvas rather than a
    /// window looking at it. One from an element instead connects it to whatever the mouse
    /// ends up over — see `mouse_right_up`.
    fn mouse_right_drag(&mut self, p: (f64, f64), raw: (u16, u16)) {
        self.mouse_dragging = true;
        if let Some((fx, fy)) = self.pan_from {
            self.pan(fx as f64 - raw.0 as f64, fy as f64 - raw.1 as f64);
            self.pan_from = Some(raw);
            return;
        }
        let Some(from) = self.connect_from else { return };
        if self.holding.is_none() {
            self.holding = Some(from);
        }
        self.cursor = Some(self.doc.element_at(p).unwrap_or(from));
    }

    /// A right-drag between two elements connects them, reusing the keyboard's own
    /// hold/carry/drop verb (`drop_relation`) — the mouse just compresses it into one
    /// gesture. A right-drag that panned the view lets go having only done that — a pan is a
    /// deliberate move, not a cancel, and undoing the current pick or hold as a side effect
    /// of it would be a surprise. A right-click mid another gesture — a held or moving
    /// reshape, a held relation — cancels that instead, the same as esc. Otherwise (an idle
    /// right-click, nothing above it to bail out of) it opens the context menu: whatever is
    /// selected stays selected, since acting on the current pick is often the whole point of
    /// asking for a menu — esc is still how to clear a selection outright.
    fn mouse_right_up(&mut self, _p: (f64, f64)) {
        let panned = self.mouse_dragging && self.pan_from.is_some();
        let attempted = self.mouse_dragging && self.holding.is_some();
        if panned {
            // Already done, in `mouse_right_drag` — nothing left to do but let go below.
        } else if attempted {
            let from = self.holding.expect("attempted implies holding");
            let to = self.cursor.unwrap_or(from);
            if to != from {
                self.drop_relation();
            } else {
                self.holding = None;
            }
        } else if self.reshape.is_some_and(|r| r.held || r.moving.is_some()) {
            self.reshape = None;
        } else if self.holding.is_some() {
            self.holding = None;
        } else if !self.mouse_dragging {
            if let Some(id) = self.connect_from {
                self.set_cursor(id);
            }
            self.ctxmenu = Some(ctxmenu::State::new(&self.whereami()));
        }
        self.connect_from = None;
        self.pan_from = None;
        self.mouse_dragging = false;
    }

    /// What the cursor is on, as a thing that can be labelled or configured.
    fn target(&self) -> Option<Target> {
        match self.focused_relation() {
            Some(r) => Some(Target::Relation(r, self.node)),
            None => self.cursor.map(Target::Element),
        }
    }

    /// What the sheet shows: what the cursor is on, or, on an empty diagram, the diagram.
    fn sheet_target(&self) -> Option<Target> {
        if self.visual && !self.selection.is_empty() {
            return Some(Target::Picked);
        }
        self.target().or(Some(Target::Diagram))
    }

    /// `P` / `:props`: the property browser on the cursor's shape, when it is a kind with rows.
    fn open_props(&mut self) {
        let Some(e) = self.cursor_element() else {
            self.say("put the cursor on an object type, an interface or an action type", Tone::Bad);
            return;
        };
        if !e.takes_rows() {
            self.say(format!("a {} has no rows — properties belong to an object type, an interface or an action type", e.kind.name().to_ascii_lowercase()), Tone::Bad);
            return;
        }
        if self.doc.element_locked(e.id) {
            self.say(form::LOCKED, Tone::Bad);
            return;
        }
        self.props = Some(props::State::new(e.id));
    }

    /// The property browser's keys.
    fn props_key(&mut self, k: KeyEvent) {
        let Some(st) = &mut self.props else { return };
        let target = st.target;
        if let Some((what, buf)) = &mut st.editing {
            match k.code {
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => buf.push(c),
                KeyCode::Esc => st.editing = None,
                KeyCode::Enter => {
                    let (what, text) = (*what, buf.clone());
                    st.editing = None;
                    let sel = st.sel;
                    self.checkpoint();
                    let Some(e) = self.doc.element_mut(target) else { return };
                    match props::typed(e, sel, what, &text) {
                        Ok(at) => {
                            e.fit_rows();
                            if let Some(st) = &mut self.props {
                                st.sel = at;
                            }
                        }
                        Err(why) => {
                            self.undo.pop();
                            self.say(why, Tone::Bad);
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        let n = self.doc.element(target).map_or(0, |e| e.properties.len());
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('P') => self.props = None,
            KeyCode::Char(':') => {
                self.props = None;
                self.cmdline = Some(cmdline::State::new(':'));
            }
            KeyCode::Char('?') => self.help = Some(help::State::new()),
            KeyCode::Char('j') | KeyCode::Down => st.move_by(1, n),
            KeyCode::Char('k') | KeyCode::Up => st.move_by(-1, n),
            KeyCode::Char('n') => st.editing = Some((props::Typing::New, String::new())),
            KeyCode::Char('r') | KeyCode::Char('i') => {
                let name = self.doc.element(target).and_then(|e| e.properties.get(st.sel)).map(|p| p.name.clone()).unwrap_or_default();
                st.editing = Some((props::Typing::Name, name));
            }
            KeyCode::Char('T') => st.editing = Some((props::Typing::Type, String::new())),
            KeyCode::Char('v') => {
                let vt = self.doc.element(target).and_then(|e| e.properties.get(st.sel)).and_then(|p| p.value_type.clone()).unwrap_or_default();
                st.editing = Some((props::Typing::ValueType, vt));
            }
            KeyCode::Char('a') => {
                let api = self.doc.element(target).and_then(|e| e.properties.get(st.sel)).and_then(|p| p.api_name.clone()).unwrap_or_default();
                st.editing = Some((props::Typing::ApiName, api));
            }
            KeyCode::Char('J') | KeyCode::Char('K') => {
                // J moves the row down, K up.
                let down = k.code == KeyCode::Char('J');
                if n < 2 {
                    return;
                }
                let sel = st.sel.min(n - 1);
                let to = if down { (sel + 1).min(n - 1) } else { sel.saturating_sub(1) };
                if to == sel {
                    return;
                }
                self.checkpoint();
                if let Some(e) = self.doc.element_mut(target) {
                    e.properties.swap(sel, to);
                }
                if let Some(st) = &mut self.props {
                    st.sel = to;
                }
            }
            KeyCode::Char(c @ ('p' | 'l' | 's' | '[' | '*' | 't' | 'd')) => {
                let sel = st.sel;
                self.checkpoint();
                let Some(e) = self.doc.element_mut(target) else { return };
                match props::apply(e, sel, c) {
                    Ok(said) => {
                        e.fit_rows();
                        let left = e.properties.len();
                        if let Some(st) = &mut self.props {
                            st.sel = st.sel.min(left.saturating_sub(1));
                        }
                        if let Some(m) = said {
                            self.say(m, Tone::Note);
                        }
                    }
                    Err(why) => {
                        self.undo.pop();
                        self.say(why, Tone::Bad);
                    }
                }
            }
            KeyCode::Char('e') => {
                let Some(edge) = self.props_fk_edge() else {
                    self.say("not a foreign key — nothing to expand", Tone::Bad);
                    return;
                };
                self.props = None;
                self.checkpoint();
                if let Some(new_id) = self.add_ontology_connection(target, &edge) {
                    self.cursor = Some(new_id);
                }
            }
            _ => {}
        }
    }

    /// `:workbench <path>` — create the folder if it is not there yet (a brand-new team
    /// folder is a normal case, not an error), remember it in the config as just accessed,
    /// and open the panel on it.
    fn open_workbench(&mut self, path: PathBuf) {
        if let Err(e) = std::fs::create_dir_all(&path) {
            self.say(format!("{}: {e}", path.display()), Tone::Bad);
            return;
        }
        self.workbench = Some(workbench::State::opened(path.clone()));
        match crate::config::touch_workbench(&path) {
            Ok(()) => self.say(format!("workbench: {}", path.display()), Tone::Good),
            Err(e) => self.say(format!("workbench: {} — not remembered: {e}", path.display()), Tone::Note),
        }
    }

    /// A diagram row picked in the workbench: every tab in that file, appended — not just
    /// whichever was `current` when it was saved — since a workbench leaf is one file
    /// deliberately allowed to hold a whole family of related diagrams. Purely additive, the
    /// same as `tabnew_from_path`, so unlike `:open` this never needs to ask about unsaved
    /// work first.
    fn workbench_open_diagram(&mut self, path: PathBuf) {
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
        match persistence::load(&path) {
            Ok(ws) => {
                // A single-tab file is named after the file itself here, not whatever name
                // it happens to carry inside — `new_diagram` already writes that name, but an
                // externally authored file might not. Either way, this is the name a second
                // Enter on the same row looks for, so opening it twice switches to the one
                // tab already open instead of piling up an empty duplicate — the bug a bare
                // "already open" name check on the file's own internal name would still miss
                // for anything written before this was true.
                let single = ws.tabs.len() == 1;
                let first_name = if single { stem.clone() } else { ws.tabs[0].name.clone() };
                if let Some(i) = self.tabs.iter().position(|t| t.name == first_name) {
                    self.switch_tab(i);
                    self.say(format!("{first_name} — already open"), Tone::Note);
                    return;
                }
                let first_new = self.tabs.len();
                for Tab { name, diagram } in ws.tabs {
                    let name = if single { stem.clone() } else { name };
                    self.tabs.push(TabSlot { name, doc: diagram, ..Default::default() });
                }
                self.switch_tab(first_new);
                self.say(format!("opened {}", path.display()), Tone::Good);
            }
            Err(e) => self.say(e.to_string(), Tone::Bad),
        }
    }

    /// `d`, confirmed: a real `rm` on disk, then the panel rescans.
    fn workbench_delete(&mut self, entry: crate::workbench::Entry) {
        let name = entry.name.clone();
        if let Err(e) = crate::workbench::delete(&entry) {
            self.say(format!("{name}: {e}"), Tone::Bad);
            return;
        }
        if let Some(st) = &mut self.workbench {
            st.rescan();
            let n = st.rows().len();
            st.sel = st.sel.min(n.saturating_sub(1));
            if st.grabbed.as_ref().is_some_and(|g| g.path == entry.path) {
                st.grabbed = None;
            }
        }
        self.say(format!("{name} deleted"), Tone::Note);
    }

    /// `n`/`N`/`r`, typed and committed: create a diagram or a folder in `dir`, or rename
    /// `selected` — whichever `what` says. A plain function rather than more `workbench_key`
    /// inline, since it needs to call back into `self.say` after the fs op, and `workbench_key`
    /// already holds a `&mut self.workbench` borrow when this is reached.
    fn workbench_commit_name(&mut self, what: workbench::Typing, text: String, dir: Option<PathBuf>, selected: Option<workbench::Row>) {
        let result = match what {
            workbench::Typing::NewDiagram => dir.ok_or_else(|| "no folder to add it to".to_string()).and_then(|d| crate::workbench::new_diagram(&d, &text).map_err(|e| e.to_string())).map(|_| format!("{text}.json added")),
            workbench::Typing::NewFolder => dir.ok_or_else(|| "no folder to add it to".to_string()).and_then(|d| crate::workbench::new_folder(&d, &text).map_err(|e| e.to_string())).map(|_| format!("{text} added")),
            workbench::Typing::Rename => selected.ok_or_else(|| "nothing selected".to_string()).and_then(|row| crate::workbench::rename(&row.entry(), &text).map_err(|e| e.to_string())).map(|_| format!("renamed to {text}")),
        };
        match result {
            Ok(msg) => {
                if let Some(st) = &mut self.workbench {
                    st.rescan();
                }
                self.say(msg, Tone::Good);
            }
            Err(e) => self.say(e, Tone::Bad),
        }
    }

    /// `p`: put whatever `m` grabbed into the folder under the cursor now.
    fn workbench_put(&mut self) {
        let Some(st) = &mut self.workbench else { return };
        let Some(grabbed) = st.grabbed.clone() else {
            self.say("nothing grabbed — m on a row first", Tone::Bad);
            return;
        };
        let Some(row) = st.selected_row() else { return };
        if !matches!(row.kind, workbench::RowKind::Folder { .. }) {
            self.say("p lands on a folder", Tone::Bad);
            return;
        }
        if row.path == grabbed.path || row.path.starts_with(&grabbed.path) {
            self.say("can't move a folder into itself", Tone::Bad);
            return;
        }
        let dest = row.path.clone();
        match crate::workbench::move_to(&grabbed, &dest) {
            Ok(_) => {
                if let Some(st) = &mut self.workbench {
                    st.grabbed = None;
                    st.rescan();
                    st.expand(&dest);
                }
                self.say(format!("{} moved", grabbed.name), Tone::Good);
            }
            Err(e) => self.say(format!("{}: {e}", grabbed.name), Tone::Bad),
        }
    }

    /// The workbench panel's keys — picking a recent folder when none is open yet, typing a
    /// new name, or navigating and acting on the tree.
    fn workbench_key(&mut self, k: KeyEvent) {
        let Some(st) = &mut self.workbench else { return };

        if st.root.is_none() {
            let n = st.recents.len();
            match k.code {
                KeyCode::Esc | KeyCode::Char('q') => self.workbench = None,
                KeyCode::Char('j') | KeyCode::Down => st.move_by(1, n),
                KeyCode::Char('k') | KeyCode::Up => st.move_by(-1, n),
                KeyCode::Enter => {
                    if let Some(path) = st.recents.get(st.sel).cloned() {
                        self.open_workbench(path);
                    }
                }
                _ => {}
            }
            return;
        }

        // Computed before `st.editing` might be borrowed below — a method call on `st`
        // borrows the whole of it, which a live `&mut st.editing` binding would refuse.
        let dir = st.current_dir();
        let selected = st.selected_row();

        if let Some((what, buf)) = &mut st.editing {
            match k.code {
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => buf.push(c),
                KeyCode::Esc => st.editing = None,
                KeyCode::Enter => {
                    let what = *what;
                    let text = buf.trim().to_string();
                    st.editing = None;
                    if text.is_empty() {
                        self.say("needs a name", Tone::Bad);
                        return;
                    }
                    self.workbench_commit_name(what, text, dir, selected);
                }
                _ => {}
            }
            return;
        }

        match k.code {
            KeyCode::Esc if st.grabbed.is_some() => st.grabbed = None,
            KeyCode::Esc | KeyCode::Char('q') => self.workbench = None,
            KeyCode::Char(':') => {
                self.workbench = None;
                self.cmdline = Some(cmdline::State::new(':'));
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let n = st.rows().len();
                st.move_by(1, n);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let n = st.rows().len();
                st.move_by(-1, n);
            }
            KeyCode::Enter => {
                let Some(row) = st.selected_row() else { return };
                match row.kind {
                    workbench::RowKind::Folder { .. } => st.toggle_expanded(&row.path),
                    workbench::RowKind::Diagram => {
                        let path = row.path;
                        self.workbench_open_diagram(path);
                    }
                }
            }
            KeyCode::Right => {
                if let Some(row) = st.selected_row()
                    && matches!(row.kind, workbench::RowKind::Folder { expanded: false })
                {
                    st.expand(&row.path);
                }
            }
            KeyCode::Left => {
                if let Some(row) = st.selected_row()
                    && matches!(row.kind, workbench::RowKind::Folder { expanded: true })
                {
                    st.toggle_expanded(&row.path);
                }
            }
            KeyCode::Char('n') => st.editing = Some((workbench::Typing::NewDiagram, String::new())),
            KeyCode::Char('N') => st.editing = Some((workbench::Typing::NewFolder, String::new())),
            KeyCode::Char('r') => {
                let name = st.selected_row().map(|r| r.name).unwrap_or_default();
                let stem = name.strip_suffix(".json").map(str::to_string).unwrap_or(name);
                st.editing = Some((workbench::Typing::Rename, stem));
            }
            KeyCode::Char('m') => {
                if let Some(row) = st.selected_row() {
                    st.grabbed = Some(row.entry());
                }
            }
            KeyCode::Char('p') => self.workbench_put(),
            KeyCode::Char('d') => {
                if let Some(row) = st.selected_row() {
                    self.confirm = Some(Confirm::DeleteWorkbenchEntry(row.entry()));
                }
            }
            _ => {}
        }
    }

    /// The layer browser's keys.
    fn layers_key(&mut self, k: KeyEvent) {
        let Some(st) = &mut self.layers else { return };
        if let Some((naming, buf)) = &mut st.editing {
            match k.code {
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => buf.push(c),
                KeyCode::Esc => st.editing = None,
                KeyCode::Enter => {
                    let name = buf.trim().to_string();
                    let naming = *naming;
                    st.editing = None;
                    if name.is_empty() {
                        return;
                    }
                    self.checkpoint();
                    match naming {
                        layers::Naming::New => {
                            let id = self.doc.add_layer(&name);
                            if let Some(st) = &mut self.layers {
                                st.select(id, &self.doc);
                            }
                            self.say(format!("new layer {name:?} — current: new shapes go on it"), Tone::Good);
                        }
                        layers::Naming::Rename => {
                            if let Some(id) = self.layers.as_ref().and_then(|s| s.selected(&self.doc))
                                && let Some(l) = self.doc.layers.iter_mut().find(|l| l.id == id)
                            {
                                l.name = name;
                            }
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        let Some(id) = st.selected(&self.doc) else {
            if k.code == KeyCode::Esc {
                self.layers = None;
            }
            return;
        };
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => self.layers = None,
            KeyCode::Char(':') => {
                self.layers = None;
                self.cmdline = Some(cmdline::State::new(':'));
            }
            KeyCode::Char('j') | KeyCode::Down => st.move_by(1, &self.doc),
            KeyCode::Char('k') | KeyCode::Up => st.move_by(-1, &self.doc),
            KeyCode::Char(' ') => {
                self.checkpoint();
                if let Some(l) = self.doc.layers.iter_mut().find(|l| l.id == id) {
                    l.visible = !l.visible;
                }
                self.ensure_cursor();
            }
            KeyCode::Char('l') => {
                self.checkpoint();
                if let Some(l) = self.doc.layers.iter_mut().find(|l| l.id == id) {
                    l.locked = !l.locked;
                }
            }
            KeyCode::Enter => {
                self.checkpoint();
                self.doc.metadata.layer = id;
                let name = self.doc.layer(id).map(|l| l.name.clone()).unwrap_or_default();
                self.say(format!("{name:?} is current — new shapes and relations go on it"), Tone::Note);
            }
            // Move the cursor's thing — or the picked set — onto this layer.
            KeyCode::Char('m') => {
                let ids: Vec<ElementId> = if self.selection.is_empty() { self.cursor.into_iter().collect() } else { self.selection.clone() };
                let rel = self.focused_relation();
                if ids.is_empty() && rel.is_none() {
                    self.say("nothing under the cursor to move", Tone::Bad);
                    return;
                }
                self.checkpoint();
                match rel {
                    Some(r) => {
                        if let Some(r) = self.doc.relation_mut(r) {
                            r.layer = id;
                        }
                    }
                    None => {
                        for eid in ids {
                            if let Some(e) = self.doc.element_mut(eid) {
                                e.layer = id;
                            }
                        }
                    }
                }
                self.say("moved", Tone::Note);
            }
            KeyCode::Char('n') => st.editing = Some((layers::Naming::New, String::new())),
            KeyCode::Char('r') | KeyCode::Char('i') => {
                let name = self.doc.layer(id).map(|l| l.name.clone()).unwrap_or_default();
                st.editing = Some((layers::Naming::Rename, name));
            }
            KeyCode::Char('J') => {
                self.checkpoint();
                self.doc.shift_layer(id, -1);
                if let Some(st) = &mut self.layers {
                    st.select(id, &self.doc);
                }
            }
            KeyCode::Char('K') => {
                self.checkpoint();
                self.doc.shift_layer(id, 1);
                if let Some(st) = &mut self.layers {
                    st.select(id, &self.doc);
                }
            }
            KeyCode::Char('d') => {
                let (els, rels) = self.doc.layer_count(id);
                self.checkpoint();
                match self.doc.remove_layer(id) {
                    Ok(()) => {
                        if let Some(st) = &mut self.layers {
                            st.sel = st.sel.min(self.doc.layers.len() - 1);
                        }
                        self.say(format!("layer removed — {els} shapes and {rels} relations moved to the layer beneath"), Tone::Note);
                    }
                    Err(why) => {
                        self.undo.pop();
                        self.say(why, Tone::Bad);
                    }
                }
            }
            _ => {}
        }
    }

    /// The model tree's keys — arrows move and fold/unfold, like the add palette, so every
    /// letter can go to the search instead.
    fn tree_key(&mut self, k: KeyEvent) {
        let Some(st) = &mut self.tree else { return };
        match k.code {
            KeyCode::Esc => self.tree = None,
            KeyCode::Char(':') => {
                self.tree = None;
                self.cmdline = Some(cmdline::State::new(':'));
            }
            KeyCode::Down => st.move_by(1, &tree::rows(&self.model_tree, &st.filter)),
            KeyCode::Up => st.move_by(-1, &tree::rows(&self.model_tree, &st.filter)),
            KeyCode::Backspace => st.retype(|f| {
                f.pop();
            }),
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => st.retype(|f| f.push(c)),
            KeyCode::Right => {
                let rows = tree::rows(&self.model_tree, &st.filter);
                if let Some(path) = rows.get(st.sel).map(|r| r.path.clone()) {
                    tree::expand(&mut self.model_tree, &path);
                }
            }
            KeyCode::Left => {
                let rows = tree::rows(&self.model_tree, &st.filter);
                if let Some(path) = rows.get(st.sel).map(|r| r.path.clone()) {
                    tree::collapse(&mut self.model_tree, &path);
                }
            }
            KeyCode::Enter => {
                let rows = tree::rows(&self.model_tree, &st.filter);
                let Some(row) = rows.get(st.sel).cloned() else { return };
                match row.kind {
                    tree::RowKind::Folder { .. } => tree::toggle(&mut self.model_tree, &row.path),
                    tree::RowKind::View { tab_index } => self.jump_to_tab(tab_index, &row.label),
                    tree::RowKind::Resource { id, .. } => {
                        self.tree = None;
                        self.place_ontology_resource(&id);
                    }
                    tree::RowKind::Link { from, to } => {
                        self.tree = None;
                        self.place_ontology_link(&from, &to);
                    }
                }
            }
            _ => {}
        }
    }

    /// A tree row's tab, by the index the import gave it, falling back to a search by name if
    /// tabs have since been closed or reordered and the index no longer lines up.
    fn jump_to_tab(&mut self, tab_index: usize, name: &str) {
        let target = if self.tabs.get(tab_index).is_some_and(|t| t.name == name) { Some(tab_index) } else { self.tabs.iter().position(|t| t.name == name) };
        self.tree = None;
        match target {
            Some(i) => self.switch_tab(i),
            None => self.say(format!("{name:?} isn't open anymore"), Tone::Bad),
        }
    }

    /// The sheet's keys. Stepped into a field, the letters are the value; out of one, they
    /// are the sheet's own commands.
    fn sheet_key(&mut self, k: KeyEvent) {
        let Some(sh) = &mut self.sheet else { return };
        if let Some(buf) = &mut sh.editing {
            match k.code {
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => buf.push(c),
                // Enter commits and stays on an invalid value; esc commits, or reverts one
                // that will not go — leaving is always allowed.
                KeyCode::Enter | KeyCode::Esc => {
                    let value = buf.clone();
                    let (Some(target), Some(f)) = (sh.target, sh.field()) else {
                        sh.editing = None;
                        return;
                    };
                    let name = f.name;
                    if value.trim() == f.value.trim() {
                        sh.editing = None;
                        return;
                    }
                    self.checkpoint();
                    match form::apply_for(&mut self.doc, target, name, &value, &self.selection) {
                        Ok(()) => {
                            if let Some(sh) = &mut self.sheet {
                                sh.editing = None;
                                sh.refresh(&self.doc);
                            }
                        }
                        Err(why) => {
                            self.undo.pop();
                            self.say(why, Tone::Bad);
                            if k.code == KeyCode::Esc
                                && let Some(sh) = &mut self.sheet
                            {
                                sh.editing = None;
                            }
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        match k.code {
            KeyCode::Esc => sh.focused = false,
            KeyCode::Char('q') => self.sheet = None,
            KeyCode::Char(':') => {
                sh.focused = false;
                self.cmdline = Some(cmdline::State::new(':'));
            }
            KeyCode::Char('?') => self.help = Some(help::State::new()),
            KeyCode::Tab => sh.next_tab(true),
            KeyCode::BackTab => sh.next_tab(false),
            KeyCode::Char('j') | KeyCode::Down => sh.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => sh.move_by(-1),
            KeyCode::Char('i') => sh.step_in(),
            KeyCode::Char('t') => sh.step_into_text(),
            KeyCode::Enter => {
                match sh.field().map(|f| (f.unit, f.name)) {
                    Some((form::Unit::Action, name)) => {
                        if let Some(target) = sh.target {
                            self.checkpoint();
                            match form::apply_for(&mut self.doc, target, name, "", &self.selection) {
                                Ok(()) => {
                                    self.say(format!("{name} — done"), Tone::Note);
                                    self.ensure_cursor();
                                    if let Some(sh) = &mut self.sheet {
                                        sh.refresh(&self.doc);
                                    }
                                }
                                Err(why) => {
                                    self.undo.pop();
                                    self.say(why, Tone::Bad);
                                }
                            }
                        }
                    }
                    Some((unit @ (form::Unit::Colour | form::Unit::Fill), name)) => {
                        if let Some(target) = sh.target {
                            let current = sh.field().and_then(|f| Colour::parse(&f.value));
                            let blank = if unit == form::Unit::Fill { "auto — the layer's own" } else { "none — the default look" };
                            self.colour = Some(colour::State::open(target, name, current, &self.recent_colours, blank));
                        }
                    }
                    Some(_) => sh.step_in(),
                    None => {}
                }
            }
            KeyCode::Char('h' | 'H' | 'l' | 'L') | KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                // Space flips a yes/no and is otherwise the same as l.
                let delta = if matches!(k.code, KeyCode::Char('h' | 'H') | KeyCode::Left) { -1 } else { 1 };
                // H/L on a number are ten single steps rather than one of ten, so a big step
                // stops at the limit — `apply`'s — instead of being refused short of it. On a
                // choice they are h/l: ten choices along is nowhere anyone is aiming for.
                let number = sh.field().is_some_and(|f| f.unit.is_number());
                let times = if matches!(k.code, KeyCode::Char('H' | 'L')) && number { form::Unit::BIG_STEP } else { 1 };
                let (Some(target), Some(name)) = (sh.target, sh.field().map(|f| f.name)) else { return };
                self.checkpoint();
                let mut moved = 0;
                for _ in 0..times {
                    let Some(value) = self.sheet.as_ref().and_then(|sh| sh.cycled(delta, &self.doc)) else { break };
                    match form::apply_for(&mut self.doc, target, name, &value, &self.selection) {
                        Ok(()) => {
                            moved += 1;
                            if let Some(sh) = &mut self.sheet {
                                sh.refresh(&self.doc);
                            }
                        }
                        Err(why) => {
                            if moved == 0 {
                                self.say(why, Tone::Bad);
                            }
                            break;
                        }
                    }
                }
                if moved == 0 {
                    self.undo.pop();
                }
            }
            _ => {}
        }
    }

    /// The colour picker's keys: it moves like the sheet, and what it picks goes through
    /// the same write as typing the name would.
    fn colour_key(&mut self, k: KeyEvent) {
        let Some(pk) = &mut self.colour else { return };
        match pk.key(k) {
            colour::Outcome::Open => {}
            colour::Outcome::Cancel => self.colour = None,
            colour::Outcome::Pick(c) => {
                let (target, field) = (pk.target, pk.field);
                self.colour = None;
                self.checkpoint();
                let value = c.map(Colour::name).unwrap_or_default();
                match form::apply_for(&mut self.doc, target, field, &value, &self.selection) {
                    Ok(()) => {
                        if let Some(c) = c {
                            self.recent_colours.retain(|r| *r != c);
                            self.recent_colours.insert(0, c);
                            self.recent_colours.truncate(colour::RECENT);
                        }
                        if let Some(sh) = &mut self.sheet {
                            sh.refresh(&self.doc);
                        }
                    }
                    Err(why) => {
                        self.undo.pop();
                        self.say(why, Tone::Bad);
                    }
                }
            }
        }
    }

    fn insert_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Esc | KeyCode::Enter => self.commit_insert(),
            KeyCode::Backspace => {
                if let Some(ins) = &mut self.insert {
                    ins.buf.pop();
                }
            }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(ins) = &mut self.insert {
                    ins.buf.push(c);
                }
            }
            _ => {}
        }
    }

    fn confirm_key(&mut self, k: KeyEvent) {
        let Some(c) = self.confirm.take() else { return };
        // `dd` deletes the picked set: the second d is the yes.
        let again = c == Confirm::DeletePicked && k.code == KeyCode::Char('d');
        if !again && !matches!(k.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            self.say("cancelled", Tone::Note);
            return;
        }
        match c {
            Confirm::DeleteElement(id) => self.delete_element(id),
            Confirm::DeletePicked => self.delete_picked(),
            Confirm::CloseTab => self.close_tab(),
            Confirm::Discard(p) => self.discard_then(p),
            Confirm::DeleteWorkbenchEntry(node) => self.workbench_delete(node),
        }
    }

    fn discard_then(&mut self, p: Pending) {
        match p {
            Pending::Quit => self.should_quit = true,
            Pending::New => self.new_document(),
            Pending::Open(path) => {
                if let Err(e) = self.open_path(path) {
                    self.say(e, Tone::Bad);
                }
            }
            Pending::Import(path) => {
                if let Err(e) = self.import_path(path) {
                    self.say(e, Tone::Bad);
                }
            }
        }
    }

    fn help_key(&mut self, k: KeyEvent) {
        let w = self.whereami();
        if self.help.as_ref().is_some_and(|h| h.searching()) {
            let h = self.help.as_mut().expect("open");
            match k.code {
                KeyCode::Esc => h.filter = None,
                KeyCode::Backspace => h.retype(|f| {
                    f.pop();
                }),
                KeyCode::Char(c) => h.retype(|f| f.push(c)),
                KeyCode::Down => h.move_by(1, &w),
                KeyCode::Up => h.move_by(-1, &w),
                KeyCode::Enter => self.run_picked(&w),
                _ => {}
            }
            return;
        }
        match k.code {
            KeyCode::Esc | KeyCode::Char('?') => self.help = None,
            KeyCode::Char('/') => {
                if let Some(h) = &mut self.help {
                    h.filter = Some(String::new());
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(h) = &mut self.help {
                    h.move_by(1, &w);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(h) = &mut self.help {
                    h.move_by(-1, &w);
                }
            }
            KeyCode::Enter => self.run_picked(&w),
            _ => {
                if let Some(cmd) = keymap::COMMANDS.iter().find(|c| c.prefix.is_none() && c.on.iter().any(|s| s.matches(&k))) {
                    self.run_cmd(cmd, (cmd.avail)(&w));
                }
            }
        }
    }

    fn run_picked(&mut self, w: &Where) {
        if let Some((cmd, avail)) = self.help.as_ref().and_then(|h| h.picked(w)) {
            self.run_cmd(cmd, avail);
        }
    }

    /// Run a command from a menu by replaying its keystrokes into the dispatcher, so there
    /// stays exactly one path through which anything happens. Whichever menu called this —
    /// `?`'s or the right-click one — closes first: a replayed keystroke must reach the
    /// diagram, not loop back into the menu that is still sitting on top of it.
    fn run_cmd(&mut self, cmd: &'static keymap::Cmd, avail: Avail) {
        match avail {
            Avail::No(why) => self.say(why, Tone::Bad),
            Avail::Yes if !cmd.runnable() => self.say(format!("{} — press it in the diagram", cmd.keys), Tone::Note),
            Avail::Yes => {
                self.help = None;
                self.ctxmenu = None;
                for s in cmd.run {
                    self.on_key(s.event());
                }
            }
        }
    }

    fn palette_key(&mut self, k: KeyEvent) {
        let Some(p) = &mut self.palette else { return };
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        // Opened out of a handle, the direction can still change: the same ^-chord that
        // opened it, or tab onto the compass and use the bare keys.
        if p.dir.is_some() {
            if let KeyCode::Char(c) = k.code
                && (ctrl || p.compass)
                && let Some(d) = palette::dir_of(c)
            {
                p.dir = Some(d);
                self.pending_port = Some(d);
                return;
            }
            if k.code == KeyCode::Tab || k.code == KeyCode::BackTab {
                p.compass = !p.compass;
                return;
            }
            if p.compass && k.code == KeyCode::Esc {
                p.compass = false;
                return;
            }
        }
        match k.code {
            KeyCode::Esc => {
                self.palette = None;
                self.pending_port = None;
            }
            KeyCode::Down | KeyCode::Tab => p.move_by(1),
            KeyCode::Up | KeyCode::BackTab => p.move_by(-1),
            KeyCode::Backspace => p.retype(|f| {
                f.pop();
            }),
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => p.retype(|f| f.push(c)),
            KeyCode::Enter => {
                let (picked, via) = (p.picked(), p.via());
                let purpose = p.purpose;
                self.palette = None;
                match (picked, purpose) {
                    (Some(kind), palette::Purpose::Add) => self.add_kind(kind),
                    (Some(kind), palette::Purpose::Relate(_)) => {
                        if let Some(from) = self.cursor {
                            self.open_off_finish(from, kind, via);
                        }
                    }
                    (None, _) => {}
                }
            }
            _ => {}
        }
    }

    fn relpick_key(&mut self, k: KeyEvent) {
        if k.code == KeyCode::Char(':') {
            self.relpick = None;
            self.pending_label = None;
            self.cmdline = Some(cmdline::State::new(':'));
            return;
        }
        let Some(p) = &mut self.relpick else { return };
        match k.code {
            KeyCode::Esc => {
                self.relpick = None;
                self.pending_port = None;
                // Opened off a shape and not related after all: the shape stays, unlinked.
                if let Some(id) = self.pending_label.take() {
                    self.set_cursor(id);
                    self.start_insert(Target::Element(id));
                }
            }
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => p.move_by(1),
            KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => p.move_by(-1),
            KeyCode::Enter => {
                if let Some((kind, verdict)) = p.picked() {
                    self.apply_relpick(kind, verdict);
                }
            }
            _ => {}
        }
    }

    fn tabpick_key(&mut self, k: KeyEvent) {
        if k.code == KeyCode::Char(':') {
            self.tabpick = None;
            self.cmdline = Some(cmdline::State::new(':'));
            return;
        }
        let Some(p) = &mut self.tabpick else { return };
        match k.code {
            KeyCode::Esc => self.tabpick = None,
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => p.move_by(1),
            KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => p.move_by(-1),
            KeyCode::Enter => {
                let view = p.picked();
                self.tabpick = None;
                self.new_tab(view);
                // The name is the next thing anyone wants to type, so the line opens ready
                // for it — enter names it, esc keeps "diagram N".
                let mut cl = cmdline::State::new(':');
                for c in "tabrename ".chars() {
                    cl.push(c);
                }
                self.cmdline = Some(cl);
                self.say(format!("new {} tab — type its name", view.badge()), Tone::Note);
            }
            _ => {}
        }
    }

    fn cmdline_key(&mut self, k: KeyEvent) {
        let Some(cl) = &mut self.cmdline else { return };
        match k.code {
            KeyCode::Esc => self.cmdline = None,
            KeyCode::Backspace => {
                if cl.backspace() {
                    self.cmdline = None;
                }
            }
            KeyCode::Up => cl.recall_prev(&self.cmd_history),
            KeyCode::Down => cl.recall_next(&self.cmd_history),
            KeyCode::Tab | KeyCode::BackTab => {
                if cl.prompt == ':' {
                    let list = excmd::candidates(cl.tab_prefix());
                    cl.cycle_to(&list, k.code == KeyCode::Tab);
                }
            }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => cl.push(c),
            KeyCode::Enter => {
                let prompt = cl.prompt;
                let line = cl.submit();
                self.cmdline = None;
                match prompt {
                    '/' => {
                        if !line.trim().is_empty() {
                            self.search = Some(line.trim().to_string());
                            self.search_step(true);
                            self.follow_camera();
                        }
                    }
                    _ => {
                        if !line.trim().is_empty() {
                            self.cmd_history.retain(|h| h != &line);
                            self.cmd_history.push(line.clone());
                            if self.cmd_history.len() > cmdline::MAX_HISTORY {
                                self.cmd_history.remove(0);
                            }
                            self.run_excmd(line);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // ─── ex-commands ────────────────────────────────────────────────────────

    pub fn run_excmd(&mut self, line: String) {
        let (word, bang, arg) = excmd::parse(&line);
        if word.is_empty() {
            return;
        }
        let Some(cmd) = excmd::resolve(&word) else {
            self.say(format!("not a command: {word} — :help commands"), Tone::Bad);
            return;
        };
        match cmd.op {
            excmd::Op::Write => {
                self.write(&arg);
            }
            excmd::Op::Wq => {
                if self.write(&arg) {
                    self.should_quit = true;
                }
            }
            excmd::Op::Quit => {
                if self.dirty() && !bang {
                    self.confirm = Some(Confirm::Discard(Pending::Quit));
                } else {
                    self.should_quit = true;
                }
            }
            excmd::Op::New => {
                if self.dirty() && !bang {
                    self.confirm = Some(Confirm::Discard(Pending::New));
                } else {
                    self.new_document();
                }
            }
            excmd::Op::Open => {
                if arg.is_empty() {
                    self.say("usage: :open <path>", Tone::Bad);
                    return;
                }
                let path = PathBuf::from(arg);
                if self.dirty() && !bang {
                    self.confirm = Some(Confirm::Discard(Pending::Open(path)));
                } else if let Err(e) = self.open_path(path) {
                    self.say(e, Tone::Bad);
                }
            }
            excmd::Op::Import => {
                if arg.is_empty() {
                    self.importdlg = Some(importdlg::State::new());
                    return;
                }
                let path = PathBuf::from(arg);
                if self.dirty() && !bang {
                    self.confirm = Some(Confirm::Discard(Pending::Import(path)));
                } else if let Err(e) = self.import_path(path) {
                    self.say(e, Tone::Bad);
                }
            }
            excmd::Op::Add => {
                if arg.is_empty() {
                    self.palette = Some(palette::State::new(self.doc.metadata.view));
                } else {
                    match ShapeKind::parse(&arg) {
                        Some(kind) => self.add_kind(kind),
                        None => self.say(format!("no such kind: {arg} — tab completes them, :help layers lists them"), Tone::Bad),
                    }
                }
            }
            excmd::Op::Idiom => {
                if arg.is_empty() {
                    let names: Vec<&str> = idiom::IDIOMS.iter().map(|i| i.name).collect();
                    self.say(format!("idioms: {} — :idiom <name>, or :help idioms", names.join(", ")), Tone::Note);
                    return;
                }
                match idiom::find(&arg) {
                    Some(i) => {
                        let origin = match self.doc.bounds() {
                            Some((bx, _, _, bb)) => (bx, bb + layout::GUT_Y * 2.0),
                            None => (2.0, 2.0),
                        };
                        self.checkpoint();
                        let ids = idiom::stamp(&mut self.doc, i, origin);
                        if let Some(first) = ids.first() {
                            self.set_cursor(*first);
                        }
                        self.follow_camera();
                        self.say(format!("stamped {} — {}", i.name, i.tagline), Tone::Good);
                    }
                    None => self.say(format!("no such idiom: {arg} — :idiom lists them"), Tone::Bad),
                }
            }
            excmd::Op::Layout => {
                if self.doc.elements.is_empty() {
                    self.say("nothing to arrange yet", Tone::Bad);
                    return;
                }
                match arg.as_str() {
                    "" | "layers" => {
                        let pos = layout::layers(&self.doc);
                        self.apply_layout(pos);
                        self.say("arranged by layer", Tone::Good);
                    }
                    "flow" => {
                        let pos = layout::flow(&self.doc);
                        self.apply_layout(pos);
                        self.say("arranged by flow", Tone::Good);
                    }
                    other => self.say(format!("unknown layout: {other} — layers or flow"), Tone::Bad),
                }
            }
            excmd::Op::Lint => self.lint(),
            excmd::Op::Kind => {
                if arg.is_empty() {
                    let v = self.doc.metadata.view;
                    self.say(format!("this is a {} view — {}", v.name(), v.tagline()), Tone::Note);
                    return;
                }
                match View::parse(&arg) {
                    Some(v) => {
                        self.checkpoint();
                        self.doc.metadata.view = v;
                        let outside = self.doc.out_of_view().len();
                        let msg = match outside {
                            0 => format!("a {} view — {}", v.name(), v.tagline()),
                            n => format!("a {} view — {n} element{} now outside it (:lint)", v.name(), if n == 1 { "" } else { "s" }),
                        };
                        self.say(msg, Tone::Good);
                    }
                    None => self.say(format!("no such view: {arg} — {}", View::ALL.iter().map(|v| v.name()).collect::<Vec<_>>().join(", ")), Tone::Bad),
                }
            }
            excmd::Op::Title => {
                self.checkpoint();
                self.doc.metadata.title = if arg.is_empty() { None } else { Some(arg.clone()) };
                self.say(if arg.is_empty() { "title cleared".to_string() } else { format!("titled {arg:?}") }, Tone::Good);
            }
            excmd::Op::Export => {
                // Bare, the dialog; with a path, the format its extension names, at the
                // defaults — the way :w with a path skips the question.
                if arg.is_empty() {
                    self.export = Some(exportdlg::State::new(self.tab_name(), self.doc.metadata.page.grid));
                    return;
                }
                let path = PathBuf::from(&arg);
                let Some(format) = crate::export::Format::of_path(&path) else {
                    self.say("which format? name the file .png, .svg, .pdf, .drawio or .html — or :export alone for the dialog", Tone::Bad);
                    return;
                };
                let o = crate::export::Options { format, grid: self.doc.metadata.page.grid, ..crate::export::Options::default() };
                self.export_now(&o, &path);
            }
            excmd::Op::TabNew => {
                let trimmed = arg.trim();
                let (kind, name) = match trimmed.split_once(char::is_whitespace) {
                    Some((k, n)) => (k, n.trim()),
                    None => (trimmed, ""),
                };
                let view = match kind {
                    "" => {
                        self.tabpick = Some(tabpick::State::new());
                        return;
                    }
                    "freeform" | "f" => View::Freeform,
                    "architecture" | "a" | "arch" => View::Free,
                    // Not a kind word — try it as a path, the whole argument (a kind takes
                    // a name after it; a path is not split apart looking for one), the same
                    // way vim's own :tabnew <file> opens a file into a new tab rather than
                    // starting one empty.
                    _ => {
                        self.tabnew_from_path(PathBuf::from(trimmed));
                        return;
                    }
                };
                self.new_tab(view);
                if !name.is_empty() {
                    self.tabs[self.tab].name = name.to_string();
                }
                self.say(format!("new {} tab: {}", view.badge(), self.tab_name()), Tone::Good);
            }
            excmd::Op::TabRename => {
                if arg.is_empty() {
                    self.say("usage: :tabrename <name>", Tone::Bad);
                    return;
                }
                self.tabs[self.tab].name = arg.clone();
                self.say(format!("tab renamed to {arg:?}"), Tone::Good);
            }
            excmd::Op::TabClose => {
                if self.tabs.len() > 1 && !self.doc.elements.is_empty() && self.dirty() && !bang {
                    self.confirm = Some(Confirm::CloseTab);
                } else {
                    self.close_tab();
                }
            }
            excmd::Op::Tab => match arg.parse::<usize>() {
                Ok(n) if n >= 1 && n <= self.tabs.len() => self.switch_tab(n - 1),
                _ => self.say(format!("which tab? 1 to {}", self.tabs.len()), Tone::Bad),
            },
            excmd::Op::Tabs => {
                let list: Vec<String> = self
                    .tabs
                    .iter()
                    .enumerate()
                    .map(|(i, t)| format!("{}{} {}", i + 1, if i == self.tab { "*" } else { "" }, t.name))
                    .collect();
                self.say(format!("tabs: {} — :tab N, gt / gT", list.join(" · ")), Tone::Note);
            }
            excmd::Op::Grid => {
                let on = match arg.as_str() {
                    "on" => true,
                    "off" => false,
                    _ => !self.doc.metadata.page.grid,
                };
                self.checkpoint();
                self.doc.metadata.page.grid = on;
                self.say(if on { "grid on" } else { "grid off — it is the diagram's setting: :diagram has the rest" }, Tone::Note);
            }
            excmd::Op::Render => {
                let mut words = arg.split_whitespace();
                let Some(out) = words.next() else {
                    self.say("usage: :render <out.png> [px] [font path]", Tone::Bad);
                    return;
                };
                let px: f32 = words.next().and_then(|p| p.parse().ok()).unwrap_or(20.0);
                let font = words.next();
                let o = crate::export::Options { grid: self.doc.metadata.page.grid, font: font.map(str::to_string), ..crate::export::Options::default() };
                match crate::render::to_png(&self.doc, &o, px, std::path::Path::new(out)) {
                    Ok((cw, ch, w, h)) => self.say(format!("rendered {cw}x{ch} cells → {w}x{h} px → {out}"), Tone::Good),
                    Err(e) => self.say(e, Tone::Bad),
                }
            }
            excmd::Op::Props => match self.props.take() {
                Some(_) => {}
                None => self.open_props(),
            },
            excmd::Op::Layers => {
                self.layers = match self.layers.take() {
                    Some(_) => None,
                    None => Some(layers::State::new(&self.doc)),
                };
            }
            excmd::Op::Tree => {
                if self.model_tree.is_empty() {
                    self.say("no model tree — :import a coArchi model folder or a Foundry ontology export first", Tone::Bad);
                } else {
                    self.tree = match self.tree.take() {
                        Some(_) => None,
                        None => Some(tree::State::new()),
                    };
                }
            }
            excmd::Op::Workbench => {
                if self.workbench.is_some() {
                    self.workbench = None;
                } else if !arg.is_empty() {
                    self.open_workbench(PathBuf::from(arg));
                } else {
                    let recents: Vec<PathBuf> = crate::config::load().workbenches.into_iter().map(PathBuf::from).collect();
                    if recents.is_empty() {
                        self.say("usage: :workbench <path> — no workbench opened before to pick from", Tone::Bad);
                    } else {
                        self.workbench = Some(workbench::State::recents(recents));
                    }
                }
            }
            excmd::Op::Sheet => {
                self.sheet = match self.sheet.take() {
                    Some(_) => None,
                    None => Some(sheet::State::open(&self.doc, self.sheet_target(), &self.selection)),
                };
            }
            excmd::Op::Diagram => {
                let mut sh = sheet::State::open(&self.doc, Some(Target::Diagram), &[]);
                sh.pinned = true;
                self.sheet = Some(sh);
            }
            excmd::Op::Ink => {
                let word = arg.trim().to_ascii_lowercase();
                match (word.as_str(), wire::Ink::parse(&word)) {
                    ("", _) => self.say(format!("ink {} — :ink lines / braille", wire::ink().name()), Tone::Note),
                    (_, Some(i)) => {
                        wire::set_ink(i);
                        match crate::config::save(|c| c.ink = Some(i.name().into())) {
                            Ok(()) => self.say(format!("ink {} — kept in the config file", i.name()), Tone::Note),
                            Err(why) => self.say(format!("ink {} for this run — not kept: {why}", i.name()), Tone::Bad),
                        }
                    }
                    (other, None) => self.say(format!("ink is lines or braille, not {other:?}"), Tone::Bad),
                }
            }
            excmd::Op::Theme => {
                let word = arg.trim().to_ascii_lowercase();
                match word.as_str() {
                    "" => self.say(format!("theme {} — from {}; :theme dark / light / auto", theme::mode().name(), theme::source().name()), Tone::Note),
                    "dark" | "light" | "auto" => {
                        let (mode, from) = match theme::Mode::parse(&word) {
                            Some(m) => (m, theme::Source::Command),
                            None => theme::choose(None, None, std::env::var("COLORFGBG").ok().as_deref()),
                        };
                        theme::set_mode(mode, from);
                        match crate::config::save(|c| c.theme = Some(word.clone())) {
                            Ok(()) => self.say(format!("theme {} — kept in the config file", mode.name()), Tone::Note),
                            Err(why) => self.say(format!("theme {} for this run — not kept: {why}", mode.name()), Tone::Bad),
                        }
                    }
                    other => self.say(format!("theme is dark, light or auto, not {other:?}"), Tone::Bad),
                }
            }
            excmd::Op::Debug => {
                self.debug = !self.debug;
                self.say(if self.debug { "debug panel on — :debug closes it" } else { "debug panel off" }, Tone::Note);
            }
            excmd::Op::Help => {
                let mut m = manual::State::new();
                if !arg.is_empty() && !m.goto_topic(&arg) {
                    self.say(format!("no help for {arg:?} — :help lists the topics"), Tone::Bad);
                    return;
                }
                self.manual = Some(m);
            }
        }
    }

    fn export_now(&mut self, o: &crate::export::Options, path: &std::path::Path) {
        let title = self.doc.metadata.title.clone().unwrap_or_else(|| self.tab_name().to_string());
        match crate::export::write(&self.doc, &title, o, path) {
            Ok(said) => self.say(format!("exported {said}"), Tone::Good),
            Err(e) => self.say(e, Tone::Bad),
        }
    }

    /// `V`: the tab as a PNG, in a scratch file, opened with whatever opens pictures here.
    /// The diagram sets the size, each shape its font; nothing is asked.
    fn preview(&mut self) {
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-preview-{}.png", exportdlg_slug(self.tab_name())));
        let o = crate::export::Options::preview();
        match crate::export::write(&self.doc, self.tab_name(), &o, &path) {
            Ok(said) => match crate::export::open_with_system(&path) {
                Ok(()) => self.say(format!("preview {said}"), Tone::Good),
                Err(e) => self.say(format!("rendered {said} — but {e}"), Tone::Bad),
            },
            Err(e) => self.say(e, Tone::Bad),
        }
    }

    fn export_key(&mut self, k: KeyEvent) {
        let Some(d) = &mut self.export else { return };
        if let Some(buf) = &mut d.editing {
            match k.code {
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => buf.push(c),
                KeyCode::Enter | KeyCode::Esc => {
                    let text = buf.clone();
                    if let Err(why) = d.commit(&text, &self.doc) {
                        if k.code == KeyCode::Esc {
                            d.editing = None;
                        }
                        self.say(why, Tone::Bad);
                    }
                }
                _ => {}
            }
            return;
        }
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => self.export = None,
            KeyCode::Char(':') => {
                self.export = None;
                self.cmdline = Some(cmdline::State::new(':'));
            }
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => d.move_by(1),
            KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => d.move_by(-1),
            KeyCode::Char('i') => d.step_in(&self.doc),
            KeyCode::Char('h') | KeyCode::Left => d.cycle(-1),
            KeyCode::Char('l') | KeyCode::Right => d.cycle(1),
            KeyCode::Enter => {
                let (o, path) = (d.options.clone(), d.file.clone());
                self.export = None;
                self.export_now(&o, &path);
            }
            _ => {}
        }
    }

    fn write(&mut self, arg: &str) -> bool {
        let path = if arg.is_empty() { self.path.clone() } else { Some(PathBuf::from(arg)) };
        let Some(path) = path else {
            self.say("no file name — :w <path>", Tone::Bad);
            return false;
        };
        match persistence::save(&self.workspace(), &path) {
            Ok(()) => {
                self.saved = self.serialized();
                self.path = Some(path.clone());
                self.say(format!("saved {}", path.display()), Tone::Good);
                true
            }
            Err(e) => {
                self.say(format!("could not save {}: {e}", path.display()), Tone::Bad);
                false
            }
        }
    }

    // ─── drawing ────────────────────────────────────────────────────────────

    pub fn draw(&mut self, f: &mut Frame) {
        // The app owns its ground: every cell starts as the palette's, so a light palette is
        // light everywhere and not light text on whatever the terminal had.
        let whole = f.area();
        f.buffer_mut().set_style(whole, Style::new().bg(theme::t().ground));
        if self.loading {
            f.render_widget(splash::Splash, f.area());
            return;
        }
        if self.present {
            // The diagram alone: no header, no footer, no grid, no cursor — the picture as
            // `:render` writes it, at the terminal's own size.
            let refused: Vec<RelationId> = self.doc.lint().into_iter().map(|p| p.relation).collect();
            f.render_widget(
                canvas::Scene {
                    doc: &self.doc,
                    cursor: None,
                    focus_rel: None,
                    focus_node: Node::Centre,
                    holding: None,
                    picked: &[],
                    camera: self.camera,
                    letters: None,
                    insert: None,
                    refused: &refused,
                    reshape: None,
                    hover: None,
                    marquee: None,
                    labels: true,
                    grid: false,
                    ink: wire::ink(),
                },
                f.area(),
            );
            return;
        }
        let [top, full0, foot] = Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
        // Unlike the right-hand sheet/layers/props dock, the workbench never floats — it is a
        // folder you are working against, not a momentary editor, so it always holds its own
        // column on the left. Clamped rather than gated on a minimum width: a terminal too
        // narrow for the full column still gets whatever is left, same as `chrome::centered`
        // clamps everywhere else, rather than the panel vanishing into a float.
        let left_width = if self.workbench.is_some() { workbench::WIDTH.min(full0.width) } else { 0 };
        let full = Rect { x: full0.x + left_width, width: full0.width - left_width, ..full0 };
        // The sheet follows the cursor: point it at whatever is under the cursor now.
        let target = self.sheet_target();
        if let Some(sh) = &mut self.sheet
            && (sh.target != target || (target == Some(Target::Picked) && sh.picked != self.selection))
            && !(sh.pinned && sh.focused)
        {
            sh.pinned = false;
            sh.retarget(&self.doc, target, &self.selection);
        }
        // The sheet and the layer browser dock on the right when there is room, the browser
        // above the sheet when both are up; on a narrow terminal they float over the diagram,
        // the sheet only while it has the keyboard.
        let docked = (self.sheet.is_some() || self.layers.is_some() || self.props.is_some()) && full.width >= sheet::DOCK_MIN;
        let body = if docked { Rect { width: full.width - sheet::WIDTH, ..full } } else { full };
        self.body = body;
        if self.view_size != (body.width, body.height) {
            self.view_size = (body.width, body.height);
            self.follow_camera();
        }
        self.draw_header(f, top);

        if let Some(m) = &mut self.manual {
            m.view = body.height.saturating_sub(2) as usize;
            f.render_widget(manual::Manual { state: m }, body);
            self.draw_footer(f, foot);
            return;
        }

        let refused: Vec<RelationId> = self.doc.lint().into_iter().map(|p| p.relation).collect();
        let letters = if self.pending_prefix == Some(Prefix::F) { Some(self.letters()) } else { None };
        let insert = self.insert.as_ref().map(|i| (i.target, i.buf.as_str()));
        f.render_widget(
            canvas::Scene {
                doc: &self.doc,
                cursor: self.cursor,
                focus_rel: self.focused_relation(),
                focus_node: self.node,
                holding: self.holding,
                picked: &self.selection,
                camera: self.camera,
                letters: letters.as_deref(),
                insert,
                refused: &refused,
                reshape: self.cursor.zip(self.reshape).map(|(id, r)| (id, r.handle, r.held)),
                hover: self.hover,
                marquee: self.marquee,
                labels: true,
                grid: self.doc.metadata.page.grid,
                ink: wire::ink(),
            },
            body,
        );
        if let Some(p) = &self.tabpick {
            let area = chrome::centered(body, tabpick::WIDTH, tabpick::HEIGHT);
            f.render_widget(tabpick::Picker { state: p }, area);
        }
        if let Some(st) = &self.tree {
            let area = chrome::centered(body, tree::WIDTH.max(48), tree::height(body.height));
            f.render_widget(tree::Browser { state: st, nodes: &self.model_tree }, area);
        }
        if let Some(st) = &self.workbench {
            let area = Rect { x: full0.x, y: full.y, width: left_width, height: full.height };
            f.render_widget(workbench::Browser { state: st }, area);
        }
        let layers_h = self.layers.as_ref().map_or(0, |_| (self.doc.layers.len() as u16 + 6).min(full.height));
        let props_h = self.props.as_ref().map_or(0, |p| props::height(self.doc.element(p.target).map_or(0, |e| e.properties.len()), full.height.saturating_sub(layers_h)));
        let browser_h = layers_h + props_h;
        if let Some(sh) = &self.sheet {
            if docked {
                let area = Rect { x: body.right(), y: full.y + browser_h, width: sheet::WIDTH, height: full.height.saturating_sub(browser_h) };
                f.render_widget(sheet::Sheet { state: sh, doc: &self.doc }, area);
            } else if sh.focused {
                let area = chrome::centered(body, sheet::WIDTH.max(48), body.height.saturating_sub(2));
                f.render_widget(sheet::Sheet { state: sh, doc: &self.doc }, area);
            }
        }
        if let Some(st) = &self.layers {
            let cursor_layer = match self.target() {
                Some(Target::Relation(r, _)) => self.doc.relation(r).map(|r| r.layer),
                Some(Target::Element(e)) => self.doc.element(e).map(|e| e.layer),
                Some(Target::Diagram) | Some(Target::Picked) | None => None,
            };
            let area = if docked {
                Rect { x: body.right(), y: full.y, width: sheet::WIDTH, height: layers_h }
            } else {
                chrome::centered(body, layers::WIDTH.max(48), layers_h)
            };
            f.render_widget(layers::Browser { state: st, doc: &self.doc, cursor_layer }, area);
        }
        if let Some(st) = &self.props {
            let area = if docked {
                Rect { x: body.right(), y: full.y + layers_h, width: sheet::WIDTH, height: props_h }
            } else {
                chrome::centered(body, props::WIDTH.max(48), props_h)
            };
            f.render_widget(props::Browser { state: st, doc: &self.doc }, area);
        }
        if let Some(pk) = &self.colour {
            let area = chrome::centered(body, colour::WIDTH, colour::height());
            f.render_widget(colour::Picker { state: pk }, area);
        }
        if let Some(d) = &self.export {
            let area = chrome::centered(body, exportdlg::WIDTH, exportdlg::HEIGHT);
            f.render_widget(exportdlg::Dialog { state: d, doc: &self.doc }, area);
        }
        if let Some(d) = &self.importdlg {
            let area = chrome::centered(body, importdlg::WIDTH, importdlg::HEIGHT);
            f.render_widget(importdlg::Dialog { state: d }, area);
        }
        if let Some(d) = &self.expandpick {
            let area = chrome::centered(body, expandpick::WIDTH, expandpick::height(d.rows.len()));
            f.render_widget(expandpick::Dialog { state: d }, area);
        }
        if let Some(d) = &self.ctxmenu {
            let area = chrome::centered(body, ctxmenu::WIDTH, ctxmenu::height(d.rows.len()));
            f.render_widget(ctxmenu::Dialog { state: d }, area);
        }
        if self.debug {
            let rows = self.debug_rows();
            let w = debug::WIDTH.min(body.width);
            let area = Rect { x: body.right() - w, width: w, ..body };
            f.render_widget(debug::Panel { rows: &rows, keys: &self.keys }, area);
        }
        if let Some(st) = &self.start {
            // Always the same size, however big the terminal: a dialog that changed shape with
            // the window would be a different dialog every time.
            let area = chrome::centered(body, start::WIDTH, start::HEIGHT);
            f.render_widget(start::Dialog { state: st }, area);
        }

        if let Some(p) = &self.palette {
            let area = chrome::centered(body, palette::WIDTH, palette::height(body.height));
            f.render_widget(palette::Palette { state: p }, area);
        }
        if let Some(p) = &self.relpick {
            let area = chrome::centered(body, relpick::WIDTH, relpick::height(p, body.height));
            f.render_widget(relpick::Picker { state: p, doc: &self.doc }, area);
        }
        if let Some(h) = &self.help {
            let w = self.whereami();
            let area = chrome::centered(body, help::WIDTH, help::height(body.height));
            f.render_widget(help::Menu { state: h, w: &w, here: self.here_label() }, area);
        }
        self.draw_footer(f, foot);

        // The wildmenu opens upward from just above the command line.
        if let Some(cl) = &self.cmdline
            && cl.prompt == ':'
        {
            let cands = excmd::candidates(cl.tab_prefix());
            let show = !cands.is_empty() && !(cands.len() == 1 && cands[0] == cl.buf);
            if show {
                let menu = wildmenu::WildMenu { candidates: &cands, current: &cl.buf };
                let h = menu.height(body.height);
                let w = menu.width(body.width);
                let area = Rect { x: body.x + 1, y: foot.y.saturating_sub(h), width: w, height: h };
                f.render_widget(menu, area);
            }
        }
    }

    /// What the debugging panel shows: the app's state, in words, live.
    fn debug_rows(&self) -> Vec<debug::Row> {
        let w = self.whereami();
        let on = match self.cursor_element() {
            Some(e) => format!("#{} {} ({})", e.id, e.display(), e.kind.slug()),
            None => "—".into(),
        };
        let rel = match self.focused_relation().and_then(|r| self.doc.relation(r)) {
            Some(r) => format!("#{} {} {}→{} port {:?}/{:?} · {}", r.id, r.kind.name(), r.from, r.to, r.from_port, r.to_port, self.node.name()),
            None => "—".into(),
        };
        let reshape = match self.reshape {
            Some(r) => format!("handle {} held {} moving {:?}", r.handle, r.held, r.moving.map(|m| m.0)),
            None => "—".into(),
        };
        let panels: Vec<&str> = [
            (self.palette.is_some(), "palette"),
            (self.relpick.is_some(), "relpick"),
            (self.sheet.is_some(), "sheet"),
            (self.layers.is_some(), "layers"),
            (self.tree.is_some(), "tree"),
            (self.props.is_some(), "props"),
            (self.workbench.is_some(), "workbench"),
            (self.help.is_some(), "help"),
            (self.manual.is_some(), "manual"),
            (self.export.is_some(), "export"),
            (self.importdlg.is_some(), "importdlg"),
            (self.expandpick.is_some(), "expandpick"),
            (self.ctxmenu.is_some(), "ctxmenu"),
            (self.tabpick.is_some(), "tabpick"),
            (self.start.is_some(), "start"),
            (self.confirm.is_some(), "confirm"),
            (self.cmdline.is_some(), "cmdline"),
            (self.insert.is_some(), "insert"),
        ]
        .iter()
        .filter(|(up, _)| *up)
        .map(|(_, n)| *n)
        .collect();
        vec![
            ("mode", format!("{:?}{}", w.mode, if self.present { " (present)" } else { "" })),
            ("keys", if self.enhanced_keys { "kitty protocol — shift+ctrl distinct".into() } else { "plain — shift+ctrl arrives as ctrl; ^hjkl move".into() }),
            ("theme", format!("{} — from {}", theme::mode().name(), theme::source().name())),
            ("ink", wire::ink().name().to_string()),
            ("panels", if panels.is_empty() { "—".into() } else { panels.join(" ") }),
            ("last key", self.last_resolved.clone()),
            ("last mouse", self.last_mouse.clone()),
            ("prefix", format!("{:?}  count {:?}", self.pending_prefix, self.count)),
            ("", String::new()),
            ("cursor", on),
            ("relation", rel),
            ("inside", reshape),
            ("holding", format!("{:?}  carrying {}  picked {}", self.holding, self.clip.is_some(), self.selection.len())),
            ("", String::new()),
            ("tab", format!("{} of {} · {}", self.tab + 1, self.tabs.len(), self.tab_name())),
            ("file", self.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "—".into())),
            ("dirty", format!("{}  undo {}  redo {}", self.dirty(), self.undo.len(), self.redo.len())),
            ("jumps", format!("back {}  fwd {}", self.back.len(), self.forward.len())),
            ("camera", format!("({}, {})  view {}x{}", self.camera.0, self.camera.1, self.view_size.0, self.view_size.1)),
            ("", String::new()),
            ("diagram", format!("{} shapes  {} relations  {} refused  view {}", self.doc.elements.len(), self.doc.relations.len(), self.doc.lint().len(), self.doc.metadata.view.name())),
            ("ids", format!("next shape {}  next relation {}", self.doc.next_element_id, self.doc.next_relation_id)),
            ("fonts", format!("{} families · {}", crate::fonts::families().len(), crate::render::find_font(None).unwrap_or_else(|| "none found".into()))),
        ]
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let badge = |text: String, bg: Color| Span::styled(text, Style::new().fg(theme::t().inverse).bg(bg).bold());
        let mut left: Vec<Span> = vec![badge(" vim-shapes ".into(), theme::t().aqua), Span::raw(" ")];
        let name = match (&self.doc.metadata.title, &self.path) {
            (Some(t), _) => t.clone(),
            (None, Some(p)) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            (None, None) => "[no name]".into(),
        };
        left.push(Span::styled(format!("{name}{}", if self.dirty() { " +" } else { "" }), Style::new().fg(theme::t().bright)));
        // The tabs, numbered in `:tab N`'s order: the one you are on is a solid block, the
        // others plain text. The colour is the indicator, and it moves the moment you gt away.
        left.push(Span::raw("  "));
        for (i, t) in self.tabs.iter().enumerate() {
            let text = format!(" {} {} ", i + 1, t.name);
            left.push(if i == self.tab {
                Span::styled(text, Style::new().fg(theme::t().inverse).bg(theme::t().ink).bold())
            } else {
                Span::styled(text, Style::new().fg(theme::t().dim))
            });
        }
        left.push(Span::raw(" "));
        left.push(badge(format!(" {} ", self.doc.metadata.view.badge()), theme::t().yellow));
        let mut right: Vec<Span> = Vec::new();
        let problems = self.doc.lint().len();
        if problems > 0 {
            right.push(badge(format!(" ⚠ {problems} "), theme::t().red));
            right.push(Span::raw(" "));
        }
        let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
        right.push(Span::styled(
            format!(" {} · {} ", plural(self.doc.elements.len(), "element"), plural(self.doc.relations.len(), "relation")),
            Style::new().fg(theme::t().dim),
        ));
        if let Some(e) = self.cursor_element() {
            right.push(Span::raw(" "));
            right.push(badge(format!(" {} ", e.kind.layer().name()), theme::layer_color(e.kind.layer())));
        }
        let right_w: u16 = right.iter().map(|s| s.width() as u16).sum();
        f.render_widget(Paragraph::new(Line::from(left)), area);
        if right_w < area.width {
            let r = Rect { x: area.x + area.width - right_w, width: right_w, ..area };
            f.render_widget(Paragraph::new(Line::from(right)), r);
        }
    }

    fn draw_footer(&self, f: &mut Frame, foot: Rect) {
        let strip = |text: String, bg: Color| Line::styled(text, Style::new().fg(theme::t().inverse).bg(bg).bold());
        let line = if let Some(c) = &self.confirm {
            let q = match c {
                Confirm::DeleteElement(id) => {
                    let name = self.doc.element(*id).map(|e| e.display()).unwrap_or_default();
                    let n = self.doc.incident(*id).len();
                    format!(" delete {name:?} and {n} relation{}? y/n ", if n == 1 { "" } else { "s" })
                }
                Confirm::DeletePicked => format!(" delete {} picked elements and their relations? d again, or y/n ", self.selection.len()),
                Confirm::CloseTab => format!(" close {:?} — its diagram is not saved. close anyway? y/n ", self.tab_name()),
                Confirm::Discard(p) => format!(" unsaved changes — {} anyway? y/n ", p.verb()),
                Confirm::DeleteWorkbenchEntry(entry) => match entry.kind {
                    crate::workbench::NodeKind::Folder => format!(" delete the folder {:?} and everything in it? y/n ", entry.name),
                    crate::workbench::NodeKind::Diagram => format!(" delete {:?}? y/n ", entry.name),
                },
            };
            strip(q, theme::t().yellow)
        } else if let Some(cl) = &self.cmdline {
            Line::from(vec![
                Span::styled(format!("{}{}", cl.prompt, cl.buf), Style::new().fg(theme::t().ink)),
                Span::styled("█", Style::new().fg(theme::t().aqua)),
            ])
        } else if let Some(ins) = &self.insert {
            let what = match ins.target {
                Target::Element(_) => "LABEL",
                Target::Diagram => "TITLE",
                Target::Picked => "LABELS",
                Target::Relation(_, node) => match node {
                    Node::Tail => "TAIL LABEL",
                    Node::Centre => "LABEL",
                    Node::Head => "HEAD LABEL",
                },
            };
            strip(format!(" {what}   type, then esc or enter "), theme::t().green)
        } else if let Some((msg, tone)) = &self.status {
            match tone {
                Tone::Good => strip(format!(" {msg} "), theme::t().green),
                Tone::Bad => strip(format!(" {msg} "), theme::t().red),
                Tone::Note => Line::styled(format!(" {msg} "), Style::new().fg(theme::t().bright)),
            }
        } else if let Some(r) = self.reshape {
            if r.moving.is_some() {
                strip(" END IN HAND   hjkl to an open handle   enter place it   esc put it back ".into(), theme::t().aqua)
            } else if r.held {
                strip(" DRAGGING   hjkl a cell   ^hjkl or HJKL four   enter or esc let go ".into(), theme::t().green)
            } else {
                let patched = self.cursor.is_some_and(|id| !self.doc.at_port(id, r.handle).is_empty());
                let here = if patched { "enter pick up the relation   x disconnect" } else { "enter take hold to drag" };
                strip(format!(" INSIDE   hjkl between the handles   {here}   o or ^hjkl/^yubn open a linked shape   esc out "), theme::t().yellow)
            }
        } else if self.pending_prefix == Some(Prefix::F) {
            strip(" jump: press an element's letter ".into(), theme::t().yellow)
        } else if self.visual {
            strip(format!(" PICKING  {} picked   space pick   H J K L move   c sheet   y copy   dd delete   esc ", self.selection.len()), theme::t().green)
        } else if let Some(from) = self.holding {
            let name = self.doc.element(from).map(|e| e.display()).unwrap_or_default();
            strip(format!(" relation from {name}: hjkl or f to another element, enter drops it   esc lets go "), theme::t().yellow)
        } else {
            let mut hints = String::new();
            for c in keymap::footer(&self.whereami()) {
                let item = format!("{} {}", c.keys, c.short);
                if hints.chars().count() + item.chars().count() + 4 > foot.width as usize {
                    break;
                }
                if !hints.is_empty() {
                    hints.push_str("  ");
                }
                hints.push_str(&item);
            }
            if let Some(n) = self.count {
                hints.push_str(&format!("   {n}"));
            }
            Line::styled(format!(" {hints} "), Style::new().fg(theme::t().dim))
        };
        f.render_widget(Paragraph::new(line), foot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Node as LinkNode;
    use crate::ontology::ShapeKind::*;
    use crate::ontology::{End, LineStyle};
    use keymap::Stroke;

    fn app() -> App {
        let mut a = App::new();
        a.loading = false;
        a
    }

    fn press(a: &mut App, keys: &str) {
        for c in keys.chars() {
            a.on_key(Stroke::k(c).event());
        }
    }

    fn key(a: &mut App, code: KeyCode) {
        a.on_key(Stroke::code(code).event());
    }

    fn two(a: &mut App) -> (ElementId, ElementId) {
        let x = a.doc.add(ApplicationComponent, "CRM", 2.0, 2.0);
        let y = a.doc.add(ApplicationService, "Contacts API", 30.0, 2.0);
        a.set_cursor(x);
        (x, y)
    }

    /// A fresh `App`'s `body` starts at the screen origin, so a world point and a screen
    /// point are the same numbers — these helpers drive `on_mouse` the way a finger would.
    fn mouse(a: &mut App, kind: MouseEventKind, at: (f64, f64)) {
        a.on_mouse(MouseEvent { kind, column: at.0 as u16, row: at.1 as u16, modifiers: KeyModifiers::NONE });
    }

    fn click(a: &mut App, btn: MouseButton, at: (f64, f64)) {
        mouse(a, MouseEventKind::Down(btn), at);
        mouse(a, MouseEventKind::Up(btn), at);
    }

    fn drag(a: &mut App, btn: MouseButton, from: (f64, f64), to: (f64, f64)) {
        mouse(a, MouseEventKind::Down(btn), from);
        mouse(a, MouseEventKind::Drag(btn), to);
        mouse(a, MouseEventKind::Up(btn), to);
    }

    #[test]
    fn the_first_key_takes_the_splash_down_and_asks_new_or_open() {
        let mut a = App::new();
        assert!(a.loading);
        press(&mut a, ":");
        assert!(!a.loading);
        assert!(a.cmdline.is_none(), "the key that dismissed the splash did nothing else");
        assert!(a.start.is_some(), "with no file asked for, the start dialog is up");
        key(&mut a, KeyCode::Esc);
        assert!(a.start.is_none(), "esc goes on with an unnamed diagram");
        assert!(a.path.is_none());
    }

    #[test]
    fn the_start_dialog_offers_a_remembered_workbench_and_opens_it() {
        // Confirming writes back through `config::touch_workbench` — isolate it from the
        // real config file the same way the theme test isolates `XDG_CONFIG_HOME`.
        let config_dir = std::env::temp_dir().join(format!("vim-shapes-start-workbench-config-{}", std::process::id()));
        // SAFETY: this test alone touches the variable, and only reads it back through the
        // config module.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &config_dir) };
        let mut a = App::new();
        press(&mut a, ":");
        assert!(a.start.is_some());
        let root = std::env::temp_dir().join(format!("vim-shapes-start-workbench-{}", std::process::id()));
        a.start.as_mut().unwrap().workbenches = vec![root.clone()];
        key(&mut a, KeyCode::Char('w'));
        assert_eq!(a.start.as_ref().unwrap().choice, start::Choice::Workbench);
        key(&mut a, KeyCode::Enter); // side: left → right
        key(&mut a, KeyCode::Enter); // confirms the one remembered workbench
        assert!(a.start.is_none(), "picking a workbench closes the dialog, like any other choice");
        assert!(a.workbench.is_some(), "and opens it");
        assert_eq!(a.workbench.as_ref().unwrap().root, Some(root.clone()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&config_dir).ok();
    }

    #[test]
    fn the_debug_panel_toggles_and_says_what_the_last_key_did() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut a = app();
        two(&mut a);
        a.run_excmd("debug".into());
        assert!(a.debug);
        press(&mut a, "u");
        assert_eq!(a.last_resolved, "nothing to undo".to_string().replace("nothing", "refused: nothing"));
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains("debug") && out.contains("refused: nothing to undo") && out.contains("CRM"));
        assert!(a.keys.ends_with(&["u".to_string()]));
        a.run_excmd("debug".into());
        assert!(!a.debug);
    }

    #[test]
    fn export_opens_a_dialog_bare_and_writes_by_extension_with_a_path() {
        let mut a = app();
        two(&mut a);
        a.run_excmd("export".into());
        assert!(a.export.is_some());
        press(&mut a, "l");
        assert_eq!(a.export.as_ref().unwrap().options.format, crate::export::Format::Svg);
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-dlg-{}.svg", std::process::id()));
        press(&mut a, "jji");
        for _ in 0..40 {
            key(&mut a, KeyCode::Backspace);
        }
        press(&mut a, &path.display().to_string());
        key(&mut a, KeyCode::Enter);
        key(&mut a, KeyCode::Enter);
        assert!(a.export.is_none());
        assert!(matches!(a.status, Some((ref m, Tone::Good)) if m.starts_with("exported")), "{:?}", a.status);
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("<svg"));
        std::fs::remove_file(&path).ok();
        // A path names its format outright.
        let mut p2 = std::env::temp_dir();
        p2.push(format!("vim-shapes-dlg-{}.html", std::process::id()));
        a.run_excmd(format!("export {}", p2.display()));
        assert!(std::fs::read_to_string(&p2).unwrap().starts_with("<!doctype html>"));
        std::fs::remove_file(&p2).ok();
        a.run_excmd("export nothing.txt".into());
        assert!(matches!(a.status, Some((ref m, Tone::Bad)) if m.contains("which format")));
    }

    #[test]
    fn the_command_line_is_reachable_from_every_mode_and_panel() {
        let mut a = app();
        let (x, y) = two(&mut a);
        a.doc.connect(RelationKind::Realization, x, y).unwrap();
        let line_open = |a: &App| a.cmdline.as_ref().is_some_and(|c| c.prompt == ':');
        // visual
        press(&mut a, "v:");
        assert!(line_open(&a), "from picking");
        key(&mut a, KeyCode::Esc);
        key(&mut a, KeyCode::Esc);
        // reshape
        press(&mut a, "i:");
        assert!(line_open(&a) && a.reshape.is_none(), "from reshaping, which it leaves");
        key(&mut a, KeyCode::Esc);
        // present
        press(&mut a, "\\:");
        assert!(line_open(&a) && !a.present, "from presenting, which it leaves so the line can be seen");
        key(&mut a, KeyCode::Esc);
        // the config panel, the relation picker, the tab picker, the manual
        press(&mut a, "c:");
        assert!(line_open(&a) && a.sheet.as_ref().is_some_and(|s| !s.focused), "from the sheet, which stays up");
        key(&mut a, KeyCode::Esc);
        a.sheet = None;
        press(&mut a, ":");
        key(&mut a, KeyCode::Esc);
        key(&mut a, KeyCode::Tab);
        press(&mut a, "r:");
        assert!(line_open(&a) && a.relpick.is_none(), "from the picker");
        key(&mut a, KeyCode::Esc);
        a.on_key(Stroke::ctrl('t').event());
        press(&mut a, ":");
        assert!(line_open(&a) && a.tabpick.is_none(), "from the tab picker");
        key(&mut a, KeyCode::Esc);
        a.run_excmd("help".into());
        press(&mut a, ":");
        assert!(line_open(&a) && a.manual.is_none(), "from the manual");
        key(&mut a, KeyCode::Esc);
        // the start dialog, when not typing a name
        let mut b = App::new();
        press(&mut b, " ");
        press(&mut b, ":");
        assert!(b.start.is_none() && b.cmdline.is_some(), "from the start dialog");
        // …but not from a label being typed: there the colon is text
        key(&mut a, KeyCode::Esc);
        press(&mut a, "t:");
        assert!(a.cmdline.is_none() && a.insert.as_ref().unwrap().buf.ends_with(':'));
    }

    #[test]
    fn the_start_dialog_makes_a_named_diagram_of_the_kind_picked() {
        let mut a = App::new();
        press(&mut a, " ");
        let dir = std::env::temp_dir();
        a.start.as_mut().unwrap().dir = dir.clone();
        key(&mut a, KeyCode::Tab);
        press(&mut a, "vim-shapes-start-app");
        key(&mut a, KeyCode::Tab);
        a.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        assert!(a.start.is_none());
        assert_eq!(a.path, Some(dir.join("vim-shapes-start-app.json")));
        assert_eq!(a.tab_name(), "vim-shapes-start-app");
        assert_eq!(a.doc.metadata.view, View::Freeform, "the first kind offered");
        assert!(!a.dirty(), "nothing typed yet, nothing to save");
        assert!(!a.path.as_ref().unwrap().exists(), "not written until :w");
    }

    #[test]
    fn hjkl_hops_between_elements_and_a_count_hops_further() {
        let mut a = app();
        let x = a.doc.add(Node, "", 0.0, 0.0);
        let y = a.doc.add(Node, "", 20.0, 0.0);
        let z = a.doc.add(Node, "", 40.0, 0.0);
        a.set_cursor(x);
        press(&mut a, "l");
        assert_eq!(a.cursor, Some(y));
        press(&mut a, "h");
        assert_eq!(a.cursor, Some(x));
        press(&mut a, "2l");
        assert_eq!(a.cursor, Some(z));
        press(&mut a, "h");
        assert_eq!(a.cursor, Some(y));
    }

    #[test]
    fn a_relation_is_drawn_in_two_moves_and_the_picker_offers_the_best_kind_first() {
        let mut a = app();
        let (x, y) = two(&mut a);
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.holding, Some(x));
        press(&mut a, "l");
        assert_eq!(a.cursor, Some(y));
        key(&mut a, KeyCode::Enter);
        assert!(a.relpick.is_some(), "dropping asks which kind");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.doc.relations.len(), 1);
        assert_eq!(a.doc.relations[0].kind, RelationKind::Realization);
        assert_eq!(a.doc.relations[0].from, x);
        assert_eq!(a.doc.relations[0].to, y);
        assert!(a.holding.is_none());
        assert!(a.focused_relation().is_some(), "the cursor lands on the new relation");
        assert!(a.doc.lint().is_empty());
    }

    #[test]
    fn a_refused_kind_is_still_drawn_and_marked() {
        let mut a = app();
        let (x, y) = two(&mut a);
        key(&mut a, KeyCode::Enter);
        press(&mut a, "l");
        key(&mut a, KeyCode::Enter);
        // Walk to the last row: association is always last, and before it come the refused.
        for _ in 0..(RelationKind::ALL.len() - 2) {
            press(&mut a, "j");
        }
        let (kind, verdict) = a.relpick.as_ref().unwrap().picked().unwrap();
        assert!(verdict.is_err(), "{kind:?} should be a refused row");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.doc.relations.len(), 1);
        assert_eq!(a.doc.lint().len(), 1);
        assert!(matches!(a.status, Some((ref m, Tone::Bad)) if m.contains("refuse")));
        let _ = (x, y);
    }

    #[test]
    fn escape_lets_go_of_a_relation_and_nothing_is_drawn() {
        let mut a = app();
        two(&mut a);
        key(&mut a, KeyCode::Enter);
        key(&mut a, KeyCode::Esc);
        assert!(a.holding.is_none());
        assert!(a.doc.relations.is_empty());
    }

    #[test]
    fn tab_walks_relations_and_x_removes_the_focused_one() {
        let mut a = app();
        let (x, y) = two(&mut a);
        a.doc.connect(RelationKind::Realization, x, y).unwrap();
        key(&mut a, KeyCode::Tab);
        assert!(a.focused_relation().is_some());
        press(&mut a, "x");
        assert!(a.doc.relations.is_empty());
        assert_eq!(a.focus, 0);
        press(&mut a, "u");
        assert_eq!(a.doc.relations.len(), 1, "undo brings it back");
    }

    #[test]
    fn gd_follows_a_relation_and_ctrl_o_comes_back() {
        let mut a = app();
        let (x, y) = two(&mut a);
        a.doc.connect(RelationKind::Realization, x, y).unwrap();
        key(&mut a, KeyCode::Tab);
        press(&mut a, "gd");
        assert_eq!(a.cursor, Some(y));
        a.on_key(Stroke::ctrl('o').event());
        assert_eq!(a.cursor, Some(x));
        a.on_key(Stroke::ctrl('i').event());
        assert_eq!(a.cursor, Some(y));
    }

    #[test]
    fn a_refused_key_prints_the_reason_the_menu_shows() {
        let mut a = app();
        press(&mut a, "u");
        assert_eq!(a.status, Some(("nothing to undo".into(), Tone::Bad)));
        two(&mut a);
        press(&mut a, "x");
        assert!(matches!(a.status, Some((ref m, Tone::Bad)) if m.contains("tab walks them")));
    }

    #[test]
    fn add_via_the_command_line_places_beside_the_cursor_and_starts_the_label() {
        let mut a = app();
        let (x, _) = two(&mut a);
        a.run_excmd("add node".into());
        assert_eq!(a.doc.elements.len(), 3);
        assert!(a.insert.is_some(), "the label is being typed");
        let new = a.cursor.unwrap();
        let (e, first) = (a.doc.element(new).unwrap().clone(), a.doc.element(x).unwrap().clone());
        assert!(e.x >= first.right(), "beside, not on top");
        press(&mut a, "db01");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.doc.element(new).unwrap().label, "db01");
        assert!(a.undo.len() >= 2, "adding and labelling are separate undo steps");
    }

    #[test]
    fn the_palette_is_a_search_and_enter_adds_what_it_found() {
        let mut a = app();
        press(&mut a, ":");
        press(&mut a, "add");
        key(&mut a, KeyCode::Enter);
        assert!(a.palette.is_some());
        press(&mut a, "server");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.doc.elements[0].kind, Node, "typing server finds Node");
    }

    #[test]
    fn the_command_line_completes_and_a_bang_skips_the_question() {
        let mut a = app();
        two(&mut a);
        press(&mut a, ":");
        press(&mut a, "lay");
        key(&mut a, KeyCode::Tab);
        assert_eq!(a.cmdline.as_ref().unwrap().buf, "layout");
        key(&mut a, KeyCode::Esc);
        a.run_excmd("q".into());
        assert!(!a.should_quit, "unsaved work: it asks");
        assert!(matches!(a.confirm, Some(Confirm::Discard(Pending::Quit))));
        press(&mut a, "n");
        assert!(!a.should_quit);
        a.run_excmd("q!".into());
        assert!(a.should_quit);
    }

    #[test]
    fn dirtiness_is_a_comparison_so_undo_makes_it_clean_again() {
        let mut a = app();
        assert!(!a.dirty());
        a.run_excmd("add actor".into());
        key(&mut a, KeyCode::Esc);
        assert!(a.dirty());
        press(&mut a, "u");
        assert!(!a.dirty(), "back to the saved state is clean");
    }

    #[test]
    fn save_then_open_round_trips_through_the_command_line() {
        let mut a = app();
        two(&mut a);
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-app-{}.json", std::process::id()));
        a.run_excmd(format!("w {}", path.display()));
        assert!(!a.dirty());
        assert!(matches!(a.status, Some((ref m, Tone::Good)) if m.starts_with("saved")));
        let mut b = app();
        b.run_excmd(format!("o {}", path.display()));
        std::fs::remove_file(&path).ok();
        assert_eq!(b.doc, a.doc);
        assert!(b.cursor.is_some());
    }

    #[test]
    fn import_reads_a_draw_io_file_back_and_asks_first_if_there_is_unsaved_work() {
        let mut a = app();
        two(&mut a);
        a.doc.connect(RelationKind::Realization, a.doc.elements[0].id, a.doc.elements[1].id).unwrap();
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-import-{}.drawio", std::process::id()));
        std::fs::write(&path, crate::drawio_export::to_xml(&a.doc)).unwrap();

        let mut b = app();
        two(&mut b);
        assert!(b.dirty());
        b.run_excmd(format!("import {}", path.display()));
        assert!(matches!(b.confirm, Some(Confirm::Discard(Pending::Import(_)))), "unsaved work is asked about first");
        press(&mut b, "y");
        std::fs::remove_file(&path).ok();

        assert_eq!(b.doc.elements.len(), 2);
        assert_eq!(b.doc.elements[0].kind, ApplicationComponent);
        assert_eq!(b.doc.relations[0].kind, RelationKind::Realization, "the exact kind, from the file's own hint");
        assert!(b.path.is_none(), "a draw.io file is not what :w would save back to");
        assert!(matches!(b.status, Some((ref m, Tone::Good)) if m.starts_with("imported")));
    }

    #[test]
    fn import_also_reads_an_archimate_exchange_file_by_its_content_not_its_extension() {
        const ARCHIMATE: &str = r#"<model xmlns="http://www.opengroup.org/xsd/archimate/3.0/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" identifier="id-m">
  <name>From Archi</name>
  <elements>
    <element identifier="id-1" xsi:type="ApplicationComponent"><name>CRM</name></element>
    <element identifier="id-2" xsi:type="ApplicationService"><name>Contacts</name></element>
  </elements>
  <relationships>
    <relationship identifier="id-3" xsi:type="Realization" source="id-1" target="id-2" />
  </relationships>
</model>"#;
        let mut path = std::env::temp_dir();
        // A ".xml" extension on purpose: the format is told apart by its content, not the name.
        path.push(format!("vim-shapes-archimate-import-{}.xml", std::process::id()));
        std::fs::write(&path, ARCHIMATE).unwrap();
        let mut a = app();
        a.run_excmd(format!("import {}", path.display()));
        std::fs::remove_file(&path).ok();
        assert_eq!(a.tab_name(), "From Archi");
        assert_eq!(a.doc.elements.len(), 2);
        assert_eq!(a.doc.elements[0].kind, ApplicationComponent);
        assert_eq!(a.doc.relations[0].kind, RelationKind::Realization);
    }

    #[test]
    fn the_model_tree_browses_a_coarchi_import_and_enter_jumps_to_a_view() {
        let mut root = std::env::temp_dir();
        root.push(format!("vim-shapes-tree-{}", std::process::id()));
        let model = root.join("model");
        std::fs::create_dir_all(model.join("application")).unwrap();
        std::fs::create_dir_all(model.join("diagrams/sub")).unwrap();
        std::fs::write(model.join("folder.xml"), r#"<archimate:ArchimateModel xmlns:archimate="http://www.archimatetool.com/archimate" name="Fixture" id="m1"/>"#).unwrap();
        std::fs::write(model.join("application/ApplicationComponent_a1.xml"), r#"<archimate:ApplicationComponent xmlns:archimate="http://www.archimatetool.com/archimate" name="CRM" id="a1"/>"#).unwrap();
        std::fs::write(model.join("application/ApplicationService_a2.xml"), r#"<archimate:ApplicationService xmlns:archimate="http://www.archimatetool.com/archimate" name="Contacts" id="a2"/>"#).unwrap();
        std::fs::write(model.join("diagrams/folder.xml"), r#"<archimate:Folder xmlns:archimate="http://www.archimatetool.com/archimate" name="Views" id="fv"/>"#).unwrap();
        std::fs::write(
            model.join("diagrams/ArchimateDiagramModel_v1.xml"),
            r#"<archimate:ArchimateDiagramModel xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:archimate="http://www.archimatetool.com/archimate" name="Billing Overview" id="v1">
  <children xsi:type="archimate:DiagramModelArchimateObject" id="n1">
    <bounds x="10" y="10" width="120" height="55"/>
    <archimateElement xsi:type="archimate:ApplicationComponent" href="ApplicationComponent_a1.xml#a1"/>
  </children>
</archimate:ArchimateDiagramModel>"#,
        )
        .unwrap();
        std::fs::write(model.join("diagrams/sub/folder.xml"), r#"<archimate:Folder xmlns:archimate="http://www.archimatetool.com/archimate" name="Nested" id="fn"/>"#).unwrap();
        std::fs::write(
            model.join("diagrams/sub/ArchimateDiagramModel_v2.xml"),
            r#"<archimate:ArchimateDiagramModel xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:archimate="http://www.archimatetool.com/archimate" name="Detail View" id="v2">
  <children xsi:type="archimate:DiagramModelArchimateObject" id="n2">
    <bounds x="10" y="10" width="120" height="55"/>
    <archimateElement xsi:type="archimate:ApplicationService" href="ApplicationService_a2.xml#a2"/>
  </children>
</archimate:ArchimateDiagramModel>"#,
        )
        .unwrap();

        let mut a = app();
        a.run_excmd(format!("import {}", root.display()));
        std::fs::remove_dir_all(&root).ok();
        assert!(!a.model_tree.is_empty(), "the coArchi import kept its folder tree");

        a.run_excmd("tree".into());
        assert!(a.tree.is_some());
        // Folded: only the top folder shows.
        assert_eq!(tree::rows(&a.model_tree, "").len(), 1);
        key(&mut a, KeyCode::Right);
        // Open: its view and its own nested (still folded) folder show too.
        let opened = tree::rows(&a.model_tree, "");
        assert_eq!(opened.len(), 3);
        assert_eq!(opened[1].label, "Billing Overview");
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Enter);

        assert!(a.tree.is_none(), "picking a view closes the tree");
        assert_eq!(a.tab_name(), "Billing Overview");
        assert_eq!(a.doc.elements[0].label, "CRM");
    }

    /// A small fixture whose one link type gives an object type something to `e`xpand.
    fn foundry_fixture_json(padding: &str) -> String {
        format!(
            r#"{{
                "version": 2,
                "typeGroups": [{{"rid": "g.crm", "displayMetadata": {{"displayName": "CRM"}}}}],
                "objectTypes": [
                    {{"id": "ot.customer", "apiName": "Customer", "displayMetadata": {{"displayName": "Customer", "description": "{padding}"}},
                     "status": {{"type": "active"}}, "typeGroups": ["g.crm"], "interfaces": [],
                     "primaryKeys": [], "titlePropertyId": null, "properties": [], "datasources": []}},
                    {{"id": "ot.order", "apiName": "Order", "displayMetadata": {{"displayName": "Order"}},
                     "status": {{"type": "active"}}, "typeGroups": ["g.crm"], "interfaces": [],
                     "primaryKeys": [], "titlePropertyId": null, "properties": [], "datasources": []}}
                ],
                "interfaces": [], "actionTypes": [], "sharedProperties": [],
                "relations": [{{"description": null, "definition": {{"type": "oneToMany", "oneToMany": {{
                    "objectTypeIdOneSide": "ot.customer", "objectTypeIdManySide": "ot.order",
                    "oneToManyLinkMetadata": {{"apiName": "orders"}}, "manyToOneLinkMetadata": {{"apiName": "customer"}}
                }}}}}}]
            }}"#
        )
    }

    #[test]
    fn import_leaves_the_diagram_empty_and_the_tree_lists_resources_by_group() {
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-ontology-{}.json", std::process::id()));
        // A real export's `actionTypes` key comes after the entire, often many-megabyte,
        // `objectTypes` array — nowhere near a cheap prefix read. The padding below pushes
        // it (and `interfaces`/`relations`) well past 4 KB, so this test would have caught
        // the bug where the content sniff required `actionTypes` inside that prefix too and
        // silently fell through to the ArchiMate reader on every real file.
        std::fs::write(&path, foundry_fixture_json(&"x".repeat(8192))).unwrap();

        let mut a = app();
        a.run_excmd(format!("import {}", path.display()));
        std::fs::remove_file(&path).ok();
        assert!(a.ontology.is_some(), "the resident index is kept, not drawn");
        assert!(!a.model_tree.is_empty(), "the tree lists resources by group");
        assert_eq!(a.doc.elements.len(), 0, "nothing is placed until a resource is picked — no more tangle");
        assert_eq!(a.tab_name(), "diagram 1");
        assert!(a.tree.is_some(), "the tree opens on its own — nothing else to do yet but pick a resource");

        let rows = tree::rows(&a.model_tree, "");
        // CRM, opened, holds Customer and Order as individually pickable resources.
        assert!(matches!(&rows[0].kind, tree::RowKind::Folder { .. }));
        assert_eq!(rows[0].label, "CRM");
        key(&mut a, KeyCode::Right);
        let opened = tree::rows(&a.model_tree, "");
        let customer = opened.iter().find(|r| r.label == "Customer").expect("Customer is a resource of its own");
        assert!(matches!(&customer.kind, tree::RowKind::Resource { .. }));
    }

    #[test]
    fn picking_a_resource_starts_a_fresh_tab_and_e_expands_its_real_connections() {
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-ontology-expand-{}.json", std::process::id()));
        std::fs::write(&path, foundry_fixture_json("")).unwrap();
        let mut a = app();
        a.run_excmd(format!("import {}", path.display()));
        std::fs::remove_file(&path).ok();
        assert!(a.tree.is_some(), "the tree opens on its own after a Foundry import");

        key(&mut a, KeyCode::Right); // open the CRM folder
        let rows = tree::rows(&a.model_tree, "");
        let i = rows.iter().position(|r| r.label == "Customer").unwrap();
        a.tree.as_mut().unwrap().sel = i;
        key(&mut a, KeyCode::Enter);

        assert!(a.tree.is_none(), "picking a resource closes the tree");
        assert_eq!(a.tab_name(), "Customer", "a fresh tab, named after it");
        assert_eq!(a.doc.elements.len(), 1, "just the one resource — not the group it came from");
        assert_eq!(a.doc.metadata.view, View::Ontology);

        key(&mut a, KeyCode::Char('e'));
        assert!(a.expandpick.is_some(), "Customer's link to Order is a real, unexpanded connection");
        key(&mut a, KeyCode::Enter);
        assert!(a.expandpick.is_none(), "adding one closes the list");
        assert_eq!(a.doc.elements.len(), 2, "Order is now on the diagram too");
        assert!(a.doc.elements.iter().any(|e| e.display() == "Order"));
        assert!(a.doc.relations.iter().any(|r| r.kind == RelationKind::LinkType));

        // Order's own edge back to Customer is already on the diagram, so there is nothing
        // left for either to expand.
        let order_id = a.doc.elements.iter().find(|e| e.display() == "Order").unwrap().id;
        a.cursor = Some(order_id);
        key(&mut a, KeyCode::Char('e'));
        assert!(a.expandpick.is_none(), "nothing left to expand — the key is refused, not opened empty");
    }

    #[test]
    fn the_workbench_always_docks_left_and_opening_the_same_diagram_twice_switches_instead_of_duplicating() {
        let root = std::env::temp_dir().join(format!("vim-shapes-workbench-key-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        let mut a = app();
        a.run_excmd(format!("workbench {}", root.display()));
        assert!(a.workbench.is_some());

        // A new diagram, named after what was typed — the same name a second Enter on the
        // row has to find again.
        key(&mut a, KeyCode::Char('n'));
        press(&mut a, "Checkout Flow");
        key(&mut a, KeyCode::Enter);
        assert!(root.join("Checkout Flow.json").exists());

        let row_i = a.workbench.as_ref().unwrap().rows().iter().position(|r| r.name == "Checkout Flow.json").unwrap();
        a.workbench.as_mut().unwrap().sel = row_i;
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.tabs.len(), 2, "the fresh diagram opened as a new tab");
        assert_eq!(a.tab_name(), "Checkout Flow");
        assert!(a.workbench.is_some(), "the panel stays open — browsing continues");

        // Back to the workbench, and Enter on the same row again.
        let row_i = a.workbench.as_ref().unwrap().rows().iter().position(|r| r.name == "Checkout Flow.json").unwrap();
        a.workbench.as_mut().unwrap().sel = row_i;
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.tabs.len(), 2, "still two — the second Enter switched to the tab already open, not a third");
        assert_eq!(a.tab_name(), "Checkout Flow");

        // The panel always reserves its own column — never a floating dialog — regardless
        // of terminal width, unlike the sheet's own right-hand dock.
        use ratatui::{backend::TestBackend, Terminal};
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        assert_eq!(a.body.x, workbench::WIDTH, "the diagram body starts after the workbench's own column");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_tree_nests_an_object_type_s_own_actions_and_links_under_it_and_picking_a_link_places_both_ends() {
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-ontology-nest-{}.json", std::process::id()));
        std::fs::write(&path, foundry_fixture_json("")).unwrap();
        let mut a = app();
        a.run_excmd(format!("import {}", path.display()));
        std::fs::remove_file(&path).ok();

        key(&mut a, KeyCode::Right); // open CRM
        let rows = tree::rows(&a.model_tree, "");
        let customer_i = rows.iter().position(|r| r.label == "Customer").unwrap();
        assert!(matches!(&rows[customer_i].kind, tree::RowKind::Resource { expandable: true, .. }), "Customer has its own link to expand");
        a.tree.as_mut().unwrap().sel = customer_i;
        key(&mut a, KeyCode::Right); // reveal Customer's own children, without picking it
        assert!(a.tree.is_some(), "→ only expands — it never picks");
        assert_eq!(a.doc.elements.len(), 0);

        let opened = tree::rows(&a.model_tree, "");
        let link_row = opened.iter().find(|r| r.label == "→ Order").expect("Customer's link to Order, nested under it");
        assert!(matches!(&link_row.kind, tree::RowKind::Link { .. }));
        a.tree.as_mut().unwrap().sel = opened.iter().position(|r| r.label == "→ Order").unwrap();
        key(&mut a, KeyCode::Enter);

        assert!(a.tree.is_none(), "picking the link closes the tree");
        assert_eq!(a.tab_name(), "Customer", "started from the link's own object type");
        assert_eq!(a.doc.elements.len(), 2, "both ends, in one move");
        assert!(a.doc.elements.iter().any(|e| e.display() == "Order"));
        assert!(a.doc.relations.iter().any(|r| r.kind == RelationKind::LinkType), "and the edge between them");
    }

    #[test]
    fn a_link_with_a_long_description_is_placed_below_not_beside() {
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-ontology-vertical-{}.json", std::process::id()));
        std::fs::write(
            &path,
            r#"{
                "version": 2,
                "typeGroups": [{"rid": "g.assets", "displayMetadata": {"displayName": "Assets"}}],
                "objectTypes": [
                    {"id": "ot.asset", "apiName": "Asset", "displayMetadata": {"displayName": "Asset"},
                     "status": {"type": "active"}, "typeGroups": ["g.assets"], "interfaces": [],
                     "primaryKeys": [], "titlePropertyId": null, "properties": [], "datasources": []},
                    {"id": "ot.location", "apiName": "AssetLocation", "displayMetadata": {"displayName": "Asset Location"},
                     "status": {"type": "active"}, "typeGroups": ["g.assets"], "interfaces": [],
                     "primaryKeys": [], "titlePropertyId": null, "properties": [], "datasources": []}
                ],
                "interfaces": [], "actionTypes": [], "sharedProperties": [],
                "relations": [{"description": "Connects an operational location to the physical assets currently placed there.",
                    "definition": {"type": "oneToMany", "oneToMany": {
                    "objectTypeIdOneSide": "ot.location", "objectTypeIdManySide": "ot.asset",
                    "oneToManyLinkMetadata": {"apiName": "locatedAssets"}, "manyToOneLinkMetadata": {"apiName": "currentLocation"}
                }}}]
            }"#,
        )
        .unwrap();

        let mut a = app();
        a.run_excmd(format!("import {}", path.display()));
        std::fs::remove_file(&path).ok();

        key(&mut a, KeyCode::Right); // open Assets
        let rows = tree::rows(&a.model_tree, "");
        let i = rows.iter().position(|r| r.label == "Asset").unwrap();
        a.tree.as_mut().unwrap().sel = i;
        key(&mut a, KeyCode::Enter); // pick Asset alone

        let asset = a.cursor.unwrap();
        let (ax, ay) = (a.doc.element(asset).unwrap().x, a.doc.element(asset).unwrap().y);
        key(&mut a, KeyCode::Char('e'));
        key(&mut a, KeyCode::Enter); // the one connection: its long-described link to Asset Location

        let location = a.doc.elements.iter().find(|e| e.display() == "Asset Location").unwrap();
        assert_eq!(location.x, ax, "straight below, not off to the side, so the description has room");
        assert!(location.y > ay, "and under it, not on top of it");
    }

    #[test]
    fn e_on_a_foreign_key_row_in_props_expands_its_link_and_focuses_the_far_end() {
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-ontology-fk-{}.json", std::process::id()));
        std::fs::write(
            &path,
            r#"{
                "version": 2,
                "typeGroups": [{"rid": "g.crm", "displayMetadata": {"displayName": "CRM"}}],
                "objectTypes": [
                    {"id": "ot.customer", "apiName": "Customer", "displayMetadata": {"displayName": "Customer"},
                     "status": {"type": "active"}, "typeGroups": ["g.crm"], "interfaces": [],
                     "primaryKeys": [], "titlePropertyId": null, "properties": [], "datasources": []},
                    {"id": "ot.order", "apiName": "Order", "displayMetadata": {"displayName": "Order"},
                     "status": {"type": "active"}, "typeGroups": ["g.crm"], "interfaces": [],
                     "primaryKeys": ["p.oid"], "titlePropertyId": null,
                     "properties": [
                        {"id": "p.oid", "apiName": "id", "displayMetadata": {"displayName": "Id", "visibility": "NORMAL"}, "status": {"type": "active"}, "baseType": {"type": "STRING"}},
                        {"id": "p.custfk", "apiName": "customerId", "displayMetadata": {"displayName": "Customer Id", "visibility": "NORMAL"}, "status": {"type": "active"}, "baseType": {"type": "STRING"}}
                     ],
                     "datasources": []}
                ],
                "interfaces": [], "actionTypes": [], "sharedProperties": [],
                "relations": [{"description": null, "definition": {"type": "oneToMany", "oneToMany": {
                    "objectTypeIdOneSide": "ot.customer", "objectTypeIdManySide": "ot.order",
                    "oneToManyLinkMetadata": {"apiName": "orders"}, "manyToOneLinkMetadata": {"apiName": "customer"},
                    "manySideForeignKeyPropertyId": "p.custfk"
                }}}]
            }"#,
        )
        .unwrap();

        let mut a = app();
        a.run_excmd(format!("import {}", path.display()));
        std::fs::remove_file(&path).ok();

        key(&mut a, KeyCode::Right); // open CRM
        let rows = tree::rows(&a.model_tree, "");
        let i = rows.iter().position(|r| r.label == "Order").unwrap();
        a.tree.as_mut().unwrap().sel = i;
        key(&mut a, KeyCode::Enter); // Order, alone
        assert_eq!(a.doc.elements.len(), 1);

        press(&mut a, "P");
        assert!(a.props.is_some());
        // id, then customerId — the foreign key.
        key(&mut a, KeyCode::Char('j'));
        assert!(a.props_fk_edge().is_some(), "customerId backs the link to Customer");

        key(&mut a, KeyCode::Char('e'));
        assert!(a.props.is_none(), "expanding closes the property browser");
        assert_eq!(a.doc.elements.len(), 2, "Customer is now on the diagram too");
        let customer = a.doc.elements.iter().find(|e| e.display() == "Customer").unwrap();
        assert_eq!(a.cursor, Some(customer.id), "focused on the element it just expanded to");
        assert!(a.doc.relations.iter().any(|r| r.kind == RelationKind::LinkType));
    }

    #[test]
    fn e_on_an_ordinary_property_row_in_props_says_so_and_changes_nothing() {
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-ontology-fk-plain-{}.json", std::process::id()));
        std::fs::write(&path, foundry_fixture_json("")).unwrap();
        let mut a = app();
        a.run_excmd(format!("import {}", path.display()));
        std::fs::remove_file(&path).ok();

        key(&mut a, KeyCode::Right);
        let rows = tree::rows(&a.model_tree, "");
        let i = rows.iter().position(|r| r.label == "Customer").unwrap();
        a.tree.as_mut().unwrap().sel = i;
        key(&mut a, KeyCode::Enter);

        press(&mut a, "P");
        assert!(a.props.is_some());
        key(&mut a, KeyCode::Char('e'));
        assert!(a.props.is_some(), "not a foreign key — the panel stays open");
        assert_eq!(a.doc.elements.len(), 1, "nothing was added");
    }

    #[test]
    fn import_with_no_path_opens_a_browser_that_walks_and_imports_a_file() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("vim-shapes-importdlg-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.json"), foundry_fixture_json("")).unwrap();

        let mut a = app();
        a.run_excmd("import".into());
        assert!(a.importdlg.is_some(), "no path opens the browser instead of an error");
        a.importdlg.as_mut().unwrap().dir = dir.clone();
        a.importdlg.as_mut().unwrap().read_dir();
        key(&mut a, KeyCode::Down); // "sub"
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.importdlg.as_ref().unwrap().dir, dir.join("sub"), "enter walks into a folder");
        key(&mut a, KeyCode::Char('h'));
        assert_eq!(a.importdlg.as_ref().unwrap().dir, dir, "h comes back up");
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Enter);
        std::fs::remove_dir_all(&dir).ok();

        assert!(a.importdlg.is_none(), "picking a file closes the dialog");
        assert!(a.ontology.is_some());
        assert_eq!(a.doc.elements.len(), 0, "empty until a resource is picked from :tree, same as :import <path>");
    }

    #[test]
    fn ctrl_enter_in_the_import_browser_imports_the_folder_it_is_standing_in() {
        let mut root = std::env::temp_dir();
        root.push(format!("vim-shapes-importdlg-folder-{}", std::process::id()));
        let model = root.join("model");
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("folder.xml"), r#"<archimate:ArchimateModel xmlns:archimate="http://www.archimatetool.com/archimate" name="Fixture" id="m1"/>"#).unwrap();
        std::fs::write(model.join("ApplicationComponent_a1.xml"), r#"<archimate:ApplicationComponent xmlns:archimate="http://www.archimatetool.com/archimate" name="CRM" id="a1"/>"#).unwrap();

        let mut a = app();
        a.run_excmd("import".into());
        a.importdlg.as_mut().unwrap().dir = model.clone();
        a.importdlg.as_mut().unwrap().read_dir();
        a.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        std::fs::remove_dir_all(&root).ok();

        assert!(a.importdlg.is_none(), "^enter imports the folder itself and closes the dialog");
        assert_eq!(a.doc.elements[0].label, "CRM");
    }

    #[test]
    fn import_asks_first_when_there_is_unsaved_work() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("vim-shapes-importdlg-dirty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.json"), r#"{"version": 2, "tabs": [{"name": "x", "diagram": {"version": 2}}]}"#).unwrap();

        let mut a = app();
        a.doc.add(Box, "dirty", 0.0, 0.0);
        a.run_excmd("import".into());
        a.importdlg.as_mut().unwrap().dir = dir.clone();
        a.importdlg.as_mut().unwrap().read_dir();
        key(&mut a, KeyCode::Down); // "a.json" — ".." is first
        key(&mut a, KeyCode::Enter);
        std::fs::remove_dir_all(&dir).ok();

        assert!(matches!(a.confirm, Some(Confirm::Discard(Pending::Import(_)))), "unsaved work is asked about first, same as :import <path>");
    }

    #[test]
    fn esc_cancels_the_import_browser_with_nothing_changed() {
        let mut a = app();
        a.run_excmd("import".into());
        assert!(a.importdlg.is_some());
        key(&mut a, KeyCode::Esc);
        assert!(a.importdlg.is_none());
    }

    #[test]
    fn delete_asks_on_the_footer_and_y_answers() {
        let mut a = app();
        let (x, y) = two(&mut a);
        a.doc.connect(RelationKind::Realization, x, y).unwrap();
        press(&mut a, "d");
        assert!(matches!(a.confirm, Some(Confirm::DeleteElement(id)) if id == x));
        press(&mut a, "y");
        assert_eq!(a.doc.elements.len(), 1);
        assert!(a.doc.relations.is_empty());
        assert_eq!(a.cursor, Some(y), "the cursor lands somewhere real");
    }

    #[test]
    fn picking_moves_copies_and_deletes_together() {
        let mut a = app();
        let (x, y) = two(&mut a);
        press(&mut a, "vl ");
        assert_eq!(a.selection, vec![x, y]);
        press(&mut a, "L");
        assert_eq!(a.doc.element(x).unwrap().x, 3.0);
        assert_eq!(a.doc.element(y).unwrap().x, 31.0);
        press(&mut a, "y");
        assert!(!a.visual);
        press(&mut a, "p");
        assert_eq!(a.doc.elements.len(), 4);
        press(&mut a, "vd");
        press(&mut a, "y");
        assert_eq!(a.doc.elements.len(), 3);
    }

    #[test]
    fn shift_ctrl_hjkl_moves_the_element_a_bigger_step() {
        let mut a = app();
        let (x, _) = two(&mut a);
        let before = a.doc.element(x).unwrap().clone();
        a.on_key(Stroke::ctrl('L').event());
        a.on_key(Stroke::ctrl('J').event());
        let e = a.doc.element(x).unwrap();
        assert_eq!((e.x, e.y), (before.x + BIG_STEP, before.y + BIG_STEP));
        press(&mut a, "2");
        a.on_key(Stroke::ctrl('H').event());
        assert_eq!(a.doc.element(x).unwrap().x, before.x + BIG_STEP - 2.0 * BIG_STEP, "a count multiplies the fours");
    }

    #[test]
    fn a_grouping_moves_with_what_is_inside_it() {
        let mut a = app();
        let g = a.doc.add(Grouping, "CRM", 0.0, 0.0);
        let inside = a.doc.add(ApplicationComponent, "", 2.0, 2.0);
        a.set_cursor(g);
        press(&mut a, "J");
        assert_eq!(a.doc.element(inside).unwrap().y, 3.0);
        assert_eq!(a.doc.element(g).unwrap().y, 1.0);
    }

    #[test]
    fn f_jumps_by_letter_and_search_finds_a_label() {
        let mut a = app();
        let (x, y) = two(&mut a);
        press(&mut a, "fb");
        assert_eq!(a.cursor, Some(y), "b is the second element in reading order");
        press(&mut a, "/");
        press(&mut a, "crm");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.cursor, Some(x));
    }

    #[test]
    fn lint_jumps_to_the_first_refused_relation() {
        let mut a = app();
        let p = a.doc.add(BusinessProcess, "Pay", 0.0, 0.0);
        let n = a.doc.add(Node, "db", 40.0, 0.0);
        let bad = a.doc.connect(RelationKind::Composition, p, n).unwrap();
        a.set_cursor(n);
        a.run_excmd("lint".into());
        assert_eq!(a.cursor, Some(p));
        assert_eq!(a.focused_relation(), Some(bad));
        assert!(matches!(a.status, Some((ref m, Tone::Bad)) if m.starts_with("1 refused")));
    }

    #[test]
    fn layout_and_idiom_are_one_undo_step_each() {
        let mut a = app();
        a.run_excmd("idiom service".into());
        assert_eq!(a.doc.elements.len(), 3);
        assert_eq!(a.undo.len(), 1);
        a.run_excmd("layout flow".into());
        assert_eq!(a.undo.len(), 2);
        press(&mut a, "uu");
        assert!(a.doc.elements.is_empty());
    }

    #[test]
    fn the_menu_runs_a_command_by_replaying_its_keys() {
        let mut a = app();
        two(&mut a);
        press(&mut a, "?");
        assert!(a.help.is_some());
        press(&mut a, "v");
        assert!(a.help.is_none());
        assert!(a.visual, "v from the menu did what v does");
    }

    #[test]
    fn the_manual_opens_at_a_topic_and_q_closes_it() {
        let mut a = app();
        a.run_excmd("help component".into());
        assert_eq!(a.manual.as_ref().unwrap().page_name(), "component");
        press(&mut a, "q");
        assert!(a.manual.is_none());
        a.run_excmd("help nonsense-topic".into());
        assert!(a.manual.is_none());
    }

    #[test]
    fn o_opens_a_related_shape_off_the_cursor_on_an_architecture_tab() {
        let mut a = app();
        let (x, _) = two(&mut a);
        press(&mut a, "o");
        assert!(matches!(a.palette.as_ref().map(|p| p.purpose), Some(palette::Purpose::Relate(ShapeKind::ApplicationComponent))));
        press(&mut a, "app-service");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.doc.elements.len(), 3);
        assert!(a.relpick.is_some(), "an architecture tab asks the kind of relation");
        key(&mut a, KeyCode::Enter);
        let new = a.doc.elements[2].id;
        let r = a.doc.relations.iter().find(|r| r.from == x && r.to == new).expect("related from the cursor's shape");
        assert_eq!(r.kind, RelationKind::Realization);
        assert_eq!(a.cursor, Some(new));
        assert!(a.insert.is_some(), "and the label is the next thing typed");
        press(&mut a, "Contacts v2");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.doc.element(new).unwrap().label, "Contacts v2");
    }

    #[test]
    fn a_suggested_line_off_an_object_type_is_drawn_without_the_picker() {
        let mut a = app();
        a.run_excmd("kind ontology".into());
        a.run_excmd("add object_type".into());
        press(&mut a, "Truck");
        key(&mut a, KeyCode::Esc);
        let truck = a.cursor.unwrap();
        press(&mut a, "o");
        let p = a.palette.as_ref().unwrap();
        assert_eq!(p.via(), Some(RelationKind::Realization), "the palette opens on the first suggested line");
        press(&mut a, "implements");
        key(&mut a, KeyCode::Enter);
        assert!(a.relpick.is_none(), "the line was the answer: no picker");
        let r = a.doc.relations.iter().find(|r| r.from == truck).expect("drawn from the object type");
        assert_eq!((r.kind, a.doc.element(r.to).unwrap().kind), (RelationKind::Implements, ShapeKind::Interface));
        assert!(a.insert.is_some(), "and the new interface's label is next");
        press(&mut a, "Vehicle");
        key(&mut a, KeyCode::Esc);
        assert!(a.doc.lint().is_empty());
    }

    #[test]
    fn o_on_a_freeform_tab_draws_a_plain_link_and_asks_nothing() {
        let mut a = app();
        a.run_excmd("tabnew freeform sketch".into());
        a.run_excmd("add box".into());
        press(&mut a, "start");
        key(&mut a, KeyCode::Esc);
        let first = a.cursor.unwrap();
        press(&mut a, "o");
        press(&mut a, "circle");
        key(&mut a, KeyCode::Enter);
        assert!(a.relpick.is_none(), "no kinds of relation on a freeform tab");
        assert!(a.insert.is_some());
        press(&mut a, "end");
        key(&mut a, KeyCode::Esc);
        let r = &a.doc.relations[0];
        assert_eq!((r.kind, r.from), (RelationKind::Link, first));
        assert!(a.doc.lint().is_empty());
        // The link's look is configured on the sheet: tab onto it, c, the style tab, cycle.
        key(&mut a, KeyCode::Tab);
        press(&mut a, "c");
        let sh = a.sheet.as_ref().unwrap();
        assert_eq!((sh.tab, sh.field().unwrap().name), (form::Tab::Text, "label"), "opened from the centre node, on its label");
        key(&mut a, KeyCode::BackTab);
        press(&mut a, "jjjjjj");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "head");
        press(&mut a, "l");
        assert_eq!(a.doc.relations[0].notation().head, End::Open, "arrow → open-arrow");
        press(&mut a, "kkk");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "line");
        press(&mut a, "l");
        assert_eq!(a.doc.relations[0].notation().line, LineStyle::Dashed);
    }

    #[test]
    fn a_link_has_three_nodes_each_with_its_own_label() {
        let mut a = app();
        let (x, y) = two(&mut a);
        let r = a.doc.connect(RelationKind::Association, x, y).unwrap();
        // The walk lands on the relation between the two shapes, then tab walks its nodes.
        press(&mut a, "l");
        assert_eq!(a.focused_relation(), Some(r), "l from the left shape reaches the link before the right shape");
        assert_eq!(a.node, LinkNode::Centre);
        key(&mut a, KeyCode::BackTab);
        assert_eq!(a.node, LinkNode::Tail);
        press(&mut a, "t");
        press(&mut a, "1");
        key(&mut a, KeyCode::Esc);
        key(&mut a, KeyCode::Tab);
        key(&mut a, KeyCode::Tab);
        assert_eq!(a.node, LinkNode::Head);
        press(&mut a, "t");
        press(&mut a, "n");
        key(&mut a, KeyCode::Esc);
        key(&mut a, KeyCode::Tab);
        key(&mut a, KeyCode::Tab);
        assert_eq!(a.node, LinkNode::Centre, "tab wraps round the three nodes");
        press(&mut a, "t");
        press(&mut a, "has");
        key(&mut a, KeyCode::Esc);
        let rel = a.doc.relation(r).unwrap();
        assert_eq!((rel.tail_label.as_deref(), rel.label.as_deref(), rel.head_label.as_deref()), (Some("1"), Some("has"), Some("n")));
        // Walking on: l again reaches the right shape; h from there is the link again.
        press(&mut a, "l");
        assert_eq!((a.cursor, a.focused_relation()), (Some(y), None));
        press(&mut a, "h");
        assert_eq!(a.focused_relation(), Some(r));
        // gd from an end goes to that end.
        key(&mut a, KeyCode::Tab);
        press(&mut a, "gd");
        assert_eq!(a.cursor, Some(y));
        press(&mut a, "gT");
        assert_eq!(a.cursor, Some(y), "gT with one tab is refused and does nothing");
    }

    #[test]
    fn diagram_pins_the_sheet_on_the_diagram_and_enter_on_a_colour_opens_the_picker() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut a = app();
        let (x, _) = two(&mut a);
        a.run_excmd("diagram".into());
        let sh = a.sheet.as_ref().unwrap();
        assert_eq!((sh.target, sh.pinned, sh.focused), (Some(Target::Diagram), true, true));
        assert_eq!(sh.field().unwrap().name, "rounded");
        press(&mut a, " ");
        assert!(a.doc.metadata.page.rounded, "space flips a yes/no on the diagram");
        // Down to the background and enter: the picker, over the sheet.
        press(&mut a, "jjj");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "background");
        key(&mut a, KeyCode::Enter);
        assert!(a.colour.is_some(), "the picker is up");
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains("colour · background") && out.contains("swatches") && out.contains("palette"), "{out}");
        assert_eq!(a.sheet.as_ref().unwrap().target, Some(Target::Diagram), "pinned: drawing did not retarget it to the cursor's shape");
        // Pick a swatch: written through the form, remembered as recent, the sheet refreshed.
        press(&mut a, "jjl");
        key(&mut a, KeyCode::Enter);
        assert!(a.colour.is_none());
        let bg = a.doc.metadata.page.background.expect("a background was set");
        assert!(matches!(bg, Colour::Hex(_)));
        assert_eq!(a.recent_colours, vec![bg]);
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().value, bg.hex());
        // A colour picked on a shape goes the same way, and the recent row offers it.
        key(&mut a, KeyCode::Esc);
        press(&mut a, "u");
        assert_eq!(a.doc.metadata.page.background, None, "one undo step");
        term.draw(|f| a.draw(f)).unwrap();
        assert!(!a.sheet.as_ref().unwrap().pinned, "esc lets the sheet follow the cursor again");
        assert_eq!(a.sheet.as_ref().unwrap().target, Some(Target::Element(x)));
        press(&mut a, "c");
        press(&mut a, "jjj");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "colour");
        key(&mut a, KeyCode::Enter);
        let pk = a.colour.as_ref().unwrap();
        assert_eq!(pk.rows[1].title, "recent");
        press(&mut a, "x");
        assert!(a.colour.is_none() && a.doc.element(x).unwrap().color.is_none(), "x picks none");
    }

    #[test]
    fn shift_ctrl_moves_the_shape_whatever_the_terminal_and_its_relations_follow() {
        // The kitty protocol: shift+ctrl+j arrives as `j` with both modifiers; ctrl+j alone
        // is the linked shape.
        let mut a = app();
        a.enhanced_keys = true;
        let (x, y) = two(&mut a);
        let r = a.doc.connect(RelationKind::Realization, x, y).unwrap();
        a.doc.relation_mut(r).unwrap().from_port = Some(5);
        let before = a.doc.element(x).unwrap().clone();
        let ends = a.doc.end_points(a.doc.relation(r).unwrap()).unwrap();
        a.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
        assert!(a.palette.is_none(), "not the add dialog");
        assert_eq!(a.doc.element(x).unwrap().y, before.y + BIG_STEP, "moved four down");
        let after = a.doc.end_points(a.doc.relation(r).unwrap()).unwrap();
        assert_eq!(after.0, (ends.0.0, ends.0.1 + BIG_STEP), "the relation's end at the port moved with it");
        assert_eq!(after.1.0, ends.1.0, "the far end, unported, still sits on the other shape's edge");
        a.on_key(Stroke::ctrl('j').event());
        assert!(a.palette.is_some(), "ctrl alone: the linked shape");
        key(&mut a, KeyCode::Esc);
        // A plain terminal: ctrl+j is all that arrives, and ctrl+h comes as Backspace. Both
        // move; the linked shape keeps its corners.
        let mut a = app();
        let (x, _) = two(&mut a);
        let before = a.doc.element(x).unwrap().clone();
        a.on_key(Stroke::ctrl('j').event());
        assert!(a.palette.is_none(), "no dialog on a plain terminal");
        assert_eq!(a.doc.element(x).unwrap().y, before.y + BIG_STEP);
        key(&mut a, KeyCode::Backspace);
        assert_eq!(a.doc.element(x).unwrap().x, before.x - BIG_STEP, "backspace is ctrl+h there");
        a.on_key(Stroke::ctrl('u').event());
        assert!(a.palette.is_some(), "a corner still opens a linked shape");
        key(&mut a, KeyCode::Esc);
        // Inside the shape, ctrl+hjkl keep their reshape meaning on a plain terminal.
        press(&mut a, "i");
        a.on_key(Stroke::ctrl('j').event());
        assert!(a.palette.is_some(), "inside: a linked shape on that side");
        key(&mut a, KeyCode::Esc);
        key(&mut a, KeyCode::Esc);
        let w = a.whereami();
        assert!(w.plain_keys);
        assert_eq!(keymap::resolve(&Stroke::ctrl('H').event(), None, &w), keymap::Resolved::Allow, "the table sees the move");
        let linked = keymap::COMMANDS.iter().find(|c| c.keys == "^h ^j ^k ^l" && c.section == keymap::Section::Relate).unwrap();
        assert!(matches!((linked.avail)(&w), keymap::Avail::No(why) if why.contains("shift+ctrl as ctrl")), "the menu says why the side chords are not the linked shape here");
    }

    #[test]
    fn a_adds_a_shape_on_its_own_from_anywhere_on_the_diagram() {
        let mut a = app();
        press(&mut a, "a");
        assert!(a.palette.is_some(), "on an empty diagram too");
        assert_eq!(a.palette.as_ref().unwrap().dir, None, "unconnected: no direction");
        press(&mut a, "data-object");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.doc.elements.len(), 1);
        assert!(a.doc.relations.is_empty());
        key(&mut a, KeyCode::Esc);
        press(&mut a, "a");
        assert!(a.palette.is_some(), "and with the cursor on a shape");
    }

    #[test]
    fn the_picker_on_a_fill_calls_blank_auto_and_a_look_dresses_the_shape() {
        let mut a = app();
        let (x, _) = two(&mut a);
        press(&mut a, "c");
        press(&mut a, "j");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "fill");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.colour.as_ref().unwrap().blank, "auto — the layer's own");
        press(&mut a, "jjl");
        key(&mut a, KeyCode::Enter);
        assert!(matches!(a.doc.element(x).unwrap().fill, crate::model::Fill::Colour(_)));
        key(&mut a, KeyCode::Enter);
        press(&mut a, "x");
        assert_eq!(a.doc.element(x).unwrap().fill, crate::model::Fill::Auto, "x on a fill is auto");
        press(&mut a, "k");
        press(&mut a, "l");
        assert_eq!(a.doc.element(x).unwrap().look(), "paper", "l on the look cycles the eight");
        press(&mut a, "lll");
        assert_eq!(a.doc.element(x).unwrap().look(), "green");
        use ratatui::{backend::TestBackend, Terminal};
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        let buf = term.backend().buffer();
        let e = a.doc.element(x).unwrap().clone();
        // The inside of a filled shape is tinted: a cell inside it is not the ground.
        let inside = &buf[((e.x + 2.0) as u16, (e.y + 2.0) as u16 + 1)];
        assert!(matches!(inside.bg, Color::Rgb(..)), "tinted inside: {:?}", inside.bg);
    }

    #[test]
    fn the_sheet_shows_the_diagram_when_the_cursor_is_on_nothing() {
        let mut a = app();
        press(&mut a, "c");
        assert_eq!(a.sheet.as_ref().unwrap().target, Some(Target::Diagram));
        press(&mut a, "t");
        press(&mut a, "Shop");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.doc.metadata.title.as_deref(), Some("Shop"), "t on the diagram is its title");
    }

    #[test]
    fn presenting_shows_the_diagram_alone_and_esc_comes_back() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut term = Terminal::new(TestBackend::new(80, 20)).unwrap();
        let mut a = app();
        two(&mut a);
        press(&mut a, "\\");
        assert!(a.present);
        term.draw(|f| a.draw(f)).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains("CRM") && !out.contains("vim-shapes") && !out.contains("·"), "no chrome, no grid");
        press(&mut a, "l");
        assert!(a.present, "editing keys are refused while presenting");
        key(&mut a, KeyCode::Esc);
        assert!(!a.present);
    }

    #[test]
    fn render_writes_a_png_when_a_font_is_on_this_machine() {
        let mut a = app();
        two(&mut a);
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-render-{}.png", std::process::id()));
        a.run_excmd(format!("render {} 12", path.display()));
        match &a.status {
            Some((m, Tone::Good)) => {
                assert!(m.starts_with("rendered"), "{m}");
                let bytes = std::fs::read(&path).unwrap();
                std::fs::remove_file(&path).ok();
                assert!(bytes.starts_with(b"\x89PNG"), "a real PNG");
            }
            Some((m, _)) => {
                assert!(m.contains("no monospace font"), "the only acceptable failure is a fontless machine: {m}");
            }
            None => panic!("render said nothing"),
        }
    }

    #[test]
    fn c_opens_the_sheet_and_a_field_is_stepped_into_typed_and_left() {
        let mut a = app();
        let (x, _) = two(&mut a);
        press(&mut a, "c");
        assert!(a.sheet.as_ref().is_some_and(|s| s.focused));
        assert_eq!(a.whereami().mode, Mode::Sheet);
        key(&mut a, KeyCode::Tab);
        key(&mut a, KeyCode::Tab);
        assert_eq!(a.sheet.as_ref().unwrap().tab, form::Tab::Arrange);
        press(&mut a, "jjj");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "width");
        press(&mut a, "i");
        key(&mut a, KeyCode::Backspace);
        key(&mut a, KeyCode::Backspace);
        press(&mut a, "24");
        key(&mut a, KeyCode::Esc);
        assert!(a.sheet.as_ref().unwrap().editing.is_none(), "esc leaves the field");
        assert_eq!(a.doc.element(x).unwrap().w, 24.0);
        // A value that will not go: enter says why and stays; esc gives up on it.
        press(&mut a, "i");
        press(&mut a, "x");
        key(&mut a, KeyCode::Enter);
        assert!(matches!(a.status, Some((ref m, Tone::Bad)) if m.contains("whole number")));
        assert!(a.sheet.as_ref().unwrap().editing.is_some());
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.doc.element(x).unwrap().w, 24.0, "nothing was written");
        // The kind cycles with h/l, and the picture behind follows at once.
        press(&mut a, "kkk");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "kind");
        press(&mut a, "l");
        assert_eq!(a.doc.element(x).unwrap().kind, ShapeKind::ApplicationInterface);
        // t is the text key here too: the text tab, on the label.
        press(&mut a, "t");
        assert_eq!(a.sheet.as_ref().unwrap().tab, form::Tab::Text);
        press(&mut a, " v2");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.doc.element(x).unwrap().label, "CRM v2");
        // esc gives the keyboard back; the sheet stays and follows the cursor.
        key(&mut a, KeyCode::Esc);
        assert!(a.sheet.as_ref().is_some_and(|s| !s.focused));
        assert_eq!(a.whereami().mode, Mode::Normal);
        press(&mut a, "l");
        use ratatui::{backend::TestBackend, Terminal};
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        let sh = a.sheet.as_ref().unwrap();
        assert_eq!(sh.target, Some(Target::Element(1)), "followed the cursor to the second shape");
        assert_eq!(sh.tab, form::Tab::Text, "…keeping the tab");
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains("Contacts API") && out.contains("arrange"), "docked, with its tabs: {out}");
        assert_eq!(a.view_size.0, 120 - sheet::WIDTH, "the diagram gave the sheet its columns");
        // c re-enters; q closes.
        press(&mut a, "c");
        assert!(a.sheet.as_ref().is_some_and(|s| s.focused));
        press(&mut a, "q");
        assert!(a.sheet.is_none());
        press(&mut a, "uuu");
        let e = a.doc.element(x).unwrap();
        assert_eq!((e.w, e.kind, e.label.as_str()), (16.0, ShapeKind::ApplicationComponent, "CRM"), "one undo step per field");
    }

    #[test]
    fn h_and_l_step_a_number_on_the_sheet_as_they_cycle_a_choice() {
        let mut a = app();
        let (x, _) = two(&mut a);
        a.set_cursor(x);
        press(&mut a, "c");
        while a.sheet.as_ref().unwrap().tab != form::Tab::Arrange {
            key(&mut a, KeyCode::Tab);
        }
        while a.sheet.as_ref().unwrap().field().unwrap().name != "width" {
            press(&mut a, "j");
        }
        let w = a.doc.element(x).unwrap().w;
        press(&mut a, "ll");
        assert_eq!(a.doc.element(x).unwrap().w, w + 2.0, "l steps up by one, each press");
        press(&mut a, "h");
        assert_eq!(a.doc.element(x).unwrap().w, w + 1.0);
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().value, format!("{}", w + 1.0), "the sheet shows it at once");
        // Past the floor, apply's own words, and nothing changes.
        a.doc.element_mut(x).unwrap().w = 4.0;
        a.sheet.as_mut().unwrap().refresh(&a.doc);
        press(&mut a, "h");
        assert_eq!(a.doc.element(x).unwrap().w, 4.0);
        key(&mut a, KeyCode::Esc);
        press(&mut a, "u");
        assert_eq!(a.doc.element(x).unwrap().w, w + 2.0, "each step is one undo — the refused one left none");
        // H/L: ten at a time, one undo, and a big step stops at the limit rather than short of it.
        press(&mut a, "c");
        press(&mut a, "L");
        assert_eq!(a.doc.element(x).unwrap().w, w + 12.0);
        a.doc.element_mut(x).unwrap().w = 7.0;
        a.sheet.as_mut().unwrap().refresh(&a.doc);
        press(&mut a, "H");
        assert_eq!(a.doc.element(x).unwrap().w, 4.0, "7 less ten stops at width's floor of 4");
        key(&mut a, KeyCode::Esc);
        press(&mut a, "u");
        assert_eq!(a.doc.element(x).unwrap().w, 7.0, "the whole big step is one undo");
        // On a choice, H/L are h/l: one along.
        press(&mut a, "c");
        while a.sheet.as_ref().unwrap().field().unwrap().name != "kind" {
            press(&mut a, "k");
        }
        let kind = a.doc.element(x).unwrap().kind;
        press(&mut a, "L");
        let next = ShapeKind::ALL[(ShapeKind::ALL.iter().position(|k| *k == kind).unwrap() + 1) % ShapeKind::ALL.len()];
        assert_eq!(a.doc.element(x).unwrap().kind, next);
    }

    #[test]
    fn the_layer_browser_makes_hides_locks_and_moves_between_layers() {
        let mut a = app();
        let (x, y) = two(&mut a);
        a.run_excmd("layers".into());
        assert!(a.layers.is_some());
        // A new layer, named; it becomes current, and the next shape lands on it.
        press(&mut a, "n");
        press(&mut a, "notes");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.doc.layers.len(), 2);
        let notes = a.doc.layers[1].id;
        assert_eq!(a.doc.metadata.layer, notes);
        key(&mut a, KeyCode::Esc);
        a.run_excmd("add text".into());
        press(&mut a, "hi");
        key(&mut a, KeyCode::Esc);
        let t = a.cursor.unwrap();
        assert_eq!(a.doc.element(t).unwrap().layer, notes);
        // Move the cursor's shape down to layer 1 with m on that row.
        a.run_excmd("layers".into());
        press(&mut a, "j");
        assert_eq!(a.layers.as_ref().unwrap().selected(&a.doc), Some(0));
        press(&mut a, "m");
        assert_eq!(a.doc.element(t).unwrap().layer, 0);
        // Hide the top layer: nothing on it; hide layer 1: the cursor has nowhere to be.
        press(&mut a, "k ");
        assert!(!a.doc.layers[1].visible);
        assert!(a.cursor.is_some(), "still on layer 1, which shows");
        press(&mut a, "j ");
        assert!(!a.doc.layers[0].visible);
        assert!(a.cursor.is_none(), "every shape is hidden, so the cursor is on nothing");
        press(&mut a, " ");
        assert_eq!(a.cursor, Some(x), "shown again: the first shape in reading order");
        // Lock layer 1: its shapes refuse to move.
        press(&mut a, "l");
        assert!(a.doc.layers[0].locked);
        key(&mut a, KeyCode::Esc);
        press(&mut a, "L");
        assert_eq!(a.doc.element(x).unwrap().x, 2.0, "did not move");
        assert!(matches!(a.status, Some((ref m, Tone::Bad)) if m.starts_with("locked")));
        let _ = y;
        // Rename, reorder, delete — on layer 1, the bottom row.
        a.run_excmd("layers".into());
        press(&mut a, "j");
        press(&mut a, "r");
        for _ in 0..8 {
            key(&mut a, KeyCode::Backspace);
        }
        press(&mut a, "base");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.doc.layers[0].name, "base");
        press(&mut a, "K");
        assert_eq!(a.doc.layers[1].name, "base", "moved up the stack");
        assert_eq!(a.layers.as_ref().unwrap().selected(&a.doc), Some(0), "the browser follows the layer it moved");
        press(&mut a, "d");
        assert_eq!(a.doc.layers.len(), 1);
        press(&mut a, "d");
        assert!(matches!(a.status, Some((ref m, Tone::Bad)) if m.contains("only layer")));
    }

    #[test]
    fn the_arrange_tab_reorders_within_a_layer_from_the_sheet() {
        let mut a = app();
        let (x, y) = two(&mut a);
        press(&mut a, "c");
        key(&mut a, KeyCode::Tab);
        key(&mut a, KeyCode::Tab);
        press(&mut a, "jjjjjjjj");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "to front");
        key(&mut a, KeyCode::Enter);
        let order: Vec<ElementId> = a.doc.elements_in_order().iter().map(|e| e.id).collect();
        assert_eq!(order, vec![y, x]);
        press(&mut a, "jjjjj");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "locked");
        press(&mut a, " ");
        assert!(a.doc.element(x).unwrap().locked, "space flips a yes/no");
        press(&mut a, "kkkkkkkkkk");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "width");
        press(&mut a, "i9");
        key(&mut a, KeyCode::Enter);
        assert!(matches!(a.status, Some((ref m, Tone::Bad)) if m.starts_with("locked")));
    }

    #[test]
    fn on_a_narrow_terminal_the_sheet_floats_only_while_it_has_the_keys() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut a = app();
        two(&mut a);
        press(&mut a, "c");
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        assert_eq!(a.view_size.0, 80, "no dock on a narrow terminal");
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains("arrange"), "floating while focused");
        key(&mut a, KeyCode::Esc);
        term.draw(|f| a.draw(f)).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(!out.contains("arrange"), "and out of the way when the diagram has the keys");
    }

    #[test]
    fn a_relation_is_configured_the_same_way_and_reversed_from_its_arrange_tab() {
        let mut a = app();
        let (x, y) = two(&mut a);
        let r = a.doc.connect(RelationKind::Realization, x, y).unwrap();
        key(&mut a, KeyCode::Tab);
        press(&mut a, "c");
        let sh = a.sheet.as_ref().unwrap();
        assert_eq!((sh.tab, sh.field().unwrap().name), (form::Tab::Text, "label"), "from the centre node, the centre label");
        key(&mut a, KeyCode::BackTab);
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "kind");
        press(&mut a, "l");
        assert_eq!(a.doc.relation(r).unwrap().kind, RelationKind::Serving);
        press(&mut a, "t");
        press(&mut a, "uses");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.doc.relation(r).unwrap().label.as_deref(), Some("uses"));
        key(&mut a, KeyCode::Tab);
        assert_eq!(a.sheet.as_ref().unwrap().tab, form::Tab::Arrange);
        press(&mut a, "l");
        assert_eq!(a.doc.relation(r).unwrap().from_port, Some(0), "auto → tl");
        press(&mut a, "jjj");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "reverse");
        key(&mut a, KeyCode::Enter);
        let rel = a.doc.relation(r).unwrap();
        assert_eq!((rel.from, rel.to), (y, x), "reversed");
        assert_eq!(a.cursor, Some(x), "the cursor's shape is unchanged");
        key(&mut a, KeyCode::Esc);
        press(&mut a, "u");
        assert_eq!(a.doc.relation(r).unwrap().from, x, "one undo step");
    }

    #[test]
    fn inside_a_shape_a_ctrl_direction_opens_a_linked_shape_that_way_anchored_at_the_ports() {
        let mut a = app();
        let x = a.doc.add(ApplicationComponent, "CRM", 20.0, 20.0);
        a.set_cursor(x);
        press(&mut a, "i");
        a.on_key(Stroke::ctrl('l').event());
        assert!(matches!(a.palette.as_ref().map(|p| p.purpose), Some(palette::Purpose::Relate(_))), "the add dialog, relating");
        press(&mut a, "app-service");
        key(&mut a, KeyCode::Enter);
        key(&mut a, KeyCode::Enter); // the best kind of relation
        let new = a.doc.elements[1].clone();
        let from = a.doc.element(x).unwrap().clone();
        assert!(new.x >= from.right(), "to the right");
        assert_eq!(new.y, from.y, "level with it");
        let r = a.doc.relations[0].clone();
        assert_eq!((r.from, r.to, r.from_port, r.to_port), (x, new.id, Some(3), Some(7)), "anchored right handle → left handle");
        assert!(a.insert.is_some(), "and the label is next");
        key(&mut a, KeyCode::Esc);
        // A corner: ^n opens down-right, anchored at the bottom-right handle.
        a.set_cursor(x);
        press(&mut a, "i");
        a.on_key(Stroke::ctrl('n').event());
        press(&mut a, "data-object");
        key(&mut a, KeyCode::Enter);
        key(&mut a, KeyCode::Enter);
        let corner = a.doc.elements[2].clone();
        assert!(corner.x >= from.right() && corner.y >= from.bottom(), "down and right: {:?}", (corner.x, corner.y));
        assert_eq!(a.doc.relations[1].from_port, Some(4));
        key(&mut a, KeyCode::Esc);
        // `o` on a handle opens out of that handle: walk to the top handle first.
        a.set_cursor(x);
        press(&mut a, "i");
        press(&mut a, "kh");
        assert_eq!(a.reshape.unwrap().handle, 1, "the top handle");
        press(&mut a, "o");
        press(&mut a, "business-event");
        key(&mut a, KeyCode::Enter);
        key(&mut a, KeyCode::Enter);
        let above = a.doc.elements[3].clone();
        assert!(above.bottom() <= from.y, "above");
        assert_eq!(a.doc.relations[2].from_port, Some(1));
    }

    #[test]
    fn hovering_reaches_into_the_gutter_and_clicking_the_arrow_opens_off_that_port() {
        let mut a = app();
        // Small and close to the origin — `App::new`'s default body is 80×22 cells, and a
        // point off the edge of it is `screen_to_world`'s "outside the diagram" already,
        // before the arrow geometry is even asked about.
        let x = a.doc.add(ApplicationComponent, "CRM", 10.0, 8.0);
        // A box whose right handle's own row is a whole number: `handle_at`'s distance
        // weights a row twice a column (cells read taller than wide), so a handle exactly
        // between two rows would need the click to land within half of half a row of it —
        // tighter than a mouse's own cell resolution ever lands.
        a.doc.element_mut(x).unwrap().h = 6.0;
        a.set_cursor(x);
        let right_arrow = a.doc.element(x).unwrap().arrows(ARROW_GAP)[3];
        let at = (right_arrow.0.round(), right_arrow.1.round());
        // A point just inside the box first sets hover the ordinary way...
        let e = a.doc.element(x).unwrap().clone();
        mouse(&mut a, MouseEventKind::Moved, (e.x + 1.0, e.y + 1.0));
        assert_eq!(a.hover, Some(x));
        // ...and a point out in the gutter, past the border, keeps it: an arrow drawn
        // outside its shape would otherwise go cold the instant the mouse crosses the edge
        // on the way to it.
        mouse(&mut a, MouseEventKind::Moved, at);
        assert_eq!(a.hover, Some(x), "hover reaches into the arrow's own gutter");
        assert!(!e.contains((at.0, at.1)), "the arrow point really is outside the box");
        click(&mut a, MouseButton::Left, at);
        assert!(matches!(a.palette.as_ref().map(|p| p.purpose), Some(palette::Purpose::Relate(_))), "the same dialog o opens");
        assert_eq!(a.pending_port, Some(3), "the right handle — the arrow clicked");
    }

    #[test]
    fn the_direction_can_change_in_the_dialog_before_the_shape_is_added() {
        let mut a = app();
        let x = a.doc.add(ApplicationComponent, "CRM", 20.0, 20.0);
        a.set_cursor(x);
        press(&mut a, "i");
        a.on_key(Stroke::ctrl('l').event());
        assert_eq!(a.palette.as_ref().unwrap().dir, Some(3), "opened to the right");
        // Change of mind: ^k while the dialog is up.
        a.on_key(Stroke::ctrl('k').event());
        assert_eq!(a.palette.as_ref().unwrap().dir, Some(1));
        // …or tab onto the compass and press a bare key — a diagonal this time.
        key(&mut a, KeyCode::Tab);
        assert!(a.palette.as_ref().unwrap().compass);
        press(&mut a, "n");
        assert_eq!(a.palette.as_ref().unwrap().dir, Some(4));
        assert_eq!(a.palette.as_ref().unwrap().filter, "", "the compass took the key, not the search");
        key(&mut a, KeyCode::Tab);
        press(&mut a, "app-service");
        key(&mut a, KeyCode::Enter);
        key(&mut a, KeyCode::Enter);
        let from = a.doc.element(x).unwrap().clone();
        let new = a.doc.elements[1].clone();
        assert!(new.x >= from.right() && new.y >= from.bottom(), "down-right, as finally chosen");
        assert_eq!(a.doc.relations[0].from_port, Some(4));
    }

    #[test]
    fn from_the_diagram_a_ctrl_direction_opens_a_linked_shape_without_stepping_in() {
        let mut a = app();
        a.enhanced_keys = true;
        let x = a.doc.add(ApplicationComponent, "CRM", 20.0, 20.0);
        a.set_cursor(x);
        a.on_key(Stroke::ctrl('j').event());
        assert!(a.palette.is_some(), "the add dialog, from nav mode");
        assert_eq!(a.palette.as_ref().unwrap().dir, Some(5), "below");
        press(&mut a, "data-object");
        key(&mut a, KeyCode::Enter);
        key(&mut a, KeyCode::Enter);
        let from = a.doc.element(x).unwrap().clone();
        let new = a.doc.elements[1].clone();
        assert!(new.y >= from.bottom(), "below it");
        assert_eq!(a.doc.relations[0].from_port, Some(5));
        key(&mut a, KeyCode::Esc);
        a.set_cursor(x);
        a.on_key(Stroke::ctrl('u').event());
        assert_eq!(a.palette.as_ref().unwrap().dir, Some(2), "up-right, a corner");
        key(&mut a, KeyCode::Esc);
        // On a relation it is refused with the reason, not silently ignored.
        key(&mut a, KeyCode::Tab);
        a.on_key(Stroke::ctrl('l').event());
        assert!(a.palette.is_none());
        assert!(matches!(a.status, Some((_, Tone::Bad))), "refused, with a reason: {:?}", a.status);
    }

    #[test]
    fn a_patched_handle_can_be_disconnected_or_its_end_picked_up_and_moved() {
        let mut a = app();
        let (x, y) = two(&mut a);
        let r = a.doc.connect(RelationKind::Realization, x, y).unwrap();
        press(&mut a, "i");
        assert_eq!(a.reshape.unwrap().handle, 3, "the right handle, where the relation leaves");
        assert!(a.whereami().patched);
        // Pick it up, walk to the top handle, place it.
        key(&mut a, KeyCode::Enter);
        assert!(a.reshape.unwrap().moving.is_some());
        press(&mut a, "kh");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.doc.relation(r).unwrap().from_port, Some(1), "re-anchored at the top");
        assert!(a.reshape.unwrap().moving.is_none());
        assert!(a.doc.at_port(x, 3).is_empty() && a.doc.at_port(x, 1) == vec![r]);
        // Pick it up again and esc: back where it was.
        key(&mut a, KeyCode::Enter);
        press(&mut a, "jl");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.doc.relation(r).unwrap().from_port, Some(1));
        assert_eq!(a.reshape.unwrap().handle, 1, "the cursor went back to the handle it took the end from");
        // x disconnects.
        press(&mut a, "x");
        assert!(a.doc.relations.is_empty());
        assert!(matches!(a.status, Some((ref m, _)) if m == "disconnected"));
        press(&mut a, "x");
        assert!(matches!(a.status, Some((ref m, Tone::Bad)) if m.contains("nothing is attached")));
        key(&mut a, KeyCode::Esc);
        press(&mut a, "u");
        assert_eq!(a.doc.relations.len(), 1, "undo brings the relation back");
    }

    #[test]
    fn reshaping_walks_the_handles_and_drags_the_one_in_hand() {
        let mut a = app();
        let (x, _) = two(&mut a);
        let before = a.doc.element(x).unwrap().clone();
        press(&mut a, "i");
        assert!(matches!(a.reshape, Some(Reshape { handle: 3, held: false, moving: None })), "starts on the right handle");
        press(&mut a, "j");
        assert_eq!(a.reshape.unwrap().handle, 4, "down from the right handle is the bottom-right corner");
        press(&mut a, "h");
        assert_eq!(a.reshape.unwrap().handle, 5, "left along the bottom is the bottom handle");
        press(&mut a, "jj");
        assert_eq!(a.doc.element(x).unwrap().h, before.h, "walking never resizes");
        key(&mut a, KeyCode::Enter);
        assert!(a.reshape.unwrap().held);
        press(&mut a, "jj");
        a.on_key(Stroke::ctrl('j').event());
        let e = a.doc.element(x).unwrap().clone();
        assert_eq!(e.h, before.h + 2.0 + BIG_STEP, "the bottom edge moved, one cell twice and four once");
        assert_eq!(e.y, before.y, "the top edge stayed");
        key(&mut a, KeyCode::Esc);
        assert!(!a.reshape.unwrap().held, "esc lets go");
        press(&mut a, "kkkk");
        assert_eq!(a.reshape.unwrap().handle, 1, "up from the bottom reaches the top");
        key(&mut a, KeyCode::Enter);
        press(&mut a, "K");
        let e = a.doc.element(x).unwrap().clone();
        assert_eq!(e.y, before.y - BIG_STEP, "the top edge moved up four");
        assert_eq!(e.bottom(), before.bottom() + 2.0 + BIG_STEP, "the bottom edge stayed where it was");
        key(&mut a, KeyCode::Esc);
        key(&mut a, KeyCode::Esc);
        assert!(a.reshape.is_none(), "a second esc leaves");
        press(&mut a, "u");
        assert_eq!(a.doc.element(x).unwrap().y, before.y, "each grab is one undo step");
        press(&mut a, "u");
        assert_eq!(a.doc.element(x).unwrap().h, before.h);
    }

    #[test]
    fn a_box_never_shrinks_past_its_minimum_and_corners_move_two_edges() {
        let mut a = app();
        let (x, _) = two(&mut a);
        press(&mut a, "i");
        press(&mut a, "khh");
        assert_eq!(a.reshape.unwrap().handle, 0, "top-left: up to the corner, then along the top");
        key(&mut a, KeyCode::Enter);
        for _ in 0..30 {
            press(&mut a, "l");
            press(&mut a, "j");
        }
        let e = a.doc.element(x).unwrap().clone();
        assert_eq!(e.w, MIN_W);
        assert_eq!(e.h, MIN_H);
        press(&mut a, "hk");
        let e = a.doc.element(x).unwrap().clone();
        assert_eq!((e.w, e.h), (MIN_W + 1.0, MIN_H + 1.0), "a corner grows both ways");
    }

    #[test]
    fn reshaping_is_the_same_gesture_on_a_freeform_tab() {
        let mut a = app();
        a.run_excmd("tabnew freeform sketch".into());
        a.run_excmd("add circle".into());
        key(&mut a, KeyCode::Esc);
        let id = a.cursor.unwrap();
        let before = a.doc.element(id).unwrap().clone();
        press(&mut a, "i");
        key(&mut a, KeyCode::Enter);
        a.on_key(Stroke::ctrl('l').event());
        press(&mut a, "l");
        assert_eq!(a.doc.element(id).unwrap().w, before.w + BIG_STEP + 1.0, "the right edge moved");
        assert_eq!(a.doc.element(id).unwrap().x, before.x, "the left edge stayed");
        assert!(a.doc.lint().is_empty());
    }

    #[test]
    fn ctrl_t_asks_the_kind_makes_the_tab_and_opens_the_line_to_name_it() {
        let mut a = app();
        two(&mut a);
        a.on_key(Stroke::ctrl('t').event());
        assert!(a.tabpick.is_some(), "asked which kind");
        press(&mut a, "j");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.tabs.len(), 2);
        assert_eq!(a.tab, 1);
        assert_eq!(a.doc.metadata.view, View::Free, "the second row is architecture");
        assert!(a.doc.elements.is_empty(), "a fresh diagram");
        assert_eq!(a.cmdline.as_ref().map(|c| c.buf.as_str()), Some("tabrename "));
        press(&mut a, "Payments");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.tab_name(), "Payments");
        // The first tab kept its diagram, cursor and undo.
        press(&mut a, "gT");
        assert_eq!(a.tab, 0);
        assert_eq!(a.doc.elements.len(), 2);
        assert!(a.cursor.is_some());
        press(&mut a, "gt");
        assert_eq!(a.tab_name(), "Payments");
    }

    #[test]
    fn export_diagram_round_trips_through_tabnew_into_a_new_tab_not_a_replaced_workspace() {
        let mut a = app();
        let (x, y) = two(&mut a);
        a.doc.connect(RelationKind::Realization, x, y).unwrap();
        let mut path = std::env::temp_dir();
        let stem = format!("vim-shapes-diagram-{}", std::process::id());
        path.push(format!("{stem}.diagram"));
        a.run_excmd(format!("export {}", path.display()));
        assert!(matches!(a.status, Some((ref m, Tone::Good)) if m.contains("2 elements")), "{:?}", a.status);

        // A second, unrelated tab is already open — :tabnew must not disturb it.
        a.run_excmd("tabnew freeform untouched".into());
        let before = a.tabs.len();
        a.run_excmd(format!("tabnew {}", path.display()));
        std::fs::remove_file(&path).ok();

        assert_eq!(a.tabs.len(), before + 1, "added, not replaced");
        // A bare diagram export carries no tab name of its own — a tab's name is workspace
        // structure, not part of the document — so the new tab is named after the file, the
        // same fallback `:open`ing one already uses.
        assert_eq!(a.tab_name(), stem);
        assert_eq!(a.doc.elements.len(), 2);
        assert!(a.doc.relations.iter().any(|r| r.kind == RelationKind::Realization));
        press(&mut a, "gT");
        assert_eq!(a.tab_name(), "untouched", "the tab open before :tabnew is still exactly as it was");
    }

    #[test]
    fn tabnew_on_a_multi_tab_file_is_refused_with_a_clear_reason_and_nothing_added() {
        let mut a = app();
        two(&mut a);
        a.run_excmd("tabnew freeform second".into());
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-workspace-{}.json", std::process::id()));
        a.run_excmd(format!("write {}", path.display()));
        let before = a.tabs.len();

        a.run_excmd(format!("tabnew {}", path.display()));
        std::fs::remove_file(&path).ok();
        assert_eq!(a.tabs.len(), before, "refused, not partially applied");
        assert!(matches!(a.status, Some((ref m, Tone::Bad)) if m.contains("2 tabs")), "{:?}", a.status);
    }

    #[test]
    fn a_freeform_tab_offers_only_plain_shapes_and_refuses_nothing() {
        let mut a = app();
        a.run_excmd("tabnew freeform sketch".into());
        assert_eq!(a.tab_name(), "sketch");
        a.run_excmd("add".into());
        let rows = a.palette.as_ref().unwrap().rows();
        assert!(rows.iter().all(|k| k.is_sketch() || k.layer() == crate::ontology::Layer::Composite), "{rows:?}");
        key(&mut a, KeyCode::Esc);
        a.run_excmd("add box".into());
        key(&mut a, KeyCode::Esc);
        a.run_excmd("add circle".into());
        key(&mut a, KeyCode::Esc);
        key(&mut a, KeyCode::Enter);
        press(&mut a, "h");
        key(&mut a, KeyCode::Enter);
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.doc.relations.len(), 1);
        assert!(a.doc.lint().is_empty());
    }

    #[test]
    fn tabs_round_trip_through_the_file_and_the_last_tab_cannot_close() {
        let mut a = app();
        two(&mut a);
        a.run_excmd("tabnew freeform notes".into());
        a.run_excmd("grid off".into());
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-tabs-{}.json", std::process::id()));
        a.run_excmd(format!("w {}", path.display()));
        let mut b = app();
        b.run_excmd(format!("o {}", path.display()));
        std::fs::remove_file(&path).ok();
        assert_eq!(b.tabs.len(), 2);
        assert_eq!(b.tab, 1, "opens on the tab that was open");
        assert_eq!(b.tab_name(), "notes");
        assert!(!b.doc.metadata.page.grid, "the grid setting is part of the file, on the diagram");
        b.run_excmd("tabclose".into());
        assert_eq!(b.tabs.len(), 1);
        assert_eq!(b.doc.elements.len(), 2, "back on the first tab, diagram intact");
        b.run_excmd("tabclose".into());
        assert_eq!(b.tabs.len(), 1, "the only tab stays");
        assert!(b.doc.metadata.page.grid, "each diagram has its own grid: the first was never turned off");
        b.run_excmd("grid".into());
        assert!(!b.doc.metadata.page.grid, ":grid toggles");
    }

    #[test]
    fn closing_a_tab_with_unsaved_work_asks_first() {
        let mut a = app();
        a.run_excmd("tabnew architecture".into());
        a.run_excmd("add node".into());
        key(&mut a, KeyCode::Esc);
        a.run_excmd("tabclose".into());
        assert!(matches!(a.confirm, Some(Confirm::CloseTab)));
        press(&mut a, "y");
        assert_eq!(a.tabs.len(), 1);
    }

    #[test]
    fn the_camera_follows_the_cursor_off_the_edge() {
        let mut a = app();
        let x = a.doc.add(Node, "", 0.0, 0.0);
        let far = a.doc.add(Node, "", 200.0, 100.0);
        a.set_cursor(x);
        a.view_size = (80, 24);
        press(&mut a, "l");
        assert_eq!(a.cursor, Some(far));
        let (cx, cy) = a.camera;
        assert!(cx > 100.0 && cy > 70.0, "camera moved: {:?}", a.camera);
    }

    #[test]
    fn t_drags_a_label_anywhere_and_zero_puts_it_back_in_one_undo_step() {
        let mut a = app();
        let (x, y) = two(&mut a);
        let r = a.doc.connect(RelationKind::Realization, x, y).unwrap();
        a.doc.relation_mut(r).unwrap().label = Some("uses".into());
        let place = |a: &App| { let l = &canvas::label_lines(a.doc.element(x).unwrap(), "CRM")[0]; (l.0, l.1) };
        let before = place(&a);
        press(&mut a, "T");
        assert_eq!(a.whereami().mode, Mode::Text);
        press(&mut a, "jjl");
        press(&mut a, "L");
        a.on_key(Stroke::ctrl('k').event());
        assert_eq!(a.doc.element(x).unwrap().text.offset, (1.0 + BIG_STEP, 2.0 - BIG_STEP), "last key: {}", a.last_resolved);
        let after = place(&a);
        assert_eq!((after.0 - before.0, after.1 - before.1), (1.0 + BIG_STEP, 2.0 - BIG_STEP), "the label's place follows");
        press(&mut a, "3j");
        assert_eq!(a.doc.element(x).unwrap().text.offset.1, 2.0 - BIG_STEP + 3.0, "a count");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.whereami().mode, Mode::Normal);
        press(&mut a, "u");
        assert_eq!(a.doc.element(x).unwrap().text.offset, (0.0, 0.0), "the whole drag is one undo step");
        // A relation node's label drags the same way, and 0 puts it back.
        key(&mut a, KeyCode::Tab);
        assert_eq!(a.target(), Some(Target::Relation(r, LinkNode::Centre)));
        let at = a.doc.label_point(a.doc.relation(r).unwrap(), LinkNode::Centre).unwrap();
        press(&mut a, "T");
        press(&mut a, "hhk");
        let moved = a.doc.label_point(a.doc.relation(r).unwrap(), LinkNode::Centre).unwrap();
        assert_eq!((moved.0 - at.0, moved.1 - at.1), (-2.0, -1.0));
        press(&mut a, "0");
        assert_eq!(a.doc.relation(r).unwrap().offsets, [(0.0, 0.0); 3]);
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.whereami().mode, Mode::Normal);
        // Written to the file only once set.
        a.doc.element_mut(x).unwrap().text.offset = (3.0, -2.0);
        let json = serde_json::to_string(&a.doc).unwrap();
        assert!(json.contains("\"offset\":[3.0,-2.0]") && !json.contains("offsets"), "{json}");
        assert_eq!(serde_json::from_str::<crate::model::Document>(&json).unwrap(), a.doc);
    }

    #[test]
    fn the_view_pans_with_z_shift_arrows_and_a_view_mode_and_stays_until_the_cursor_moves() {
        let mut a = app();
        let (x, _) = two(&mut a);
        a.view_size = (80, 24);
        a.camera = (0.0, 0.0);
        press(&mut a, "zl");
        assert_eq!(a.camera, (PAN_X, 0.0));
        press(&mut a, "2zj");
        assert_eq!(a.camera, (PAN_X, 2.0 * PAN_Y), "a count multiplies");
        press(&mut a, "zH");
        assert_eq!(a.camera, (PAN_X - 40.0, 2.0 * PAN_Y), "half the view");
        a.on_key(Stroke::shifted(KeyCode::Up).event());
        assert_eq!(a.camera, (PAN_X - 40.0, PAN_Y), "shift+arrow");
        assert_eq!(a.cursor, Some(x), "the cursor did not move");
        // A draw does not pull the view back to the cursor: the pan was on purpose.
        use ratatui::{backend::TestBackend, Terminal};
        let mut term = Terminal::new(TestBackend::new(80, 26)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        assert_eq!(a.camera.0, PAN_X - 40.0, "still where it was panned");
        // View mode: hjkl pan, the cursor stays; esc leaves the view where it is.
        press(&mut a, "zv");
        assert_eq!(a.whereami().mode, Mode::View);
        press(&mut a, "llj");
        assert_eq!(a.camera, (PAN_X - 40.0 + 2.0 * PAN_X, 2.0 * PAN_Y));
        press(&mut a, "K");
        assert_eq!(a.camera.1, 2.0 * PAN_Y - 12.0);
        assert_eq!(a.cursor, Some(x));
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.whereami().mode, Mode::Normal);
        press(&mut a, "l");
        assert_ne!(a.cursor, Some(x));
        assert!(!a.panned, "moving the cursor lets the view follow it again");
        a.pan(200.0, 0.0);
        press(&mut a, "h");
        assert!(a.camera.0 < 100.0, "and it does follow, once the cursor is off the screen: {:?}", a.camera);
    }

    #[test]
    fn visual_mode_picks_shapes_configures_what_they_share_moves_them_together_and_dd_deletes() {
        let mut a = app();
        let (x, y) = two(&mut a);
        let z = a.doc.add(DataObject, "invoice", 2.0, 20.0);
        let r = a.doc.connect(RelationKind::Access, x, z).unwrap();
        a.set_cursor(x);
        press(&mut a, "v");
        press(&mut a, "l ");
        assert_eq!(a.whereami().mode, Mode::Visual);
        assert_eq!(a.selection, vec![x, y]);
        // c: the sheet on the set, what they share, mixed where they differ.
        press(&mut a, "c");
        let sh = a.sheet.as_ref().unwrap();
        assert_eq!((sh.target, sh.picked.clone()), (Some(Target::Picked), vec![x, y]));
        press(&mut a, "jjj");
        assert_eq!(a.sheet.as_ref().unwrap().field().unwrap().name, "colour");
        press(&mut a, "l");
        assert!([x, y].iter().all(|&id| a.doc.element(id).unwrap().color.is_some()), "one l painted both");
        press(&mut a, "t");
        let f = a.sheet.as_ref().unwrap();
        assert!(f.field().unwrap().mixed && f.editing.as_deref() == Some(""), "mixed labels: typing starts blank");
        press(&mut a, "same");
        key(&mut a, KeyCode::Enter);
        assert!([x, y].iter().all(|&id| a.doc.element(id).unwrap().label == "same"));
        key(&mut a, KeyCode::Esc);
        press(&mut a, "u");
        assert_eq!(a.doc.element(x).unwrap().label, "CRM", "one undo step for the set — last key: {}", a.last_resolved);
        assert_eq!(a.whereami().mode, Mode::Visual, "still picking");
        use ratatui::{backend::TestBackend, Terminal};
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains("2 shapes picked") && out.contains("mixed"), "{out}");
        // HJKL move the set; the relation to the unpicked shape follows its moved end.
        let before = a.doc.end_points(a.doc.relation(r).unwrap()).unwrap();
        press(&mut a, "J");
        assert!([x, y].iter().all(|&id| a.doc.element(id).unwrap().y == 3.0));
        let after = a.doc.end_points(a.doc.relation(r).unwrap()).unwrap();
        assert_ne!(after.0, before.0, "the end on the moved shape moved");
        assert_eq!(a.doc.element(z).unwrap().y, 20.0, "the unpicked shape stayed");
        // dd deletes the set.
        press(&mut a, "d");
        assert!(matches!(a.confirm, Some(Confirm::DeletePicked)));
        press(&mut a, "d");
        assert_eq!(a.doc.elements.len(), 1);
        assert!(a.doc.relations.is_empty(), "and the relations on them");
    }

    #[test]
    fn angle_brackets_lean_a_shape_on_the_diagram_and_inside_it_and_the_sheet_has_the_field() {
        let mut a = app();
        let (x, _) = two(&mut a);
        press(&mut a, ">>");
        assert_eq!(a.doc.element(x).unwrap().skew, 2.0);
        press(&mut a, "3<");
        assert_eq!(a.doc.element(x).unwrap().skew, -1.0, "a count");
        press(&mut a, "}}");
        press(&mut a, "{");
        assert_eq!(a.doc.element(x).unwrap().skew_y, 1.0, "down and up");
        press(&mut a, "i");
        press(&mut a, ">");
        press(&mut a, "{");
        assert_eq!((a.doc.element(x).unwrap().skew, a.doc.element(x).unwrap().skew_y), (0.0, 0.0), "inside the shape too");
        key(&mut a, KeyCode::Esc);
        press(&mut a, "u");
        assert_eq!(a.doc.element(x).unwrap().skew_y, 1.0, "each lean is one undo step");
        form::apply(&mut a.doc, Target::Element(x), "skew x", "4").unwrap();
        form::apply(&mut a.doc, Target::Element(x), "skew y", "0").unwrap();
        let e = a.doc.element(x).unwrap();
        let prims = crate::shapes::drawn(&a.doc.metadata.page, e, 0.0);
        let crate::shapes::CurvePrimitive::Lines(ls) = &prims[0] else { panic!() };
        assert_eq!(ls[0].0, (e.x - 2.0, e.y), "the outline leans: top-left two cells left");
        let json = serde_json::to_string(&a.doc).unwrap();
        assert!(json.contains("\"skew\":4.0") && !json.contains("skew_y"));
    }

    #[test]
    fn in_visual_mode_the_resize_keys_stretch_the_picked_set_places_and_sizes_together() {
        let mut a = app();
        let x = a.doc.add(Box, "a", 0.0, 0.0);
        let y = a.doc.add(Box, "b", 40.0, 10.0);
        let z = a.doc.add(Box, "c", 100.0, 0.0);
        a.set_cursor(x);
        let (w, h) = (a.doc.element(x).unwrap().w, a.doc.element(x).unwrap().h);
        press(&mut a, "v");
        a.selection = vec![x, y];
        // The set spans 0..40+w across; = grows that by four, scaling every place and size.
        press(&mut a, "=");
        let gw = 40.0 + w;
        let fx = (gw + BIG_STEP) / gw;
        let (ex, ey) = (a.doc.element(x).unwrap().clone(), a.doc.element(y).unwrap().clone());
        assert_eq!((ex.x, ex.w), (0.0, (w * fx).round()), "the first stays at the box's edge and widens");
        assert_eq!((ey.x, ey.w), ((40.0 * fx).round(), (w * fx).round()), "the second moves out and widens");
        assert_eq!((ex.y, ey.y, ex.h), (0.0, 10.0, h), "nothing changed down");
        assert_eq!(a.doc.element(z).unwrap().x, 100.0, "the unpicked shape stayed");
        press(&mut a, "2+");
        let gh = 10.0 + h;
        let fy = (gh + BIG_STEP) / gh;
        assert_eq!((a.doc.element(y).unwrap().y, a.doc.element(x).unwrap().h), ((10.0 * fy).round(), (h * fy).round()), "a count, down");
        press(&mut a, "u");
        press(&mut a, "u");
        assert_eq!((a.doc.element(y).unwrap().x, a.doc.element(y).unwrap().y), (40.0, 10.0), "each stretch is one undo step");
        for _ in 0..30 {
            press(&mut a, "-");
        }
        assert!(a.doc.element(x).unwrap().w >= 4.0 && a.doc.element(y).unwrap().x >= 0.0, "it stops before folding");
    }

    #[test]
    fn theme_switches_the_palette_keeps_the_choice_in_the_config_file_and_says_why() {
        let dir = std::env::temp_dir().join(format!("vim-shapes-theme-{}", std::process::id()));
        // SAFETY: this test alone touches the variable, and only reads it back through the
        // config module.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };
        let mut a = app();
        two(&mut a);
        a.run_excmd("theme light".into());
        assert_eq!(theme::mode(), theme::Mode::Light);
        assert_eq!(theme::source(), theme::Source::Command);
        assert!(matches!(&a.status, Some((m, Tone::Note)) if m.contains("kept")), "{:?}", a.status);
        assert_eq!(crate::config::load_from(&dir.join("vim-shapes").join("config.json")).theme.as_deref(), Some("light"));
        use ratatui::{backend::TestBackend, Terminal};
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        let buf = term.backend().buffer();
        assert_eq!(buf[(100, 28)].bg, theme::LIGHT.ground, "an empty cell is the palette's ground, not the terminal's");
        let label = buf.content.iter().find(|c| c.bg == theme::LIGHT.yellow).expect("the cursor's label sits on the light yellow");
        assert_eq!(label.fg, theme::LIGHT.inverse);
        assert!(buf.content.iter().any(|c| c.fg == theme::LIGHT.ink), "ordinary text is the light ink");
        assert!(!buf.content.iter().any(|c| c.fg == theme::DARK.ink || c.bg == theme::DARK.panel || c.fg == theme::DARK.dim), "nothing dark left on screen");
        a.run_excmd("theme".into());
        assert!(matches!(&a.status, Some((m, _)) if m.contains("light") && m.contains(":theme")), "{:?}", a.status);
        a.run_excmd("theme sideways".into());
        assert!(matches!(&a.status, Some((_, Tone::Bad))));
        a.run_excmd("theme dark".into());
        assert_eq!(theme::mode(), theme::Mode::Dark);
        assert!(a.debug_rows().iter().any(|(k, v)| *k == "theme" && v.contains("dark")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn p_opens_the_property_browser_and_its_keys_shape_an_object_type_s_rows() {
        let mut a = app();
        a.doc.metadata.view = View::Ontology;
        let x = a.doc.add(ShapeKind::ObjectType, "Airport", 2.0, 2.0);
        let plain = a.doc.add(ShapeKind::Box, "note", 40.0, 2.0);
        a.set_cursor(plain);
        press(&mut a, "P");
        assert!(a.props.is_none(), "a plain box has no rows");
        assert!(matches!(&a.status, Some((m, Tone::Bad)) if m.contains("no rows")));
        a.set_cursor(x);
        let h0 = a.doc.element(x).unwrap().h;
        press(&mut a, "P");
        assert!(a.props.is_some());
        press(&mut a, "n");
        press(&mut a, "code");
        key(&mut a, KeyCode::Enter);
        press(&mut a, "n");
        press(&mut a, "name");
        key(&mut a, KeyCode::Enter);
        press(&mut a, "k");
        press(&mut a, "p");
        press(&mut a, "j");
        press(&mut a, "l");
        press(&mut a, "T");
        press(&mut a, "str");
        key(&mut a, KeyCode::Enter);
        press(&mut a, "n");
        press(&mut a, "location");
        key(&mut a, KeyCode::Enter);
        press(&mut a, "t");
        let e = a.doc.element(x).unwrap().clone();
        assert_eq!(e.properties.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["code", "name", "location"]);
        assert!(e.properties[0].primary_key && e.properties[1].title);
        assert_eq!(e.properties[2].base_type, crate::model::BaseType::Integer, "t cycles the type once");
        assert_eq!(e.h, 2.0 + Element::HEADER + 3.0, "the box grew to hold three rows");
        assert!(e.h > h0);
        press(&mut a, "K");
        assert_eq!(a.doc.element(x).unwrap().properties[1].name, "location", "K moves the row up");
        use ratatui::{backend::TestBackend, Terminal};
        let mut term = Terminal::new(TestBackend::new(120, 32)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains("Airport — properties") && out.contains("⚿ code") && out.contains("✎ name"), "the browser and the box both show the rows: {out}");
        // The rows on the cursor's shape are the ink on the tint, in either palette.
        for mode in [theme::Mode::Light, theme::Mode::Dark] {
            theme::set_mode(mode, theme::Source::Default);
            term.draw(|f| a.draw(f)).unwrap();
            let buf = term.backend().buffer();
            let row = buf.content.iter().find(|c| c.bg == theme::t().hilite && c.symbol() == "⚿").expect("a row on the tint");
            assert_eq!(row.fg, theme::t().ink, "{} palette", mode.name());
        }
        theme::set_mode(theme::Mode::Dark, theme::Source::Default);
        assert!(out.contains("───"), "the header rule is drawn");
        key(&mut a, KeyCode::Esc);
        assert!(a.props.is_none());
        press(&mut a, "u");
        assert_eq!(a.doc.element(x).unwrap().properties[1].name, "name", "each change is one undo step");
        a.run_excmd("props".into());
        assert!(a.props.is_some(), ":props opens it too");
        a.run_excmd("lint".into());
        assert!(matches!(&a.status, Some((_, Tone::Good))), "a keyed, titled object type passes: {:?}", a.status);
        let f = a.doc.add(ShapeKind::ObjectType, "Flight", 40.0, 20.0);
        a.doc.connect(RelationKind::LinkType, x, f).unwrap();
        a.run_excmd("lint".into());
        assert!(matches!(&a.status, Some((m, Tone::Bad)) if m.contains("Flight has no properties") && m.contains("1 more")), "the schema's checks reach :lint, counted: {:?}", a.status);
    }

    #[test]
    fn an_ontology_exports_its_reference_and_its_definition_and_lands_in_draw_io_as_classes() {
        let mut a = app();
        a.doc.metadata.view = View::Ontology;
        let x = a.doc.add(ShapeKind::ObjectType, "Airport", 2.0, 2.0);
        let y = a.doc.add(ShapeKind::Interface, "Place", 40.0, 2.0);
        let mut code = crate::model::Property::new("code");
        code.primary_key = true;
        a.doc.element_mut(x).unwrap().properties = vec![code];
        a.doc.element_mut(x).unwrap().status = crate::model::Status::Deprecated;
        a.doc.element_mut(y).unwrap().status = crate::model::Status::Experimental;
        a.doc.connect(RelationKind::Implements, x, y).unwrap();
        let dir = std::env::temp_dir().join(format!("vim-shapes-ontology-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        a.run_excmd(format!("export {}", dir.join("o.md").display()));
        let md = std::fs::read_to_string(dir.join("o.md")).unwrap();
        assert!(md.contains("### Airport *(deprecated)*") && md.contains("| `code` | string | ⚿ |"), "{md}");
        a.run_excmd(format!("export {}", dir.join("o.json").display()));
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("o.json")).unwrap()).unwrap();
        assert_eq!(v["objectTypes"][0]["primaryKey"], "code");
        assert_eq!(v["interfaces"][0]["status"], "experimental");
        let xml = crate::drawio_export::to_xml(&a.doc);
        assert!(xml.contains("swimlane;fontStyle=1;childLayout=stackLayout") && xml.contains("value=\"⚿ code: string\"") && xml.contains("parent=\"e0\""), "{xml}");
        assert!(xml.contains("«interface» Place") && xml.contains("dashed=1;"), "an interface is a dashed class: {xml}");
        assert!(xml.contains("opacity=50;"), "a deprecated type fades");
        let e = a.doc.element(x).unwrap();
        assert_eq!((e.tag(), e.tag_marked(true)), ("object ✗".into(), "object x".into()));
        assert_eq!(a.doc.element(y).unwrap().drawn_line(), crate::ontology::LineStyle::Dashed);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_deprecated_type_and_row_wear_a_red_x_and_nothing_else_is_red() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut a = app();
        a.doc.metadata.view = View::Ontology;
        let x = a.doc.add(ShapeKind::ObjectType, "Airport", 2.0, 2.0);
        let mut old = crate::model::Property::new("iata");
        old.status = crate::model::Status::Deprecated;
        let e = a.doc.element_mut(x).unwrap();
        e.properties = vec![crate::model::Property::new("code"), old];
        e.status = crate::model::Status::Deprecated;
        e.h = 10.0;
        a.cursor = None;
        let mut term = Terminal::new(TestBackend::new(80, 20)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        let buf = term.backend().buffer();
        let marks: Vec<_> = buf.content.iter().filter(|c| c.symbol() == "✗").collect();
        assert_eq!(marks.len(), 2, "one on the tag, one on the row");
        assert!(marks.iter().all(|c| c.fg == theme::t().red), "both red");
        assert!(!buf.content.iter().any(|c| c.symbol() == "†"), "the dagger is gone");
    }

    #[test]
    fn ink_switches_between_line_art_and_braille_and_keeps_the_choice() {
        let dir = std::env::temp_dir().join(format!("vim-shapes-ink-{}", std::process::id()));
        // SAFETY: this test alone touches the variable, and only reads it back through the
        // config module.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };
        let mut a = app();
        two(&mut a);
        a.run_excmd("ink lines".into());
        assert_eq!(wire::ink(), wire::Ink::Lines);
        assert_eq!(crate::config::load_from(&dir.join("vim-shapes").join("config.json")).ink.as_deref(), Some("lines"));
        use ratatui::{backend::TestBackend, Terminal};
        let mut term = Terminal::new(TestBackend::new(100, 24)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains("┌") && out.contains("┘") && !out.chars().any(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch)), "line art, no braille: {out}");
        a.run_excmd("ink braille".into());
        assert_eq!(wire::ink(), wire::Ink::Braille);
        term.draw(|f| a.draw(f)).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.chars().any(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch)), "braille again");
        a.run_excmd("ink".into());
        assert!(matches!(&a.status, Some((m, _)) if m.contains("braille")));
        a.run_excmd("ink crayon".into());
        assert!(matches!(&a.status, Some((_, Tone::Bad))));
        a.run_excmd("ink lines".into());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn g_wraps_a_picked_set_gp_goes_up_to_it_and_gu_dissolves_it() {
        let mut a = app();
        let (x, y) = two(&mut a);
        let outside = a.doc.add(DataObject, "invoice", 90.0, 40.0);
        press(&mut a, "v");
        a.selection = vec![x, y];
        press(&mut a, "g");
        assert!(!a.visual && a.selection.is_empty(), "picking is over");
        let g = a.cursor.expect("the cursor is on the new grouping");
        assert_eq!(a.doc.element(g).unwrap().kind, ShapeKind::Grouping);
        assert_eq!(a.whereami().mode, Mode::Insert, "type its name");
        press(&mut a, "Sales");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.doc.element(g).unwrap().label, "Sales");
        // It holds what it was drawn round, and nothing else.
        let mut members = a.doc.members(g);
        members.sort();
        assert_eq!(members, vec![x, y]);
        assert!(!members.contains(&outside));
        assert_eq!(a.doc.elements_in_order()[0].id, g, "the box is behind what it holds");
        // It moves them with it.
        let before = a.doc.element(x).unwrap().y;
        press(&mut a, "J");
        assert_eq!(a.doc.element(x).unwrap().y, before + 1.0);
        assert_eq!(a.doc.element(outside).unwrap().y, 40.0, "and nothing else");
        // gp goes up to it from a member; gu dissolves it and leaves them.
        a.set_cursor(x);
        assert!(a.whereami().in_group);
        press(&mut a, "gp");
        assert_eq!(a.cursor, Some(g));
        let (px, py) = (a.doc.element(x).unwrap().x, a.doc.element(x).unwrap().y);
        press(&mut a, "gu");
        assert!(a.doc.element(g).is_none());
        assert_eq!(a.doc.elements.len(), 3, "what was inside stays");
        assert_eq!((a.doc.element(x).unwrap().x, a.doc.element(x).unwrap().y), (px, py), "where it was");
        press(&mut a, "u");
        assert!(a.doc.element(g).is_some(), "one undo step");
        a.set_cursor(outside);
        assert!(!a.whereami().in_group);
        press(&mut a, "gu");
        assert!(matches!(&a.status, Some((m, Tone::Bad)) if m.contains("on a grouping")));
    }

    #[test]
    fn a_frame_renders_every_surface_without_panicking() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let mut a = App::new();
        term.draw(|f| a.draw(f)).unwrap();
        a.loading = false;
        a.run_excmd("idiom stack".into());
        a.run_excmd("idiom motivation".into());
        term.draw(|f| a.draw(f)).unwrap();
        press(&mut a, "?");
        term.draw(|f| a.draw(f)).unwrap();
        key(&mut a, KeyCode::Esc);
        press(&mut a, ":add");
        term.draw(|f| a.draw(f)).unwrap();
        key(&mut a, KeyCode::Enter);
        term.draw(|f| a.draw(f)).unwrap();
        key(&mut a, KeyCode::Esc);
        key(&mut a, KeyCode::Enter);
        press(&mut a, "l");
        key(&mut a, KeyCode::Enter);
        term.draw(|f| a.draw(f)).unwrap();
        key(&mut a, KeyCode::Esc);
        a.run_excmd("help".into());
        term.draw(|f| a.draw(f)).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains("contents"));
    }

    // ─── the mouse ──────────────────────────────────────────────────────────

    #[test]
    fn a_mouse_click_selects_and_changes_nothing_else() {
        let mut a = app();
        let (_x, y) = two(&mut a);
        let before = a.doc.clone();
        click(&mut a, MouseButton::Left, (32.0, 4.0));
        assert_eq!(a.cursor, Some(y));
        assert_eq!(a.doc, before, "a click alone changes nothing but the cursor");
        assert!(a.undo.is_empty(), "no undo step for a plain click");
    }

    #[test]
    fn a_click_on_ground_deselects() {
        let mut a = app();
        let (x, _y) = two(&mut a);
        a.selection = vec![x];
        a.visual = true;
        click(&mut a, MouseButton::Left, (60.0, 15.0));
        assert_eq!(a.cursor, None);
        assert!(!a.visual && a.selection.is_empty());
    }

    #[test]
    fn a_left_drag_on_a_body_moves_it_as_one_undo_step() {
        let mut a = app();
        let (x, _y) = two(&mut a);
        drag(&mut a, MouseButton::Left, (10.0, 4.0), (14.0, 4.0));
        let e = a.doc.element(x).unwrap();
        assert_eq!((e.x, e.y), (6.0, 2.0), "moved by the drag's delta");
        assert_eq!(a.undo.len(), 1, "one undo step for the whole drag, not one per event");
    }

    #[test]
    fn a_left_drag_on_an_unpatched_handle_resizes_and_lets_go_on_release() {
        let mut a = app();
        let (x, _y) = two(&mut a);
        drag(&mut a, MouseButton::Left, (18.0, 2.0), (22.0, 2.0));
        let e = a.doc.element(x).unwrap();
        assert_eq!(e.w, 20.0, "the right edge followed the drag");
        assert!(a.reshape.is_none(), "a mouse handle-drag is one-shot: it ends on release");
    }

    #[test]
    fn a_left_drag_on_a_patched_handle_reroutes_to_an_open_port_and_refuses_a_taken_one() {
        let mut a = app();
        let (x, y) = two(&mut a);
        let r1 = a.doc.connect(RelationKind::Link, x, y).unwrap();
        a.doc.relation_mut(r1).unwrap().from_port = Some(1); // the top handle
        let r2 = a.doc.connect(RelationKind::Link, x, y).unwrap();
        a.doc.relation_mut(r2).unwrap().from_port = Some(5); // the bottom handle — taken

        drag(&mut a, MouseButton::Left, (10.0, 2.0), (10.0, 7.0));
        assert_eq!(a.doc.relation(r1).unwrap().from_port, Some(1), "the bottom handle is r2's — refused");

        drag(&mut a, MouseButton::Left, (10.0, 2.0), (2.0, 2.0));
        assert_eq!(a.doc.relation(r1).unwrap().from_port, Some(0), "the top-left handle was open");
    }

    #[test]
    fn a_marquee_selects_what_it_covers_and_nothing_outside_it() {
        let mut a = app();
        let (x, y) = two(&mut a);
        drag(&mut a, MouseButton::Left, (0.0, 0.0), (20.0, 10.0));
        assert_eq!(a.selection, vec![x], "y sits outside the box");
        assert!(a.visual);
        let _ = y;
    }

    #[test]
    fn a_right_drag_between_two_elements_opens_the_relpick_as_a_keyboard_hold_and_drop_would() {
        let mut a = app();
        let (x, y) = two(&mut a);
        drag(&mut a, MouseButton::Right, (10.0, 4.0), (38.0, 4.0));
        assert!(a.relpick.is_some());
        assert_eq!(a.holding, Some(x), "holding stays set until the picker resolves, same as the keyboard path");
        let _ = y;
    }

    #[test]
    fn a_plain_right_click_opens_the_menu_and_leaves_the_pick_alone() {
        let mut a = app();
        let (x, _y) = two(&mut a);
        a.selection = vec![x];
        a.visual = true;
        click(&mut a, MouseButton::Right, (10.0, 4.0));
        assert!(a.relpick.is_none());
        assert!(a.visual && a.selection == vec![x], "an idle right-click opens the menu — acting on the pick is the point, so it stays");
        assert!(a.ctxmenu.is_some(), "the context menu opened");
    }

    #[test]
    fn the_context_menu_runs_a_row_by_replaying_its_keys() {
        let mut a = app();
        let (x, _y) = two(&mut a);
        a.set_cursor(x);
        click(&mut a, MouseButton::Right, (2.0, 2.0));
        assert!(a.ctxmenu.is_some(), "an idle right-click on an element opens the menu");
        let i = a
            .ctxmenu
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .position(|r| r.cmd.keys == "v")
            .expect("v is available in normal mode, same as the ? menu's own replay test");
        a.ctxmenu.as_mut().unwrap().sel = i;
        key(&mut a, KeyCode::Enter);
        assert!(a.ctxmenu.is_none(), "running a row closes the menu");
        assert!(a.visual, "v from the menu did what v does");
    }

    #[test]
    fn a_right_drag_from_empty_ground_pans_by_exactly_the_mouse_s_own_movement() {
        let mut a = app();
        let before = a.camera;
        mouse(&mut a, MouseEventKind::Down(MouseButton::Right), (60.0, 18.0));
        mouse(&mut a, MouseEventKind::Drag(MouseButton::Right), (40.0, 12.0));
        // Content follows the hand: dragging left and up reveals what was off to the left
        // and above, so the camera itself moves the opposite way of the mouse.
        assert_eq!(a.camera, (before.0 + 20.0, before.1 + 6.0));
        mouse(&mut a, MouseEventKind::Drag(MouseButton::Right), (20.0, 8.0));
        // A second drag event is incremental from where the last one left off, not from the
        // original down — a fast, uneven stream of drag events still tracks the mouse exactly.
        assert_eq!(a.camera, (before.0 + 40.0, before.1 + 10.0));
        mouse(&mut a, MouseEventKind::Up(MouseButton::Right), (20.0, 8.0));
        assert_eq!(a.camera, (before.0 + 40.0, before.1 + 10.0), "released where it was — no snap-back");
    }

    #[test]
    fn panning_by_right_drag_leaves_the_current_pick_and_hold_untouched() {
        let mut a = app();
        let (x, _y) = two(&mut a);
        a.selection = vec![x];
        a.visual = true;
        drag(&mut a, MouseButton::Right, (60.0, 18.0), (40.0, 12.0));
        assert!(a.visual && a.selection == vec![x], "a pan is a deliberate view move, not a cancel");
    }

    #[test]
    fn scroll_pans_the_camera_by_the_keyboard_s_own_step() {
        let mut a = app();
        let before = a.camera;
        mouse(&mut a, MouseEventKind::ScrollDown, (10.0, 10.0));
        mouse(&mut a, MouseEventKind::ScrollRight, (10.0, 10.0));
        assert_eq!(a.camera, (before.0 + PAN_X, before.1 + PAN_Y));
    }

    #[test]
    fn hovering_shows_a_handle_without_clicking_and_leaves_no_trace_in_the_document() {
        let mut a = app();
        let (x, _y) = two(&mut a);
        mouse(&mut a, MouseEventKind::Moved, (10.0, 4.0));
        assert_eq!(a.hover, Some(x));
        assert!(a.reshape.is_none(), "hovering is not a grab");
    }
}

#[cfg(test)]
mod eyeball {
    //! Eyeball the screen: cargo test eyeball -- --ignored --nocapture
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn dump(term: &Terminal<TestBackend>) -> String {
        let buf = term.backend().buffer();
        (0..buf.area.height)
            .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    #[ignore]
    fn render_the_general_palette() {
        let mut term = Terminal::new(TestBackend::new(118, 44)).unwrap();
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("tabnew freeform gallery".into());
        a.run_excmd("grid off".into());
        let kinds = [
            ShapeKind::Triangle, ShapeKind::PredefinedProcess, ShapeKind::Document, ShapeKind::InternalStorage, ShapeKind::Cube, ShapeKind::Step,
            ShapeKind::Trapezoid, ShapeKind::Tape, ShapeKind::Note, ShapeKind::Card, ShapeKind::Callout, ShapeKind::StickFigure,
            ShapeKind::DataStorage, ShapeKind::Delay, ShapeKind::Display, ShapeKind::ManualInput, ShapeKind::OffPage, ShapeKind::BlockArrow,
            ShapeKind::DoubleArrow, ShapeKind::And, ShapeKind::Or, ShapeKind::Square, ShapeKind::Ellipse,
        ];
        let (mut x, mut y) = (1.0, 1.0);
        for (i, k) in kinds.iter().enumerate() {
            let (w, _) = k.default_size();
            a.doc.add(*k, k.short(), x, y);
            x += w + 3.0;
            if (i + 1) % 6 == 0 {
                x = 1.0;
                y += 8.0;
            }
        }
        a.set_cursor(0);
        a.cursor = None;
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    #[test]
    #[ignore]
    fn render_the_hover_arrows() {
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("tabnew freeform gallery".into());
        a.run_excmd("grid off".into());
        let x = a.doc.add(ShapeKind::Box, "Items", 10.0, 6.0);
        a.set_cursor(x);
        a.hover = Some(x);
        for ink in ["lines", "braille"] {
            a.run_excmd(format!("ink {ink}"));
            let mut term = Terminal::new(TestBackend::new(60, 20)).unwrap();
            term.draw(|f| a.draw(f)).unwrap();
            println!("-- {ink} --\n{}", dump(&term));
        }
    }

    #[test]
    #[ignore]
    fn render_the_right_click_menu() {
        let mut term = Terminal::new(TestBackend::new(90, 26)).unwrap();
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("tabnew freeform gallery".into());
        let x = a.doc.add(ShapeKind::Box, "Items", 10.0, 6.0);
        a.set_cursor(x);
        a.ctxmenu = Some(ctxmenu::State::new(&a.whereami()));
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    #[test]
    #[ignore]
    fn render_the_compass_in_the_add_dialog() {
        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("add component".into());
        a.on_key(keymap::Stroke::code(KeyCode::Esc).event());
        a.on_key(keymap::Stroke::k('i').event());
        a.on_key(keymap::Stroke::ctrl('n').event());
        a.on_key(keymap::Stroke::code(KeyCode::Tab).event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    #[test]
    #[ignore]
    fn write_every_export_of_the_stack_idiom() {
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("idiom stack".into());
        let dir = std::env::temp_dir();
        for ext in ["svg", "pdf", "html", "png"] {
            a.run_excmd(format!("export {}", dir.join(format!("stack.{ext}")).display()));
            println!("{:?}", a.status);
        }
        a.run_excmd("export".into());
        let mut term = Terminal::new(TestBackend::new(90, 20)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    #[test]
    #[ignore]
    fn render_the_layer_browser() {
        let mut term = Terminal::new(TestBackend::new(120, 24)).unwrap();
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("idiom service".into());
        a.run_excmd("layers".into());
        a.on_key(keymap::Stroke::k('n').event());
        for c in "annotations".chars() {
            a.on_key(keymap::Stroke::k(c).event());
        }
        a.on_key(keymap::Stroke::code(KeyCode::Enter).event());
        a.on_key(keymap::Stroke::k('l').event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    #[test]
    #[ignore]
    fn render_the_debug_panel() {
        let mut term = Terminal::new(TestBackend::new(120, 28)).unwrap();
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("idiom service".into());
        a.run_excmd("debug".into());
        a.on_key(keymap::Stroke::code(KeyCode::Tab).event());
        a.on_key(keymap::Stroke::k('u').event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    /// `cargo test eyeball_the_wire -- --ignored`: a mixed diagram in the wireframe ink, as text.
    #[test]
    #[ignore]
    fn eyeball_the_wire() {
        use ratatui::{backend::TestBackend, Terminal};
        wire::set_ink(wire::Ink::Lines);
        let mut a = App::new();
        a.loading = false;
        let x = a.doc.add(ShapeKind::ApplicationComponent, "Order service", 2.0, 2.0);
        let y = a.doc.add(ShapeKind::ApplicationService, "Orders API", 34.0, 2.0);
        let z = a.doc.add(ShapeKind::Node, "db host", 2.0, 12.0);
        let c = a.doc.add(ShapeKind::Cloud, "internet", 34.0, 12.0);
        let e = a.doc.add(ShapeKind::Ellipse, "customer", 62.0, 3.0);
        let d = a.doc.add(ShapeKind::Cylinder, "orders db", 62.0, 13.0);
        let dm = a.doc.add(ShapeKind::Diamond, "ok?", 20.0, 22.0);
        a.doc.connect(RelationKind::Realization, x, y).unwrap();
        let r = a.doc.connect(RelationKind::Serving, z, x).unwrap();
        a.doc.relation_mut(r).unwrap().label = Some("serves".into());
        a.doc.connect(RelationKind::Link, c, e).unwrap();
        let l = a.doc.connect(RelationKind::Link, y, d).unwrap();
        let mut n = a.doc.relation(l).unwrap().notation();
        n.route = crate::ontology::Route::Orthogonal;
        n.tail = crate::ontology::End::Bar;
        n.head = crate::ontology::End::Crow;
        a.doc.relation_mut(l).unwrap().style = Some(n);
        a.doc.connect(RelationKind::Link, z, dm).unwrap();
        a.set_cursor(x);
        let mut term = Terminal::new(TestBackend::new(90, 32)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        eprintln!("{}", dump(&term));
    }

    /// `cargo test eyeball_the_ruled_grid -- --ignored`: the screen with a ruled grid, as text.
    #[test]
    #[ignore]
    fn eyeball_the_ruled_grid() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut a = App::new();
        a.loading = false;
        let x = a.doc.add(ShapeKind::Box, "order", 4.0, 2.0);
        let y = a.doc.add(ShapeKind::Ellipse, "customer", 30.0, 4.0);
        a.doc.connect(RelationKind::Link, x, y).unwrap();
        a.doc.metadata.page.grid_style = crate::model::GridStyle::Lines;
        a.set_cursor(x);
        let mut term = Terminal::new(TestBackend::new(64, 14)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        eprintln!("{}", dump(&term));
    }

    /// `cargo test eyeball_branching -- --ignored --nocapture`: `o` off an object type — the
    /// palette with its suggested lines on top — and then the picker, grouped by family.
    #[test]
    #[ignore]
    fn eyeball_branching() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("kind ontology".into());
        let t = a.doc.add(ShapeKind::ObjectType, "Truck", 4.0, 2.0);
        a.set_cursor(t);
        a.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        let mut term = Terminal::new(TestBackend::new(110, 34)).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        eprintln!("{}\n", dump(&term));
        a.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let i = a.doc.add(ShapeKind::Interface, "Vehicle", 40.0, 2.0);
        a.relpick = Some(relpick::State::new(&a.doc, t, i, None));
        term.draw(|f| a.draw(f)).unwrap();
        eprintln!("{}", dump(&term));
    }

    /// `cargo test eyeball_both_themes -- --ignored`: the same screen in each palette, with
    /// the sheet up, to look at.
    #[test]
    #[ignore]
    fn eyeball_both_themes() {
        use ratatui::{backend::TestBackend, Terminal};
        for mode in [theme::Mode::Dark, theme::Mode::Light] {
            theme::set_mode(mode, theme::Source::Flag);
            let mut a = App::new();
            a.loading = false;
            let x = a.doc.add(ShapeKind::ApplicationComponent, "Order service", 2.0, 2.0);
            let y = a.doc.add(ShapeKind::ApplicationService, "Contacts API", 34.0, 2.0);
            let z = a.doc.add(ShapeKind::Node, "db host", 2.0, 12.0);
            a.doc.connect(RelationKind::Realization, x, y).unwrap();
            let r = a.doc.connect(RelationKind::Serving, z, x).unwrap();
            a.doc.relation_mut(r).unwrap().label = Some("serves".into());
            a.doc.element_mut(y).unwrap().color = Some(crate::ontology::Paint::Purple.into());
            a.set_cursor(x);
            a.on_key(keymap::Stroke::k('c').event());
            let mut term = Terminal::new(TestBackend::new(120, 32)).unwrap();
            term.draw(|f| a.draw(f)).unwrap();
            let light = mode == theme::Mode::Light;
            let o = crate::export::Options { appearance: if light { crate::export::Appearance::Light } else { crate::export::Appearance::Dark }, ..crate::export::Options::default() };
            let img = crate::render::rasterize_with(term.backend().buffer(), 18.0, crate::render::FONT_PATHS[0], None, &o).unwrap();
            let out = std::env::var("EYEBALL_DIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
            let path = std::path::PathBuf::from(out).join(format!("theme-{}.png", mode.name()));
            img.save(&path).unwrap();
            eprintln!("wrote {}", path.display());
        }
        theme::set_mode(theme::Mode::Dark, theme::Source::Default);
    }

    #[test]
    #[ignore]
    fn write_the_clean_preview() {
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("tabnew freeform sketch".into());
        let x = a.doc.add(ShapeKind::Box, "test", 2.0, 2.0);
        let y = a.doc.add(ShapeKind::Box, "test", 40.0, 2.0);
        a.doc.connect(RelationKind::Link, x, y).unwrap();
        let c = a.doc.add(ShapeKind::Cloud, "internet", 2.0, 12.0);
        let d = a.doc.add(ShapeKind::Cylinder, "db", 40.0, 12.0);
        let r = a.doc.connect(RelationKind::Link, c, d).unwrap();
        let mut n = a.doc.relation(r).unwrap().notation();
        n.line = crate::ontology::LineStyle::Dashed;
        n.head = crate::ontology::End::Triangle;
        a.doc.relation_mut(r).unwrap().style = Some(n);
        let mut path = std::path::PathBuf::from("/private/tmp/claude-501/-Users-circuits-code-offgriddev-vim-shapes/f7dc6f1a-b208-4bd7-9a3b-d2dd0f5427f4/scratchpad");
        path.push("clean.png");
        let o = crate::export::Options::preview();
        println!("{:?}", crate::export::write(&a.doc, "t", &o, &path));
        path.set_extension("svg");
        let o2 = crate::export::Options { format: crate::export::Format::Svg, ..crate::export::Options::preview() };
        println!("{:?}", crate::export::write(&a.doc, "t", &o2, &path));
        path.set_extension("pdf");
        let o3 = crate::export::Options { format: crate::export::Format::Pdf, ..crate::export::Options::preview() };
        println!("{:?}", crate::export::write(&a.doc, "t", &o3, &path));
    }

    #[test]
    #[ignore]
    fn render_the_start_dialog() {
        let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let mut a = App::new();
        a.on_key(keymap::Stroke::k(' ').event());
        a.on_key(keymap::Stroke::code(KeyCode::Tab).event());
        for c in "claims".chars() {
            a.on_key(keymap::Stroke::k(c).event());
        }
        a.on_key(keymap::Stroke::code(KeyCode::Tab).event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
        a.on_key(keymap::Stroke::code(KeyCode::Tab).event());
        a.on_key(keymap::Stroke::k('j').event());
        a.on_key(keymap::Stroke::code(KeyCode::Tab).event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    #[test]
    #[ignore]
    fn render_a_link_with_erd_ends_and_three_labels() {
        let mut term = Terminal::new(TestBackend::new(80, 14)).unwrap();
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("tabnew freeform erd".into());
        let x = a.doc.add(ShapeKind::Box, "Customer", 2.0, 3.0);
        let y = a.doc.add(ShapeKind::Box, "Order", 40.0, 3.0);
        let r = a.doc.connect(RelationKind::Link, x, y).unwrap();
        {
            let rel = a.doc.relation_mut(r).unwrap();
            rel.style = Some(crate::ontology::Notation { line: crate::ontology::LineStyle::Solid, tail: crate::ontology::End::Bar, head: crate::ontology::End::Crow, width: 1, color: None, route: crate::ontology::Route::Straight, end_size: crate::ontology::EndSize::Normal, opacity: 100 });
            rel.tail_label = Some("1".into());
            rel.label = Some("places".into());
            rel.head_label = Some("n".into());
        }
        a.set_cursor(x);
        a.on_key(keymap::Stroke::code(KeyCode::Tab).event());
        a.on_key(keymap::Stroke::code(KeyCode::Tab).event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    #[test]
    #[ignore]
    fn render_the_config_panel() {
        let mut term = Terminal::new(TestBackend::new(90, 16)).unwrap();
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("idiom service".into());
        a.on_key(keymap::Stroke::k('c').event());
        a.on_key(keymap::Stroke::k('j').event());
        a.on_key(keymap::Stroke::k('j').event());
        a.on_key(keymap::Stroke::k('j').event());
        a.on_key(keymap::Stroke::k('j').event());
        a.on_key(keymap::Stroke::k('i').event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    #[test]
    #[ignore]
    fn render_reshape_handles() {
        let mut term = Terminal::new(TestBackend::new(70, 16)).unwrap();
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("add circle".into());
        a.on_key(keymap::Stroke::k('d').event());
        a.on_key(keymap::Stroke::k('b').event());
        a.on_key(keymap::Stroke::code(KeyCode::Esc).event());
        a.on_key(keymap::Stroke::k('i').event());
        a.on_key(keymap::Stroke::k('j').event());
        a.on_key(keymap::Stroke::code(KeyCode::Enter).event());
        a.on_key(keymap::Stroke::k('j').event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    #[test]
    #[ignore]
    fn render_two_tabs_with_the_grid() {
        let mut term = Terminal::new(TestBackend::new(100, 18)).unwrap();
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("idiom service".into());
        a.run_excmd("tabrename claims".into());
        a.run_excmd("tabnew freeform whiteboard".into());
        a.run_excmd("add box".into());
        a.on_key(keymap::Stroke::k('p').event());
        a.on_key(keymap::Stroke::k('l').event());
        a.on_key(keymap::Stroke::k('a').event());
        a.on_key(keymap::Stroke::k('n').event());
        a.on_key(keymap::Stroke::code(KeyCode::Esc).event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    #[test]
    #[ignore]
    fn render_basic_shapes() {
        let mut term = Terminal::new(TestBackend::new(100, 26)).unwrap();
        let mut a = App::new();
        a.loading = false;
        let mut x = 2.0;
        for k in [ShapeKind::Box, ShapeKind::Circle, ShapeKind::Diamond, ShapeKind::Cylinder, ShapeKind::Cloud, ShapeKind::Text] {
            let id = a.doc.add(k, k.name(), x, 3.0);
            x += k.default_size().0 + 4.0;
            a.set_cursor(id);
        }
        let c = a.doc.add(ShapeKind::ApplicationComponent, "Billing", 2.0, 14.0);
        a.doc.connect(RelationKind::Flow, 0, c).unwrap();
        a.doc.connect(RelationKind::Association, 5, 1).unwrap();
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }

    #[test]
    #[ignore]
    fn render_the_stack_idiom() {
        let mut term = Terminal::new(TestBackend::new(110, 42)).unwrap();
        let mut a = App::new();
        a.loading = false;
        a.run_excmd("idiom stack".into());
        a.run_excmd("kind layered".into());
        a.on_key(keymap::Stroke::code(KeyCode::Tab).event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
        a.on_key(keymap::Stroke::k('?').event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
        a.on_key(keymap::Stroke::code(KeyCode::Esc).event());
        a.run_excmd("add".into());
        a.on_key(keymap::Stroke::k('d').event());
        a.on_key(keymap::Stroke::k('a').event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
        a.on_key(keymap::Stroke::code(KeyCode::Esc).event());
        a.on_key(keymap::Stroke::code(KeyCode::Esc).event());
        a.on_key(keymap::Stroke::code(KeyCode::Enter).event());
        a.on_key(keymap::Stroke::k('j').event());
        a.on_key(keymap::Stroke::code(KeyCode::Enter).event());
        term.draw(|f| a.draw(f)).unwrap();
        println!("{}", dump(&term));
    }
}
