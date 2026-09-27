//! The start dialog — what opens when there is nothing to open.
//!
//! Started with no file, the app assumes a new diagram and asks the two questions a new one
//! needs answered, or offers to open an old one instead. One panel, always the same size, in
//! two halves: the choice on the left — new, or open — and on the right whatever that choice
//! needs. `Tab` walks the halves. For a new diagram the right half is a name, then the two
//! kinds of diagram drawn as small pictures to pick between; for opening, it is the working
//! directory, walked like any open dialog. `^Enter` confirms from anywhere once everything
//! is valid; `Enter` on the last thing does too.

use super::{chrome, theme};
use crate::ontology::{Shape, View};
use crate::shapes::{self, CurvePrimitive};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line as CLine, Points};
use ratatui::widgets::Paragraph;
use std::path::{Path, PathBuf};

pub const WIDTH: u16 = 84;
pub const HEIGHT: u16 = 24;
/// The left half: wide enough for its two words and their markers.
const LEFT_W: u16 = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    New,
    Open,
    /// One of the folders `:workbench` has opened before — the title screen's own shortcut
    /// into a team's (or a person's) library, the same list `:workbench` bare offers once
    /// you are already in.
    Workbench,
}

/// Which part of the new-diagram form has the keyboard.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NewFocus {
    Name,
    Kind,
}

/// One row of the directory listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

/// What the dialog decided, if anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// Start a new diagram, to be saved at `path`, of this kind.
    New { path: PathBuf, view: View },
    Open(PathBuf),
    /// Open this folder as the architecture workbench — an empty diagram besides, the same
    /// as `Dismiss`, since picking a workbench is not picking a diagram to start on.
    Workbench(PathBuf),
    /// Esc: go on with an unnamed, empty diagram — vim's own answer to no file.
    Dismiss,
}

pub struct State {
    pub side: Side,
    pub choice: Choice,
    pub name: String,
    /// Index into [`KINDS`].
    pub kind: usize,
    pub new_focus: NewFocus,
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    pub sel: usize,
    /// Every folder `:workbench` has opened before, most-recently-accessed first — read once
    /// from the config, the same list `:workbench` bare offers once you are already in.
    pub workbenches: Vec<PathBuf>,
    /// Index into `workbenches`.
    pub wb_sel: usize,
    pub error: Option<String>,
}

/// The two kinds, in the order they are drawn.
pub const KINDS: [View; 2] = [View::Freeform, View::Free];

impl State {
    pub fn new() -> State {
        let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut s = State {
            side: Side::Left,
            choice: Choice::New,
            name: String::new(),
            kind: 0,
            new_focus: NewFocus::Name,
            dir,
            entries: Vec::new(),
            sel: 0,
            workbenches: crate::config::load().workbenches.into_iter().map(PathBuf::from).collect(),
            wb_sel: 0,
            error: None,
        };
        s.read_dir();
        s
    }

