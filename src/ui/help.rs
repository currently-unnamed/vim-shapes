//! The command menu (`?`).
//!
//! Everything it knows comes from [`crate::ui::keymap`]. It renders *every* command, always —
//! the ones you can use here in full colour, the rest dimmed and annotated with the reason
//! they are out of reach. A menu that hid what you cannot currently do would be shorter and
//! would never teach you that `gd` exists, or that the way to reach it is to step onto a
//! relation.

use super::keymap::{self, Avail, Cmd, Section, Where};
use super::theme;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub struct State {
    pub sel: usize,
    /// The search query. `None` until `/` is pressed — until then the letters are commands,
    /// not text, which is what lets the menu be a menu and not just a list.
    pub filter: Option<String>,
}

impl State {
    pub fn new() -> State {
        State { sel: 0, filter: None }
    }

    pub fn query(&self) -> &str {
        self.filter.as_deref().unwrap_or("")
    }

    pub fn searching(&self) -> bool {
        self.filter.is_some()
    }

    pub fn picked(&self, w: &Where) -> Option<(&'static Cmd, Avail)> {
        visible(w, self.query()).get(self.sel).copied()
    }

    pub fn move_by(&mut self, delta: isize, w: &Where) {
        let n = visible(w, self.query()).len();
        if n == 0 {
            self.sel = 0;
            return;
        }
        self.sel = (self.sel as isize + delta).rem_euclid(n as isize) as usize;
    }

    pub fn retype(&mut self, f: impl FnOnce(&mut String)) {
        f(self.filter.get_or_insert_with(String::new));
        self.sel = 0;
    }
}

/// Every command that survives the filter, in menu order, with its availability here.
pub fn visible(w: &Where, filter: &str) -> Vec<(&'static Cmd, Avail)> {
    let f = filter.trim().to_ascii_lowercase();
    let mut out = Vec::new();
    for s in Section::ALL {
        for (c, a) in keymap::section(s, w) {
            let hit = f.is_empty()
                || c.keys.to_ascii_lowercase().contains(&f)
                || (c.what)(w).to_ascii_lowercase().contains(&f)
                || s.name().contains(&f);
            if hit {
                out.push((c, a));
            }
        }
    }
    out
}

enum Row {
    Head(&'static str),
    Note(&'static str),
    Item(usize, &'static Cmd, Avail),
}

fn rows(w: &Where, filter: &str) -> Vec<Row> {
    let mut out = Vec::new();
    let mut sec = None;
    for (i, (c, a)) in visible(w, filter).iter().enumerate() {
        if sec != Some(c.section) {
            sec = Some(c.section);
            out.push(Row::Head(c.section.name()));
            out.extend(c.section.note().iter().map(|n| Row::Note(n)));
        }
        out.push(Row::Item(i, c, *a));
    }
    out
}

pub const WIDTH: u16 = 84;

fn fit(s: &str, room: usize) -> String {
    if s.chars().count() <= room {
        return s.to_string();
    }
    let mut t: String = s.chars().take(room.saturating_sub(1)).collect();
    t.push('…');
    t
}

pub fn height(avail: u16) -> u16 {
    avail.saturating_sub(2).max(6)
}

pub struct Menu<'a> {
    pub state: &'a State,
    pub w: &'a Where,
    /// Where the cursor is, in words. The menu is context-sensitive and has to say so, or its
    /// dimmed rows look like bugs.
    pub here: String,
}

impl Widget for Menu<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = super::chrome::panel(buf, area, &format!("commands — {}", self.here), theme::t().aqua);
        if inner.height < 3 {
            return;
        }
        let cols = inner.width as usize;
        let filter = self.state.query();
        let all = rows(self.w, filter);
        let view = (inner.height - 2) as usize;

        let mut lines = vec![if self.state.searching() {
            Line::from(vec![
                Span::styled(" / ", Style::new().fg(theme::t().aqua).bold()),
                Span::styled(filter.to_string(), Style::new().fg(theme::t().ink)),
                Span::styled("█", Style::new().fg(theme::t().aqua)),
            ])
        } else {
            Line::styled(" dimmed = not available here, and why", Style::new().fg(theme::t().dim).italic())
        }];

        let mut above = 0usize;
        let mut below = 0usize;
        if all.is_empty() {
            lines.push(Line::styled(format!(" nothing matches {filter:?}"), Style::new().fg(theme::t().red)));
        } else {
            let cur = all
                .iter()
                .position(|r| matches!(r, Row::Item(i, _, _) if *i == self.state.sel))
                .unwrap_or(0);
            let max_off = all.len().saturating_sub(view);
            let off = cur.saturating_sub(view / 2).min(max_off);
            above = off;
            below = all.len().saturating_sub(off + view);
            lines.extend(all.iter().skip(off).take(view).map(|row| match row {
                Row::Head(name) => Line::styled(format!(" ── {name} "), Style::new().fg(theme::t().dim).italic()),
                Row::Note(n) => Line::styled(
                    format!("    {}", fit(n, cols.saturating_sub(5))),
                    Style::new().fg(theme::t().yellow).italic(),
                ),
                Row::Item(i, c, a) => {
                    let on = *i == self.state.sel;
                    let keys = format!(" {:<9}", c.keys);
                    let what = format!(" {}", (c.what)(self.w));
                    let used = keys.chars().count() + what.chars().count();
                    match a {
                        Avail::Yes => {
                            let key_style = if on {
                                Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold()
                            } else {
                                Style::new().fg(theme::t().aqua).bold()
                            };
                            let what_style = if on {
                                Style::new().fg(theme::t().ink).bold()
                            } else {
                                Style::new().fg(theme::t().muted)
                            };
                            Line::from(vec![Span::styled(keys, key_style), Span::styled(what, what_style)])
                        }
                        Avail::No(why) => {
                            let key_style = if on {
                                Style::new().fg(theme::t().inverse).bg(theme::t().dim)
                            } else {
                                Style::new().fg(theme::t().dim)
                            };
                            let room = cols.saturating_sub(used + 4);
                            Line::from(vec![
                                Span::styled(keys, key_style),
                                Span::styled(what, Style::new().fg(theme::t().dim)),
                                Span::styled(format!("  · {}", fit(why, room)), Style::new().fg(theme::t().dim).italic()),
                            ])
                        }
                    }
                }
            }));
        }

        let hint = if self.state.searching() {
            " enter run   ↑/↓ pick   esc stop searching"
        } else {
            " press a key to run it   / search   j/k pick   enter run   esc close"
        };
        let more = match (above, below) {
            (0, 0) => String::new(),
            (a, 0) => format!("▴ {a} more "),
            (0, b) => format!("▾ {b} more "),
            (a, b) => format!("▴ {a}  ▾ {b} more "),
        };
        let pad = cols.saturating_sub(hint.chars().count() + more.chars().count()).max(1);
        lines.push(Line::from(vec![
            Span::styled(hint, Style::new().fg(theme::t().dim)),
            Span::raw(" ".repeat(pad)),
            Span::styled(more, Style::new().fg(theme::t().aqua).bold()),
        ]));
        Paragraph::new(lines).render(inner, buf);
    }
}
