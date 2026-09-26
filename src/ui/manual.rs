//! The in-app manual — `:help`, made to read like vim's own help files.
//!
//! `:help` opens at the contents page; `:help {topic}` jumps to a page — an exact tag wins,
//! otherwise a unique prefix. Inside it is a read-only buffer: `j`/`k` and `^d`/`^u` scroll,
//! `gg`/`G` jump the ends, `Tab` moves between the `|links|`, `Enter` follows one, `^o` walks
//! back the way you came.
//!
//! The prose pages are authored below; the layer, element and relation pages are generated
//! from the ontology, so `:help component` can never describe a kind the app does not have —
//! and a test asserts every `|link|` resolves to a real tag.

use super::{chrome, excmd, theme};
use crate::ontology::{self, idiom, ShapeKind, Layer, RelationKind, View, RULES};
use ratatui::prelude::*;
use ratatui::widgets::{Clear, Paragraph, Widget};
use std::fmt::Write as _;

pub struct Page {
    pub name: String,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Text,
    Tag,
    Link,
    Head,
    Dim,
}

type Seg = (String, Kind);

pub struct State {
    pages: Vec<Page>,
    tags: Vec<(String, usize)>,
    cur: usize,
    scroll: usize,
    sel_link: usize,
    history: Vec<(usize, usize, usize)>,
    /// Rows the text area had last frame, so half-page scrolling has a page to measure.
    pub view: usize,
    pending_g: bool,
    lines: Vec<Vec<Seg>>,
    /// `(line, segment index)` of every link on the page, in reading order.
    links: Vec<(usize, usize)>,
}

impl State {
    pub fn new() -> State {
        let pages = build_pages();
        let tags = build_tags(&pages);
        let mut s = State {
            pages,
            tags,
            cur: 0,
            scroll: 0,
            sel_link: 0,
            history: Vec::new(),
            view: 20,
            pending_g: false,
            lines: Vec::new(),
            links: Vec::new(),
        };
        s.reparse();
        s
    }

    fn reparse(&mut self) {
        self.lines = self.pages[self.cur].text.lines().map(parse_line).collect();
        self.links = self
            .lines
            .iter()
            .enumerate()
            .flat_map(|(li, segs)| segs.iter().enumerate().filter(|(_, (_, k))| *k == Kind::Link).map(move |(si, _)| (li, si)))
            .collect();
        self.sel_link = 0;
    }

    pub fn resolve(&self, topic: &str) -> Option<usize> {
        let want = topic.trim();
        if want.is_empty() {
            return self.tags.iter().find(|(t, _)| t == "help.txt").map(|(_, i)| *i);
        }
        if let Some((_, i)) = self.tags.iter().find(|(t, _)| t == want) {
            return Some(*i);
        }
        let low = want.to_ascii_lowercase();
        if let Some((_, i)) = self.tags.iter().find(|(t, _)| t.to_ascii_lowercase() == low) {
            return Some(*i);
        }
        self.tags.iter().find(|(t, _)| t.to_ascii_lowercase().starts_with(&low)).map(|(_, i)| *i)
    }

    pub fn goto_topic(&mut self, topic: &str) -> bool {
        match self.resolve(topic) {
            Some(i) => {
                self.jump_to(i);
                true
            }
            None => false,
        }
    }

    fn jump_to(&mut self, idx: usize) {
        if idx != self.cur {
            self.history.push((self.cur, self.scroll, self.sel_link));
        }
        self.cur = idx;
        self.scroll = 0;
        self.reparse();
    }

    pub fn back(&mut self) {
        if let Some((p, s, l)) = self.history.pop() {
            self.cur = p;
            self.reparse();
            self.scroll = s;
            self.sel_link = l.min(self.links.len().saturating_sub(1));
        }
    }

    pub fn page_name(&self) -> &str {
        &self.pages[self.cur].name
    }

    fn max_scroll(&self) -> usize {
        self.lines.len().saturating_sub(self.view.max(1))
    }

    pub fn scroll_by(&mut self, delta: isize) {
        let want = self.scroll as isize + delta;
        self.scroll = want.clamp(0, self.max_scroll() as isize) as usize;
    }

    pub fn next_link(&mut self, forward: bool) {
        let n = self.links.len();
        if n == 0 {
            return;
        }
        self.sel_link = if forward { (self.sel_link + 1) % n } else { (self.sel_link + n - 1) % n };
        // Keep the link on screen.
        let (line, _) = self.links[self.sel_link];
        if line < self.scroll || line >= self.scroll + self.view {
            self.scroll = line.saturating_sub(self.view / 2).min(self.max_scroll());
        }
    }

    /// Follow the highlighted link. Returns false if there is none or it does not resolve.
    pub fn follow(&mut self) -> bool {
        let Some(&(line, seg)) = self.links.get(self.sel_link) else { return false };
        let target = self.lines[line][seg].0.clone();
        self.goto_topic(&target)
    }

    /// One key, the reader's way. Returns true when the manual should close.
    pub fn key(&mut self, k: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let was_g = self.pending_g;
        self.pending_g = false;
        match (k.code, ctrl) {
            (KeyCode::Esc, _) | (KeyCode::Char('q'), false) => return true,
            (KeyCode::Char('j'), false) | (KeyCode::Down, _) => self.scroll_by(1),
            (KeyCode::Char('k'), false) | (KeyCode::Up, _) => self.scroll_by(-1),
            (KeyCode::Char('d'), true) | (KeyCode::PageDown, _) => self.scroll_by((self.view / 2) as isize),
            (KeyCode::Char('u'), true) | (KeyCode::PageUp, _) => self.scroll_by(-((self.view / 2) as isize)),
            (KeyCode::Char('G'), false) => self.scroll = self.max_scroll(),
            (KeyCode::Char('g'), false) => {
                if was_g {
                    self.scroll = 0;
                } else {
                    self.pending_g = true;
                }
            }
            (KeyCode::Tab, _) => self.next_link(true),
            (KeyCode::BackTab, _) => self.next_link(false),
            (KeyCode::Enter, _) | (KeyCode::Char(']'), true) => {
                self.follow();
            }
            (KeyCode::Char('o'), true) => self.back(),
            _ => {}
        }
        false
    }
}

/// Every tag in the manual — what `:help` completes against.
pub fn tags() -> Vec<String> {
    let pages = build_pages();
    let mut v: Vec<String> = build_tags(&pages).into_iter().map(|(t, _)| t).collect();
    v.sort();
    v.dedup();
    v
}