    /// The listing: `..` first, then folders, then diagram files, then the rest, each group
    /// sorted — the way an open dialog lays a folder out.
    fn read_dir(&mut self) {
        let mut dirs = Vec::new();
        let mut json = Vec::new();
        let mut other = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir {
                    dirs.push(name);
                } else if name.ends_with(".json") {
                    json.push(name);
                } else {
                    other.push(name);
                }
            }
        }
        for v in [&mut dirs, &mut json, &mut other] {
            v.sort_by_key(|n| n.to_ascii_lowercase());
        }
        self.entries = Vec::new();
        if self.dir.parent().is_some() {
            self.entries.push(Entry { name: "..".into(), is_dir: true });
        }
        self.entries.extend(dirs.into_iter().map(|name| Entry { name, is_dir: true }));
        self.entries.extend(json.into_iter().map(|name| Entry { name, is_dir: false }));
        self.entries.extend(other.into_iter().map(|name| Entry { name, is_dir: false }));
        self.sel = 0;
    }

    fn enter_dir(&mut self, name: &str) {
        let next = if name == ".." {
            self.dir.parent().map(Path::to_path_buf).unwrap_or_else(|| self.dir.clone())
        } else {
            self.dir.join(name)
        };
        if next.is_dir() {
            self.dir = next;
            self.read_dir();
        }
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.sel)
    }

    /// The path a new diagram would be saved at: the name as typed, `.json` added if it has
    /// no extension, relative to the directory the dialog is standing in.
    pub fn new_path(&self) -> PathBuf {
        let mut name = self.name.trim().to_string();
        if Path::new(&name).extension().is_none() {
            name.push_str(".json");
        }
        let p = PathBuf::from(&name);
        if p.is_absolute() { p } else { self.dir.join(p) }
    }

    /// Why the dialog cannot confirm yet, or nothing because it can.
    pub fn invalid(&self) -> Option<String> {
        match self.choice {
            Choice::New => {
                if self.name.trim().is_empty() {
                    return Some("give the diagram a file name".into());
                }
                let p = self.new_path();
                if p.is_dir() {
                    return Some(format!("{} is a folder", p.display()));
                }
                if p.exists() {
                    return Some(format!("{} already exists — open it instead", p.file_name().unwrap_or_default().to_string_lossy()));
                }
                None
            }
            Choice::Open => match self.selected() {
                Some(e) if !e.is_dir => None,
                _ => Some("pick a file to open".into()),
            },
            Choice::Workbench => {
                if self.workbenches.is_empty() {
                    Some("no workbench opened before — pick new or open, then :workbench <path> once you are in".into())
                } else {
                    None
                }
            }
        }
    }

    fn confirm(&mut self) -> Outcome {
        if let Some(why) = self.invalid() {
            self.error = Some(why);
            return Outcome::Nothing;
        }
        match self.choice {
            Choice::New => Outcome::New { path: self.new_path(), view: KINDS[self.kind] },
            Choice::Open => Outcome::Open(self.dir.join(&self.selected().expect("valid").name)),
            Choice::Workbench => Outcome::Workbench(self.workbenches[self.wb_sel].clone()),
        }
    }

    pub fn key(&mut self, k: KeyEvent) -> Outcome {
        self.error = None;
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        // ^Enter from anywhere. Some terminals deliver it as a bare Enter, which is why Enter
        // on the last thing in each half confirms as well.
        if k.code == KeyCode::Enter && ctrl {
            return self.confirm();
        }
        match k.code {
            KeyCode::Esc => return Outcome::Dismiss,
            // Tab walks left → the right half's parts → left again.
            KeyCode::Tab => {
                self.side = match (self.side, self.choice, self.new_focus) {
                    (Side::Left, _, _) => {
                        self.new_focus = NewFocus::Name;
                        Side::Right
                    }
                    (Side::Right, Choice::New, NewFocus::Name) => {
                        self.new_focus = NewFocus::Kind;
                        Side::Right
                    }
                    _ => Side::Left,
                };
                return Outcome::Nothing;
            }
            KeyCode::BackTab => {
                self.side = match (self.side, self.choice, self.new_focus) {
                    (Side::Right, Choice::New, NewFocus::Kind) => {
                        self.new_focus = NewFocus::Name;
                        Side::Right
                    }
                    (Side::Right, _, _) => Side::Left,
                    (Side::Left, Choice::New, _) => {
                        self.new_focus = NewFocus::Kind;
                        Side::Right
                    }
                    (Side::Left, Choice::Open | Choice::Workbench, _) => Side::Right,
                };
                return Outcome::Nothing;
            }
            _ => {}
        }
        match self.side {
            Side::Left => match k.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.choice = match self.choice {
                        Choice::New => Choice::Open,
                        Choice::Open => Choice::Workbench,
                        Choice::Workbench => Choice::New,
                    };
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.choice = match self.choice {
                        Choice::New => Choice::Workbench,
                        Choice::Open => Choice::New,
                        Choice::Workbench => Choice::Open,
                    };
                }
                KeyCode::Char('n') => self.choice = Choice::New,
                KeyCode::Char('o') => self.choice = Choice::Open,
                KeyCode::Char('w') => self.choice = Choice::Workbench,
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    self.side = Side::Right;
                    self.new_focus = NewFocus::Name;
                }
                _ => {}
            },
            Side::Right => match (self.choice, self.new_focus) {
                (Choice::New, NewFocus::Name) => match k.code {
                    KeyCode::Char(c) if !ctrl => self.name.push(c),
                    KeyCode::Backspace => {
                        self.name.pop();
                    }
                    KeyCode::Enter | KeyCode::Down => self.new_focus = NewFocus::Kind,
                    _ => {}
                },
                (Choice::New, NewFocus::Kind) => match k.code {
                    KeyCode::Char('h') | KeyCode::Char('l') | KeyCode::Char('j') | KeyCode::Char('k') | KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                        self.kind = (self.kind + 1) % KINDS.len();
                    }
                    KeyCode::Up => self.new_focus = NewFocus::Name,
                    KeyCode::Enter => return self.confirm(),
                    _ => {}
                },
                (Choice::Open, _) => match k.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if !self.entries.is_empty() {
                            self.sel = (self.sel + 1) % self.entries.len();
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if !self.entries.is_empty() {
                            self.sel = (self.sel + self.entries.len() - 1) % self.entries.len();
                        }
                    }
                    KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => self.enter_dir(".."),
                    KeyCode::Char('l') | KeyCode::Right => {
                        if let Some(e) = self.selected().cloned()
                            && e.is_dir
                        {
                            self.enter_dir(&e.name);
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(e) = self.selected().cloned() {
                            if e.is_dir {
                                self.enter_dir(&e.name);
                            } else {
                                return self.confirm();
                            }
                        }
                    }
                    KeyCode::Char('g') => self.sel = 0,
                    KeyCode::Char('G') => self.sel = self.entries.len().saturating_sub(1),
                    _ => {}
                },
                (Choice::Workbench, _) => match k.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if !self.workbenches.is_empty() {
                            self.wb_sel = (self.wb_sel + 1) % self.workbenches.len();
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if !self.workbenches.is_empty() {
                            self.wb_sel = (self.wb_sel + self.workbenches.len() - 1) % self.workbenches.len();
                        }
                    }
                    KeyCode::Enter => return self.confirm(),
                    _ => {}
                },
            },
        }
        Outcome::Nothing
    }
}

