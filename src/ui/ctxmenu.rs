//! The right-click menu — every command `?` would show in full colour here, none of the
//! ones it would dim, since a menu that lasts one click has no room to teach what you
//! cannot do, only to offer what you can.
//!
//! Not a second list of commands: `State::new` reads `keymap::COMMANDS` through the exact
//! filter `help::visible` already computes for the `?` cheatsheet, so the two can never say
//! different things about what a key does right now.
//!
//! Drawn as a dropdown at the point it was opened, not centred on the diagram — `at` is that
//! point, and `area` turns it into the clamped rect both the widget and a mouse hit-test read.
//! The mouse can drive it exactly as `j`/`k`/`enter`/`esc` do: hovering a row moves `sel` the
//! same as `j`/`k` would, a left click on a row is `enter`, and a click anywhere else — or any
//! other button — is `esc`, the way any dropdown closes when the mouse goes elsewhere.

use super::help;
use super::keymap::{Cmd, Where};
use super::{chrome, theme};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub const WIDTH: u16 = 44;

pub fn height(rows: usize) -> u16 {
    (rows as u16 + 2).clamp(3, 16)
}

/// `Cmd` carries function pointers over `Where`, not data comparable in the usual sense, so
/// this stays a plain `Copy` type rather than deriving `PartialEq`/`Debug` it would have to
/// invent for `Cmd` too — tests compare a `Run`'s command by its `keys` instead.
#[derive(Clone, Copy)]
pub enum Outcome {
    Nothing,
    Cancel,
    Run(&'static Cmd),
}

/// A row is just the command — `Cmd::title` is already the two or three words the row shows;
/// nothing here needs computing fresh the way `Cmd::what`'s full sentence would.
pub struct Row {
    pub cmd: &'static Cmd,
}

pub struct State {
    pub rows: Vec<Row>,
    pub sel: usize,
    /// The screen point it was opened at — a right-click, not the area it happens to render
    /// in — so it can be drawn as a dropdown at the mouse rather than centred on the diagram.
    pub at: (u16, u16),
    /// The same point, in world cells — what `a` and `p`, run from here, land at instead of
    /// "beside the cursor" (there may not even be one, on empty ground).
    pub world: (f64, f64),
}

impl State {
    /// Every command available right now, in the same order the `?` menu lists them —
    /// dimmed rows dropped (a right-click that opens a dead end teaches nothing a click
    /// should have to sit through), anything the menu could not replay dropped with them, and
    /// anything that isn't a complete one-shot action — see [`Cmd::menu`] — dropped too: a
    /// motion, a chord that only makes sense mid-gesture, or something a drag already does
    /// more directly has no place in a menu that lasts one click.
    pub fn new(w: &Where, at: (u16, u16), world: (f64, f64)) -> State {
        let rows = help::visible(w, "")
            .into_iter()
            .filter(|(c, a)| a.ok() && c.runnable() && c.menu)
            .map(|(c, _)| Row { cmd: c })
            .collect();
        State { rows, sel: 0, at, world }
    }

    /// Where it draws, anchored at `self.at` inside `area` — the one geometry both the widget
    /// and a mouse hit-test read, so they can never disagree about where a row actually is.
    pub fn area(&self, area: Rect) -> Rect {
        chrome::near(area, self.at, WIDTH, height(self.rows.len()))
    }

    /// Which row a screen point lands on, if it's within the list at all — not the title bar,
    /// not the hint line, not past the last row. `area` is this menu's own rect, from
    /// [`State::area`].
    pub fn row_at(&self, area: Rect, col: u16, row: u16) -> Option<usize> {
        let body = chrome::panel_body(area, true);
        if col < body.x || col >= body.right() || row < body.y || row >= body.bottom() {
            return None;
        }
        let i = self.scroll(body.height as usize) + (row - body.y) as usize;
        (i < self.rows.len()).then_some(i)
    }

    /// The first row shown, given how many fit — kept as one function so the widget's own
    /// scrolling and a hit-test's can never compute it two different ways.
    fn scroll(&self, view: usize) -> usize {
        if view == 0 {
            return 0;
        }
        self.sel.saturating_sub(view / 2).min(self.rows.len().saturating_sub(view))
    }

    pub fn key(&mut self, k: KeyEvent) -> Outcome {
        match k.code {
            KeyCode::Esc => Outcome::Cancel,
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.rows.is_empty() {
                    self.sel = (self.sel + 1) % self.rows.len();
                }
                Outcome::Nothing
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !self.rows.is_empty() {
                    self.sel = (self.sel + self.rows.len() - 1) % self.rows.len();
                }
                Outcome::Nothing
            }
            KeyCode::Enter if !self.rows.is_empty() => Outcome::Run(self.rows[self.sel].cmd),
            // A row's own key, typed rather than walked to with j/k — the same one-key
            // replay `run` gives `enter`, so a row already known by its letter runs at
            // once. `run.len() == 1` keeps this to a bare key: a chord like `gp` replays
            // two strokes and needs the `g` first, so it is never mistaken for plain `p`.
            _ => self.rows.iter().find(|r| r.cmd.run.len() == 1 && r.cmd.run[0].matches(&k)).map_or(Outcome::Nothing, |r| Outcome::Run(r.cmd)),
        }
    }
}

