//! Every command, written down once.
//!
//! This table is the single source of truth for *what the app can do and when*. The menu
//! (`?`) lists it, the footer summarises it, and `on_key` refuses from it. So a command cannot
//! be advertised in the menu and then sit inert under the cursor, nor carry one explanation in
//! the help and a different one in the status line — the two are the same string.
//!
//! ## A command is one key, not one meaning
//!
//! `Enter` means three different things depending on what the cursor is on: it starts a
//! relation, drops one, or changes the kind of one. Listing it three times would be a
//! reference that teaches none of them, and would leave the dispatcher with three candidate
//! refusals to choose between. So a `Cmd` is keyed by the *keystroke*, and both its
//! description and its availability are functions of where you are.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Which list a command appears under in the menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Move,
    Relate,
    Edit,
    Diagram,
    Tab,
    File,
    Mode,
}

impl Section {
    pub fn name(self) -> &'static str {
        match self {
            Section::Move => "getting around",
            Section::Relate => "relations",
            Section::Edit => "editing",
            Section::Diagram => "the diagram",
            Section::Tab => "tabs",
            Section::File => "files",
            Section::Mode => "modes",
        }
    }

    /// A line under the heading, for the things a list of keys cannot say.
    pub fn note(self) -> &'static [&'static str] {
        match self {
            Section::Relate => &[
                "a relation is drawn in two moves: enter on an element takes hold of one,",
                "hjkl carries it to another element, enter drops it. The picker then asks",
                "which KIND it is — the kinds the rules allow first, the rest dimmed with",
                "the reason. A refused kind is still drawn, and marked, and :lint lists it.",
                "",
                "a relation is a stop on the walk like any shape: hjkl land on it. On one,",
                "tab walks its three nodes, enter changes its kind, x removes it, gd jumps",
                "to the element at this end.",
            ],
            Section::Diagram => &[
                "adding, arranging and checking are ex-commands: :add opens the palette",
                "(or :add <kind> skips it), :idiom stamps a worked shape, :layout arranges",
                "everything by layer or by flow, :kind sets what sort of diagram this is,",
                ":lint lists what the rules refuse. :help is the manual. :debug opens a",
                "panel of the app's state — the mode, the cursor, what the last keys did.",
            ],
            Section::Tab => &[
                "each tab is a diagram of its own — FREEFORM (plain shapes, a whiteboard)",
                "or ARCHITECTURE (the ontology, narrowed by :kind). The file holds them",
                "all. :tabnew, :tabrename <name>, :tabclose, :tab N, :tabs.",
            ],
            Section::File => &[
                ":w [path] saves, :o <path> opens, :n starts fresh, :q quits (asks if",
                "there is unsaved work; :q! does not), :export <path.drawio> writes a",
                "draw.io file. ZZ and ZQ are the vim chords for :wq and :q!.",
            ],
            _ => &[],
        }
    }

    pub const ALL: [Section; 7] = [
        Section::Move,
        Section::Relate,
        Section::Edit,
        Section::Diagram,
        Section::Tab,
        Section::File,
        Section::Mode,
    ];
}

/// Which keyboard the app is currently wearing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    /// Picking out elements with space, to act on all of them at once.
    Visual,
    /// Typing a label. The letters are text, and the only commands are the ones that end it.
    Insert,
    /// Reshaping an element: the cursor is on one of the eight handles of its box, and hjkl
    /// either walk the handles or, with one in hand, drag it.
    Reshape,
    /// Presenting: the diagram alone, no chrome, no grid, no cursor — as `:render` writes it.
    Present,
    /// Moving a label: hjkl drag the cursor's label anywhere, the outside of its shape
    /// included.
    Text,
    /// Panning: hjkl move the view, and the cursor stays where it was.
    View,
    /// The sheet has the keyboard: its fields, not the diagram, take the keys.
    Sheet,
    /// The manual: a surface of its own, with a reader's keys.
    Manual,
}

/// Whether the cursor is on the element itself, or on one of its relations.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Body,
    Relation,
}

/// What the cursor is sitting on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Spot {
    pub focus: Focus,
    /// The element is a grouping or a location — moving it carries what is inside.
    pub composite: bool,
    /// How many relations touch this element — what `tab` has to walk.
    pub relations: usize,
    /// The focused relation is one the rules refuse.
    pub refused: bool,
}

/// Everything about where you are that gates a command.
///
/// A plain snapshot, deliberately: a predicate that could reach into the `App` could also
/// *change* it, and asking "what can I do here" must never do anything.
#[derive(Clone, Copy, Debug)]
pub struct Where {
    pub mode: Mode,
    /// `None` when the diagram is empty.
    pub on: Option<Spot>,
    /// A relation is in hand, waiting to be dropped.
    pub holding: bool,
    /// Something has been yanked and not yet pasted.
    pub carrying: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub can_back: bool,
    pub can_fwd: bool,
    /// How many elements are picked in visual mode.
    pub picked: usize,
    /// There is a search pattern to repeat.
    pub searched: bool,
    pub elements: usize,
    pub dirty: bool,
    /// Reshaping, and a handle is in hand — hjkl drag it rather than walking to the next.
    pub held: bool,
    /// Reshaping, and the handle under the cursor has a relation attached.
    pub patched: bool,
    /// Reshaping, and a relation's end has been picked up to move to another handle.
    pub moving_end: bool,
    /// How many tabs there are — `gt` has nothing to cycle through with one.
    pub tabs: usize,
    /// The terminal cannot tell shift+ctrl from ctrl (no kitty keyboard protocol), so on
    /// the diagram ^hjkl are the move, and the linked-shape chords are the corners.
    pub plain_keys: bool,
    /// The cursor's element is inside a grouping — there is a parent to go up to.
    pub in_group: bool,
}

