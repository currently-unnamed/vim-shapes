//! The colour picker — one picker, for every colour field there is.
//!
//! A colour field takes a name or a hex typed into it, and `h`/`l` cycle the palette's ten
//! names; that covers the terminal's own accents. Enter on the field opens this: the
//! palette by name, the colours picked lately, and a grid of swatches — twelve hues by
//! eight lightnesses under a row of greys — the way every colour picker lays them out, so
//! the hand knows where teal is. A hex can be typed here too, with `#` or `i`.
//!
//! It writes nothing itself: what it picks goes back through `form::apply`, the same write
//! the sheet makes, so a colour chosen here and a name typed there are one thing.

use super::canvas::Target;
use super::{chrome, theme};
use crate::ontology::{self, Colour, Paint};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

/// How many recent colours are kept.
pub const RECENT: usize = 12;

/// Wide enough for twelve swatches of two cells with a space between, in the panel's margins.
pub const WIDTH: u16 = 44;

/// One row of the picker: a title, and the colours under it. `None` is "no colour" — the
/// default look, whatever that is for the field.
pub struct Row {
    pub title: &'static str,
    pub cells: Vec<Option<Colour>>,
}

pub struct State {
    pub target: Target,
    pub field: &'static str,
    /// What picking nothing means for this field — `none` for a line, `auto` for a fill.
    pub blank: &'static str,
    pub rows: Vec<Row>,
    pub row: usize,
    pub col: usize,
    /// A hex being typed, when it is.
    pub editing: Option<String>,
}

pub enum Outcome {
    /// Still up.
    Open,
    Cancel,
    /// Picked: a colour, or none.
    Pick(Option<Colour>),
}

impl State {
    pub fn open(target: Target, field: &'static str, current: Option<Colour>, recent: &[Colour], blank: &'static str) -> State {
        let mut rows = vec![Row {
            title: "palette",
            cells: std::iter::once(None).chain(Paint::ALL.into_iter().map(|p| Some(Colour::Named(p)))).collect(),
        }];
        if !recent.is_empty() {
            rows.push(Row { title: "recent", cells: recent.iter().map(|c| Some(*c)).collect() });
        }
        for (i, sw) in ontology::swatches().into_iter().enumerate() {
            rows.push(Row { title: if i == 0 { "swatches" } else { "" }, cells: sw.into_iter().map(|c| Some(Colour::Hex(c))).collect() });
        }
        let mut s = State { target, field, blank, rows, row: 0, col: 0, editing: None };
        // Open on the colour it has, where the picker shows it; a hex not in the grid opens
        // on the palette's "none", the same as nothing set.
        if let Some(c) = current {
            let want = c.on(theme::mode() == theme::Mode::Light);
            let found = s.rows.iter().enumerate().find_map(|(ri, r)| r.cells.iter().position(|x| x.is_some_and(|x| x.on(theme::mode() == theme::Mode::Light) == want)).map(|ci| (ri, ci)));
            if let Some((r, c)) = found {
                s.row = r;
                s.col = c;
            }
        }
        s
    }

    pub fn picked(&self) -> Option<Colour> {
        self.rows.get(self.row).and_then(|r| r.cells.get(self.col).copied().flatten())
    }

    fn move_by(&mut self, dr: isize, dc: isize) {
        let n = self.rows.len() as isize;
        self.row = (self.row as isize + dr).rem_euclid(n) as usize;
        let w = self.rows[self.row].cells.len() as isize;
        self.col = (self.col.min(w as usize - 1) as isize + dc).rem_euclid(w) as usize;
    }

    pub fn key(&mut self, k: KeyEvent) -> Outcome {
        if let Some(buf) = &mut self.editing {
            match k.code {
                KeyCode::Esc => self.editing = None,
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Enter => {
                    let text = buf.clone();
                    self.editing = None;
                    if text.trim().is_empty() {
                        return Outcome::Pick(None);
                    }
                    if let Some(c) = Colour::parse(&text) {
                        return Outcome::Pick(Some(c));
                    }
                    // Not a colour: stay, with what was typed, so it can be fixed.
                    self.editing = Some(text);
                }
                KeyCode::Char(c) if buf.len() < 24 => buf.push(c),
                _ => {}
            }
            return Outcome::Open;
        }
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => Outcome::Cancel,
            KeyCode::Enter | KeyCode::Char(' ') => Outcome::Pick(self.picked()),
            KeyCode::Char('h') | KeyCode::Left => {
                self.move_by(0, -1);
                Outcome::Open
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.move_by(0, 1);
                Outcome::Open
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_by(1, 0);
                Outcome::Open
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_by(-1, 0);
                Outcome::Open
            }
            KeyCode::Char('i') | KeyCode::Char('#') => {
                self.editing = Some(self.picked().map(|c| c.hex()).unwrap_or_else(|| "#".into()));
                Outcome::Open
            }
            KeyCode::Char('x') | KeyCode::Char('n') => Outcome::Pick(None),
            _ => Outcome::Open,
        }
    }
}