pub struct Dialog<'a> {
    pub state: &'a State,
}

impl Widget for Dialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let inner = chrome::panel(buf, area, "menu", theme::t().sand);
        let hint = " j/k or hover   enter or click runs it   esc";
        let body = chrome::hint(buf, inner, hint);
        if body.height == 0 {
            return;
        }
        if s.rows.is_empty() {
            Paragraph::new(Line::styled(" nothing to do here", Style::new().fg(theme::t().dim))).render(body, buf);
            return;
        }
        let view = body.height as usize;
        let off = s.scroll(view);
        let lines: Vec<Line> = s
            .rows
            .iter()
            .enumerate()
            .skip(off)
            .take(view)
            .map(|(i, row)| {
                let picked = i == s.sel;
                let text = format!("{}{:<8}{}", chrome::marker(picked), row.cmd.keys, row.cmd.title);
                let style = if picked { Style::new().fg(theme::t().inverse).bg(theme::t().sand).bold() } else { Style::new().fg(theme::t().bright) };
                Line::styled(text, style)
            })
            .collect();
        Paragraph::new(lines).render(body, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::keymap;
    use crate::ui::Mode;
    use crossterm::event::KeyModifiers;

    #[test]
    fn only_complete_one_shot_actions_are_offered_not_every_available_key() {
        let w = keymap::sample_wheres().into_iter().find(|w| w.mode == Mode::Normal && w.on.is_some()).expect("a sample with the cursor on a shape");
        let s = State::new(&w, (5, 5), (5.0, 5.0));
        assert!(!s.rows.iter().any(|r| r.cmd.keys == "gg / G"), "pure navigation has no place in a one-click menu");
        assert!(!s.rows.iter().any(|r| r.cmd.keys == "H J K L"), "a repeatable step, awkward from a menu that closes after one click");
        assert!(s.rows.iter().any(|r| r.cmd.keys == "d"), "a complete action stays");
    }

    #[test]
    fn every_row_is_available_and_replayable_here() {
        let w = keymap::sample_wheres().remove(0);
        let s = State::new(&w, (5, 5), (5.0, 5.0));
        for row in &s.rows {
            assert!((row.cmd.avail)(&w).ok(), "{} listed but not available", row.cmd.keys);
            assert!(row.cmd.runnable(), "{} listed but has nothing to replay", row.cmd.keys);
        }
    }

    #[test]
    fn moving_and_cancel() {
        let w = keymap::sample_wheres().remove(0);
        let mut s = State::new(&w, (5, 5), (5.0, 5.0));
        assert!(s.rows.len() > 1, "the normal mode has more than one thing to run, so moving means something here");
        s.key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(s.sel, 1, "j moves down");
        s.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(s.sel, 2, "down does the same as j");
        s.key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(s.sel, 1, "k moves back up");
        s.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(s.sel, 0, "up does the same as k");
        s.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(s.sel, s.rows.len() - 1, "k wraps from the top to the bottom");
        s.sel = 0;

        let first = s.rows[0].cmd;
        match s.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)) {
            Outcome::Run(cmd) => assert_eq!(cmd.keys, first.keys),
            Outcome::Nothing => panic!("expected Run, got Nothing"),
            Outcome::Cancel => panic!("expected Run, got Cancel"),
        }
        match s.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)) {
            Outcome::Cancel => {}
            Outcome::Nothing => panic!("expected Cancel, got Nothing"),
            Outcome::Run(_) => panic!("expected Cancel, got Run"),
        }
    }

    #[test]
    fn it_opens_as_a_dropdown_at_the_point_and_a_point_finds_its_own_row() {
        let w = keymap::sample_wheres().remove(0);
        let s = State::new(&w, (10, 5), (10.0, 5.0));
        let full = Rect::new(0, 0, 100, 30);
        let area = s.area(full);
        assert_eq!((area.x, area.y), (10, 5), "room to spare: right at the point it opened on");
        // The title bar and the hint line are not rows.
        assert_eq!(s.row_at(area, area.x, area.y), None, "the title strip");
        assert_eq!(s.row_at(area, area.x, area.y + area.height - 1), None, "the hint line");
        assert_eq!(s.row_at(area, area.x + 1, area.y + 1), Some(0), "the first row");
        assert_eq!(s.row_at(area, 0, 0), None, "off the menu entirely");
    }
}
