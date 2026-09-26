//! The diagram, rendered: the cell grid rasterised through a real monospace font into a PNG.
//!
//! The tempting shortcut is to draw the shapes again with lines and curves at pixel resolution.
//! That produces a nicer picture and a DIFFERENT picture from the one on screen, which breaks
//! the only claim this export makes — that the thing in the terminal and the thing in the file
//! are one rendering. So the cells are drawn exactly as the terminal draws them: every glyph,
//! braille included, through a font, at a cell size the font's own metrics decide.
//!
//! Braille is the catch. Most monospace fonts have no braille block, and a terminal quietly
//! falls back to one that does; so does this — a small stack of fonts, the first that has a
//! glyph drawing it — which is why a rendering here matches what a terminal showed.

use image::{Rgba, RgbaImage};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;

use crate::fonts;
use crate::model::{Align, Document, Node};
use crate::ui::{canvas, theme};

/// Fonts a terminal would be set in, and the symbol fonts it falls back to for braille.
pub const FONT_PATHS: &[&str] = &[
    "/System/Library/Fonts/SFNSMono.ttf",
    "/System/Library/Fonts/Menlo.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/System/Library/Fonts/Apple Symbols.ttf",
    "/System/Library/Fonts/Supplemental/Apple Symbols.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
];

/// The most cells a rendering may span either way. A diagram wider than this is a diagram
/// that wants splitting into tabs, and a 400-column render at 20 px is already 4,000 wide.
const MAX_CELLS: u16 = 400;

/// Where the cells' world starts: the diagram's top-left, `border` cells in.
fn origin(doc: &Document, border: f64) -> (f64, f64) {
    let (bx, by, _, _) = doc.bounds().unwrap_or((0.0, 0.0, 20.0, 5.0));
    ((bx - border).floor(), (by - border).floor())
}

/// Lay the diagram out as cells: the whole diagram, framed by its bounds with `border` cells
/// round it, no chrome. With `labels` off the shapes' labels are left for the rasteriser,
/// which sets each in its own font.
pub fn cells(doc: &Document, grid: bool, labels: bool, border: f64) -> Buffer {
    let (bx, by, br, bb) = doc.bounds().unwrap_or((0.0, 0.0, 20.0, 5.0));
    let w = ((br - bx + border * 2.0).ceil() as u16).clamp(8, MAX_CELLS);
    let h = ((bb - by + border * 2.0).ceil() as u16).clamp(3, MAX_CELLS);
    let area = Rect::new(0, 0, w, h);
    let mut buf = Buffer::empty(area);
    let refused: Vec<_> = doc.lint().into_iter().map(|p| p.relation).collect();
    canvas::Scene {
        doc,
        cursor: None,
        focus_rel: None,
        focus_node: Node::Centre,
        holding: None,
        picked: &[],
        camera: origin(doc, border),
        letters: None,
        insert: None,
        refused: &refused,
        reshape: None,
        labels,
        grid,
        ink: crate::ui::wire::ink(),
    }
    .render(area, &mut buf);
    buf
}

/// The first font that exists — the explicit one if it was given and is there.
pub fn find_font(explicit: Option<&str>) -> Option<String> {
    if let Some(p) = explicit {
        return std::path::Path::new(p).is_file().then(|| p.to_string());
    }
    FONT_PATHS.iter().find(|p| std::path::Path::new(p).is_file()).map(|p| p.to_string())
}

fn load(path: &str) -> Option<fontdue::Font> {
    let bytes = std::fs::read(path).ok()?;
    fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()
}

/// A terminal colour as pixels. The named colours are what the theme leaves to the terminal,
/// so they take the values a dark terminal would give them — or, in a light rendering, the
/// dark ink a light terminal would.
fn rgb(c: Color, fallback: [u8; 3]) -> [u8; 3] {
    match c {
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Black => [0, 0, 0],
        Color::White => [235, 219, 178],
        Color::Gray => [189, 174, 147],
        Color::DarkGray => [146, 131, 116],
        Color::Red => [204, 36, 29],
        Color::Green => [142, 192, 124],
        Color::Yellow => [215, 153, 33],
        Color::Blue => [69, 133, 136],
        Color::Magenta => [211, 134, 155],
        Color::Cyan => [131, 165, 152],
        _ => fallback,
    }
}