impl Where {
    fn normal(&self) -> bool {
        self.mode == Mode::Normal
    }
    fn on_body(&self) -> bool {
        self.on.is_some_and(|s| s.focus == Focus::Body)
    }
    fn on_relation(&self) -> bool {
        self.on.is_some_and(|s| s.focus == Focus::Relation)
    }
    /// The cursor is on a grouping — something that can be dissolved.
    fn on_group(&self) -> bool {
        self.on.is_some_and(|s| s.composite && s.focus == Focus::Body)
    }
}

/// Whether a command can be run here — and if not, why not. The reason is the entire point:
/// "unavailable" teaches nothing; "put the cursor on a relation — tab" teaches both what the
/// command is for and what to do next.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Avail {
    Yes,
    No(&'static str),
}

impl Avail {
    pub fn ok(self) -> bool {
        self == Avail::Yes
    }

    /// Both must hold; the first reason that fails is the one given.
    pub fn and(self, other: Avail) -> Avail {
        match self {
            Avail::Yes => other,
            no => no,
        }
    }
}

const fn need(c: bool, why: &'static str) -> Avail {
    if c { Avail::Yes } else { Avail::No(why) }
}

/// One keystroke. `ctrl` is matched exactly: `o` and `^o` are different commands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stroke {
    pub code: KeyCode,
    pub ctrl: bool,
    /// Shift, for a key that is not a letter — an arrow. A letter carries its own shift in
    /// its case, and terminals disagree about reporting it besides.
    pub shift: bool,
}

impl Stroke {
    pub const fn k(c: char) -> Stroke {
        Stroke { code: KeyCode::Char(c), ctrl: false, shift: false }
    }
    pub const fn ctrl(c: char) -> Stroke {
        Stroke { code: KeyCode::Char(c), ctrl: true, shift: false }
    }
    pub const fn code(code: KeyCode) -> Stroke {
        Stroke { code, ctrl: false, shift: false }
    }
    pub const fn shifted(code: KeyCode) -> Stroke {
        Stroke { code, ctrl: false, shift: true }
    }
    pub fn matches(&self, k: &KeyEvent) -> bool {
        let shift_ok = matches!(self.code, KeyCode::Char(_)) || k.modifiers.contains(KeyModifiers::SHIFT) == self.shift;
        k.code == self.code && k.modifiers.contains(KeyModifiers::CONTROL) == self.ctrl && shift_ok
    }
    pub fn event(&self) -> KeyEvent {
        let mut m = KeyModifiers::NONE;
        if self.ctrl {
            m |= KeyModifiers::CONTROL;
        }
        if self.shift {
            m |= KeyModifiers::SHIFT;
        }
        KeyEvent::new(self.code, m)
    }
}

/// Which two-key prefix a command lives under. A keypress arms the prefix and the *next* key
/// decides what it meant, the way vim's own `g` works.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prefix {
    G,
    /// `Z`, vim's own doubled capital chord for save-and-quit / force-quit.
    Z,
    /// `z`, for the view: `zz` centres it.
    Zz,
    /// `f`, waiting for an element's letter. While it is pending every element wears one.
    F,
}

pub struct Cmd {
    /// How the keystroke is written in the menu.
    pub keys: &'static str,
    /// What it does *here*. A function, because `Enter` does not have one answer.
    pub what: fn(&Where) -> &'static str,
    /// A word or two for the footer. Empty — which is most of them — keeps it out of the
    /// footer entirely. The footer is not a cheatsheet; `?` is. Only the commands that act
    /// directly on whatever the cursor is on earn a place along the bottom.
    pub short: &'static str,
    pub section: Section,
    /// The keystrokes that trigger it, aliases included.
    pub on: &'static [Stroke],
    pub prefix: Option<Prefix>,
    pub avail: fn(&Where) -> Avail,
    /// What to replay to run it straight from the menu. Empty means it cannot be — there is
    /// no sense in "move the cursor" from inside a menu that took the cursor keys.
    pub run: &'static [Stroke],
}

impl Cmd {
    pub fn runnable(&self) -> bool {
        !self.run.is_empty()
    }
}

const ONLY_NORMAL: &str = "only in the diagram — esc first";
const EMPTY: &str = "the diagram is empty — :add puts the first element down";
const ON_RELATION: &str = "put the cursor on a relation — tab walks them";
const ON_BODY: &str = "on a relation — esc back to the element";

