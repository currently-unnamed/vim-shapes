//! The `:` line's completion popup — every word the one under the cursor could become,
//! narrowing live as you keep typing and cycled with Tab / Shift-Tab. Anchored just above the
//! command line, opening upward: a dropdown has something below it to point at; this one has
//! not. Sized to its own content, never to the width of the diagram behind it.
//!
//! Carries no state of its own: which entry is "selected" is derived every frame by finding
//! where the buffer's current text sits in the candidate list.

use super::theme;
use ratatui::prelude::*;
use ratatui::widgets::{Clear, Paragraph};

const MAX_ROWS: u16 = 8;
const MAX_WIDTH: u16 = 44;

pub struct WildMenu<'a> {
    pub candidates: &'a [String],
    pub current: &'a str,
}

impl WildMenu<'_> {
    pub fn height(&self, avail: u16) -> u16 {
        (self.candidates.len() as u16).min(MAX_ROWS).min(avail)
    }

    pub fn width(&self, avail: u16) -> u16 {
        let longest = self.candidates.iter().map(|c| c.chars().count()).max().unwrap_or(0);
        let cap = MAX_WIDTH.min(avail);
        let floor = 12.min(cap);
        ((longest + 2) as u16).clamp(floor, cap)
    }
}

impl Widget for WildMenu<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.candidates.is_empty() || area.height == 0 || area.width == 0 {
            return;
        }
        Clear.render(area, buf);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_bg(theme::t().panel);
            }
        }
        let view = area.height as usize;
        let sel = self.candidates.iter().position(|c| c == self.current);
        let max_off = self.candidates.len().saturating_sub(view);
        let off = sel.unwrap_or(0).saturating_sub(view / 2).min(max_off);
        let scrolls = self.candidates.len() > view;
        let text_width = area.width.saturating_sub(scrolls as u16) as usize;
        let lines: Vec<Line> = self
            .candidates
            .iter()
            .enumerate()
            .skip(off)
            .take(view)
            .map(|(i, c)| {
                let style = if Some(i) == sel {
                    Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold()
                } else {
                    Style::new().fg(theme::t().ink).bg(theme::t().grid)
                };
                Line::styled(format!("{:<text_width$}", format!(" {c}")), style)
            })
            .collect();
        Paragraph::new(lines).render(area, buf);
        if scrolls {
            let x = area.right() - 1;
            let thumb_len = (((view * view) as f32 / self.candidates.len() as f32).round() as u16).clamp(1, area.height);
            let thumb_at = if max_off > 0 {
                ((off as f32 / max_off as f32) * (area.height - thumb_len) as f32).round() as u16
            } else {
                0
            };
            for row in 0..area.height {
                let on = row >= thumb_at && row < thumb_at + thumb_len;
                buf[(x, area.y + row)].set_bg(if on { theme::t().muted } else { theme::t().grid });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn width_fits_the_longest_candidate_and_respects_the_cap_and_floor() {
        let cands = strs(&["export diagrams/lead.drawio", "export diagrams/lead-2.drawio"]);
        assert_eq!(WildMenu { candidates: &cands, current: "" }.width(100), 31);
        let one = strs(&["a"]);
        assert_eq!(WildMenu { candidates: &one, current: "" }.width(100), 12);
        assert_eq!(WildMenu { candidates: &one, current: "" }.width(1), 1);
        let long = strs(&["a very much longer candidate than any real command name here"]);
        assert_eq!(WildMenu { candidates: &long, current: "" }.width(200), MAX_WIDTH);
    }

    #[test]
    fn height_caps_at_max_rows_and_at_what_is_available() {
        let cands: Vec<String> = (0..50).map(|i| i.to_string()).collect();
        assert_eq!(WildMenu { candidates: &cands, current: "" }.height(100), MAX_ROWS);
        assert_eq!(WildMenu { candidates: &cands, current: "" }.height(3), 3);
    }
}