/// The panel's height: a line for the value, one per titled row and its swatches, the hint.
pub fn height() -> u16 {
    // Value line, blank, palette (2), recent (2, at most), swatches title + 9 rows, hint, chrome.
    2 + 2 + 2 + 10 + 1 + 2
}

pub struct Picker<'a> {
    pub state: &'a State,
}

impl Widget for Picker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let inner = chrome::panel(buf, area, &format!("colour · {}", s.field), theme::t().aqua);
        let hint = match s.editing {
            Some(_) => " type a hex or a name, enter takes it, esc back",
            None => " hjkl pick  enter take  i hex  x clear  esc",
        };
        let body = chrome::hint(buf, inner, hint);
        let mut lines: Vec<Line> = Vec::new();
        // The value: what is under the cursor, by name and by hex, or the hex being typed.
        lines.push(match &s.editing {
            Some(t) => Line::from(vec![Span::styled(format!(" {t}█"), Style::new().fg(theme::t().green).bold())]),
            None => match s.picked() {
                Some(c) => Line::from(vec![
                    Span::styled(" ██ ", Style::new().fg(theme::colour(c))),
                    Span::styled(c.name(), Style::new().fg(theme::t().ink).bold()),
                    Span::styled(format!("  {}", c.hex()), Style::new().fg(theme::t().dim)),
                ]),
                None => Line::styled(format!(" {}", s.blank), Style::new().fg(theme::t().dim)),
            },
        });
        lines.push(Line::raw(""));
        for (ri, r) in s.rows.iter().enumerate() {
            if !r.title.is_empty() {
                lines.push(Line::styled(format!(" {}", r.title), Style::new().fg(theme::t().yellow).bold()));
            }
            let mut spans: Vec<Span> = vec![Span::raw(" ")];
            for (ci, c) in r.cells.iter().enumerate() {
                let on = ri == s.row && ci == s.col;
                let (glyph, style) = match c {
                    Some(c) => ("██", Style::new().fg(theme::colour(*c))),
                    None => ("··", Style::new().fg(theme::t().dim)),
                };
                // The pick: bracketed in white, which reads on any swatch.
                spans.push(Span::styled(if on { "[" } else { " " }, Style::new().fg(theme::t().ink).bold()));
                spans.push(Span::styled(glyph, style));
                spans.push(Span::styled(if on { "]" } else { " " }, Style::new().fg(theme::t().ink).bold()));
            }
            lines.push(Line::from(spans));
        }
        Paragraph::new(lines).render(body, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn the_picker_opens_on_the_colour_it_has_and_moves_like_the_sheet() {
        let mut s = State::open(Target::Diagram, "background", Some(Colour::Named(Paint::Blue)), &[], "none");
        assert_eq!((s.row, s.col), (0, 6), "blue is sixth in the palette after none");
        assert_eq!(s.rows.len(), 1 + 9, "palette and swatches; no recent row yet");
        s.key(key('j'));
        s.key(key('l'));
        assert_eq!((s.row, s.col), (1, 7));
        assert!(matches!(s.picked(), Some(Colour::Hex(_))));
        s.key(key('k'));
        assert_eq!(s.row, 0);
        s.key(key('k'));
        assert_eq!(s.row, 9, "wraps");
        assert!(matches!(s.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)), Outcome::Cancel));
    }

    #[test]
    fn recent_colours_get_a_row_and_a_hex_can_be_typed() {
        let recent = [Colour::Hex([1, 2, 3])];
        let mut s = State::open(Target::Diagram, "grid colour", Some(Colour::Hex([1, 2, 3])), &recent, "none");
        assert_eq!((s.rows[1].title, s.row, s.col), ("recent", 1, 0), "opens on it, in the recent row");
        s.key(key('#'));
        assert_eq!(s.editing.as_deref(), Some("#010203"));
        for _ in 0..7 {
            s.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        }
        for c in "#4a90d9".chars() {
            s.key(key(c));
        }
        match s.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)) {
            Outcome::Pick(Some(c)) => assert_eq!(c.hex(), "#4a90d9"),
            _ => panic!("a typed hex is picked"),
        }
        let mut s = State::open(Target::Diagram, "grid colour", None, &[], "none");
        s.key(key('i'));
        for c in "zz".chars() {
            s.key(key(c));
        }
        assert!(matches!(s.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)), Outcome::Open), "not a colour: stays to be fixed");
        assert!(matches!(s.key(key('x')), Outcome::Open), "typing, x is a character");
        s.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(s.key(key('x')), Outcome::Pick(None)), "x clears the colour");
    }
}