#[rustfmt::skip]
pub static COMMANDS: &[Cmd] = &[
    // ── getting around ──────────────────────────────────────────────────────
    Cmd {
        keys: "h j k l",
        short: "",
        what: |w| match () {
            _ if w.mode == Mode::Sheet => "j/k the next field; h/l cycle a choice",
            _ if w.mode == Mode::Text => "drag the label a cell that way — anywhere, outside the shape too (a count: further)",
            _ if w.mode == Mode::View => "pan the view that way (a count: further)",
            _ if w.mode == Mode::Reshape && w.held => "drag the handle a cell that way",
            _ if w.mode == Mode::Reshape => "move to the next handle that way",
            _ if w.holding => "carry the relation to the element in that direction",
            _ if w.mode == Mode::Visual => "move to the element in that direction",
            _ => "move to the nearest shape or relation that way (a count hops further)",
        },
        section: Section::Move,
        on: &[Stroke::k('h'), Stroke::k('j'), Stroke::k('k'), Stroke::k('l'),
              Stroke::code(KeyCode::Left), Stroke::code(KeyCode::Down),
              Stroke::code(KeyCode::Up), Stroke::code(KeyCode::Right)],
        prefix: None,
        avail: |w| need(w.mode == Mode::View || (matches!(w.mode, Mode::Normal | Mode::Visual | Mode::Reshape | Mode::Sheet | Mode::Text) && w.on.is_some()), EMPTY),
        run: &[],
    },
    Cmd {
        keys: "^h ^j ^k ^l",
        short: "",
        what: |w| match w.held {
            true => "drag the handle four cells that way (H J K L do the same)",
            false => "open a linked shape on that side — left, below, above, right",
        },
        section: Section::Edit,
        on: &[Stroke::ctrl('h'), Stroke::ctrl('j'), Stroke::ctrl('k'), Stroke::ctrl('l'),
              Stroke::ctrl('H'), Stroke::ctrl('J'), Stroke::ctrl('K'), Stroke::ctrl('L'),
              Stroke::code(KeyCode::Backspace)],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Reshape => need(!w.moving_end, "place the end first — enter, or esc"),
            _ => Avail::No("only inside a shape — i"),
        },
        run: &[],
    },
    Cmd {
        keys: "^y ^u ^b ^n",
        short: "",
        what: |_| "open a linked shape at that corner — up-left, up-right, down-left, down-right",
        section: Section::Edit,
        on: &[Stroke::ctrl('y'), Stroke::ctrl('u'), Stroke::ctrl('b'), Stroke::ctrl('n')],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Reshape => need(!w.held && !w.moving_end, "let go first — enter, or esc"),
            _ => Avail::No("only inside a shape — i"),
        },
        run: &[],
    },
    // The same eight directions from the diagram itself, standing on a shape: the linked
    // shape opens out of that side or corner without stepping in first.
    Cmd {
        keys: "^h ^j ^k ^l",
        short: "",
        what: |_| "open a linked shape on that side — left, below, above, right — as from inside",
        section: Section::Relate,
        on: &[Stroke::ctrl('h'), Stroke::ctrl('j'), Stroke::ctrl('k'), Stroke::ctrl('l')],
        prefix: None,
        avail: |w| {
            need(!w.plain_keys, "this terminal sends shift+ctrl as ctrl, so ^hjkl move the shape here — ^yubn, o, or i then a side")
                .and(need(w.normal() && w.on_body() && !w.holding, ON_BODY))
        },
        run: &[],
    },
    Cmd {
        keys: "^y ^u ^b ^n",
        short: "",
        what: |_| "open a linked shape at that corner — as from inside",
        section: Section::Relate,
        on: &[Stroke::ctrl('y'), Stroke::ctrl('u'), Stroke::ctrl('b'), Stroke::ctrl('n')],
        prefix: None,
        avail: |w| need(w.normal() && w.on_body() && !w.holding, ON_BODY),
        run: &[],
    },
    Cmd {
        keys: "g",
        short: "",
        what: |w| match w.mode {
            Mode::Visual => "wrap what you picked into a grouping — a box around them, named",
            _ => "prefix: gg first, gd follows a relation, gp up to the grouping, gu dissolves one, gt the next tab",
        },
        section: Section::Edit,
        on: &[Stroke::k('g')],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Visual => need(w.picked > 0, "nothing picked yet — space"),
            Mode::Normal => Avail::Yes,
            _ => need(false, ONLY_NORMAL),
        },
        // A bare `g` outside visual mode is half a command, so the menu does not press it.
        run: &[],
    },
    Cmd {
        keys: "gp",
        short: "",
        what: |_| "up to the grouping this is inside — its box, label and all",
        section: Section::Edit,
        on: &[Stroke::k('p')],
        prefix: Some(Prefix::G),
        avail: |w| need(w.normal() && w.in_group, "not inside a grouping"),
        run: &[Stroke::k('g'), Stroke::k('p')],
    },
    Cmd {
        keys: "gu",
        short: "",
        what: |_| "dissolve this grouping — the box goes, what was inside stays where it is",
        section: Section::Edit,
        on: &[Stroke::k('u')],
        prefix: Some(Prefix::G),
        avail: |w| need(w.normal() && w.on_group(), "put the cursor on a grouping to dissolve it"),
        run: &[Stroke::k('g'), Stroke::k('u')],
    },
    Cmd {
        keys: "a",
        short: "add",
        what: |_| "add a shape, on its own — the palette, as :add",
        section: Section::Edit,
        on: &[Stroke::k('a')],
        prefix: None,
        avail: |w| need(w.normal() && !w.holding, "put the relation down first"),
        run: &[Stroke::k('a')],
    },
    Cmd {
        keys: "gg / G",
        short: "",
        what: |_| "jump to the first / last element, reading top to bottom",
        section: Section::Move,
        on: &[Stroke::k('G')],
        prefix: None,
        avail: |w| need(w.normal() && w.on.is_some(), EMPTY),
        run: &[Stroke::k('G')],
    },
    Cmd {
        keys: "gg",
        short: "",
        what: |_| "jump to the first element",
        section: Section::Move,
        on: &[Stroke::k('g')],
        prefix: Some(Prefix::G),
        avail: |w| need(w.normal() && w.on.is_some(), EMPTY),
        run: &[Stroke::k('g'), Stroke::k('g')],
    },
    Cmd {
        keys: "f",
        short: "",
        what: |_| "jump to an element by its letter — every element wears one until you pick",
        section: Section::Move,
        on: &[Stroke::k('f')],
        prefix: None,
        avail: |w| need(matches!(w.mode, Mode::Normal | Mode::Visual) && w.elements > 1, "nothing to jump between yet"),
        run: &[Stroke::k('f')],
    },
    Cmd {
        keys: "/",
        short: "",
        what: |_| "search labels — enter jumps to the first match",
        section: Section::Move,
        on: &[Stroke::k('/')],
        prefix: None,
        avail: |w| need(w.normal() && w.on.is_some(), EMPTY),
        run: &[Stroke::k('/')],
    },
    Cmd {
        keys: "n / N",
        short: "",
        what: |_| "the next / previous element matching the last search",
        section: Section::Move,
        on: &[Stroke::k('n'), Stroke::k('N')],
        prefix: None,
        avail: |w| need(w.normal() && w.searched, "nothing searched for yet — /"),
        run: &[Stroke::k('n')],
    },
    Cmd {
        keys: "tab",
        short: "",
        what: |w| match () {
            _ if w.mode == Mode::Sheet => "the sheet's next tab — style, text, arrange (shift: back)",
            _ if w.on_relation() => "the next node of this link — tail, centre, head (shift: back)",
            _ => "step onto this shape's first relation (shift: its last)",
        },
        section: Section::Move,
        on: &[Stroke::code(KeyCode::Tab), Stroke::code(KeyCode::BackTab)],
        prefix: None,
        avail: |w| {
            if w.mode == Mode::Sheet { return Avail::Yes; }
            if !w.normal() { return Avail::No(ONLY_NORMAL); }
            let Some(s) = w.on else { return Avail::No(EMPTY) };
            need(s.relations > 0, "no relations here yet — enter starts one")
        },
        run: &[Stroke::code(KeyCode::Tab)],
    },
    Cmd {
        keys: "gd",
        short: "",
        what: |_| "go to the element at this end of the relation (from the centre, the far end)",
        section: Section::Move,
        on: &[Stroke::k('d')],
        prefix: Some(Prefix::G),
        avail: |w| need(w.normal() && w.on_relation(), ON_RELATION),
        run: &[Stroke::k('g'), Stroke::k('d')],
    },
    Cmd {
        keys: "^o / ^i",
        short: "",
        what: |_| "back / forward along the jumps gd and / have made",
        section: Section::Move,
        on: &[Stroke::ctrl('o'), Stroke::ctrl('i')],
        prefix: None,
        avail: |w| need(w.normal() && (w.can_back || w.can_fwd), "no jumps to retrace yet"),
        run: &[Stroke::ctrl('o')],
    },
    Cmd {
        keys: "zz",
        short: "",
        what: |_| "centre the view on the cursor",
        section: Section::Move,
        on: &[Stroke::k('z')],
        prefix: Some(Prefix::Zz),
        avail: |w| need(w.normal() && w.on.is_some(), EMPTY),
        run: &[Stroke::k('z'), Stroke::k('z')],
    },
    Cmd {
        keys: "zh zj zk zl",
        short: "",
        what: |_| "pan the view a step that way; the cursor stays put (a count: further)",
        section: Section::Move,
        on: &[Stroke::k('h'), Stroke::k('j'), Stroke::k('k'), Stroke::k('l')],
        prefix: Some(Prefix::Zz),
        avail: |w| need(w.normal(), ONLY_NORMAL),
        run: &[],
    },
    Cmd {
        keys: "zH zJ zK zL",
        short: "",
        what: |_| "pan the view half a screen that way",
        section: Section::Move,
        on: &[Stroke::k('H'), Stroke::k('J'), Stroke::k('K'), Stroke::k('L')],
        prefix: Some(Prefix::Zz),
        avail: |w| need(w.normal(), ONLY_NORMAL),
        run: &[],
    },
    Cmd {
        keys: "⇧←↓↑→",
        short: "",
        what: |_| "shift+arrows: pan the view a step that way (a count: further)",
        section: Section::Move,
        on: &[Stroke::shifted(KeyCode::Left), Stroke::shifted(KeyCode::Down), Stroke::shifted(KeyCode::Up), Stroke::shifted(KeyCode::Right)],
        prefix: None,
        avail: |w| need(w.normal(), ONLY_NORMAL),
        run: &[],
    },
    Cmd {
        keys: "zv",
        short: "",
        what: |_| "pan freely: hjkl move the view, HJKL half a screen, esc comes back — the cursor stays",
        section: Section::Move,
        on: &[Stroke::k('v')],
        prefix: Some(Prefix::Zz),
        avail: |w| need(w.normal(), ONLY_NORMAL),
        run: &[Stroke::k('z'), Stroke::k('v')],
    },
    // ── relations ───────────────────────────────────────────────────────────
    Cmd {
        keys: "enter",
        short: "relate",
        what: |w| match () {
            _ if w.mode == Mode::Sheet => "run this action, or step into this field",
            _ if w.mode == Mode::Text => "leave the label here",
            _ if w.mode == Mode::View => "back to the cursor",
            _ if w.mode == Mode::Reshape && w.moving_end && w.patched => "this handle is taken — pick an open one",
            _ if w.mode == Mode::Reshape && w.moving_end => "place the relation's end at this handle",
            _ if w.mode == Mode::Reshape && w.held => "let go of the handle",
            _ if w.mode == Mode::Reshape && w.patched => "pick up the relation attached here, to move its end",
            _ if w.mode == Mode::Reshape => "take hold of this handle — then hjkl drag it",
            _ if w.holding => "drop the relation on this element, and pick its kind",
            _ if w.on_relation() => "change this relation's kind",
            _ => "take hold of a new relation from this element",
        },
        section: Section::Relate,
        on: &[Stroke::code(KeyCode::Enter)],
        prefix: None,
        avail: |w| need(matches!(w.mode, Mode::Text | Mode::View) || (matches!(w.mode, Mode::Normal | Mode::Reshape | Mode::Sheet) && w.on.is_some()), EMPTY),
        run: &[Stroke::code(KeyCode::Enter)],
    },
    Cmd {
        keys: "o",
        short: "linked shape",
        what: |w| match w.mode {
            Mode::Reshape => "open a linked shape out of THIS handle — a corner handle, at that corner",
            _ => "open a new shape off this one, already related — the palette, then the kind of relation",
        },
        section: Section::Relate,
        on: &[Stroke::k('o')],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Reshape => need(!w.held && !w.moving_end, "let go first — enter, or esc"),
            _ => need(w.normal() && w.on_body() && !w.holding, ON_BODY),
        },
        run: &[Stroke::k('o')],
    },
    Cmd {
        keys: "r",
        short: "",
        what: |_| "change this relation's kind",
        section: Section::Relate,
        on: &[Stroke::k('r')],
        prefix: None,
        avail: |w| need(w.normal() && w.on_relation() && !w.holding, ON_RELATION),
        run: &[Stroke::k('r')],
    },
    Cmd {
        keys: "x",
        short: "remove",
        what: |w| match w.mode {
            Mode::Reshape => "disconnect the relation attached at this handle",
            _ => "remove this relation",
        },
        section: Section::Relate,
        on: &[Stroke::k('x')],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Reshape => need(w.patched && !w.held && !w.moving_end, "nothing is attached at this handle"),
            _ => need(w.normal() && w.on_relation() && !w.holding, ON_RELATION),
        },
        run: &[Stroke::k('x')],
    },
    // ── editing ─────────────────────────────────────────────────────────────
    Cmd {
        keys: "t",
        short: "text",
        what: |w| match () {
            _ if w.mode == Mode::Sheet => "the label — the text tab, typing",
            _ if w.on_relation() => "type a label on this node of the link — esc or enter when done",
            _ => "type this shape's label — esc or enter when done",
        },
        section: Section::Edit,
        on: &[Stroke::k('t')],
        prefix: None,
        avail: |w| need((w.normal() || w.mode == Mode::Sheet) && w.on.is_some() && !w.holding, EMPTY),
        run: &[Stroke::k('t')],
    },
    Cmd {
        keys: "c",
        short: "sheet",
        what: |w| match (w.mode, w.on_relation(), w.on.is_some()) {
            (Mode::Visual, ..) => "the sheet, on the picked set — only what they share; a value set here is set on all of them",
            (_, true, _) => "the sheet, on this relation — its kind, look, labels and ports, in three tabs",
            (_, _, true) => "the sheet, on this shape — every property in its own unit, in three tabs",
            (_, _, false) => "the sheet, on the diagram itself — its looks, its grid, its page (:diagram, from anywhere)",
        },
        section: Section::Edit,
        on: &[Stroke::k('c')],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Visual => need(w.picked > 0, "nothing picked yet — space"),
            _ => need(w.normal() && !w.holding, "put the shape down first"),
        },
        run: &[Stroke::k('c')],
    },
    Cmd {
        keys: "i",
        short: "",
        what: |_| "type into this field — esc or enter to leave it",
        section: Section::Edit,
        on: &[],
        prefix: None,
        avail: |w| need(w.mode == Mode::Sheet, "only in the sheet — c"),
        run: &[],
    },
    Cmd {
        keys: "q",
        short: "",
        what: |_| "close the sheet",
        section: Section::Edit,
        on: &[Stroke::k('q')],
        prefix: None,
        avail: |w| need(w.mode == Mode::Sheet, "only in the sheet — c"),
        run: &[],
    },
    Cmd {
        keys: "d",
        short: "",
        what: |w| match w.mode {
            Mode::Visual => "delete the picked elements, and their relations — dd, or d then y",
            _ => "delete this element, and every relation on it (asks first)",
        },
        section: Section::Edit,
        on: &[Stroke::k('d')],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Visual => need(w.picked > 0, "nothing picked yet — space"),
            _ => need(w.normal() && w.on_body() && !w.holding, ON_BODY),
        },
        run: &[Stroke::k('d')],
    },
    Cmd {
        keys: "H J K L",
        short: "",
        what: |w| match w.mode {
            Mode::Visual => "move the picked elements a cell (a count: that many)",
            Mode::Text => "drag the label four cells that way",
            Mode::View => "pan the view half a screen that way",
            _ if w.on.is_some_and(|s| s.composite) => "move this box and everything inside it",
            _ => "move this element a cell (a count: that many)",
        },
        section: Section::Edit,
        on: &[Stroke::k('H'), Stroke::k('J'), Stroke::k('K'), Stroke::k('L')],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Visual => need(w.picked > 0, "nothing picked yet — space"),
            Mode::Reshape => need(w.held, "take hold of a handle first — enter"),
            Mode::Text | Mode::View => Avail::Yes,
            _ => need(w.normal() && w.on_body() && !w.holding, ON_BODY),
        },
        run: &[],
    },
    Cmd {
        keys: "i",
        short: "step in",
        what: |_| "step into this shape — its box, with handles on its sides and corners to drag",
        section: Section::Edit,
        on: &[Stroke::k('i')],
        prefix: None,
        avail: |w| need(w.normal() && w.on_body() && !w.holding, "a relation has no box to step into — c configures it"),
        run: &[Stroke::k('i')],
    },
    Cmd {
        keys: "P",
        short: "",
        what: |_| "the property browser: this type's rows — name, type, primary key, title, shared, array — as :props",
        section: Section::Edit,
        on: &[Stroke::k('P')],
        prefix: None,
        avail: |w| need(w.normal() && w.on_body() && !w.holding, ON_BODY),
        run: &[Stroke::k('P')],
    },
    Cmd {
        keys: "T",
        short: "",
        what: |_| "move this label: hjkl drag it anywhere — outside the shape too — HJKL four cells, 0 puts it back, esc leaves",
        section: Section::Edit,
        on: &[Stroke::k('T')],
        prefix: None,
        avail: |w| need(w.normal() && w.on.is_some() && !w.holding, EMPTY),
        run: &[Stroke::k('T')],
    },
    Cmd {
        keys: "0",
        short: "",
        what: |_| "put the label back where its alignment sets it",
        section: Section::Edit,
        on: &[Stroke::k('0')],
        prefix: None,
        avail: |w| need(w.mode == Mode::Text, "only while moving a label — T"),
        run: &[],
    },
    Cmd {
        keys: "^H ^J ^K ^L",
        short: "",
        what: |w| match (w.mode, w.plain_keys) {
            (Mode::Visual, false) => "move the picked elements four cells (shift+ctrl; a count: that many fours)",
            (Mode::Visual, true) => "move the picked elements four cells (ctrl, or shift+ctrl; a count: that many fours)",
            (_, false) => "move this element four cells (shift+ctrl; a count: that many fours)",
            (_, true) => "move this element four cells (ctrl, or shift+ctrl — this terminal cannot tell them apart)",
        },
        section: Section::Edit,
        on: &[Stroke::ctrl('H'), Stroke::ctrl('J'), Stroke::ctrl('K'), Stroke::ctrl('L')],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Visual => need(w.picked > 0, "nothing picked yet — space"),
            Mode::Text | Mode::View => Avail::Yes,
            _ => need(w.normal() && w.on_body() && !w.holding, ON_BODY),
        },
        run: &[],
    },
    Cmd {
        keys: "< > { }",
        short: "",
        what: |_| "lean the shape a cell left / right, and up / down — a rectangle becomes a parallelogram (a count: further)",
        section: Section::Edit,
        on: &[Stroke::k('<'), Stroke::k('>'), Stroke::k('{'), Stroke::k('}')],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Reshape => need(!w.held && !w.moving_end, "let go first — enter, or esc"),
            _ => need(w.normal() && w.on_body() && !w.holding, ON_BODY),
        },
        run: &[],
    },
    Cmd {
        keys: "- = _ +",
        short: "",
        what: |w| match w.mode {
            Mode::Visual => "stretch the picked set: narrower / wider by four cells, shorter / taller by two — places and sizes scale together (a count: further)",
            _ => "narrower / wider, shorter / taller",
        },
        section: Section::Edit,
        on: &[Stroke::k('-'), Stroke::k('='), Stroke::k('_'), Stroke::k('+')],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Visual => need(w.picked > 0, "nothing picked yet — space"),
            _ => need(w.normal() && w.on_body() && !w.holding, ON_BODY),
        },
        run: &[],
    },
    Cmd {
        keys: "u",
        short: "",
        what: |_| "undo — every edit is one step",
        section: Section::Edit,
        on: &[Stroke::k('u')],
        prefix: None,
        avail: |w| need(matches!(w.mode, Mode::Normal | Mode::Visual) && w.can_undo, "nothing to undo"),
        run: &[Stroke::k('u')],
    },
    Cmd {
        keys: "^r",
        short: "",
        what: |_| "redo",
        section: Section::Edit,
        on: &[Stroke::ctrl('r')],
        prefix: None,
        avail: |w| need(matches!(w.mode, Mode::Normal | Mode::Visual) && w.can_redo, "nothing to redo"),
        run: &[Stroke::ctrl('r')],
    },
    Cmd {
        keys: "y",
        short: "",
        what: |w| match w.mode {
            Mode::Visual => "copy the picked elements, with the relations among them",
            _ => "copy this element",
        },
        section: Section::Edit,
        on: &[Stroke::k('y')],
        prefix: None,
        avail: |w| match w.mode {
            Mode::Visual => need(w.picked > 0, "nothing picked yet — space"),
            _ => need(w.normal() && w.on_body() && !w.holding, ON_BODY),
        },
        run: &[Stroke::k('y')],
    },
    Cmd {
        keys: "p",
        short: "",
        what: |_| "paste what was copied, beside the cursor",
        section: Section::Edit,
        on: &[Stroke::k('p')],
        prefix: None,
        avail: |w| need(w.normal() && w.carrying, "nothing copied yet — y"),
        run: &[Stroke::k('p')],
    },
    Cmd {
        keys: "v",
        short: "",
        what: |w| match w.mode {
            Mode::Visual => "stop picking",
            _ => "start picking elements out, to move, copy or delete them together",
        },
        section: Section::Edit,
        on: &[Stroke::k('v')],
        prefix: None,
        avail: |w| need(matches!(w.mode, Mode::Normal | Mode::Visual) && w.on.is_some() && !w.holding, EMPTY),
        run: &[Stroke::k('v')],
    },
    Cmd {
        keys: "space",
        short: "pick",
        what: |_| "pick this element — or put it back",
        section: Section::Edit,
        on: &[Stroke::k(' ')],
        prefix: None,
        avail: |w| need(w.mode == Mode::Visual && w.on.is_some(), "only while picking — v"),
        run: &[],
    },
    // ── the diagram ─────────────────────────────────────────────────────────
    Cmd {
        keys: "V",
        short: "",
        what: |_| "preview — render this tab to a PNG and open it, no questions asked",
        section: Section::Diagram,
        on: &[Stroke::k('V')],
        prefix: None,
        avail: |w| need(w.normal() && w.elements > 0, EMPTY),
        run: &[Stroke::k('V')],
    },
    Cmd {
        keys: "\\",
        short: "",
        what: |w| match w.mode {
            Mode::Present => "back to editing",
            _ => "present — the diagram alone, as :render writes it; esc or \\ comes back",
        },
        section: Section::Diagram,
        on: &[Stroke::k('\\')],
        prefix: None,
        avail: |w| need(matches!(w.mode, Mode::Normal | Mode::Present), ONLY_NORMAL),
        run: &[Stroke::k('\\')],
    },
    Cmd {
        keys: ":",
        short: "",
        what: |_| "the command line — :add, :w, :layout, :help, tab-completed. Reachable from anywhere but a text field",
        section: Section::Diagram,
        on: &[Stroke::k(':')],
        prefix: None,
        // From every mode: a user must always be able to reach the line. Only a text being
        // typed — a label — owns the colon, and esc is one key away there.
        avail: |w| need(w.mode != Mode::Insert, "finish the label first — esc"),
        run: &[Stroke::k(':')],
    },
    Cmd {
        keys: "?",
        short: "commands",
        what: |_| "this menu — every command, and what it can do here",
        section: Section::Diagram,
        on: &[Stroke::k('?')],
        prefix: None,
        avail: |w| need(w.mode != Mode::Manual, "close the manual first — q"),
        run: &[Stroke::k('?')],
    },
    // ── tabs ────────────────────────────────────────────────────────────────
    Cmd {
        keys: "^t",
        short: "",
        what: |_| "a new tab — asks whether it is freeform or architecture, then its name",
        section: Section::Tab,
        on: &[Stroke::ctrl('t')],
        prefix: None,
        avail: |w| need(w.normal(), ONLY_NORMAL),
        run: &[Stroke::ctrl('t')],
    },
    Cmd {
        keys: "gt / gT",
        short: "",
        what: |_| "the next / previous tab",
        section: Section::Tab,
        on: &[Stroke::k('t'), Stroke::k('T')],
        prefix: Some(Prefix::G),
        avail: |w| need(w.normal() && w.tabs > 1, "only one tab — ^t makes another"),
        run: &[Stroke::k('g'), Stroke::k('t')],
    },
    // ── files ───────────────────────────────────────────────────────────────
    Cmd {
        keys: "ZZ",
        short: "",
        what: |w| if w.dirty { "save and quit — there is unsaved work" } else { "save and quit" },
        section: Section::File,
        on: &[Stroke::k('Z')],
        prefix: Some(Prefix::Z),
        avail: |w| need(w.normal(), ONLY_NORMAL),
        run: &[Stroke::k('Z'), Stroke::k('Z')],
    },
    Cmd {
        keys: "ZQ",
        short: "",
        what: |_| "quit without saving",
        section: Section::File,
        on: &[Stroke::k('Q')],
        prefix: Some(Prefix::Z),
        avail: |w| need(w.normal(), ONLY_NORMAL),
        run: &[Stroke::k('Z'), Stroke::k('Q')],
    },
    // ── modes ───────────────────────────────────────────────────────────────
    Cmd {
        keys: "esc",
        short: "",
        what: |w| match () {
            _ if w.mode == Mode::Insert => "finish the label",
            _ if w.mode == Mode::Sheet => "the keyboard back to the diagram — the sheet stays, and follows",
            _ if w.mode == Mode::Reshape && w.moving_end => "put the relation's end back where it was",
            _ if w.mode == Mode::Reshape && w.held => "let go of the handle",
            _ if w.mode == Mode::Reshape => "step back out of the shape",
            _ if w.mode == Mode::Text => "leave the label where it is",
            _ if w.mode == Mode::View => "back to the cursor — the view stays where you panned it",
            _ if w.mode == Mode::Present => "back to editing",
            _ if w.holding => "let go of the relation",
            _ if w.mode == Mode::Visual => "stop picking",
            _ if w.on_relation() => "back to the element",
            _ => "back out of whatever you are in",
        },
        section: Section::Mode,
        on: &[Stroke::code(KeyCode::Esc)],
        prefix: None,
        avail: |w| need(
            matches!(w.mode, Mode::Insert | Mode::Reshape | Mode::Present | Mode::Sheet | Mode::Text | Mode::View) || w.holding || w.mode == Mode::Visual || w.on_relation(),
            "nothing to back out of",
        ),
        run: &[Stroke::code(KeyCode::Esc)],
    },
    Cmd {
        keys: "j/k",
        short: "",
        what: |_| "scroll the manual (^d / ^u by half a page, gg / G to the ends)",
        section: Section::Mode,
        on: &[],
        prefix: None,
        avail: |w| need(w.mode == Mode::Manual, "only in the manual — :help"),
        run: &[],
    },
    Cmd {
        keys: "tab enter",
        short: "",
        what: |_| "in the manual: tab moves between links, enter follows one, ^o goes back",
        section: Section::Mode,
        on: &[],
        prefix: None,
        avail: |w| need(w.mode == Mode::Manual, "only in the manual — :help"),
        run: &[],
    },
];

