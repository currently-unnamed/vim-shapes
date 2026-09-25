use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::text::Span;
use ratatui::widgets::canvas::{Canvas, Context, Line as CanvasLine, Points};
use ratatui::Frame;

use crate::app::App;
use crate::model::{Document, Edge, Node};
use crate::render::flip_y;
use crate::shapes::{self, CurvePrimitive};

const NORMAL_COLOR: Color = Color::Gray;
const FOCUSED_COLOR: Color = Color::Cyan;
const CONNECT_TARGET_COLOR: Color = Color::Magenta;
const EDGE_COLOR: Color = Color::Green;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let extent_y = area.height as f64;
    let canvas = Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([0.0, area.width as f64])
        .y_bounds([0.0, extent_y])
        .paint(move |ctx| paint(ctx, app, extent_y));

    frame.render_widget(canvas, area);
}

fn paint(ctx: &mut Context, app: &App, extent_y: f64) {
    let connect_target = app.pending_connect.map(|c| c.target);
    for node in &app.document.nodes {
        let color = if connect_target == Some(node.id) {
            CONNECT_TARGET_COLOR
        } else if app.focused_node == Some(node.id) {
            FOCUSED_COLOR
        } else {
            NORMAL_COLOR
        };
        paint_node(ctx, node, color, extent_y);
    }
    for edge in &app.document.edges {
        paint_edge(ctx, edge, &app.document, extent_y);
    }
}

fn paint_node(ctx: &mut Context, node: &Node, color: Color, extent_y: f64) {
    for prim in shapes::curve_primitives(node) {
        match prim {
            CurvePrimitive::Points(points) => {
                let flipped: Vec<(f64, f64)> =
                    points.iter().map(|&(x, y)| (x, flip_y(y, extent_y))).collect();
                ctx.draw(&Points { coords: &flipped, color });
            }
            CurvePrimitive::Lines(lines) => {
                for (a, b) in lines {
                    ctx.draw(&CanvasLine {
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

    if !node.label.is_empty() {
        let (cx, cy) = node.center();
        let start_x = cx - node.label.len() as f64 / 2.0;
        ctx.print(start_x, flip_y(cy, extent_y), Span::styled(node.label.clone(), Style::default().fg(Color::White)));
    }
}

fn paint_edge(ctx: &mut Context, edge: &Edge, document: &Document, extent_y: f64) {
    let (Some(from), Some(to)) = (document.node(edge.from), document.node(edge.to)) else {
        return;
    };
    let (x1, y1) = from.center();
    let (x2, y2) = to.center();
    let (y1, y2) = (flip_y(y1, extent_y), flip_y(y2, extent_y));
    ctx.draw(&CanvasLine { x1, y1, x2, y2, color: EDGE_COLOR });

    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < f64::EPSILON {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let head_len = 1.2;
    let spread = 0.6;
    let tip = (x2, y2);
    let base = (x2 - ux * head_len, y2 - uy * head_len);
    let left = (base.0 - uy * spread, base.1 + ux * spread);
    let right = (base.0 + uy * spread, base.1 - ux * spread);
    ctx.draw(&CanvasLine { x1: tip.0, y1: tip.1, x2: left.0, y2: left.1, color: EDGE_COLOR });
    ctx.draw(&CanvasLine { x1: tip.0, y1: tip.1, x2: right.0, y2: right.1, color: EDGE_COLOR });
}