/// A cell's size in pixels, the way a terminal decides it: the advance across, ascent +
/// descent + gap down, and where the baseline sits.
fn cell_metrics(font: &fontdue::Font, px: f32) -> Result<(u32, u32, i32), String> {
    let m = font.horizontal_line_metrics(px).ok_or("the font has no line metrics")?;
    let cw = font.metrics('M', px).advance_width.ceil().max(1.0) as u32;
    let chh = (m.ascent - m.descent + m.line_gap).ceil().max(1.0) as u32;
    Ok((cw, chh, m.ascent.round() as i32))
}

/// Draw one glyph at `px` with its origin's baseline at `(x, baseline_y)`, blended over
/// whatever is already there. Coverage blends the glyph's colour over the ground, which is
/// what makes antialiased type read as type rather than as a stencil.
fn glyph(out: &mut RgbaImage, f: &fontdue::Font, ch: char, px: f32, x: i32, baseline_y: i32, fg: [u8; 3]) -> f32 {
    glyph_look(out, f, ch, px, x, baseline_y, fg, false, false)
}

/// A glyph with a faked look: `heavy` draws it twice a pixel apart, `slant` shifts each row
/// by its height above the baseline — for a family without the face.
#[allow(clippy::too_many_arguments)]
fn glyph_look(out: &mut RgbaImage, f: &fontdue::Font, ch: char, px: f32, x: i32, baseline_y: i32, fg: [u8; 3], heavy: bool, slant: bool) -> f32 {
    let (met, bitmap) = f.rasterize(ch, px);
    if met.width == 0 || met.height == 0 {
        return met.advance_width + if heavy { 1.0 } else { 0.0 };
    }
    let gy = baseline_y - met.height as i32 - met.ymin;
    for y in 0..met.height {
        let shear = if slant { ((gy + y as i32 - baseline_y) as f32 * -0.2).round() as i32 } else { 0 };
        for xx in 0..met.width {
            let a = bitmap[y * met.width + xx] as f32 / 255.0;
            if a <= 0.0 {
                continue;
            }
            let (px_, py) = (x + met.xmin + xx as i32 + shear, gy + y as i32);
            if px_ < 0 || py < 0 || px_ as u32 >= out.width() || py as u32 >= out.height() {
                continue;
            }
            let bg = out.get_pixel(px_ as u32, py as u32).0;
            // Over a transparent ground the glyph's own alpha is its coverage.
            let mut v = [0u8; 4];
            let ba = bg[3] as f32 / 255.0;
            for k in 0..3 {
                v[k] = if ba > 0.0 { (bg[k] as f32 + (fg[k] as f32 - bg[k] as f32) * a).round() as u8 } else { fg[k] };
            }
            v[3] = ((ba + a * (1.0 - ba)) * 255.0).round() as u8;
            out.put_pixel(px_ as u32, py as u32, Rgba(v));
            if heavy && (px_ as u32 + 1) < out.width() {
                out.put_pixel(px_ as u32 + 1, py as u32, Rgba(v));
            }
        }
    }
    met.advance_width + if heavy { 1.0 } else { 0.0 }
}

/// A solid box, for a label's band.
fn fill(out: &mut RgbaImage, x0: i32, y0: i32, w: i32, h: i32, c: [u8; 3]) {
    for y in y0.max(0)..(y0 + h).min(out.height() as i32) {
        for x in x0.max(0)..(x0 + w).min(out.width() as i32) {
            out.put_pixel(x as u32, y as u32, Rgba([c[0], c[1], c[2], 255]));
        }
    }
}