pub struct Dialog<'a> {
    pub state: &'a State,
}

impl Widget for Dialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let inner = chrome::panel(buf, area, "vim-shapes — a new diagram, or an old one", theme::t().aqua);
        let hint = match (s.side, s.choice) {
            (Side::Left, _) => " j/k choose   tab or enter → the right side   ^enter confirm   esc: an unnamed diagram",
            (Side::Right, Choice::New) => " type the name   tab → the kind   h/l pick   enter or ^enter confirm   esc",
            (Side::Right, Choice::Open) => " j/k pick   enter or l into a folder   h or backspace up   enter on a file opens   esc",
            (Side::Right, Choice::Workbench) => " j/k pick   enter or ^enter opens it   esc",
        };
        let body = chrome::hint(buf, inner, hint);
        if body.height < 4 || body.width < LEFT_W + 20 {
            return;
        }
        // The error, if any, takes the row above the hint.
        let body = match &s.error {
            Some(why) => {
                let y = body.bottom() - 1;
                buf.set_stringn(body.x, y, format!(" {why}"), body.width as usize, Style::new().fg(theme::t().red).bold());
                Rect { height: body.height - 1, ..body }
            }
            None => body,
        };
        let left = Rect { width: LEFT_W, ..body };
        let right = Rect { x: body.x + LEFT_W + 1, width: body.width - LEFT_W - 1, ..body };

        // ── the left half: the choice ───────────────────────────────────────
        let dim = s.side != Side::Left;
        let row = |choice: Choice, label: &str| -> Line {
            let on = s.choice == choice;
            let style = match (on, dim) {
                (true, false) => Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold(),
                (true, true) => Style::new().fg(theme::t().aqua).bold(),
                _ => Style::new().fg(theme::t().dim),
            };
            Line::styled(format!("{}{label:<12}", chrome::marker(on)), style)
        };
        Paragraph::new(vec![Line::raw(""), row(Choice::New, " new"), row(Choice::Open, " open"), row(Choice::Workbench, " workbench")]).render(left, buf);
        // A faint divider of spaces is the gap; the halves need no line between them.

        match s.choice {
            Choice::New => self.render_new(right, buf),
            Choice::Open => self.render_open(right, buf),
            Choice::Workbench => self.render_workbench(right, buf),
        }
    }
}

