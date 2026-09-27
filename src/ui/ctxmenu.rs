//! The right-click menu — every command `?` would show in full colour here, none of the
//! ones it would dim, since a menu that lasts one click has no room to teach what you
//! cannot do, only to offer what you can.
//!
//! Not a second list of commands: `State::new` reads `keymap::COMMANDS` through the exact
//! filter `help::visible` already computes for the `?` cheatsheet, so the two can never say
//! different things about what a key does right now.

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

/// A row: the command, and what it does *here* — read once, from the `Where` the menu was
/// opened with, since `Cmd::what` can otherwise say something that was only true at a
/// different cursor position by the time this renders.
pub struct Row {
    pub cmd: &'static Cmd,
    pub what: String,
}

pub struct State {
    pub rows: Vec<Row>,
    pub sel: usize,
}

impl State {
    /// Every command available right now, in the same order the `?` menu lists them —
    /// dimmed rows dropped (a right-click that opens a dead end teaches nothing a click
    /// should have to sit through) and anything the menu could not replay dropped with them.
    pub fn new(w: &Where) -> State {
        let rows = help::visible(w, "")
            .into_iter()
            .filter(|(c, a)| a.ok() && c.runnable())
            .map(|(c, _)| Row { cmd: c, what: (c.what)(w).to_string() })
            .collect();
        State { rows, sel: 0 }
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
            _ => Outcome::Nothing,
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
        let hint = " j/k pick   enter runs it   esc";
        let body = chrome::hint(buf, inner, hint);
        if body.height == 0 {
            return;
        }
        if s.rows.is_empty() {
            Paragraph::new(Line::styled(" nothing to do here", Style::new().fg(theme::t().dim))).render(body, buf);
            return;
        }
        let view = body.height as usize;
        let off = s.sel.saturating_sub(view / 2).min(s.rows.len().saturating_sub(view));
        let lines: Vec<Line> = s
            .rows
            .iter()
            .enumerate()
            .skip(off)
            .take(view)
            .map(|(i, row)| {
                let picked = i == s.sel;
                let text = format!("{}{:<8}{}", chrome::marker(picked), row.cmd.keys, row.what);
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
    use crossterm::event::KeyModifiers;

    #[test]
    fn every_row_is_available_and_replayable_here() {
        let w = keymap::sample_wheres().remove(0);
        let s = State::new(&w);
        for row in &s.rows {
            assert!((row.cmd.avail)(&w).ok(), "{} listed but not available", row.cmd.keys);
            assert!(row.cmd.runnable(), "{} listed but has nothing to replay", row.cmd.keys);
        }
    }

    #[test]
    fn moving_and_cancel() {
        let w = keymap::sample_wheres().remove(0);
        let mut s = State::new(&w);
        assert!(!s.rows.is_empty(), "the normal mode always has something to run");
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
}
