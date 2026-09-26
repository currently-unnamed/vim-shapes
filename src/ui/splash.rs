//! The title screen — up until a key is pressed.
//!
//! Modelled on vim's own startup: a small logo, the name and version, a rule, one line to
//! press a key, and a rule. The logo is a three-element diagram in the layer colours, fading into the
//! dark as it runs right, as if the rest of the architecture were off the edge of the screen.

use super::theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

pub struct Splash;

fn fade(c: Color, t: f32) -> Color {
    let (r, g, b) = if let Color::Rgb(r, g, b) = c { (r, g, b) } else { (128, 128, 128) };
    let k = 1.0 - t.clamp(0.0, 1.0);
    Color::Rgb((r as f32 * k) as u8, (g as f32 * k) as u8, (b as f32 * k) as u8)
}

const LOGO: [&str; 5] = [
    "╭────────╮        ╭────────╮",
    "│ actor  │ ─────▷ │ service │",
    "╰────────╯        ╰───┬────╯",
    "                 ╭────┴─────╮",
    "                 │component │",
];

impl Widget for Splash {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bg = Style::new().bg(theme::t().ground);
        Block::default().style(bg).render(area, buf);
        let colours = [theme::t().orange, theme::t().orange, theme::t().orange, theme::t().aqua, theme::t().aqua];
        let logo_w = LOGO.iter().map(|r| r.chars().count()).max().unwrap_or(1) as u16;
        let span = (logo_w.max(2) - 1) as f32;
        let logo: Vec<Line> = LOGO
            .iter()
            .enumerate()
            .map(|(r, row)| {
                Line::from(
                    row.chars()
                        .enumerate()
                        .map(|(c, ch)| {
                            if ch == ' ' {
                                Span::styled(" ", bg)
                            } else {
                                Span::styled(ch.to_string(), Style::new().fg(fade(colours[r], c as f32 / span)).bg(theme::t().inverse))
                            }
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        const RULE_W: usize = 36;
        let rule = || Line::styled("─".repeat(RULE_W), Style::new().fg(theme::t().grid).bg(theme::t().inverse));
        let text: Vec<Line> = vec![
            Line::styled("", bg),
            Line::from(vec![
                Span::styled(":", Style::new().fg(theme::t().grid).bg(theme::t().inverse)),
                Span::styled("vim-shapes", Style::new().fg(theme::t().green).bg(theme::t().inverse).bold()),
                Span::styled(concat!("  v", env!("CARGO_PKG_VERSION")), Style::new().fg(theme::t().dim).bg(theme::t().inverse)),
            ]),
            rule(),
            Line::styled("press any key to continue", Style::new().fg(theme::t().dim).bg(theme::t().inverse)),
            rule(),
        ];
        let total = LOGO.len() as u16 + text.len() as u16;
        let top = area.y + area.height.saturating_sub(total) / 2;
        let logo_rect = Rect {
            x: area.x + area.width.saturating_sub(logo_w) / 2,
            y: top,
            width: logo_w.min(area.width),
            height: (LOGO.len() as u16).min(area.height),
        };
        Paragraph::new(logo).render(logo_rect, buf);
        let text_rect = Rect {
            y: top + LOGO.len() as u16,
            height: (text.len() as u16).min(area.height.saturating_sub(LOGO.len() as u16)),
            ..area
        };
        Paragraph::new(text).alignment(Alignment::Center).render(text_rect, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn the_splash_shows_the_logo_name_and_tagline() {
        let mut term = Terminal::new(TestBackend::new(70, 20)).unwrap();
        term.draw(|f| f.render_widget(Splash, f.area())).unwrap();
        let text: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("actor") && text.contains("component"));
        assert!(text.contains(":vim-shapes"));
        assert!(text.contains("press any key to continue"));
        assert!(!text.contains("driven like vim"), "no tagline on the title screen");
    }
}
