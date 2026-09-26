//! The debugging panel — `:debug` toggles it.
//!
//! Everything the app knows about where it is, in one column down the right-hand side: the
//! mode and what the cursor is on, the camera, the tab, the undo depth, the last keys pressed
//! and what the table made of them, the fonts it found, what the rules refuse. Live, so a
//! bug reproduces in front of it. It is a panel, not a mode: the keyboard stays the diagram's.

use super::{chrome, theme};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

/// How many keystrokes the panel remembers.
pub const KEEP: usize = 12;

pub const WIDTH: u16 = 44;

/// One line of the panel: a name and a value.
pub type Row = (&'static str, String);

pub struct Panel<'a> {
    pub rows: &'a [Row],
    pub keys: &'a [String],
}

impl Widget for Panel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = chrome::panel(buf, area, "debug", theme::t().purple);
        let body = chrome::hint(buf, inner, " :debug closes");
        let mut lines: Vec<Line> = Vec::new();
        for (name, value) in self.rows {
            if name.is_empty() {
                lines.push(Line::raw(""));
                continue;
            }
            let room = (body.width as usize).saturating_sub(13);
            let value: String = if value.chars().count() > room { format!("{}…", value.chars().take(room.saturating_sub(1)).collect::<String>()) } else { value.clone() };
            lines.push(Line::from(vec![
                Span::styled(format!("{name:<12}"), Style::new().fg(theme::t().purple)),
                Span::styled(value, Style::new().fg(theme::t().ink)),
            ]));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled("keys, newest last", Style::new().fg(theme::t().dim).italic()));
        lines.push(Line::styled(self.keys.join(" "), Style::new().fg(theme::t().yellow)));
        Paragraph::new(lines).render(body, buf);
    }
}

/// A mouse event, written for the "last mouse" row — the mouse's answer to `describe`.
pub fn describe_mouse(m: &crossterm::event::MouseEvent) -> String {
    use crossterm::event::{MouseButton, MouseEventKind};
    let button = |b: MouseButton| match b {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    };
    let kind = match m.kind {
        MouseEventKind::Down(b) => format!("{} down", button(b)),
        MouseEventKind::Up(b) => format!("{} up", button(b)),
        MouseEventKind::Drag(b) => format!("{} drag", button(b)),
        MouseEventKind::Moved => "moved".into(),
        MouseEventKind::ScrollUp => "scroll up".into(),
        MouseEventKind::ScrollDown => "scroll down".into(),
        MouseEventKind::ScrollLeft => "scroll left".into(),
        MouseEventKind::ScrollRight => "scroll right".into(),
    };
    format!("{kind} ({}, {})", m.column, m.row)
}

/// A keystroke, written the way the menu writes one.
pub fn describe(k: &crossterm::event::KeyEvent) -> String {
    use crossterm::event::{KeyCode, KeyModifiers};
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let name = match k.code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "S-tab".into(),
        KeyCode::Backspace => "bs".into(),
        KeyCode::Left => "←".into(),
        KeyCode::Right => "→".into(),
        KeyCode::Up => "↑".into(),
        KeyCode::Down => "↓".into(),
        other => format!("{other:?}").to_ascii_lowercase(),
    };
    if ctrl { format!("^{name}") } else { name }
}