impl Dialog<'_> {
    fn render_new(&self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let on_name = s.side == Side::Right && s.new_focus == NewFocus::Name;
        let on_kind = s.side == Side::Right && s.new_focus == NewFocus::Kind;
        let name_style = if on_name { Style::new().fg(theme::t().green).bold() } else { Style::new().fg(theme::t().bright) };
        let name = Line::from(vec![
            Span::styled(format!("{}name  ", chrome::marker(on_name)), Style::new().fg(theme::t().dim)),
            Span::styled(s.name.clone(), name_style),
            Span::styled(if on_name { "█" } else { "" }, Style::new().fg(theme::t().green)),
            Span::styled(
                if s.name.trim().is_empty() { "".to_string() } else { format!("   → {}", s.new_path().file_name().unwrap_or_default().to_string_lossy()) },
                Style::new().fg(theme::t().dim),
            ),
        ]);
        Paragraph::new(vec![Line::raw(""), name]).render(area, buf);

        // The two kinds, as pictures, side by side.
        let pics_y = area.y + 3;
        let pic_h = area.height.saturating_sub(4).min(9);
        if pic_h < 4 {
            return;
        }
        let pic_w = (area.width / KINDS.len() as u16).saturating_sub(3).min(30);
        buf.set_stringn(area.x, pics_y - 1, format!("{}kind", chrome::marker(on_kind)), area.width as usize, Style::new().fg(theme::t().dim));
        for (i, view) in KINDS.iter().enumerate() {
            let x = area.x + 1 + i as u16 * (pic_w + 3);
            let rect = Rect { x, y: pics_y, width: pic_w, height: pic_h };
            let chosen = s.kind == i;
            let accent = match (chosen, on_kind) {
                (true, true) => theme::t().green,
                (true, false) => theme::t().aqua,
                _ => theme::t().dim,
            };
            let caption = format!("{}{}", chrome::marker(chosen), view.badge());
            buf.set_stringn(x, rect.bottom(), &caption, pic_w as usize, Style::new().fg(accent).bold());
            picture(*view, chosen).render(Rect { height: pic_h - 1, ..rect }, buf);
            if chosen {
                // A ground under the chosen picture, so the choice reads before its caption.
                for yy in rect.y..rect.bottom() {
                    for xx in rect.x..rect.right() {
                        buf[(xx, yy)].set_bg(theme::t().grid);
                    }
                }
            }
        }
    }

    fn render_open(&self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let on = s.side == Side::Right;
        let dir = s.dir.display().to_string();
        let path_line = Line::from(vec![
            Span::styled(chrome::marker(on).to_string(), Style::new().fg(theme::t().dim)),
            Span::styled(fit(&dir, area.width as usize - 2), Style::new().fg(theme::t().bright).bold()),
        ]);
        Paragraph::new(vec![Line::raw(""), path_line]).render(area, buf);
        let list = Rect { y: area.y + 3, height: area.height.saturating_sub(3), ..area };
        let view = list.height as usize;
        if view == 0 {
            return;
        }
        let max_off = s.entries.len().saturating_sub(view);
        let off = s.sel.saturating_sub(view / 2).min(max_off);
        let lines: Vec<Line> = s
            .entries
            .iter()
            .enumerate()
            .skip(off)
            .take(view)
            .map(|(i, e)| {
                let picked = i == s.sel;
                let text = format!("{}{}{}", chrome::marker(picked), e.name, if e.is_dir { "/" } else { "" });
                let style = match (picked, on, e.is_dir, e.name.ends_with(".json")) {
                    (true, true, ..) => Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold(),
                    (true, false, ..) => Style::new().fg(theme::t().aqua).bold(),
                    (_, _, true, _) => Style::new().fg(theme::t().aqua),
                    (_, _, _, true) => Style::new().fg(theme::t().bright),
                    _ => Style::new().fg(theme::t().dim),
                };
                Line::styled(fit(&text, list.width as usize), style)
            })
            .collect();
        Paragraph::new(lines).render(list, buf);
    }

    fn render_workbench(&self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let on = s.side == Side::Right;
        if s.workbenches.is_empty() {
            let msg = "no workbench opened before — new or open, then :workbench <path> once you are in";
            Paragraph::new(vec![Line::raw(""), Line::styled(msg, Style::new().fg(theme::t().dim))]).render(area, buf);
            return;
        }
        let heading = Line::styled("opened before, most recent first", Style::new().fg(theme::t().dim));
        let list = Rect { y: area.y + 2, height: area.height.saturating_sub(2), ..area };
        Paragraph::new(vec![Line::raw(""), heading]).render(area, buf);
        let view = list.height as usize;
        if view == 0 {
            return;
        }
        let max_off = s.workbenches.len().saturating_sub(view);
        let off = s.wb_sel.saturating_sub(view / 2).min(max_off);
        let lines: Vec<Line> = s
            .workbenches
            .iter()
            .enumerate()
            .skip(off)
            .take(view)
            .map(|(i, p)| {
                let picked = i == s.wb_sel;
                let text = format!("{}{}", chrome::marker(picked), p.display());
                let style = match (picked, on) {
                    (true, true) => Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold(),
                    (true, false) => Style::new().fg(theme::t().aqua).bold(),
                    _ => Style::new().fg(theme::t().bright),
                };
                Line::styled(fit(&text, list.width as usize), style)
            })
            .collect();
        Paragraph::new(lines).render(list, buf);
    }
}

