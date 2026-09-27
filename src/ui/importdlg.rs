//! The import dialog — `:import` with no path.
//!
//! `:import <path>` has always taken a typed path; this is the picker for when there isn't
//! one yet. It walks a directory the same way `start.rs`'s "open" half does — same keys, same
//! look — but it is its own dialog rather than a third mode bolted onto that one: `start`'s
//! `Outcome::Dismiss` means "proceed with an empty diagram," which is nonsense to reach for
//! mid-session with unsaved work already open, and `start` never lets a folder itself be the
//! chosen thing, which `:import` needs for a coArchi model. So this repeats the walk rather
//! than straining those invariants, and adds the one thing `start`'s browser has no use for:
//! `^Enter` imports the directory you are standing in, not a file inside it.

use super::{chrome, theme};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use std::path::{Path, PathBuf};

pub const WIDTH: u16 = 70;
pub const HEIGHT: u16 = 20;

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
    Import(PathBuf),
    Cancel,
}

pub struct State {
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    pub sel: usize,
    pub error: Option<String>,
}

impl State {
    pub fn new() -> State {
        let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut s = State { dir, entries: Vec::new(), sel: 0, error: None };
        s.read_dir();
        s
    }

    /// The listing: `..` first, then folders, then files that look importable, then the
    /// rest, each group sorted — the same grouping `start.rs`'s open browser uses.
    pub(crate) fn read_dir(&mut self) {
        let mut dirs = Vec::new();
        let mut likely = Vec::new();
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
                } else if name.ends_with(".json") || name.ends_with(".xml") {
                    likely.push(name);
                } else {
                    other.push(name);
                }
            }
        }
        for v in [&mut dirs, &mut likely, &mut other] {
            v.sort_by_key(|n| n.to_ascii_lowercase());
        }
        self.entries = Vec::new();
        if self.dir.parent().is_some() {
            self.entries.push(Entry { name: "..".into(), is_dir: true });
        }
        self.entries.extend(dirs.into_iter().map(|name| Entry { name, is_dir: true }));
        self.entries.extend(likely.into_iter().map(|name| Entry { name, is_dir: false }));
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

    pub fn key(&mut self, k: KeyEvent) -> Outcome {
        self.error = None;
        if k.code == KeyCode::Enter && k.modifiers.contains(KeyModifiers::CONTROL) {
            return Outcome::Import(self.dir.clone());
        }
        match k.code {
            KeyCode::Esc => return Outcome::Cancel,
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
                        return Outcome::Import(self.dir.join(&e.name));
                    }
                }
            }
            KeyCode::Char('g') => self.sel = 0,
            KeyCode::Char('G') => self.sel = self.entries.len().saturating_sub(1),
            _ => {}
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
        let inner = chrome::panel(buf, area, "vim-shapes — import", theme::t().aqua);
        let hint = " j/k pick   enter or l into a folder   h or backspace up   enter on a file imports   ^enter imports this folder   esc";
        let body = chrome::hint(buf, inner, hint);
        if body.height < 4 {
            return;
        }
        let body = match &s.error {
            Some(why) => {
                let y = body.bottom() - 1;
                buf.set_stringn(body.x, y, format!(" {why}"), body.width as usize, Style::new().fg(theme::t().red).bold());
                Rect { height: body.height - 1, ..body }
            }
            None => body,
        };

        let dir = s.dir.display().to_string();
        let path_line = Line::styled(fit(&dir, body.width as usize), Style::new().fg(theme::t().bright).bold());
        Paragraph::new(vec![Line::raw(""), path_line]).render(body, buf);

        let list = Rect { y: body.y + 3, height: body.height.saturating_sub(3), ..body };
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
                let style = match (picked, e.is_dir) {
                    (true, _) => Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold(),
                    (false, true) => Style::new().fg(theme::t().aqua),
                    (false, false) => Style::new().fg(theme::t().bright),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn press(s: &mut State, code: KeyCode) -> Outcome {
        s.key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn ctrl_enter(s: &mut State) -> Outcome {
        s.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL))
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("vim-shapes-importdlg-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("a.json"), "{}").unwrap();
        std::fs::write(d.join("notes.txt"), "").unwrap();
        d
    }

    #[test]
    fn walking_a_folder_and_confirming_on_a_file() {
        let d = tmpdir("walk");
        let mut s = State::new();
        s.dir = d.clone();
        s.read_dir();
        let names: Vec<&str> = s.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["..", "sub", "a.json", "notes.txt"], "up, folders, importable-looking files, the rest");
        press(&mut s, KeyCode::Char('j'));
        press(&mut s, KeyCode::Enter);
        assert_eq!(s.dir, d.join("sub"), "enter walks into a folder");
        press(&mut s, KeyCode::Char('h'));
        assert_eq!(s.dir, d, "h comes back up");
        press(&mut s, KeyCode::Char('j'));
        press(&mut s, KeyCode::Char('j'));
        assert_eq!(press(&mut s, KeyCode::Enter), Outcome::Import(d.join("a.json")));
        assert_eq!(press(&mut s, KeyCode::Esc), Outcome::Cancel);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn ctrl_enter_imports_the_current_folder_regardless_of_selection() {
        let d = tmpdir("folder");
        let mut s = State::new();
        s.dir = d.clone();
        s.read_dir();
        press(&mut s, KeyCode::Char('j'));
        assert_eq!(ctrl_enter(&mut s), Outcome::Import(d.clone()));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_dialog_draws_the_listing() {
        use ratatui::{backend::TestBackend, Terminal};
        let d = tmpdir("draw");
        let mut term = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
        let mut s = State::new();
        s.dir = d.clone();
        s.read_dir();
        term.draw(|f| f.render_widget(Dialog { state: &s }, f.area())).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains(".."), "the listing starts with the way up");
        assert!(out.contains("sub"));
        std::fs::remove_dir_all(&d).ok();
    }
}