/// What a keystroke does here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    Allow,
    Refuse(&'static str),
    Unknown,
}

/// Look a keystroke up in the table. A pending prefix selects its own two-key commands first
/// and falls through to the plain ones otherwise, which is what the dispatcher does with a
/// dangling prefix.
pub fn resolve(k: &KeyEvent, pending: Option<Prefix>, w: &Where) -> Resolved {
    if let Some(p) = pending
        && let Some(r) = find(k, Some(p), w)
    {
        return r;
    }
    find(k, None, w).unwrap_or(Resolved::Unknown)
}

fn find(k: &KeyEvent, prefix: Option<Prefix>, w: &Where) -> Option<Resolved> {
    let mut why = None;
    for c in COMMANDS.iter().filter(|c| c.prefix == prefix && c.on.iter().any(|s| s.matches(k))) {
        match (c.avail)(w) {
            Avail::Yes => return Some(Resolved::Allow),
            Avail::No(reason) => why.get_or_insert(reason),
        };
    }
    why.map(Resolved::Refuse)
}

/// Every command in a section, with what it can do here.
pub fn section(s: Section, w: &Where) -> Vec<(&'static Cmd, Avail)> {
    COMMANDS.iter().filter(|c| c.section == s).map(|c| (c, (c.avail)(w))).collect()
}

