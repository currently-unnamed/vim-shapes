//! Export: the diagram as a file somebody else can open.
//!
//! Five formats, two kinds. **PNG** is the raster: the terminal's own cells drawn through a
//! font, the one export that is exactly the picture on screen (see `render`). **SVG, PDF and
//! HTML** are vector: the same geometry the canvas paints — every outline segment, every
//! braille-sampled point, every label — written as lines, dots and text, so they scale and
//! the text is text. **XML** is the draw.io file, for editing on.
//!
//! One `Picture` feeds the three vector writers, built once from the document by the same
//! functions the canvas uses (`shapes::outline`, `shapes::relation`, `canvas::label_lines`),
//! which is what keeps five files and one screen from ever disagreeing about where a line is.

use std::fmt::Write as _;
use std::path::Path;

use ratatui::style::Color;

use crate::model::{Document, Node};
use crate::ontology::{mix, Colour, Layer, Paint};
use crate::shapes::{self, CurvePrimitive};
use crate::ui::{canvas, theme};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Format {
    #[default]
    Png,
    Svg,
    Pdf,
    Xml,
    Html,
    /// This tab, whole — every element and relation, exactly as `:w` would save it, but on
    /// its own rather than wrapped in the workspace it came from. What `:tabnew <path>` reads
    /// back as a new tab, on this document or another.
    Diagram,
    /// An ontology diagram as a Markdown reference — a wiki page generated from the picture.
    Markdown,
    /// An ontology diagram as a plain definition, in the SDK's casing.
    Definition,
}

impl Format {
    pub const ALL: [Format; 8] = [Format::Png, Format::Svg, Format::Pdf, Format::Xml, Format::Html, Format::Diagram, Format::Markdown, Format::Definition];

    pub fn name(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Svg => "svg",
            Format::Pdf => "pdf",
            Format::Xml => "xml",
            Format::Html => "html",
            Format::Diagram => "diagram",
            Format::Markdown => "md",
            Format::Definition => "json",
        }
    }

    pub fn tagline(self) -> &'static str {
        match self {
            Format::Png => "a picture — the terminal's cells drawn through a font",
            Format::Svg => "vector: lines, dots and text that scale",
            Format::Pdf => "vector, one page",
            Format::Xml => "a draw.io file, to edit on",
            Format::Html => "a web page with the vector picture in it",
            Format::Diagram => "this tab alone, lossless — :tabnew <path> reads it back as a new tab",
            Format::Markdown => "an ontology's reference as a wiki page: object types, properties, links, actions",
            Format::Definition => "an ontology's definition as JSON, in the SDK's casing",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Xml => "drawio",
            f => f.name(),
        }
    }

    pub fn parse(s: &str) -> Option<Format> {
        let want = s.trim().to_ascii_lowercase();
        Format::ALL.into_iter().find(|f| f.name() == want || f.extension() == want)
    }

    /// The format a path's extension asks for.
    pub fn of_path(p: &Path) -> Option<Format> {
        p.extension().and_then(|e| Format::parse(&e.to_string_lossy()))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Appearance {
    Dark,
    Light,
}

/// Which picture: the terminal's own, in braille and cells, or the clean one a desktop tool
/// would draw — solid outlines, filled arrowheads, sans-serif labels, a ruled grid.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Style {
    #[default]
    Terminal,
    Clean,
}

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::Terminal => "terminal",
            Style::Clean => "clean",
        }
    }
}

/// Everything the export dialog asks.
#[derive(Clone, Debug)]
pub struct Options {
    pub format: Format,
    /// Percent. 100 is a cell ten pixels wide and twenty tall, and 20 px type in the PNG.
    pub zoom: u32,
    pub transparent: bool,
    pub appearance: Appearance,
    /// Cells of margin round the diagram.
    pub border: u32,
    pub grid: bool,
    /// The PNG's font, when not each shape's own; `None` is the first monospace font found.
    pub font: Option<String>,
    pub style: Style,
}

impl Default for Options {
    fn default() -> Self {
        Options { format: Format::Png, zoom: 100, transparent: false, appearance: Appearance::Dark, border: 2, grid: false, font: None, style: Style::Terminal }
    }
}

impl Options {
    /// What `V` previews: the clean picture, on paper, no grid — the shapes and relations are
    /// what the eye should land on, not graph paper under them. The grid is still there to
    /// turn on from the export dialog; it just is not what a first look defaults to.
    pub fn preview() -> Options {
        Options { style: Style::Clean, appearance: Appearance::Light, grid: false, ..Options::default() }
    }
}