fn fit(s: &str, room: usize) -> String {
    if s.chars().count() <= room {
        return s.to_string();
    }
    format!("…{}", s.chars().skip(s.chars().count() - room.saturating_sub(1)).collect::<String>())
}

/// The kind, as a small braille picture: a box and a circle joined by a line for freeform;
/// three stacked boxes, each in its layer's colour, for architecture.
fn picture(view: View, chosen: bool) -> impl Widget {
    let dim = move |c: Color| if chosen { c } else { theme::t().dim };
    Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([0.0, 28.0])
        .y_bounds([0.0, 8.0])
        .paint(move |ctx| {
            let flip = |y: f64| 8.0 - y;
            let draw = |ctx: &mut ratatui::widgets::canvas::Context, shape: Shape, x: f64, y: f64, w: f64, h: f64, c: Color| {
                for prim in shapes::outline(shape, x, y, w, h) {
                    match prim {
                        CurvePrimitive::Points(pts) => {
                            let p: Vec<(f64, f64)> = pts.into_iter().map(|(x, y)| (x, flip(y))).collect();
                            ctx.draw(&Points { coords: &p, color: c });
                        }
                        CurvePrimitive::Lines(ls) => {
                            for (a, b) in ls {
                                ctx.draw(&CLine { x1: a.0, y1: flip(a.1), x2: b.0, y2: flip(b.1), color: c });
                            }
                        }
                    }
                }
            };
            match view {
                View::Freeform => {
                    draw(ctx, Shape::Rectangle, 1.0, 1.0, 9.0, 4.0, dim(theme::t().sand));
                    draw(ctx, Shape::Ellipse, 17.0, 3.0, 10.0, 4.5, dim(theme::t().sand));
                    ctx.draw(&CLine { x1: 10.0, y1: flip(3.0), x2: 17.0, y2: flip(5.2), color: dim(theme::t().sand) });
                }
                _ => {
                    draw(ctx, Shape::RoundedRectangle, 2.0, 0.3, 24.0, 2.0, dim(theme::t().orange));
                    draw(ctx, Shape::Rectangle, 2.0, 3.0, 24.0, 2.0, dim(theme::t().aqua));
                    draw(ctx, Shape::Rectangle, 2.0, 5.7, 24.0, 2.0, dim(theme::t().green));
                    for x in [8.0, 20.0] {
                        ctx.draw(&CLine { x1: x, y1: flip(2.3), x2: x, y2: flip(3.0), color: dim(theme::t().structure) });
                        ctx.draw(&CLine { x1: x, y1: flip(5.0), x2: x, y2: flip(5.7), color: dim(theme::t().structure) });
                    }
                }
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(s: &mut State, code: KeyCode) -> Outcome {
        s.key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn ctrl_enter(s: &mut State) -> Outcome {
        s.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL))
    }

    /// One directory per test: two tests sharing one would race each other's cleanup.
    fn tmpdir(tag: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("vim-shapes-start-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("a.json"), "{}").unwrap();
        std::fs::write(d.join("notes.txt"), "").unwrap();
        d
    }

    #[test]
    fn tab_walks_left_then_the_name_then_the_kind_then_left_again() {
        let mut s = State::new();
        assert_eq!(s.side, Side::Left);
        press(&mut s, KeyCode::Tab);
        assert_eq!((s.side, s.new_focus), (Side::Right, NewFocus::Name));
        press(&mut s, KeyCode::Tab);
        assert_eq!((s.side, s.new_focus), (Side::Right, NewFocus::Kind));
        press(&mut s, KeyCode::Tab);
        assert_eq!(s.side, Side::Left);
    }

    #[test]
    fn j_k_cycles_through_all_three_choices_and_wraps() {
        let mut s = State::new();
        assert_eq!(s.choice, Choice::New);
        press(&mut s, KeyCode::Char('j'));
        assert_eq!(s.choice, Choice::Open);
        press(&mut s, KeyCode::Char('j'));
        assert_eq!(s.choice, Choice::Workbench);
        press(&mut s, KeyCode::Char('j'));
        assert_eq!(s.choice, Choice::New, "wraps forward");
        press(&mut s, KeyCode::Char('k'));
        assert_eq!(s.choice, Choice::Workbench, "and wraps back");
        press(&mut s, KeyCode::Char('w'));
        assert_eq!(s.choice, Choice::Workbench, "w picks it directly, like n and o do their own");
    }

    #[test]
    fn workbench_with_nothing_remembered_is_refused_and_picking_a_remembered_one_confirms() {
        let mut s = State::new();
        s.choice = Choice::Workbench;
        s.workbenches = Vec::new();
        assert_eq!(ctrl_enter(&mut s), Outcome::Nothing, "nothing to pick yet");
        assert!(s.error.as_deref().unwrap().contains("no workbench"), "{:?}", s.error);

        s.workbenches = vec![PathBuf::from("/team/architecture"), PathBuf::from("/me/personal")];
        s.side = Side::Right;
        press(&mut s, KeyCode::Char('j'));
        assert_eq!(s.wb_sel, 1);
        assert_eq!(press(&mut s, KeyCode::Enter), Outcome::Workbench(PathBuf::from("/me/personal")));
    }

    #[test]
    fn a_new_diagram_needs_a_name_and_takes_json_and_the_kind_picked() {
        let d = tmpdir("new");
        let mut s = State::new();
        s.dir = d.clone();
        s.read_dir();
        assert_eq!(ctrl_enter(&mut s), Outcome::Nothing, "no name yet");
        assert!(s.error.as_deref().unwrap().contains("file name"));
        press(&mut s, KeyCode::Tab);
        for c in "flow".chars() {
            press(&mut s, KeyCode::Char(c));
        }
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.new_focus, NewFocus::Kind);
        press(&mut s, KeyCode::Char('l'));
        assert_eq!(KINDS[s.kind], View::Free, "freeform → architecture");
        let out = press(&mut s, KeyCode::Enter);
        assert_eq!(out, Outcome::New { path: d.join("flow.json"), view: View::Free });
        // A name that is already a file is refused, with a way out.
        s.name = "a.json".into();
        assert_eq!(ctrl_enter(&mut s), Outcome::Nothing);
        assert!(s.error.as_deref().unwrap().contains("open it instead"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn opening_walks_folders_and_confirms_on_a_file() {
        let d = tmpdir("open");
        let mut s = State::new();
        s.dir = d.clone();
        s.read_dir();
        press(&mut s, KeyCode::Char('j'));
        assert_eq!(s.choice, Choice::Open);
        press(&mut s, KeyCode::Tab);
        let names: Vec<&str> = s.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["..", "sub", "a.json", "notes.txt"], "up, folders, diagrams, the rest");
        press(&mut s, KeyCode::Char('j'));
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.dir, d.join("sub"), "enter walks into a folder");
        press(&mut s, KeyCode::Char('h'));
        assert_eq!(s.dir, d, "h comes back up");
        assert_eq!(ctrl_enter(&mut s), Outcome::Nothing, "a folder is not a file");
        press(&mut s, KeyCode::Char('j'));
        press(&mut s, KeyCode::Char('j'));
        assert_eq!(press(&mut s, KeyCode::Enter), Outcome::Open(d.join("a.json")));
        assert_eq!(press(&mut s, KeyCode::Esc), Outcome::Dismiss);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_dialog_draws_both_halves() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut term = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
        let mut s = State::new();
        s.name = "flow".into();
        term.draw(|f| f.render_widget(Dialog { state: &s }, f.area())).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains("new") && out.contains("open") && out.contains("freeform") && out.contains("architecture"));
        assert!(out.contains("flow.json"), "the name says where it will be saved");
        assert!(out.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)), "the kinds are pictures");
        s.choice = Choice::Open;
        term.draw(|f| f.render_widget(Dialog { state: &s }, f.area())).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains(".."), "the listing starts with the way up");
    }
}