/// The shapes' labels, each set in its own font and size — or the rendering's — and placed by
/// its alignment inside the shape's pixel box.
#[allow(clippy::too_many_arguments)]
fn labels(out: &mut RgbaImage, doc: &Document, primary: &fontdue::Font, px: f32, cw: u32, chh: u32, baseline: i32, border: f64, light: bool) {
    let (ox, oy) = origin(doc, border);
    let fg = if light { [60, 56, 54] } else { [235, 219, 178] };
    let mut cache: Vec<((String, bool, bool), Option<fontdue::Font>)> = Vec::new();
    for e in &doc.elements {
        if e.label.trim().is_empty() || e.kind.is_composite() {
            continue;
        }
        let t = &e.text;
        let key = t.font.as_deref().map(|name| (name.to_string(), t.bold, t.italic));
        let own: Option<&fontdue::Font> = key.as_ref().and_then(|k| {
            if !cache.iter().any(|(n, _)| n == k) {
                cache.push((k.clone(), fonts::face_of(&k.0, k.1, k.2).and_then(|p| load(&p.to_string_lossy()))));
            }
            cache.iter().find(|(n, _)| n == k).and_then(|(_, f)| f.as_ref())
        });
        // The cell font has no faces of its own: bold and italic are faked on it.
        let (heavy, slant) = (t.bold && own.is_none(), t.italic && own.is_none());
        let f = own.unwrap_or(primary);
        let fg = t.color.map(|c| c.on(light)).unwrap_or(fg);
        let band = t.band.then_some(if light { [230, 220, 190] } else { [60, 56, 54] });
        let size = e.text.size.map(|s| s as f32).unwrap_or(px);
        // The line a cell row's baseline sits on, at this size: the row's own baseline, or —
        // for bigger type — the row's bottom, so a large label grows upward into the box.
        let base_at = |row: f64| -> i32 {
            let top = ((row - oy) * chh as f64) as i32;
            if size <= px { top + baseline } else { top + chh as i32 - 2 }
        };
        for (x, y, line) in canvas::label_lines(e, &e.label) {
            let width: f32 = line.chars().map(|c| f.metrics(c, size).advance_width).sum();
            // Horizontal placement is re-done in pixels, since a different size changes how
            // wide the text really is; the cell-based x is right only for the cell font.
            let left_px = ((e.x + 1.0 - ox) * cw as f64) as f32;
            let right_px = ((e.right() - 1.0 - ox) * cw as f64) as f32;
            let mut cx = match e.text.align {
                Align::Left => left_px,
                Align::Centre => (left_px + right_px - width) / 2.0,
                Align::Right => right_px - width,
            }
            .max(left_px);
            let _ = x;
            let by = base_at(y);
            if let Some(b) = band {
                fill(out, cx.round() as i32 - 2, by - (size * 0.9) as i32, width as i32 + 4, (size * 1.2) as i32, b);
            }
            let start = cx;
            for ch in line.chars() {
                cx += glyph_look(out, f, ch, size, cx.round() as i32, by, fg, heavy, slant);
            }
            if t.underline {
                fill(out, start.round() as i32, by + (size * 0.1) as i32, (cx - start) as i32, (size / 14.0).max(1.0) as i32, fg);
            }
        }
    }
}

/// Rasterise a cell grid at `px` pixels per em through `font`, falling back through the
/// symbol fonts for any glyph it lacks. With `doc`, the shapes' labels are set afterwards in
/// their own fonts and sizes.
#[cfg(test)]
pub fn rasterize(buf: &Buffer, px: f32, font: &str, doc: Option<&Document>) -> Result<RgbaImage, String> {
    rasterize_with(buf, px, font, doc, &crate::export::Options::default())
}