fn build_tags(pages: &[Page]) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for (i, p) in pages.iter().enumerate() {
        for line in p.text.lines() {
            for (s, k) in parse_line(line) {
                if k == Kind::Tag {
                    out.push((s, i));
                }
            }
        }
    }
    out
}

/// The little markup: `*tag*` defines a tag, `|link|` links to one, a line starting `#` is a
/// heading, a line starting `>` is dim.
fn parse_line(line: &str) -> Vec<Seg> {
    if let Some(h) = line.strip_prefix('#') {
        return vec![(h.trim().to_string(), Kind::Head)];
    }
    if let Some(d) = line.strip_prefix('>') {
        return vec![(d.to_string(), Kind::Dim)];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '*' || c == '|' {
            // A tag or link is a single token between two of the same delimiter, no spaces.
            if let Some(end) = chars[i + 1..].iter().position(|&d| d == c)
                && end > 0
                && !chars[i + 1..i + 1 + end].iter().any(|d| d.is_whitespace())
            {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), Kind::Text));
                }
                let word: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push((word, if c == '*' { Kind::Tag } else { Kind::Link }));
                i += end + 2;
                continue;
            }
        }
        cur.push(c);
        i += 1;
    }
    if !cur.is_empty() {
        out.push((cur, Kind::Text));
    }
    out
}

pub struct Manual<'a> {
    pub state: &'a State,
}

impl Widget for Manual<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_symbol(" ");
                buf[(x, y)].set_bg(theme::t().panel);
            }
        }
        let head = format!(" help — {}", self.state.page_name());
        buf.set_stringn(area.x, area.y, &head, area.width as usize, Style::new().fg(theme::t().bright).bg(theme::t().panel).bold());
        let body = Rect { y: area.y + 1, height: area.height.saturating_sub(1), ..area };
        let body = chrome::hint(buf, body, " j/k scroll   ^d/^u page   tab next link   enter follow   ^o back   q close");
        let mut lines: Vec<Line> = Vec::new();
        for (li, segs) in self.state.lines.iter().enumerate().skip(self.state.scroll).take(body.height as usize) {
            let mut spans = vec![Span::raw(" ")];
            for (si, (s, k)) in segs.iter().enumerate() {
                let selected = self.state.links.get(self.state.sel_link) == Some(&(li, si));
                spans.push(match k {
                    Kind::Text => Span::styled(s.clone(), Style::new().fg(theme::t().muted)),
                    Kind::Tag => Span::styled(s.clone(), Style::new().fg(theme::t().yellow).bold()),
                    Kind::Link if selected => Span::styled(s.clone(), Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold()),
                    Kind::Link => Span::styled(s.clone(), Style::new().fg(theme::t().aqua).underlined()),
                    Kind::Head => Span::styled(s.clone(), Style::new().fg(theme::t().bright).bold()),
                    Kind::Dim => Span::styled(s.clone(), Style::new().fg(theme::t().dim)),
                });
            }
            lines.push(Line::from(spans));
        }
        Paragraph::new(lines).render(body, buf);
    }
}

// ─── the pages ──────────────────────────────────────────────────────────────

fn page(name: &str, text: String) -> Page {
    Page { name: name.to_string(), text }
}