/// A cell in pixels at 100%: what draw.io's own export uses for a terminal cell's aspect.
pub const CELL_W: f64 = 10.0;
pub const CELL_H: f64 = 20.0;

impl Options {
    pub fn scale(&self) -> f64 {
        self.zoom.max(1) as f64 / 100.0
    }

    /// The picture's size in pixels for the vector formats and, near enough, the PNG.
    pub fn size_px(&self, doc: &Document) -> (u32, u32) {
        let (w, h) = extent(doc, self.border);
        ((w * CELL_W * self.scale()).round() as u32, (h * CELL_H * self.scale()).round() as u32)
    }
}

/// The diagram's extent in cells with `border` cells round it, and where its world starts.
pub fn frame_of(doc: &Document, border: u32) -> (f64, f64, f64, f64) {
    frame(doc, border)
}

fn frame(doc: &Document, border: u32) -> (f64, f64, f64, f64) {
    // Laid out on paper, the picture is the page — the way a page is printed.
    if doc.metadata.page.page_view {
        let (w, h) = doc.metadata.page.size();
        return (0.0, 0.0, w, h);
    }
    let (bx, by, br, bb) = doc.bounds().unwrap_or((0.0, 0.0, 20.0, 5.0));
    let b = border as f64;
    ((bx - b).floor(), (by - b).floor(), (br - bx + 2.0 * b).ceil().max(8.0), (bb - by + 2.0 * b).ceil().max(3.0))
}

fn extent(doc: &Document, border: u32) -> (f64, f64) {
    let (_, _, w, h) = frame(doc, border);
    (w, h)
}

// ─── the picture ────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Line { a: (f64, f64), b: (f64, f64), color: [u8; 3], width: f64 },
    Dot { p: (f64, f64), color: [u8; 3] },
    /// A run of joined segments, for the clean picture; dashed as a real dash pattern.
    /// `dash` is `(on, off)` in pixels; `None` is solid.
    Polyline { pts: Vec<(f64, f64)>, color: [u8; 3], width: f64, dash: Option<(f64, f64)> },
    /// A filled polygon, optionally stroked — an arrowhead, or a shape's paper fill.
    Polygon { pts: Vec<(f64, f64)>, fill: [u8; 3], stroke: Option<([u8; 3], f64)> },
    Text {
        x: f64,
        y: f64,
        text: String,
        color: [u8; 3],
        font: Option<String>,
        size: f64,
        bold: bool,
        italic: bool,
        underline: bool,
        /// A filled band under the text, in this colour.
        band: Option<[u8; 3]>,
    },
}

/// The diagram as pixels-to-be: a size, a ground, and items in pixel coordinates.
#[derive(Clone, Debug)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// `None` is transparent.
    pub ground: Option<[u8; 3]>,
    pub items: Vec<Item>,
}

fn rgb(c: Color, light: bool) -> [u8; 3] {
    match c {
        _ if light && c == theme::t().bright => [60, 56, 54],
        Color::Rgb(r, g, b) => [r, g, b],
        Color::White => if light { [60, 56, 54] } else { [235, 219, 178] },
        Color::DarkGray => [146, 131, 116],
        _ => if light { [60, 56, 54] } else { [235, 219, 178] },
    }
}

fn paint_rgb(c: Colour, light: bool) -> [u8; 3] {
    c.on(light)
}