/// The rasteriser with the export dialog's choices: a transparent or a light ground, and the
/// border the cells were laid out with.
pub fn rasterize_with(buf: &Buffer, px: f32, font: &str, doc: Option<&Document>, o: &crate::export::Options) -> Result<RgbaImage, String> {
    let light = o.appearance == crate::export::Appearance::Light;
    let primary = load(font).ok_or_else(|| format!("could not read the font at {font}"))?;
    let fallbacks: Vec<fontdue::Font> = FONT_PATHS.iter().filter(|p| **p != font).filter_map(|p| load(p)).collect();
    let (cw, chh, baseline) = cell_metrics(&primary, px)?;
    let (cols, rows) = (buf.area.width as u32, buf.area.height as u32);
    let ground = if light {
        [251, 241, 199]
    } else {
        let Color::Rgb(r, g, b) = theme::t().panel else { unreachable!() };
        [r, g, b]
    };
    let ground_px = if o.transparent { Rgba([0, 0, 0, 0]) } else { Rgba([ground[0], ground[1], ground[2], 255]) };
    let mut out = RgbaImage::from_pixel(cols * cw, rows * chh, ground_px);

    for r in 0..rows {
        for c in 0..cols {
            let cell = &buf[(c as u16, r as u16)];
            let bg = match cell.bg {
                Color::Reset => ground,
                other => rgb(other, ground),
            };
            let mut fg = rgb(cell.fg, [235, 219, 178]);
            // A light rendering: the ink that would be the terminal's foreground goes dark.
            if light && matches!(cell.fg, Color::White | Color::Reset) || (light && cell.fg == theme::t().bright) {
                fg = [60, 56, 54];
            }
            let (ox, oy) = (c * cw, r * chh);
            if bg != ground {
                for y in 0..chh {
                    for x in 0..cw {
                        out.put_pixel(ox + x, oy + y, Rgba([bg[0], bg[1], bg[2], 255]));
                    }
                }
            }
            let Some(ch) = cell.symbol().chars().next() else { continue };
            if ch == ' ' {
                continue;
            }
            // The first font that actually has the glyph — a terminal's own fallback rule.
            let f = std::iter::once(&primary)
                .chain(fallbacks.iter())
                .find(|f| f.lookup_glyph_index(ch) != 0)
                .unwrap_or(&primary);
            glyph(&mut out, f, ch, px, ox as i32, oy as i32 + baseline, fg);
        }
    }
    if let Some(doc) = doc {
        labels(&mut out, doc, &primary, px, cw, chh, baseline, o.border as f64, light);
    }
    Ok(out)
}

/// Render a diagram to a PNG with the export options. Returns the picture's size in cells
/// and pixels.
pub fn to_png(doc: &Document, o: &crate::export::Options, px: f32, path: &std::path::Path) -> Result<(u16, u16, u32, u32), String> {
    let font = find_font(o.font.as_deref()).ok_or("no monospace font found — :render <out.png> <font path>, or install DejaVu Sans Mono")?;
    let buf = cells(doc, o.grid, false, o.border as f64);
    let img = rasterize_with(&buf, px, &font, Some(doc), o)?;
    img.save(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((buf.area.width, buf.area.height, img.width(), img.height()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::{RelationKind, ShapeKind};

    fn doc() -> Document {
        let mut d = Document::default();
        let a = d.add(ShapeKind::ApplicationComponent, "Billing", 10.0, 6.0);
        let b = d.add(ShapeKind::DataObject, "invoice", 40.0, 6.0);
        d.connect(RelationKind::Access, a, b).unwrap();
        d
    }

    #[test]
    fn the_cells_frame_the_whole_diagram_with_a_margin_and_no_chrome() {
        let buf = cells(&doc(), false, true, 2.0);
        // 10..52 wide, 6..12 tall, plus two cells of margin either side.
        assert_eq!((buf.area.width, buf.area.height), (46, 10));
        let text: String = (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
            .map(|(x, y)| buf[(x, y)].symbol().to_string())
            .collect();
        assert!(text.contains("Billing") && text.contains("invoice"));
        assert!(!text.contains("vim-shapes"), "no header, no footer");
    }

    #[test]
    fn an_empty_diagram_still_renders_to_something() {
        let buf = cells(&Document::default(), true, true, 2.0);
        assert!(buf.area.width >= 8 && buf.area.height >= 3);
    }

    /// The rasteriser, when a font is on this machine. Skipped, and said so, when there is none —
    /// a build box with no fonts is not a broken rasteriser.
    #[test]
    fn a_rendering_is_the_cells_through_a_font_at_a_size_the_font_decides() {
        let Some(font) = find_font(None) else {
            eprintln!("no font on this machine — rasteriser test skipped");
            return;
        };
        let buf = cells(&doc(), false, false, 2.0);
        let img = rasterize(&buf, 16.0, &font, Some(&doc())).expect("rasterize");
        assert_eq!(img.width() % buf.area.width as u32, 0, "an exact number of cells across");
        assert_eq!(img.height() % buf.area.height as u32, 0, "…and down");
        let lit = img.pixels().filter(|p| p.0 != [29, 32, 33, 255]).count();
        assert!(lit > 100, "something was drawn: {lit} pixels differ from the ground");
    }
}