fn build_pages() -> Vec<Page> {
    let mut pages = vec![
        page(
            "help.txt",
            "*help.txt*  vim-shapes — architecture diagrams, driven like vim\n\
             \n\
             # contents\n\
             \n\
             |intro|        what this is, in a page\n\
             |moving|       getting around the diagram\n\
             |relations|    drawing a line that means something\n\
             |mouse|        click, drag, connect, scroll\n\
             |editing|      labels, moving, sizing, undo, picking\n\
             |layers|       the ontology: the kinds of shapes, by layer\n\
             |relation-kinds| the ways two elements join, by family\n\
             |views|        what sort of diagram this is\n\
             |tabs|         several diagrams in one file\n\
             |stack|        layers: show, hide, lock, and the drawing order\n\
             |idioms|       worked shapes to start from\n\
             |rules|        the lessons, in words\n\
             |files|        saving, opening, exporting\n\
             |render|       the diagram as a picture\n\
             |palette|      the ten colours\n\
             |ontology-kind| the ontology layer: object types, properties, links, actions\n\
             |commands|     every : command\n\
             \n\
             Press ? anywhere for the command menu — every key, and whether it works\n\
             where you are standing. :help <topic> jumps to a page; :help component\n\
             describes a kind, :help serving a relation.\n\
             \n\
             The diagram is drawn in LINE ART by default — box drawing, arc corners,\n\
             diagonals — which a screenshare can follow; :ink braille draws it in dots\n\
             instead, and :ink lines comes back. The choice is kept with the theme.\n\
             \n\
             When something looks wrong, :debug opens a panel of the app's state down the\n\
             right-hand side — the mode, what the cursor is on, the tab, the camera, the\n\
             last keys pressed and what the command table made of each — live, so a bug\n\
             reproduces in front of it. :debug again closes it.\n"
                .to_string(),
        ),
        page(
            "intro",
            "*intro*  what this is\n\
             \n\
             A diagram here is a MODEL, not a picture. Every box is a kind of thing — an\n\
             actor, a process, a component, a node — and every line is a kind of\n\
             relationship. The kinds come from the |layers| of enterprise architecture, and\n\
             which line may join which boxes is written down once, in the |rules|.\n\
             \n\
             The app never refuses a line. It DRAWS what you ask, marks what the rules\n\
             refuse, and :lint lists it. A diagram is a sketch before it is a model.\n\
             \n\
             # starting\n\
             \n\
             Opened with no file, the title screen gives way to a dialog: NEW, or OPEN.\n\
             tab walks its two halves. New asks a file name and which kind of diagram —\n\
             freeform or architecture, drawn as pictures, h/l to pick; open walks the\n\
             working directory like any open dialog, enter into a folder, h up, enter on\n\
             a file. ^enter confirms from anywhere once everything is valid; esc goes on\n\
             with an unnamed diagram. vim-shapes <file> skips all of it.\n\
             \n\
             # the first minute\n\
             \n\
             >  :add            the palette — type what you mean, enter adds it; the picked\n\
             >                  kind is drawn beside the list, in braille, in its colour\n\
             >  type a label    then esc — t types text again later\n\
             >  o               a NEW shape off this one, already related: pick its kind,\n\
             >                  then (architecture) the kind of relation, then type its label\n\
             >  :w out.json     save\n\
             \n\
             Two existing shapes are joined by hand: enter takes hold of a relation,\n\
             hjkl carries it to the other shape, enter drops it and the picker asks\n\
             which kind.\n\
             \n\
             See |moving|, |relations|, |editing|. Press ? for the keys that work here.\n"
                .to_string(),
        ),
        page(
            "moving",
            "*moving*  getting around\n\
             \n\
             There is no free cursor. The cursor is always ON an element, and hjkl hops\n\
             to the nearest element in that direction — a count hops further: 3l.\n\
             \n\
             >  h j k l     the nearest element that way\n\
             >  gg G        the first / last element, reading top to bottom\n\
             >  f           every element wears a letter; press one to jump there\n\
             >  /           search labels; n and N walk the matches\n\
             >  tab         on a shape: its first relation; on a relation: its next node\n\
             >  gd          on a relation: go to its other end\n\
             >  ^o ^i       back and forward along the jumps gd and / have made\n\
             >  zz          centre the view on the cursor\n\
             >  zh zj zk zl pan the view a step; zH zJ zK zL half a screen; shift+arrows too\n\
             >  zv          pan freely: hjkl move the view, esc comes back — the cursor stays\n\
             \n\
             The view follows the cursor, moving as little as it can. A diagram is very\n\
             often bigger than the terminal, and that is fine.\n"
                .to_string(),
        ),
        page(
            "relations",
            "*relations*  drawing a line that means something\n\
             \n\
             A relation is drawn in two moves. Enter on an element takes hold of one;\n\
             hjkl carries it to another element — the line follows, dashed; enter drops\n\
             it. Esc lets go.\n\
             \n\
             Dropping opens the PICKER: every relation kind, the ones the rules allow\n\
             between these two elements first and lit, the rest dimmed with the reason.\n\
             Both halves are grouped by family — structural, dependency, dynamic,\n\
             ontology schema, action rules, other — so from an object type the schema\n\
             is on top and the architecture's families sit together below. The first\n\
             lit row is the most specific line the rules permit, so enter twice draws\n\
             the best line. A dimmed row can still be chosen: the relation is drawn,\n\
             marked ⚠, and listed by :lint. See |relation-kinds|.\n\
             \n\
             # a shape and its relation, in one move\n\
             \n\
             ^hjkl on a shape opens a linked shape on that side, ^yubn at that corner —\n\
             from the diagram, without stepping in — and the same chords work from inside\n\
             the shape, out of its handles.\n\
             \n\
             o on a shape OPENS a new shape off it, already related: the palette asks\n\
             which kind of shape (the ones that can take a specific relation from here\n\
             lit, the rest dimmed — they can still be associated), then on an\n\
             architecture tab the picker asks which kind of relation, and then you are\n\
             typing the new shape's label. On a freeform tab there is no picker: the\n\
             two are joined by a plain |link|, and you type the label at once.\n\
             \n\
             On an architecture tab the palette opens with SUGGESTED LINES on top: a\n\
             relation and a kind together — implements → Interface, calls → Function —\n\
             grouped by family. A relation is suggested when it can reach only a few\n\
             kinds from here, because that is what makes it say something; one that\n\
             reaches a dozen (serving, triggering) is still there, through the kinds\n\
             below and the picker. Enter on a suggested line draws it outright, no\n\
             picker. Every kind of shape is still listed underneath, and typing\n\
             searches both — a relation's name finds its lines.\n\
             \n\
             # the three nodes of a link\n\
             \n\
             A relation is a stop on the walk like any shape: hjkl land on it, at the\n\
             middle of its line, and carry on from it. (tab on a shape steps onto its\n\
             first relation directly, for the one the walk would not reach.) On one,\n\
             the cursor is on one of its three NODES — the tail, the centre, the head —\n\
             and each can carry a label of its own: a cardinality, a role, a port at the\n\
             ends, the name in the middle.\n\
             \n\
             >  tab          the next node: tail, centre, head (shift-tab: back)\n\
             >  t            label this node\n\
             >  enter or r   change its kind\n\
             >  x            remove it\n\
             >  c            the sheet: its kind, look, three labels, ports, reverse\n\
             >  gd           go to the element at this end (from the centre: the far end)\n\
             >  esc          back to the element\n\
             \n\
             # a link's look\n\
             \n\
             c on any relation opens its configuration: its kind, then its look — the\n\
             line solid, dashed or dotted; its width, 1, 2 or 3 dots; either end\n\
             nothing, an arrow, an open arrow, a triangle, a diamond, a hollow diamond,\n\
             a dot, or an entity-relationship crow's foot, bar or ring; and its colour\n\
             from the |palette| — then its three labels. h/l cycle each. A kind gives\n\
             the look its default, and a setting made here becomes the relation's own;\n\
             changing the kind puts it back to the kind's. The rules judge a relation\n\
             by its kind, never by its look.\n\
             \n\
             The kinds are described in |relation-kinds|. The one lesson worth knowing\n\
             first: the layer above never touches a component directly — it uses a\n\
             |business-service| or |app-service|, which the component |realization|s.\n"
                .to_string(),
        ),
        page(
            "mouse",
            "*mouse*  click, drag, connect, scroll\n\
             \n\
             Every gesture below ends up in the same document methods hjkl and enter\n\
             already use — a shape moved by mouse is not drawn a second way, only held\n\
             by a different hand.\n\
             \n\
             >  click                 land the cursor on what is under it — a shape or\n\
             >                        a relation\n\
             >  click on ground       let go of the pick\n\
             >  drag a body           move it (the whole pick, if it is part of one)\n\
             >  hover a shape         its eight handles show — the same ones i opens\n\
             >  drag an open handle   resize that corner or edge\n\
             >  drag a patched handle move the relation's end to another handle on the\n\
             >                        same shape\n\
             >  right-drag,           the same picker enter/hjkl/enter opens, for a NEW\n\
             >  shape to shape        relation between the two\n\
             >  right-click           let go of whatever is in hand — a held handle, a\n\
             >                        relation being carried, or a pick — the same as esc\n\
             >  scroll                pan the view\n\
             \n\
             A drag that never moves anything is a click: nothing is written to the\n\
             document until something actually moves, so landing the cursor by mouse\n\
             costs exactly as little as landing it with hjkl.\n\
             \n\
             Dragging from empty ground draws a marquee; letting go picks every shape\n\
             it touches, the same set V and a motion would. See |relations| for what\n\
             the picker does once a line is dropped, and |editing| for undo — a mouse\n\
             drag is one undo step, like any edit.\n\
             \n\
             :debug's \"last mouse\" row says what the last mouse event was, the way\n\
             \"last key\" does for the keyboard — the first thing to check when a click\n\
             seems to do nothing.\n"
                .to_string(),
        ),
        page(
            "editing",
            "*editing*  labels, moving, sizing, undo, picking\n\
             \n\
             >  a            add a shape on its own — the palette, as :add\n\
             >  g            in visual mode: wrap what you picked into a grouping, named\n\
             >  gp  gu       up to the grouping this is inside; dissolve the one you are on\n\
             >  t            type the label — text (esc or enter when done)\n\
             >  T            move the label: hjkl drag it anywhere, HJKL four, 0 back, esc\n\
             >  P            the property browser, on an object type's rows (see |ontology-kind|)\n\
             >  c            the sheet: every property, in three tabs, docked on the right\n\
             >  H J K L      move the element a cell; a count moves it further\n\
             >  ^H ^J ^K ^L  shift+ctrl: move it four cells at a time; relations follow\n\
             >  - = _ +      narrower / wider / shorter / taller\n\
             >  < > { }      lean the shape a cell left / right, up / down — inside it too\n\
             >  i            step into the shape: drag the sides and corners of its box\n\
             >  d            delete the element and its relations (asks first)\n\
             >  y  p         copy the element; paste it beside the cursor\n\
             >  u  ^r        undo, redo — every edit is one step\n\
             \n\
             A |grouping| or |location| moves with everything drawn inside it. What is\n\
             inside is decided by position, so dragging an element into a box is how it\n\
             joins — there is nothing to join and nothing stored. In visual mode g wraps\n\
             what you picked in one and opens the line to name it; gp goes up to the\n\
             grouping a shape is inside, and gu dissolves the one under the cursor,\n\
             leaving what was inside where it is.\n\
             \n\
             A terminal without the kitty keyboard protocol sends shift+ctrl+h as ctrl+h.\n\
             There, ^h ^j ^k ^l on the diagram are the four-cell move as well, and a\n\
             linked shape comes from o, the corners ^y ^u ^b ^n, or i then a side. The\n\
             ? menu says so where you stand, and :debug names the kind of terminal.\n\
             \n\
             # the sheet\n\
             \n\
             c opens the SHEET, docked down the right: the properties of whatever the\n\
             cursor is on, in three tabs — STYLE, TEXT, ARRANGE — and it follows the\n\
             cursor while it is up. c gives it the keyboard; esc hands the keyboard back\n\
             to the diagram and leaves the sheet up, so you walk the graph with hjkl and\n\
             watch it retarget; c re-enters; q closes. The tab and the field you were on\n\
             are kept across shapes that have them, so tuning six links is l, l, l.\n\
             \n\
             >  j/k          the next field       tab / shift-tab    the next tab\n\
             >  i            type into the field (esc or enter leaves it)\n\
             >  h/l          cycle a choice, or step a number by one\n\
             >  H/L          step a number by ten, stopping at its limit\n\
             >  t            the label\n\
             >  enter        run an action, or step into the field\n\
             >  esc          keyboard back to the diagram     q     close\n\
             \n\
             A shape's sheet: style — LOOK (eight fill-and-line pairs: paper, grey,\n\
             blue, green, orange, yellow, red, purple), FILL (auto for the layer's own\n\
             pastel in the exports, none, or a colour), OUTLINE on or off, COLOUR of the\n\
             line (the |palette|'s ten or any hex, blank for the layer's own), LINE\n\
             (solid, dashed, dotted), STROKE (1 to 3 dots) and OPACITY (10 to 100 %). On\n\
             screen a fill tints the inside and opacity fades toward the ground; the\n\
             exports fill, dash and blend for real; text — the label, BOLD, ITALIC,\n\
             UNDERLINE (the terminal's own; the family's faces in the pictures), TEXT\n\
             COLOUR, a LABEL BAND under it, WRAP (off cuts it to one line), LABEL WIDTH\n\
             in cells (blank is the inside; wider spills out), PADDING, font and size\n\
             (which the |render| honours, since a terminal has one font), align and\n\
             valign; arrange — the kind, x, y, width, height in cells, snap to grid, the\n\
             drawing order (to front, to back, bring forward, send backward), its layer,\n\
             and its lock (see |stack|). A relation's: style — its kind and its look:\n\
             a LOOK for the line colour, its ROUTE (straight; orthogonal, out along the\n\
             longer axis, across, and in, turning where ELBOW on the arrange tab says;\n\
             curved), line, width, either end, END SIZE, colour and opacity;\n\
             text — its three labels, their font, size, bold, italic, colour and band,\n\
             and LABEL AT, per cent along the line for the centre one; arrange — the\n\
             port at each end, REVERSE, which\n\
             swaps the ends for a line drawn the wrong way round, and the same drawing\n\
             order, layer and lock. Each field left is one undo step, and the diagram\n\
             changes at once. On a narrow terminal the sheet floats, while it has the keys.\n\
             \n\
             # the diagram's own sheet\n\
             \n\
             :diagram (or c on an empty diagram) puts the sheet on the DIAGRAM itself and\n\
             holds it there until esc. Style — ROUNDED (every box gets corners), SKETCH\n\
             (every outline drawn by an unsteady hand, the same way every frame), SHADOW\n\
             (a shade under each shape, in the exports), and two grounds: BACKGROUND and\n\
             GRID COLOUR. Text — the title and the |views| kind. Arrange — the grid on or\n\
             off, GRID STYLE (auto: dots here, ruled on paper; or dots / lines for\n\
             every surface), GRID SIZE in cells — the density,\n\
             on screen and in every picture — PAGE VIEW (the page's edge on the canvas, and every\n\
             export framed to the page instead of to the shapes), PAPER (us-letter, a4,\n\
             16:9 and the rest, or WxH in cells), ORIENTATION, and the page's width and\n\
             height, which make it a custom size. All of it is the diagram's, saved in\n\
             the file, and none of it is written until set.\n\
             \n\
             # colours\n\
             \n\
             Every colour field is the same: the |palette|'s ten names, or any colour as\n\
             a hex — #4a90d9 — typed with i, or cycled with h/l. Enter on the field opens\n\
             the PICKER: the palette by name, the colours picked lately, and a grid of\n\
             swatches — twelve hues by eight lightnesses under a row of greys. hjkl move,\n\
             enter takes the swatch, i or # types a hex, x clears the colour, esc leaves.\n\
             \n\
             # reshaping\n\
             \n\
             Every shape lives in a box, and i steps INTO the shape, onto that box: a\n\
             handle appears on each side and each corner. A handle is a PORT — where a\n\
             relation attaches — and it is also what you drag to reshape. hjkl move\n\
             between the handles; a ring marks one with a relation attached.\n\
             \n\
             >  enter          on an open handle: take hold; hjkl drag a cell, ^hjkl four;\n\
             >                 enter or esc lets go. A side moves one edge, a corner two.\n\
             >  enter          on a patched handle: pick up the relation's end; hjkl to an\n\
             >                 open handle, enter places it there, esc puts it back\n\
             >  x              disconnect the relation attached at this handle\n\
             >  ^h ^j ^k ^l    open a LINKED shape on that side — the add dialog, then\n\
             >                 (architecture) the kind of relation, then its label. It lands\n\
             >                 beside that side, joined handle to facing handle.\n\
             >  ^y ^u ^b ^n    the same at a corner: up-left, up-right, down-left, down-right\n\
             >  o              the same, out of whichever handle you are standing on\n\
             >                 In the dialog a compass shows the direction; change your mind\n\
             >                 with the same ^-chord, or tab onto the compass and hjkl / yubn.\n\
             >  esc            step back out\n\
             \n\
             Each grab, move and disconnect is one undo step.\n\
             \n\
             # picking\n\
             \n\
             v starts picking; space picks the element under the cursor (or puts it\n\
             back); hjkl moves between elements as usual. Then H J K L move the picked\n\
             set together (^H ^J ^K ^L four cells; a relation to a shape left behind\n\
             follows its moved end), - = and _ + STRETCH it narrower / wider and\n\
             shorter / taller — places and sizes scale together, as a desktop group\n\
             does dragged by a corner — y copies it with the relations among it, and dd\n\
             deletes it — d asks, and a second d is the yes. c opens the sheet on the\n\
             SET: only what the shapes share, MIXED where they differ, and a value set\n\
             there is set on all of them at once, in one undo step. Its arrange tab\n\
             ends with the set's own actions: ALIGN left / centre / right / top /\n\
             middle / bottom and MATCH width / height, to the first picked shape, and\n\
             DISTRIBUTE across / down, which keep the ends and space the rest. Esc\n\
             stops picking.\n"
                .to_string(),
        ),
        page(
            "stack",
            "*stack*  layers, and the drawing order\n\
             \n\
             Every shape and relation sits on a LAYER, and the layers are a stack: the\n\
             back one is drawn first. Most diagram tools have layers and show none of\n\
             them — just \"to front\" and \"to back\". Here :layers opens a BROWSER, the\n\
             way a picture editor has one: the stack top first, each layer shown or\n\
             hidden, unlocked or locked, with what sits on it counted, the one new things\n\
             go on marked ▸, and the cursor's layer marked ←.\n\
             \n\
             >  j/k       the layer          space     show / hide it\n\
             >  l         lock / unlock it   enter     make it current: new things go on it\n\
             >  m         move the cursor's shape or relation — or the picked set — onto it\n\
             >  n         a new layer, named, on top, and current\n\
             >  r         rename it          J / K     move it down / up the stack\n\
             >  d         delete it — what was on it goes to the layer beneath\n\
             >  esc       close\n\
             \n\
             A hidden layer's shapes are not drawn and cannot be stood on, and a relation\n\
             to a hidden shape is hidden with it. A locked layer's shapes and relations\n\
             stay where and as they are: moving, reshaping, relabelling and deleting are\n\
             refused, with the reason. A single shape or relation can be locked too, on\n\
             the sheet's arrange tab.\n\
             \n\
             # the drawing order\n\
             \n\
             Within a layer things are drawn in document order, and the sheet's arrange\n\
             tab moves one about: to front, to back, bring forward, send backward — each\n\
             one enter, each one undo step. The same tab shows which layer a thing is on,\n\
             cycled with h/l, and its lock. A shape's has snap to grid as well.\n\
             \n\
             Until a second layer is made, none of this is written to the file: a diagram\n\
             with the one layer is the file it always was.\n"
                .to_string(),
        ),
        page(
            "tabs",
            "*tabs*  several diagrams in one file\n\
             \n\
             Each tab is a diagram of its own, with its own cursor, view and undo. The file\n\
             holds them all, the way a draw.io file holds pages. The header lists them,\n\
             numbered; the one you are on is the solid block.\n\
             \n\
             A tab is one of two kinds, chosen when it is made:\n\
             \n\
             >  freeform       plain shapes, like a whiteboard — no ontology, nothing refused\n\
             >  architecture   the enterprise ontology; :kind narrows it to one view\n\
             \n\
             >  ^t                   a new tab — asks which kind, then opens the line to name it\n\
             >  :tabnew <kind> [name] the same, in one line\n\
             >  :tabrename <name>     rename this tab\n\
             >  gt  gT                the next / previous tab\n\
             >  :tab N                go to tab N\n\
             >  :tabs                 list them\n\
             >  :tabclose             close this tab; asks if its diagram is unsaved\n\
             \n\
             A workspace with hundreds of tabs — what a large coArchi import turns into — is\n\
             not something gt or a remembered number reaches. See |tree|.\n\
             \n\
             # the grid\n\
             \n\
             Faint dots mark the canvas so an empty diagram still reads as a surface with a\n\
             position on it. :grid toggles them, :grid on / off says so outright. The\n\
             setting is part of the file, so a workspace opens the way it was left.\n"
                .to_string(),
        ),
        page(
            "render",
            "*render*  the diagram as a picture\n\
             \n\
             :render <out.png> writes this tab as a PNG. Not a redrawing: the CELLS, exactly\n\
             as the terminal shows them — every glyph, braille included — rasterised through\n\
             a real monospace font, so the thing on screen and the thing in the file are\n\
             one rendering. The whole diagram is framed by its bounds with a cell of margin,\n\
             no chrome, no cursor; the grid is drawn if it is on.\n\
             \n\
             >  :render out.png            20 px per em, the first monospace font found\n\
             >  :render out.png 32         bigger type, a bigger picture\n\
             >  :render out.png 20 <font>  a font file of your own\n\
             >  vim-shapes --render <file> <out.png> [--tab N] [--px 20] [--font <path>]\n\
             \n\
             Most monospace fonts have no braille, so a small stack of fonts is tried per\n\
             glyph — the terminal's own fallback rule — and the symbol fonts on the machine\n\
             draw what the main font cannot.\n\
             \n\
             \\ PRESENTS: the diagram alone on the screen, no header, footer, grid or\n\
             cursor, framed the way the rendering is. esc or \\ comes back.\n\
             \n\
             # V — preview\n\
             \n\
             V renders the tab to a PNG in a scratch file and opens it with whatever\n\
             opens pictures on this machine. It is the CLEAN picture, the way a desktop\n\
             diagram tool draws one: white paper, a ruled grid, solid outlines, filled\n\
             arrowheads, sans-serif labels. Nothing is asked: the diagram sets the size,\n\
             each shape its own font and size.\n\
             \n\
             # :export — the dialog\n\
             \n\
             :export alone opens the export dialog, the questions any diagram tool's\n\
             image dialog asks: the FORMAT — png, svg, pdf, html, or xml (a draw.io file,\n\
             to edit on); the STYLE — terminal (the braille picture, the terminal's own)\n\
             or clean (the drawn one V previews); the FILE, named after the tab; the ZOOM,\n\
             with the width and height it gives shown and typeable; a TRANSPARENT ground;\n\
             a light or dark APPEARANCE; the BORDER in cells; the GRID. j/k, i to type,\n\
             h/l to cycle, enter exports. :export <file.svg> skips the dialog and writes\n\
             the format the extension names at the defaults.\n"
                .to_string(),
        ),
        page(
            "files",
            "*files*  saving, opening, exporting\n\
             \n\
             >  :w [path]         save — to the file opened, or the path given\n\
             >  :o <path>         open\n\
             >  :import <path>    a draw.io file, an ArchiMate exchange file, or a coArchi folder\n\
             >  :n                start an empty diagram\n\
             >  :q                quit; asks if there is unsaved work. :q! does not\n\
             >  ZZ  ZQ            :wq and :q!, as vim chords\n\
             >  :export           the export dialog — png, svg, pdf, xml (draw.io), html\n\
             >  :export <file>    the format the extension names, at the defaults\n\
             >  V                 preview: the tab as a PNG, opened at once\n\
             \n\
             The file holds every tab (see |tabs|). It is JSON, pretty-printed, and names kinds by their slug —\n\
             \"kind\": \"business_process\" — so it diffs and can be edited by hand. A file\n\
             that names an element that is not there is refused whole rather than opened\n\
             half-broken.\n\
             \n\
             :import reads three other things back, replacing this diagram — a directory\n\
             means coArchi, otherwise the two file formats are told apart by content, not\n\
             extension. All three are generous, not exact.\n\
             \n\
             A draw.io file — one this app exported, or one from the desktop tool itself.\n\
             A cell this app wrote comes back as the exact kind it left as; a shape from a\n\
             genuine diagrams.net file is guessed at from its own stencil, and an unrecognized\n\
             one becomes a plain box. Neither a shape's paint nor a relation's line weight\n\
             survives the trip — only its kind, its label and its box.\n\
             \n\
             An ArchiMate model exchange file — what Archi writes with File > Export > Model\n\
             to Open Exchange Format — or a coArchi model repository, the same model kept as\n\
             a folder of small files under git instead (point :import at the repository or\n\
             its model folder either way). Either format's own element and relation names\n\
             mostly line up with this ontology's — a BusinessActor comes back as one — and\n\
             what doesn't (a newer ArchiMate 3.x type, a junction, Contract, Representation,\n\
             and a few others this ontology has no counterpart for) becomes a plain box\n\
             carrying its ArchiMate type in its label, so nothing is silently dropped. Views\n\
             become one tab each, placed where the file said; whatever was never put on a\n\
             view — and everything, if there are no views at all — lands on its own tab,\n\
             arranged by layer. A coArchi import also keeps the repository's own folders —\n\
             see |tree| for browsing hundreds of them.\n\
             \n\
             Either way, run :lint afterward to see what the rules make of what came in.\n\
             \n\
             Unsaved work is a COMPARISON against what was last saved, not a flag: undo\n\
             back to the saved state and the diagram is clean again.\n\
             \n\
             >  vim-shapes --check <file>    lint a file and exit\n\
             >  vim-shapes --ontology        the whole ontology, as JSON\n"
                .to_string(),
        ),
        page(
            "tree",
            "*tree*  browsing a coArchi import's own folders\n\
             \n\
             A coArchi model repository organizes its views into folders — Business,\n\
             Application, whatever the modeller made underneath. A large one imports into\n\
             hundreds of tabs, and neither the header nor gt / :tab N is a way to find one\n\
             of them: :tree is. It folds and searches the same folders the repository had,\n\
             and opens on a folder only if a view is somewhere under it — one holding\n\
             nothing but elements never diagrammed has nowhere to send you, so it does not\n\
             appear at all.\n\
             \n\
             >  type          search — every folder and view along a matching path, unfolded\n\
             >  ↑ / ↓         move the selection\n\
             >  enter  →      open a folder; on a view, jump to its tab and close\n\
             >  ←             fold a folder\n\
             >  esc           close, keeping the tab you were already on\n\
             \n\
             The tree is not saved — it is a memory of how the file just read in was laid\n\
             out, not part of the diagram itself, so it is there only until the next :open,\n\
             :new, or import of something else.\n"
                .to_string(),
        ),
    ];

    // layers
    let mut t = String::from("*layers*  the ontology: the kinds of shapes\n\n");
    t.push_str("Every element lives on one layer and is one of three sorts of thing: ACTIVE\n\
                structure (something that acts), BEHAVIOUR (something that happens), or\n\
                PASSIVE structure (something acted upon). The layer says where; the sort\n\
                says what for; and the |rules| turn on the pairing.\n\
                \n\
                The last layer, |sketch|, is plain shapes: a box, a circle, a cylinder, a\n\
                note. They mean whatever their label says, the rules leave them alone, and\n\
                they are offered in every view — for the parts of a picture the ontology\n\
                has no word for.\n\n");
    for l in Layer::ALL {
        let _ = writeln!(t, "# |{}|  — {}", l.name(), l.tagline());
        for k in ShapeKind::ALL.iter().filter(|k| k.layer() == l) {
            let _ = writeln!(t, "  |{:<22}| {}", k.slug(), k.tagline());
        }
        t.push('\n');
    }
    pages.push(page("layers", t));

    for l in Layer::ALL {
        let mut t = format!("*{}*  {}\n\n", l.name(), l.tagline());
        if let Some(s) = l.storey() {
            let _ = writeln!(t, "Storey {s} of the core stack: realized by the layer below, serving the layer above.\n");
        }
        t.push_str("# kinds\n\n");
        for k in ShapeKind::ALL.iter().filter(|k| k.layer() == l) {
            let _ = writeln!(t, "|{:<22}| {:<10} {}", k.slug(), k.category().name().split(' ').next().unwrap_or(""), k.tagline());
        }
        t.push_str("\nSee |layers| for the others.\n");
        pages.push(page(l.name(), t));
    }

    // elements
    for k in ShapeKind::ALL {
        let mut t = format!("*{}*  {}\n\n", k.slug(), k.name());
        let _ = writeln!(t, "layer |{}| · {} · drawn as a {}\n", k.layer().name(), k.category().name(), k.shape().name());
        for l in chrome::wrap(k.summary(), 76) {
            t.push_str(&l);
            t.push('\n');
        }
        t.push_str("\n# relations it can start\n\n");
        for r in RelationKind::ALL {
            if r == RelationKind::Association {
                continue;
            }
            let targets: Vec<&str> = ShapeKind::ALL.iter().filter(|d| ontology::allowed(r, k, **d).is_ok()).map(|d| d.slug()).collect();
            if !targets.is_empty() {
                let _ = writeln!(t, "|{}| → {}", r.name(), targets.join(", "));
            }
        }
        t.push_str("\n# relations it can receive\n\n");
        for r in RelationKind::ALL {
            if r == RelationKind::Association {
                continue;
            }
            let sources: Vec<&str> = ShapeKind::ALL.iter().filter(|s| ontology::allowed(r, **s, k).is_ok()).map(|s| s.slug()).collect();
            if !sources.is_empty() {
                let _ = writeln!(t, "|{}| ← {}", r.name(), sources.join(", "));
            }
        }
        t.push_str("\n|association| joins anything to anything, and says the least.\n");
        pages.push(page(k.slug(), t));
    }

    // relation kinds
    let mut t = String::from("*relation-kinds*  the ways two elements join, family by family\n\n");
    for f in ontology::Family::ALL {
        let _ = writeln!(t, "# {} — {}\n", f.name(), f.tagline());
        for r in RelationKind::ALL.into_iter().filter(|r| r.family() == f) {
            let _ = writeln!(t, "|{:<15}| {:<20} {}", r.name(), r.verb(), r.tagline());
        }
        t.push('\n');
    }
    t.push_str("\nEach one's page lists which kinds it may join. The rules are advisory: a\n\
                refused relation is drawn and marked, and :lint lists it. See |rules|.\n");
    pages.push(page("relation-kinds", t));

    for r in RelationKind::ALL {
        let mut t = format!("*{}*  A {} B\n\n", r.name(), r.verb());
        for l in chrome::wrap(r.summary(), 76) {
            t.push_str(&l);
            t.push('\n');
        }
        let n = r.notation();
        let _ = writeln!(
            t,
            "\n# notation\n\na {} line; {} at the start, {} at the end.\n",
            n.line.name(),
            n.tail.describe(),
            n.head.describe()
        );
        if !matches!(r, RelationKind::Association | RelationKind::Link) {
            let from: Vec<&str> = ShapeKind::ALL
                .iter()
                .filter(|s| ShapeKind::ALL.iter().any(|d| ontology::allowed(r, **s, *d).is_ok()))
                .map(|s| s.slug())
                .collect();
            t.push_str("# may start from\n\n");
            for l in chrome::wrap(&from.join(", "), 76) {
                t.push_str(&l);
                t.push('\n');
            }
            t.push_str("\nEach kind's own page says exactly what it may reach — :help component.\n");
        }
        pages.push(page(r.name(), t));
    }

    // the palette
    let mut t = String::from("*palette*  the ten colours\n\n\
        A shape's outline and a link's line can each be painted in one of ten named\n\
        colours, set in its configuration (c). Named rather than hex, because a colour\n\
        here is meant to MEAN something — a warning, a path, a group — and the values\n\
        are the theme's own accents, so a painted shape sits in the same picture as an\n\
        unpainted one. Blank is the default: a layer's colour for a shape, grey for a\n\
        link. The draw.io export keeps them.\n\n");
    for p in crate::ontology::Paint::ALL {
        let _ = writeln!(t, ">  {:<8} {}", p.name(), p.hex());
    }
    pages.push(page("palette", t));

    // views
    let mut t = String::from("*views*  what sort of diagram this is\n\n\
        :kind <view> says what the diagram is about, and the palette then offers only\n\
        the layers that belong. Free — the default — offers everything.\n\n");
    for v in View::ALL {
        let _ = writeln!(t, "*view-{}*  {}", v.name(), v.tagline());
        let _ = writeln!(t, "    layers: {}\n", v.layers().iter().map(|l| l.name()).collect::<Vec<_>>().join(", "));
    }
    t.push_str(":lint lists any element outside the view — listed, never refused.\n");
    pages.push(page("views", t));

    // idioms
    let mut t = String::from("*idioms*  worked shapes to start from\n\n\
        :idiom <name> stamps one into the diagram beside the cursor. Every idiom is\n\
        built and linted by the test suite, so what it teaches is a shape the rules pass.\n\n");
    for i in idiom::IDIOMS {
        let _ = writeln!(t, "*idiom-{}*  {}\n", i.name, i.tagline);
        for l in chrome::wrap(i.story, 76) {
            t.push_str(&l);
            t.push('\n');
        }
        t.push('\n');
        for (a, k, label) in i.elements {
            let _ = writeln!(t, ">  {a:<12} |{}|  {label}", k.slug());
        }
        for (k, f, to) in i.relations {
            let _ = writeln!(t, ">  {f} |{}| {to}", k.name());
        }
        t.push('\n');
    }
    pages.push(page("idioms", t));

    // rules
    let mut t = String::from("*rules*  the lessons, in words\n\n");
    for (n, r) in RULES.iter().enumerate() {
        let _ = writeln!(t, "# {}", n + 1);
        for l in chrome::wrap(r, 76) {
            t.push_str(&l);
            t.push('\n');
        }
        t.push('\n');
    }
    pages.push(page("rules", t));

    // commands
    let mut t = String::from("*commands*  every : command\n\n\
        Bare keys are grammar — moving, relating, editing — and ? lists them where you\n\
        stand. Everything else is a : command, tab-completed, with the argument completed\n\
        too: :add completes kinds, :kind completes views, :o completes paths.\n\n");
    for c in excmd::COMMANDS {
        let aliases = if c.aliases.is_empty() { String::new() } else { format!(" ({})", c.aliases.iter().map(|a| format!(":{a}")).collect::<Vec<_>>().join(" ")) };
        let _ = writeln!(t, ">  :{:<9} {}{}", c.name, c.what, aliases);
    }
    t.push_str("\nA ! after :q or :o discards unsaved work without asking.\n");
    pages.push(page(
        "ontology-kind",
        "*ontology-kind*  the ontology layer of the architecture\n\
         \n\
         The architecture has an ONTOLOGY LAYER: a data platform's ontology, in Palantir\n\
         Foundry's own words, beside the business it twins and the applications that read\n\
         it. :kind ontology narrows the palette to that layer with business and application;\n\
         the free view offers it with everything else. The palette offers:\n\
         \n\
         >  object type       the schema of an entity or event, with its properties\n\
         >  interface         the shape object types share — dashed; implemented, extended\n\
         >  action type       a set of edits made at once — a step, with its parameters\n\
         >  function          code an action calls\n\
         >  shared property   one property definition used on many types\n\
         >  value type        a field type with meaning and constraints\n\
         >  datasource        what backs an object type\n\
         >  object type group a box around the object types it groups\n\
         \n\
         The relations: LINK-TYPE between object types, its ends the cardinality — a bar for\n\
         one, a crow's foot for many, one-to-many by default; its centre label the name and\n\
         its end labels the API name on each side. IMPLEMENTS (object type → interface,\n\
         dashed, hollow head), EXTENDS (interface → interface), an action's verbs CREATES,\n\
         MODIFIES, DELETES, LINKS, UNLINKS (dotted arrows to the types it touches), CALLS\n\
         (action → function), USES (a type → a shared property or value type), BACKED-BY\n\
         (object type → datasource). Between two ontology types the architecture's relations\n\
         are refused, and the ontology's elsewhere, each with the reason. Where the two meet:\n\
         an object type REALIZES the business object it is the twin of (a datasource realizes\n\
         a data object), the application layer ACCESSES object types, and an action or a\n\
         function SERVES the process that invokes it.\n\
         \n\
         # properties\n\
         \n\
         An object type is its properties: a box with a header and a compartment of rows —\n\
         a mark, the name, the base type. ⚿ the primary key, ✎ the title, ✱ shared, [] an\n\
         array, ‹Email› a value type, * a required parameter, a red ✗ deprecated. P (or :props)\n\
         on an object type, an interface or an action type opens the PROPERTY BROWSER,\n\
         docked above the sheet; the box grows to hold its rows.\n\
         \n\
         >  j/k          the row\n\
         >  n            a new row below — type its name, enter\n\
         >  r            rename\n\
         >  t   T        the base type: t cycles, T types one by name or prefix\n\
         >  p   l        the primary key; the title — one of each, the mark moves\n\
         >  s   [   *    shared; an array; required (a parameter)\n\
         >  v   a        the value type; the API name\n\
         >  J/K          move the row down / up\n\
         >  d            delete\n\
         >  esc          back to the diagram\n\
         \n\
         The sheet adds API NAME, PLURAL, STATUS (experimental draws dashed; deprecated fades\n\
         and wears a red ✗ — an x on paper) and VISIBILITY.\n\
         \n\
         # documentation\n\
         \n\
         :export ontology.md writes the diagram as a reference page — object types with\n\
         property tables, link types with cardinality, interfaces with implementers,\n\
         actions with parameters and rules. :export ontology.json writes a plain definition\n\
         in the SDK's casing. Draw.io gets UML class cells. :lint and --check add the\n\
         schema's checks: no primary key, two of them, a title that is not a string, an\n\
         unnamed link type, an interface nothing implements, an action with no rule.\n"
            .to_string(),
    ));
    pages.push(page("commands", t));

    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_link_resolves_to_a_real_tag() {
        let pages = build_pages();
        let tags = build_tags(&pages);
        for p in &pages {
            for line in p.text.lines() {
                for (s, k) in parse_line(line) {
                    if k == Kind::Link {
                        assert!(tags.iter().any(|(t, _)| *t == s), "page {}: |{s}| points nowhere", p.name);
                    }
                }
            }
        }
    }

    #[test]
    fn tags_are_single_tokens_and_unique() {
        let pages = build_pages();
        let tags = build_tags(&pages);
        for (i, (t, _)) in tags.iter().enumerate() {
            assert!(!t.contains(' '), "tag {t:?} has a space");
            assert!(!tags[i + 1..].iter().any(|(o, _)| o == t), "tag {t:?} is defined twice");
        }
    }

    #[test]
    fn help_opens_at_the_contents_and_a_topic_resolves_by_prefix() {
        let mut s = State::new();
        assert_eq!(s.page_name(), "help.txt");
        assert!(s.goto_topic("compon"));
        assert_eq!(s.page_name(), "component");
        assert!(s.goto_topic("Serving"));
        assert_eq!(s.page_name(), "serving");
        s.back();
        assert_eq!(s.page_name(), "component");
        assert!(!s.goto_topic("no-such-thing"));
    }

    #[test]
    fn tab_and_enter_follow_a_link() {
        let mut s = State::new();
        s.next_link(true);
        s.next_link(false);
        assert!(s.follow(), "the first link on the contents page resolves");
        assert_eq!(s.page_name(), "intro");
    }

    #[test]
    fn every_element_and_relation_kind_has_a_page() {
        let s = State::new();
        for k in ShapeKind::ALL {
            assert!(s.resolve(k.slug()).is_some(), "no page for {}", k.slug());
        }
        for r in RelationKind::ALL {
            assert!(s.resolve(r.name()).is_some(), "no page for {}", r.name());
        }
    }
}
