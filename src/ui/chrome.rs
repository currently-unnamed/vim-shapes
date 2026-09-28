//! The app's own look, in one place.
//!
//! The rule is not "no borders". It is *the content defines its own edge, and chrome that
//! repeats what the content already says is deleted*. A surface that owns the body needs no
//! edge — the terminal is the edge. A panel floating over the diagram genuinely does need one,
//! or it reads as text that has landed on other text; what it does not need is a drawn
//! rectangle. A filled ground and a title strip separate it more clearly than a line does, and
//! they cost one row instead of two.
//!
//! - **A header line**, not a title bar in a box: the surface's name, then its state.
//! - **A hint line** along the bottom, dim: the keys that work *here*.
//! - **A marker, not a replacement**: `▸` on the focused row, so rows do not shift sideways
//!   when the cursor arrives.
//! - **Fills and spacing carry the structure**, never rules. No box-drawing character lives
//!   in this module.

use super::theme;
use ratatui::prelude::*;
use ratatui::widgets::Clear;

/// The `▸` that marks the focused row, and the space that keeps every other row in step with
/// it — a marker rather than an indent, so rows do not shift sideways when the cursor arrives.
pub fn marker(on: bool) -> &'static str {
    if on { "▸" } else { " " }
}

/// The hint line along the bottom: the keys that work here. Returns the rect above it.
pub fn hint(buf: &mut Buffer, area: Rect, keys: &str) -> Rect {
    if area.height == 0 {
        return area;
    }
    let y = area.bottom() - 1;
    buf.set_stringn(area.x, y, keys, area.width as usize, Style::new().fg(theme::t().dim));
    Rect { height: area.height - 1, ..area }
}

/// A floating panel's ground and title strip. Returns the rect to draw into.
///
/// A filled ground darker than the terminal's own, so the panel reads as something lying *on*
/// the diagram; a title strip, full width, in the panel's accent; a column of margin either
/// side, because the thing a border was really buying was the gap.
pub fn panel(buf: &mut Buffer, area: Rect, title: &str, accent: Color) -> Rect {
    Clear.render(area, buf);
    if area.height == 0 || area.width == 0 {
        return area;
    }
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let cell = &mut buf[(x, y)];
            cell.set_symbol(" ");
            cell.set_bg(theme::t().panel);
        }
    }
    let bar = format!(" {title}");
    buf.set_stringn(
        area.x,
        area.y,
        format!("{bar:<w$}", w = area.width as usize),
        area.width as usize,
        Style::new().fg(theme::t().inverse).bg(accent).bold(),
    );
    Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(1),
    }
}

/// A rect of `w × h` centred in `area`, clamped to it.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// A rect of `w × h` anchored at a screen point — a dropdown at the mouse, not centred on the
/// area — clamped so it stays whole inside `area` rather than running off an edge it opened
/// near, the way a real one flips instead of clipping.
pub fn near(area: Rect, at: (u16, u16), w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = at.0.clamp(area.x, area.x + area.width - w);
    let y = at.1.clamp(area.y, area.y + area.height - h);
    Rect { x, y, width: w, height: h }
}

/// Where `panel` (and, if `hinted`, `hint` after it) leave the content — the same rects those
/// two compute, without a `Buffer` to draw into, so a mouse hit-test can agree with what was
/// drawn on screen without redrawing it. Keep this in step with `panel`'s and `hint`'s own
/// arithmetic if either changes.
pub fn panel_body(area: Rect, hinted: bool) -> Rect {
    if area.height == 0 || area.width == 0 {
        return area;
    }
    let inner = Rect { x: area.x + 1, y: area.y + 1, width: area.width.saturating_sub(2), height: area.height.saturating_sub(1) };
    if hinted { Rect { height: inner.height.saturating_sub(1), ..inner } } else { inner }
}

/// Break `text` into lines no wider than `width`, on spaces. A word longer than the width is
/// cut rather than overflowing.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let w = word.chars().count();
        if cur.is_empty() {
            if w > width {
                cur = word.chars().take(width).collect();
            } else {
                cur = word.to_string();
            }
        } else if cur.chars().count() + 1 + w <= width {
            cur.push(' ');
            cur.push_str(word);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = if w > width { word.chars().take(width).collect() } else { word.to_string() };
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render<F: FnOnce(&mut Buffer, Rect)>(w: u16, h: u16, f: F) -> Vec<String> {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        f(&mut buf, area);
        (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect()).collect()
    }

    /// The panel draws no box — that is the whole grammar, asserted.
    #[test]
    fn a_panel_has_a_title_strip_and_not_one_box_drawing_character() {
        let out = render(20, 5, |b, a| {
            panel(b, a, "add", theme::t().green);
        });
        assert!(out[0].starts_with(" add"), "the strip is the title: {:?}", out[0]);
        let all: String = out.concat();
        for ch in ['╭', '╮', '╰', '╯', '│', '─', '┌', '┐', '└', '┘', '├', '┤'] {
            assert!(!all.contains(ch), "a panel draws no {ch:?}");
        }
    }

    #[test]
    fn a_panel_spends_one_row_on_chrome_where_a_border_spent_two() {
        let area = Rect::new(0, 0, 30, 10);
        let mut buf = Buffer::empty(area);
        let inner = panel(&mut buf, area, "load", theme::t().green);
        assert_eq!(inner.height, 9);
        assert_eq!(inner.y, 1);
        assert_eq!(inner.width, 28, "a column of margin either side");
    }

    #[test]
    fn panel_body_agrees_with_what_panel_and_hint_actually_draw_into() {
        let area = Rect::new(2, 3, 30, 10);
        let mut buf = Buffer::empty(area);
        let inner = panel(&mut buf, area, "menu", theme::t().sand);
        assert_eq!(panel_body(area, false), inner, "panel alone");
        let body = hint(&mut buf, inner, "esc");
        assert_eq!(panel_body(area, true), body, "panel, then hint's own trim");
    }

    #[test]
    fn near_anchors_at_the_point_but_never_runs_off_the_area() {
        let area = Rect::new(0, 0, 40, 20);
        assert_eq!(near(area, (5, 5), 10, 4), Rect::new(5, 5, 10, 4), "room to spare: right at the point");
        assert_eq!(near(area, (38, 19), 10, 4), Rect::new(30, 16, 10, 4), "flips back to stay whole");
        assert_eq!(near(area, (0, 0), 50, 30), Rect::new(0, 0, 40, 20), "bigger than the area: clamped to it");
    }

    #[test]
    fn wrap_breaks_on_spaces_and_cuts_a_word_wider_than_the_line() {
        assert_eq!(wrap("account opening service", 10), vec!["account", "opening", "service"]);
        assert_eq!(wrap("abcdefghijkl", 5), vec!["abcde"]);
        assert_eq!(wrap("a b c", 10), vec!["a b c"]);
        assert!(wrap("", 10).is_empty());
    }
}
