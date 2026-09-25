use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::text::Span;
use ratatui::widgets::canvas::{Canvas, Context, Points};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use crate::app::{App, Focus};
use crate::model::{Node, NodeStyle, ShapeKind};
use crate::render::flip_y;
use crate::shapes::{self, CurvePrimitive};

const ROW_HEIGHT: u16 = 2;
const ICON_COLOR: Color = Color::Gray;
const ICON_COLOR_SELECTED: Color = Color::Cyan;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let sidebar_focused = app.focus == Focus::Sidebar;
    let divider = Block::default().borders(Borders::RIGHT).border_style(Style::default().fg(Color::DarkGray));
    let inner = divider.inner(area);
    frame.render_widget(divider, area);

    let rows = Layout::vertical(
        std::iter::repeat_n(Constraint::Length(ROW_HEIGHT), ShapeKind::ALL.len()),
    )
    .split(inner);

    for (i, kind) in ShapeKind::ALL.into_iter().enumerate() {
        let selected = i == app.sidebar_index;
        draw_row(frame, rows[i], kind, selected, sidebar_focused);
    }
}

fn draw_row(frame: &mut Frame, area: Rect, kind: ShapeKind, selected: bool, sidebar_focused: bool) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let bg = match (selected, sidebar_focused) {
        (true, true) => Color::Rgb(30, 60, 70),
        (true, false) => Color::Rgb(24, 24, 24),
        (false, _) => Color::Reset,
    };
    let icon_color = if selected { ICON_COLOR_SELECTED } else { ICON_COLOR };
    let extent_y = area.height as f64;
    let icon_w = (area.width as f64 * 0.35).clamp(3.0, 8.0);
    let icon = Node {
        id: 0,
        kind,
        x: 0.6,
        y: 0.3,
        w: icon_w,
        h: (extent_y - 0.6).max(1.0),
        label: String::new(),
        style: NodeStyle::default(),
    };
    let name = kind.name();
    let label_x = icon.x + icon.w + 1.0;

    let canvas = Canvas::default()
        .marker(Marker::Braille)
        .background_color(bg)
        .x_bounds([0.0, area.width as f64])
        .y_bounds([0.0, extent_y])
        .paint(move |ctx| {
            paint_icon(ctx, &icon, icon_color, extent_y);
            ctx.print(
                label_x,
                flip_y(extent_y / 2.0, extent_y),
                Span::styled(name, Style::default().fg(Color::White)),
            );
        });
    frame.render_widget(canvas, area);
}

fn paint_icon(ctx: &mut Context, node: &Node, color: Color, extent_y: f64) {
    for prim in shapes::curve_primitives(node) {
        match prim {
            CurvePrimitive::Points(points) => {
                let flipped: Vec<(f64, f64)> = points.iter().map(|&(x, y)| (x, flip_y(y, extent_y))).collect();
                ctx.draw(&Points { coords: &flipped, color });
            }
            CurvePrimitive::Lines(lines) => {
                for (a, b) in lines {
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: a.0,
                        y1: flip_y(a.1, extent_y),
                        x2: b.0,
                        y2: flip_y(b.1, extent_y),
                        color,
                    });
                }
            }
        }
    }
}