/// Build the picture. Everything is placed by the functions the canvas draws with.
pub fn picture(doc: &Document, o: &Options) -> Picture {
    let light = o.appearance == Appearance::Light;
    let (ox, oy, wc, hc) = frame(doc, o.border);
    let s = o.scale();
    let px = |x: f64| (x - ox) * CELL_W * s;
    let py = |y: f64| (y - oy) * CELL_H * s;
    let mut items = Vec::new();

    if o.grid {
        let dot = doc.metadata.page.grid_color.map(|c| c.on(light)).unwrap_or(if light { [200, 190, 170] } else { [60, 56, 54] });
        let (gx, gy) = doc.metadata.page.grid_step();
        let (gx, gy) = (gx as f64, gy as f64);
        if doc.metadata.page.grid_style.ruled_on_screen() {
            // Ruled: a thin line along every grid row and column.
            let mut y = (oy / gy).ceil() * gy;
            while y < oy + hc {
                items.push(Item::Line { a: (px(ox), py(y + 0.5)), b: (px(ox + wc), py(y + 0.5)), color: dot, width: 0.5 });
                y += gy;
            }
            let mut x = (ox / gx).ceil() * gx;
            while x < ox + wc {
                items.push(Item::Line { a: (px(x + 0.5), py(oy)), b: (px(x + 0.5), py(oy + hc)), color: dot, width: 0.5 });
                x += gx;
            }
        } else {
            let mut y = (oy / gy).ceil() * gy;
            while y < oy + hc {
                let mut x = (ox / gx).ceil() * gx;
                while x < ox + wc {
                    items.push(Item::Dot { p: (px(x + 0.5), py(y + 0.5)), color: dot });
                    x += gx;
                }
                y += gy;
            }
        }
    }

    let refused: Vec<_> = doc.lint().into_iter().map(|p| p.relation).collect();
    for r in &doc.relations {
        let Some(pts) = doc.route(r) else { continue };
        let n = r.notation();
        let ground_rgb = doc.metadata.page.background.map(|c| c.on(light)).unwrap_or(if light { [251, 241, 199] } else { rgb(theme::t().panel, false) });
        let color = if refused.contains(&r.id) { paint_rgb(Paint::Red.into(), light) } else { mix(n.color.map(|c| paint_rgb(c, light)).unwrap_or(rgb(theme::t().structure, light)), ground_rgb, 1.0 - n.opacity as f64 / 100.0) };
        let (lines, points) = shapes::relation_along(&pts, &n);
        for (a, b) in lines {
            items.push(Item::Line { a: (px(a.0), py(a.1)), b: (px(b.0), py(b.1)), color, width: s.max(0.5) });
        }
        for p in points {
            items.push(Item::Dot { p: (px(p.0), py(p.1)), color });
        }
        for node in Node::ALL {
            if let (Some(text), Some((cx, cy))) = (r.label_at(node), doc.label_point(r, node)) {
                let w = text.chars().count() as f64;
                let t = &r.text;
                let color = t.color.map(|c| c.on(light)).unwrap_or(color);
                let band = t.band.then_some(if light { [230, 220, 190] } else { [60, 56, 54] });
                items.push(Item::Text { x: px(cx - w / 2.0), y: py(cy), text: text.to_string(), color, font: t.font.clone(), size: t.size.map_or(16.0 * s, |n| n as f64 * s * 0.8), bold: t.bold, italic: t.italic, underline: false, band });
            }
        }
    }

    let page = &doc.metadata.page;
    let ground_rgb = page.background.map(|c| c.on(light)).unwrap_or(if light { [251, 241, 199] } else { rgb(theme::t().panel, false) });
    for e in &doc.elements {
        let color = e.color.map(|c| paint_rgb(c, light)).unwrap_or(rgb(theme::layer_color(e.kind.layer()), light));
        let color = mix(color, ground_rgb, 1.0 - e.drawn_opacity() as f64 / 100.0);
        let width = (e.stroke as f64).max(1.0) * s;
        // A shadow: the outline again, a little down and right, in the dimmest grey.
        if page.shadow && e.casts_shadow() {
            let shade = rgb(Color::DarkGray, light);
            for prim in shapes::drawn_at(page, e, e.x + 0.6, e.y + 0.3, e.w, e.h, 0.35) {
                if let CurvePrimitive::Lines(ls) = prim {
                    for (a, b) in ls {
                        items.push(Item::Line { a: (px(a.0), py(a.1)), b: (px(b.0), py(b.1)), color: shade, width });
                    }
                }
            }
        }
        let prims = if e.outline { shapes::patterned(shapes::drawn(page, e, 0.35), e.drawn_line()) } else { Vec::new() };
        for prim in prims {
            match prim {
                CurvePrimitive::Lines(ls) => {
                    for (a, b) in ls {
                        items.push(Item::Line { a: (px(a.0), py(a.1)), b: (px(b.0), py(b.1)), color, width });
                    }
                }
                CurvePrimitive::Points(ps) => {
                    for p in ps {
                        items.push(Item::Dot { p: (px(p.0), py(p.1)), color });
                    }
                }
            }
        }
        let ink = rgb(Color::White, light);
        let dim = rgb(Color::DarkGray, light);
        if e.kind.is_composite() {
            items.push(Item::Text { x: px(e.x + 3.0), y: py(e.y), text: e.kind.short().into(), color: dim, font: None, size: 14.0 * s, bold: false, italic: false, underline: false, band: None });
            if !e.label.is_empty() {
                items.push(Item::Text { x: px(e.x + 3.0), y: py(e.y + 1.0), text: e.label.clone(), color: ink, font: e.text.font.clone(), size: 16.0 * s, bold: true, italic: false, underline: false, band: None });
            }
            continue;
        }
        if !e.kind.is_sketch() && e.h >= 4.0 {
            let tw = e.kind.short().chars().count() as f64;
            items.push(Item::Text { x: px(e.x + (e.w - tw) / 2.0), y: py(e.y + 1.0), text: e.kind.short().into(), color: dim, font: None, size: 14.0 * s, bold: false, italic: false, underline: false, band: None });
            // The deprecated mark after the kind, in red, as the screen draws it: a cell
            // font advances 0.6 em a character, and the mark sits one space past the kind.
            if let Some(m) = e.status.mark(false) {
                let x = px(e.x + (e.w - tw) / 2.0) + (tw + 1.0) * 14.0 * s * 0.6;
                items.push(Item::Text { x, y: py(e.y + 1.0), text: m.into(), color: paint_rgb(Paint::Red.into(), light), font: None, size: 14.0 * s, bold: true, italic: false, underline: false, band: None });
            }
        }
        let size = e.text.size.map(|n| n as f64 * s * 0.8).unwrap_or(16.0 * s);
        let t = &e.text;
        let ink = t.color.map(|c| c.on(light)).unwrap_or(ink);
        let band = t.band.then_some(if light { [230, 220, 190] } else { [60, 56, 54] });
        for (x, y, line) in canvas::label_lines(e, &e.label) {
            items.push(Item::Text { x: px(x), y: py(y), text: line, color: ink, font: t.font.clone(), size, bold: t.bold, italic: t.italic, underline: t.underline, band });
        }
        // The compartment, as the screen draws it: a rule, then the rows in the cell font.
        if let Some(ry) = e.header_rule() {
            items.push(Item::Line { a: (px(e.x + 1.0), py(ry + 0.5)), b: (px(e.right() - 1.0), py(ry + 0.5)), color, width: s.max(0.5) });
            let gone = crate::model::Status::Deprecated.mark(false).unwrap_or_default();
            for (x, y, line) in e.row_lines() {
                // A row opens with its mark: a deprecated one's ✗ is set apart, in red, and a
                // space holds its cell so the row's columns do not move.
                let line = match line.strip_prefix(gone) {
                    Some(rest) => {
                        items.push(Item::Text { x: px(x), y: py(y), text: gone.into(), color: paint_rgb(Paint::Red.into(), light), font: None, size: 16.0 * s, bold: true, italic: false, underline: false, band: None });
                        format!(" {rest}")
                    }
                    None => line,
                };
                items.push(Item::Text { x: px(x), y: py(y), text: line, color: ink, font: None, size: 16.0 * s, bold: false, italic: false, underline: false, band: None });
            }
        }
    }

    let (width, height) = o.size_px(doc);
    let ground = if o.transparent { None } else { Some(ground_rgb) };
    Picture { width, height, ground, items }
}

