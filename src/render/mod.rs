mod canvas;
mod sidebar;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, Focus, Mode};

/// Ratatui's `Canvas` uses a math-style coordinate system where y increases
/// upward, but the document model treats y as increasing downward (row-like,
/// matching `j` = down / `k` = up). Every y fed to a canvas paint call must
/// go through this flip, or shapes render upside down.
pub(super) fn flip_y(doc_y: f64, extent_y: f64) -> f64 {
    extent_y - doc_y
}

pub fn draw(frame: &mut Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(0)])
        .split(root[0]);

    sidebar::draw(frame, body[0], app);
    canvas::draw(frame, body[1], app);
    draw_status_line(frame, root[1], app);
}

fn draw_status_line(frame: &mut Frame, area: Rect, app: &App) {
    let (mode_label, mode_color) = match &app.mode {
        Mode::Normal => ("NORMAL", Color::Cyan),
        Mode::Insert => ("INSERT", Color::Green),
        Mode::Visual => ("VISUAL", Color::Magenta),
        Mode::Command { buffer } => {
            let line = Line::from(vec![
                Span::styled(format!(":{buffer}"), Style::default().fg(Color::Yellow)),
            ]);
            let para = Paragraph::new(line).style(Style::default().bg(Color::Black));
            frame.render_widget(para, area);
            return;
        }
    };
    let focus_label = match app.focus {
        Focus::Sidebar => "SIDEBAR",
        Focus::Canvas => "CANVAS",
    };

    let mut spans = vec![
        Span::styled(format!(" {mode_label} "), Style::default().fg(mode_color)),
        Span::styled(format!(" {focus_label} "), Style::default().fg(Color::DarkGray)),
    ];
    if let Some(msg) = &app.status_message {
        spans.push(Span::styled(format!("  {msg}"), Style::default().fg(Color::White)));
    }
    let para = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
    frame.render_widget(para, area);
}
