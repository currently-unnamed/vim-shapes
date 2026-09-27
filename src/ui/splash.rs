//! The title screen — up until a key is pressed.
//!
//! Modelled on vim's own startup: a small logo, the name and version, a rule, one line to
//! press a key, and a rule. The logo is a three-element diagram in the layer colours, fading into the
//! dark as it runs right, as if the rest of the architecture were off the edge of the screen.

use super::theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

pub struct Splash;

// Every row is 28 columns, corner-for-corner: each box's top and bottom border has exactly
// as many dashes as its content row has interior characters, and the ┬/┴ connector sits on
// the same column in both the row above and the row below it. Keeping every row the same
// width also keeps the fade below honest — a shorter row would reach full fade early.
const LOGO: [&str; 5] = [
    "╭───────╮        ╭─────────╮",
    "│ actor │ ─────▷ │ service │",
    "╰───────╯        ╰────┬────╯",
    "                  ╭───┴────╮",
    "                  │ object │",
];

impl Widget for Splash {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let ground = theme::t().ground;
        let bg = Style::new().bg(ground);
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
                                let opacity = (100.0 - 100.0 * (c as f32 / span).clamp(0.0, 1.0)) as u8;
                                Span::styled(ch.to_string(), Style::new().fg(theme::fade(colours[r], opacity, ground)).bg(ground))
                            }
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        const RULE_W: usize = 36;
        let rule = || Line::styled("─".repeat(RULE_W), Style::new().fg(theme::t().grid).bg(ground));
        let text: Vec<Line> = vec![
            Line::styled("", bg),
            Line::from(vec![
                Span::styled(":", Style::new().fg(theme::t().grid).bg(ground)),
                Span::styled("vim-shapes", Style::new().fg(theme::t().green).bg(ground).bold()),
                Span::styled(concat!("  v", env!("CARGO_PKG_VERSION")), Style::new().fg(theme::t().dim).bg(ground)),
            ]),
            rule(),
            Line::styled("press any key to continue", Style::new().fg(theme::t().dim).bg(ground)),
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
        assert!(text.contains("actor") && text.contains("object"));
        assert!(text.contains(":vim-shapes"));
        assert!(text.contains("press any key to continue"));
        assert!(!text.contains("driven like vim"), "no tagline on the title screen");
    }
}