/// What the footer shows: the commands available right now that are worth a glance. Generated
/// from the same table as the menu, so it cannot promise a key the menu says is unavailable.
pub fn footer(w: &Where) -> Vec<&'static Cmd> {
    let mut v: Vec<&'static Cmd> =
        COMMANDS.iter().filter(|c| !c.short.is_empty() && (c.avail)(w).ok()).collect();
    v.sort_by_key(|c| c.keys != "?");
    v
}

#[cfg(test)]
pub(crate) fn sample_wheres() -> Vec<Where> {
    let base = Where {
        mode: Mode::Normal,
        on: None,
        holding: false,
        carrying: false,
        can_undo: false,
        can_redo: false,
        can_back: false,
        can_fwd: false,
        picked: 0,
        searched: false,
        elements: 0,
        dirty: false,
        tabs: 2,
        held: false,
        patched: false,
        moving_end: false,
        plain_keys: false,
        in_group: false,
    };
    let body = Spot { focus: Focus::Body, composite: false, relations: 0, refused: false };
    let rel = Spot { focus: Focus::Relation, composite: false, relations: 2, refused: false };
    let mut v = vec![base];
    for mode in [Mode::Normal, Mode::Visual, Mode::Insert, Mode::Manual, Mode::Reshape, Mode::Present, Mode::Sheet, Mode::Text, Mode::View] {
        for on in [None, Some(body), Some(rel)] {
            for holding in [false, true] {
                for held in [false, true] {
                    let carrying = held;
                    v.push(Where {
                        mode,
                        on,
                        holding,
                        carrying,
                        held,
                        patched: holding,
                        moving_end: carrying && !held,
                        can_undo: true,
                        can_redo: true,
                        can_back: true,
                        can_fwd: true,
                        picked: 2,
                        searched: true,
                        elements: 3,
                        ..base
                    });
                }
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant that makes the lookup unambiguous: no two commands are available on the
    /// same key in the same place.
    #[test]
    fn no_two_commands_are_available_on_one_key_in_one_place() {
        for w in sample_wheres() {
            for (i, a) in COMMANDS.iter().enumerate() {
                for b in &COMMANDS[i + 1..] {
                    if a.prefix != b.prefix || !(a.avail)(&w).ok() || !(b.avail)(&w).ok() {
                        continue;
                    }
                    let shared = a.on.iter().any(|s| b.on.contains(s));
                    assert!(!shared, "{} and {} are both live on one key at {w:?}", a.keys, b.keys);
                }
            }
        }
    }

    /// The footer is not a cheatsheet.
    #[test]
    fn the_footer_stays_short_wherever_you_stand() {
        for w in sample_wheres() {
            // Seven: the way to everything else, a shape on its own, and the five gestures
            // that act on what the cursor is on — relate, open a linked shape, text, step
            // in, configure. Nothing more.
            let n = footer(&w).len();
            assert!(n <= 7, "the footer grew to {n} commands at {w:?}");
        }
    }

    #[test]
    fn the_footer_offers_only_what_works_here() {
        let body = Spot { focus: Focus::Body, composite: false, relations: 0, refused: false };
        let w = Where { on: Some(body), ..sample_wheres()[0] };
        let keys: Vec<&str> = footer(&w).iter().map(|c| c.keys).collect();
        assert_eq!(keys[0], "?", "the way to everything else comes first");
        assert!(keys.contains(&"enter") && keys.contains(&"t") && keys.contains(&"c") && keys.contains(&"o"));
        assert!(!keys.contains(&"x"), "x removes a relation, and the cursor is on an element");
    }

    #[test]
    fn a_refusal_carries_the_reason_the_menu_shows() {
        let w = sample_wheres()[0];
        match resolve(&Stroke::k('u').event(), None, &w) {
            Resolved::Refuse(why) => assert_eq!(why, "nothing to undo"),
            _ => panic!("u on a fresh diagram is refused, with a reason"),
        }
        match resolve(&Stroke::k('!').event(), None, &w) {
            Resolved::Unknown => {}
            _ => panic!("! is not a command"),
        }
    }

    #[test]
    fn every_command_has_a_section_and_a_description_everywhere() {
        for w in sample_wheres() {
            for c in COMMANDS {
                assert!(!(c.what)(&w).is_empty(), "{} says nothing at {w:?}", c.keys);
            }
        }
    }
}