// ─── writers ────────────────────────────────────────────────────────────

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A row's baseline: text sits on the lower part of its cell.
fn baseline(y: f64, size: f64) -> f64 {
    y + CELL_H * (size / 16.0) * 0.72
}

pub fn svg(p: &Picture) -> String {
    let mut s = String::new();
    let _ = writeln!(s, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#, p.width, p.height, p.width, p.height);
    if let Some(g) = p.ground {
        let _ = writeln!(s, r#"  <rect width="100%" height="100%" fill="{}"/>"#, hex(g));
    }
    for it in &p.items {
        match it {
            Item::Line { a, b, color, width } => {
                let _ = writeln!(s, r#"  <line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="{:.1}" stroke-linecap="round"/>"#, a.0, a.1, b.0, b.1, hex(*color), width * 1.5);
            }
            Item::Dot { p: (x, y), color } => {
                let _ = writeln!(s, r#"  <circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="{}"/>"#, x, y, (p.width as f64 / 400.0).clamp(0.9, 3.0), hex(*color));
            }
            Item::Polyline { pts, color, width, dash } => {
                let d: Vec<String> = pts.iter().map(|(x, y)| format!("{x:.1},{y:.1}")).collect();
                let dash = dash.map(|(on, off)| format!(r#" stroke-dasharray="{on:.1} {off:.1}""#)).unwrap_or_default();
                let _ = writeln!(s, r#"  <polyline points="{}" fill="none" stroke="{}" stroke-width="{:.1}" stroke-linecap="round" stroke-linejoin="round"{dash}/>"#, d.join(" "), hex(*color), width);
            }
            Item::Polygon { pts, fill, stroke } => {
                let d: Vec<String> = pts.iter().map(|(x, y)| format!("{x:.1},{y:.1}")).collect();
                let st = match stroke {
                    Some((c, w)) => format!(r#" stroke="{}" stroke-width="{:.1}" stroke-linejoin="round""#, hex(*c), w),
                    None => String::new(),
                };
                let _ = writeln!(s, r#"  <polygon points="{}" fill="{}"{st}/>"#, d.join(" "), hex(*fill));
            }
            Item::Text { x, y, text, color, font, size, bold, italic, underline, band } => {
                let mono = p.items.iter().any(|i| matches!(i, Item::Dot { .. } | Item::Line { .. }));
                // The band: a box the text's rough width, a little taller than the type.
                if let Some(b) = band {
                    let w = text.chars().count() as f64 * size * if mono { 0.6 } else { 0.55 };
                    let _ = writeln!(s, r#"  <rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{}"/>"#, x - 2.0, baseline(*y, *size) - size * 0.9, w + 4.0, size * 1.2, hex(*b));
                }
                let family = match (font, mono) {
                    (Some(f), true) => format!("'{}', Menlo, 'DejaVu Sans Mono', monospace", xml_escape(f)),
                    (Some(f), false) => format!("'{}', Helvetica, Arial, sans-serif", xml_escape(f)),
                    (None, true) => "Menlo, 'DejaVu Sans Mono', monospace".into(),
                    (None, false) => "Helvetica, Arial, sans-serif".into(),
                };
                let _ = writeln!(
                    s,
                    r#"  <text x="{:.1}" y="{:.1}" fill="{}" font-family="{}" font-size="{:.1}"{}{}{}>{}</text>"#,
                    x,
                    baseline(*y, *size),
                    hex(*color),
                    family,
                    size,
                    if *bold { r#" font-weight="bold""# } else { "" },
                    if *italic { r#" font-style="italic""# } else { "" },
                    if *underline { r#" text-decoration="underline""# } else { "" },
                    xml_escape(text)
                );
            }
        }
    }
    s.push_str("</svg>\n");
    s
}

pub fn html(p: &Picture, title: &str) -> String {
    let bg = p.ground.map(hex).unwrap_or_else(|| "transparent".into());
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{}</title>\n<style>body{{margin:0;background:{bg};display:flex;justify-content:center;align-items:center;min-height:100vh}}svg{{max-width:100%;height:auto}}</style>\n</head><body>\n{}</body></html>\n",
        xml_escape(title),
        svg(p)
    )
}

/// A one-page PDF, written by hand: lines, dots and text in Courier. No compression, no
/// embedded fonts — a file any reader opens, from a writer small enough to read.
pub fn pdf(p: &Picture) -> Vec<u8> {
    let clean = p.items.iter().any(|i| matches!(i, Item::Polygon { .. } | Item::Polyline { .. }));
    let (w, h) = (p.width as f64, p.height as f64);
    let flip = |y: f64| h - y;
    let mut c = String::new();
    if let Some(g) = p.ground {
        let _ = writeln!(c, "{} {} {} rg 0 0 {w:.1} {h:.1} re f", g[0] as f64 / 255.0, g[1] as f64 / 255.0, g[2] as f64 / 255.0);
    }
    let col = |k: [u8; 3]| format!("{:.3} {:.3} {:.3}", k[0] as f64 / 255.0, k[1] as f64 / 255.0, k[2] as f64 / 255.0);
    for it in &p.items {
        match it {
            Item::Line { a, b, color, width } => {
                let _ = writeln!(c, "{} RG {:.2} w 1 J {:.1} {:.1} m {:.1} {:.1} l S", col(*color), width * 1.5, a.0, flip(a.1), b.0, flip(b.1));
            }
            Item::Dot { p: (x, y), color } => {
                let r = (w / 400.0).clamp(0.9, 3.0);
                let _ = writeln!(c, "{} rg {:.1} {:.1} {:.1} {:.1} re f", col(*color), x - r / 2.0, flip(*y) - r / 2.0, r, r);
            }
            Item::Polyline { pts, color, width, dash } => {
                if pts.len() < 2 {
                    continue;
                }
                let dash = dash.map(|(on, off)| format!("[{on:.1} {off:.1}] 0 d ")).unwrap_or_else(|| "[] 0 d ".into());
                let _ = write!(c, "{} RG {:.2} w 1 J 1 j {dash}{:.1} {:.1} m", col(*color), width, pts[0].0, flip(pts[0].1));
                for q in &pts[1..] {
                    let _ = write!(c, " {:.1} {:.1} l", q.0, flip(q.1));
                }
                let _ = writeln!(c, " S");
            }
            Item::Polygon { pts, fill, stroke } => {
                if pts.len() < 3 {
                    continue;
                }
                let _ = write!(c, "{} rg [] 0 d {:.1} {:.1} m", col(*fill), pts[0].0, flip(pts[0].1));
                for q in &pts[1..] {
                    let _ = write!(c, " {:.1} {:.1} l", q.0, flip(q.1));
                }
                match stroke {
                    Some((sc, w)) => {
                        let _ = writeln!(c, " h {} RG {:.2} w B", col(*sc), w);
                    }
                    None => {
                        let _ = writeln!(c, " h f");
                    }
                }
            }
            Item::Text { x, y, text, color, size, bold, italic, underline, band, .. } => {
                // Courier is the one font every reader has and no file has to carry; what it
                // cannot show — a glyph outside Latin-1 — becomes a question mark rather than
                // a missing box, which at least says something was there.
                let latin: String = text.chars().map(|ch| if (ch as u32) < 256 { ch } else { '?' }).collect();
                let esc = latin.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
                // The four faces every reader has: regular, bold, oblique, bold oblique.
                let font = match (bold, italic) {
                    (false, false) => "/F1",
                    (true, false) => "/F2",
                    (false, true) => "/F3",
                    (true, true) => "/F4",
                };
                let w = latin.chars().count() as f64 * size * if clean { 0.55 } else { 0.6 };
                let base = flip(baseline(*y, *size));
                if let Some(b) = band {
                    let _ = writeln!(c, "{} rg {:.1} {:.1} {:.1} {:.1} re f", col(*b), x - 2.0, base - size * 0.3, w + 4.0, size * 1.2);
                }
                let _ = writeln!(c, "BT {} rg {font} {:.1} Tf {:.1} {:.1} Td ({esc}) Tj ET", col(*color), size, x, base);
                if *underline {
                    let _ = writeln!(c, "{} RG 0.8 w {:.1} {:.1} m {:.1} {:.1} l S", col(*color), x, base - size * 0.15, x + w, base - size * 0.15);
                }
            }
        }
    }
    let mut out: Vec<u8> = Vec::new();
    let mut offsets = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");
    let objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.1} {h:.1}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R /F3 7 0 R /F4 8 0 R >> >> >>"),
        format!("<< /Length {} >>\nstream\n{c}endstream", c.len()),
        // Courier for the terminal picture, Helvetica for the clean one: every reader has both,
        // in all four faces.
        format!("<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>", if clean { "Helvetica" } else { "Courier" }),
        format!("<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>", if clean { "Helvetica-Bold" } else { "Courier-Bold" }),
        format!("<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>", if clean { "Helvetica-Oblique" } else { "Courier-Oblique" }),
        format!("<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>", if clean { "Helvetica-BoldOblique" } else { "Courier-BoldOblique" }),
    ];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{obj}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

/// Write the diagram in `o.format` to `path`. Returns a line saying what was written.
pub fn write(doc: &Document, title: &str, o: &Options, path: &Path) -> Result<String, String> {
    let build = |o: &Options| if o.style == Style::Clean { crate::clean::picture(doc, o) } else { picture(doc, o) };
    match o.format {
        Format::Png if o.style == Style::Clean => {
            let p = build(o);
            let img = crate::clean::raster(&p);
            img.save(path).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(format!("{}x{} px → {}", img.width(), img.height(), path.display()))
        }
        Format::Png => {
            let px = 20.0 * o.scale() as f32;
            let (cw, ch, w, h) = crate::render::to_png(doc, o, px, path)?;
            Ok(format!("{cw}x{ch} cells → {w}x{h} px → {}", path.display()))
        }
        Format::Xml => {
            crate::drawio_export::export(doc, path).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(format!("{} elements, {} relations → {}", doc.elements.len(), doc.relations.len(), path.display()))
        }
        Format::Diagram => {
            let text = serde_json::to_string_pretty(doc).expect("a document always serializes");
            std::fs::write(path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(format!("{} elements, {} relations → {}", doc.elements.len(), doc.relations.len(), path.display()))
        }
        Format::Markdown => {
            std::fs::write(path, crate::ontology::doc::markdown(doc)).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(format!("{} types documented → {}", doc.elements.iter().filter(|e| e.kind.layer() == Layer::Ontology).count(), path.display()))
        }
        Format::Definition => {
            let text = serde_json::to_string_pretty(&crate::ontology::doc::definition(doc)).map_err(|e| e.to_string())?;
            std::fs::write(path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(format!("{} types defined → {}", doc.elements.iter().filter(|e| e.kind.layer() == Layer::Ontology).count(), path.display()))
        }
        f => {
            let p = build(o);
            let bytes = match f {
                Format::Svg => svg(&p).into_bytes(),
                Format::Html => html(&p, title).into_bytes(),
                Format::Pdf => pdf(&p),
                _ => unreachable!(),
            };
            std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(format!("{}x{} px → {}", p.width, p.height, path.display()))
        }
    }
}

/// Open a file with whatever the system opens it with — the preview's other half.
pub fn open_with_system(path: &Path) -> Result<(), String> {
    let cmd = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(cmd)
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not run {cmd}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::{RelationKind, ShapeKind};

    fn doc() -> Document {
        let mut d = Document::default();
        let a = d.add(ShapeKind::ApplicationComponent, "Billing", 10.0, 6.0);
        let b = d.add(ShapeKind::DataObject, "invoice <v2>", 40.0, 6.0);
        let r = d.connect(RelationKind::Access, a, b).unwrap();
        d.relation_mut(r).unwrap().label = Some("reads".into());
        d
    }

    #[test]
    fn the_picture_is_the_diagram_with_a_border_at_the_zoom_asked() {
        let o = Options::default();
        let p = picture(&doc(), &o);
        // 10..60 across, 6..12 down, two cells of border: 54 × 10 cells.
        assert_eq!((p.width, p.height), (540, 200));
        assert!(p.items.iter().any(|i| matches!(i, Item::Text { text, .. } if text == "Billing")));
        assert!(p.items.iter().any(|i| matches!(i, Item::Text { text, .. } if text == "reads")));
        let o2 = Options { zoom: 200, ..o };
        assert_eq!(picture(&doc(), &o2).width, 1080, "zoom scales the picture");
        let o3 = Options { transparent: true, ..Options::default() };
        assert!(picture(&doc(), &o3).ground.is_none());
    }

    #[test]
    fn on_paper_the_picture_is_the_page_with_the_ground_and_shadows_the_diagram_asks() {
        let mut d = doc();
        // A rounded corner only replaces `Shape::Rectangle`; a component now carries its own
        // side-rails instead, so swap it for a plain rectangle kind to still exercise that.
        let billing = d.elements.iter().find(|e| e.label == "Billing").unwrap().id;
        d.element_mut(billing).unwrap().kind = ShapeKind::Artifact;
        d.metadata.page.page_view = true;
        d.metadata.page.paper = crate::model::Paper::A5;
        d.metadata.page.background = Some(Colour::Hex([10, 20, 30]));
        d.metadata.page.shadow = true;
        d.metadata.page.rounded = true;
        let o = Options { border: 9, ..Options::default() };
        let p = picture(&d, &o);
        assert_eq!((p.width, p.height), (580, 820), "an A5 page at 100 %, the border ignored");
        assert_eq!(p.ground, Some([10, 20, 30]));
        let plain = picture(&doc(), &Options::default());
        let lines = |p: &Picture| p.items.iter().filter(|i| matches!(i, Item::Line { .. })).count();
        let dots = |p: &Picture| p.items.iter().filter(|i| matches!(i, Item::Dot { .. })).count();
        assert!(lines(&p) > lines(&plain), "the shadows are more lines");
        assert!(dots(&p) > dots(&plain), "and rounded corners are dots");
        assert_eq!(frame(&d, 9), (0.0, 0.0, 58.0, 41.0));
    }

    #[test]
    fn the_terminal_picture_rules_its_grid_when_the_diagram_says_lines() {
        let mut d = doc();
        let o = Options { grid: true, ..Options::default() };
        let dots = picture(&d, &o).items.iter().filter(|i| matches!(i, Item::Dot { .. })).count();
        d.metadata.page.grid_style = crate::model::GridStyle::Lines;
        let p = picture(&d, &o);
        let thin = p.items.iter().filter(|i| matches!(i, Item::Line { width, .. } if *width == 0.5)).count();
        assert!(thin > 4 && p.items.iter().filter(|i| matches!(i, Item::Dot { .. })).count() < dots, "rules instead of dots");
    }

    #[test]
    fn a_label_s_look_reaches_the_svg_and_the_pdf_in_their_own_words() {
        let mut d = doc();
        let id = d.elements[0].id;
        let t = &mut d.element_mut(id).unwrap().text;
        t.bold = true;
        t.italic = true;
        t.underline = true;
        t.band = true;
        t.color = Some(Colour::Hex([10, 20, 30]));
        let p = picture(&d, &Options::default());
        assert!(p.items.iter().any(|i| matches!(i, Item::Text { bold: true, italic: true, underline: true, band: Some(_), color: [10, 20, 30], .. })));
        let s = svg(&p);
        assert!(s.contains(r#"font-weight="bold" font-style="italic" text-decoration="underline""#), "{s}");
        assert!(s.contains("<rect"), "a band under the label");
        let pdf = String::from_utf8_lossy(&pdf(&p)).to_string();
        assert!(pdf.contains("/F4 ") && pdf.contains("Courier-BoldOblique") && pdf.contains(" re f") && pdf.contains(" l S"), "{}", &pdf[..600]);
    }

    #[test]
    fn svg_and_html_are_well_formed_and_escaped() {
        let s = svg(&picture(&doc(), &Options::default()));
        assert!(s.starts_with("<svg") && s.trim_end().ends_with("</svg>"));
        assert!(s.contains("&lt;v2&gt;"), "labels are escaped: {s}");
        assert!(s.contains("<line") && s.contains("<circle") && s.contains("<text"));
        let h = html(&picture(&doc(), &Options::default()), "claims & co");
        assert!(h.starts_with("<!doctype html>") && h.contains("<title>claims &amp; co</title>") && h.contains("<svg"));
    }

    #[test]
    fn the_pdf_has_its_objects_where_the_xref_says() {
        let bytes = pdf(&picture(&doc(), &Options::default()));
        let text = String::from_utf8_lossy(&bytes).to_string();
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("(Billing) Tj"));
        assert!(text.contains("(invoice <v2>) Tj"), "parentheses are what needs escaping, not angle brackets");
        // Every xref offset points at "N 0 obj".
        let xref_at: usize = text.rsplit("startxref\n").next().unwrap().trim().lines().next().unwrap().parse().unwrap();
        let table = &text[xref_at..];
        // "xref", "0 7", the free entry, then one line per object.
        for (i, line) in table.lines().skip(3).take(6).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(text[off..].starts_with(&format!("{} 0 obj", i + 1)), "object {} is where the xref says", i + 1);
        }
    }

    #[test]
    fn a_format_is_known_by_its_extension() {
        assert_eq!(Format::of_path(Path::new("a.drawio")), Some(Format::Xml));
        assert_eq!(Format::of_path(Path::new("a.SVG")), Some(Format::Svg));
        assert_eq!(Format::of_path(Path::new("a.txt")), None);
        assert_eq!(Format::parse("html"), Some(Format::Html));
    }

    #[test]
    fn every_vector_format_writes_a_file() {
        let d = doc();
        for f in [Format::Svg, Format::Html, Format::Pdf, Format::Xml] {
            let mut path = std::env::temp_dir();
            path.push(format!("vim-shapes-export-{}.{}", std::process::id(), f.extension()));
            let o = Options { format: f, ..Options::default() };
            let said = write(&d, "t", &o, &path).expect("write");
            assert!(said.contains(&path.display().to_string()));
            assert!(std::fs::metadata(&path).unwrap().len() > 100);
            std::fs::remove_file(&path).ok();
        }
    }

    #[test]
    fn diagram_format_writes_a_bare_document_that_reads_back_identical() {
        let d = doc();
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-export-{}.diagram", std::process::id()));
        let o = Options { format: Format::Diagram, ..Options::default() };
        write(&d, "t", &o, &path).expect("write");
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let read_back: Document = serde_json::from_str(&text).expect("the same shape a bare diagram file already opens as");
        assert_eq!(read_back, d, "lossless — every field, not just the picture");
    }
}
