//! The export dialog — `:export` with nothing after it.
//!
//! The questions any diagram tool's image dialog asks, in the panel grammar everything else
//! here uses: `j`/`k` to a field, `i` to type into it, `h`/`l` to cycle a choice, `Enter` to
//! export. The size is shown rather than asked: the diagram sets it, the zoom scales it, and
//! typing a width or a height sets the zoom that gives it.

use super::{chrome, theme};
use crate::export::{Appearance, Format, Options, Style as PicStyle};
use crate::model::Document;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Format,
    Style,
    File,
    Zoom,
    Width,
    Height,
    Transparent,
    Appearance,
    Border,
    Grid,
}

impl Field {
    pub const ALL: [Field; 10] = [
        Field::Format,
        Field::Style,
        Field::File,
        Field::Zoom,
        Field::Width,
        Field::Height,
        Field::Transparent,
        Field::Appearance,
        Field::Border,
        Field::Grid,
    ];

    fn name(self) -> &'static str {
        match self {
            Field::Format => "format",
            Field::Style => "style",
            Field::File => "file",
            Field::Zoom => "zoom",
            Field::Width => "width",
            Field::Height => "height",
            Field::Transparent => "transparent",
            Field::Appearance => "appearance",
            Field::Border => "border",
            Field::Grid => "grid",
        }
    }

    fn unit(self) -> &'static str {
        match self {
            Field::Format => "png / svg / pdf / xml / html / diagram",
            Field::Style => "terminal (braille) / clean (drawn)",
            Field::File => "path",
            Field::Zoom => "percent",
            Field::Width | Field::Height => "px — sets the zoom",
            Field::Transparent => "yes / no (png, svg, html)",
            Field::Appearance => "dark / light",
            Field::Border => "cells",
            Field::Grid => "yes / no",
        }
    }

    fn typed(self) -> bool {
        matches!(self, Field::File | Field::Zoom | Field::Width | Field::Height | Field::Border)
    }
}

pub struct State {
    pub options: Options,
    pub file: PathBuf,
    pub sel: usize,
    pub editing: Option<String>,
}

impl State {
    /// Opened for a tab: the file is the tab's name in the working directory, in the format's
    /// own extension.
    pub fn new(tab_name: &str, grid: bool) -> State {
        let appearance = if super::theme::mode() == super::theme::Mode::Light { crate::export::Appearance::Light } else { crate::export::Appearance::Dark };
        let options = Options { grid, appearance, ..Options::default() };
        let mut s = State { options, file: PathBuf::new(), sel: 0, editing: None };
        s.file = PathBuf::from(format!("{}.{}", slug(tab_name), s.options.format.extension()));
        s
    }

    pub fn field(&self) -> Field {
        Field::ALL[self.sel.min(Field::ALL.len() - 1)]
    }

    pub fn move_by(&mut self, delta: isize) {
        let n = Field::ALL.len() as isize;
        self.sel = (self.sel as isize + delta).rem_euclid(n) as usize;
    }

    /// The value a field shows.
    pub fn value(&self, f: Field, doc: &Document) -> String {
        let (w, h) = self.options.size_px(doc);
        match f {
            Field::Format => self.options.format.name().into(),
            Field::Style => self.options.style.name().into(),
            Field::File => self.file.display().to_string(),
            Field::Zoom => format!("{}", self.options.zoom),
            Field::Width => w.to_string(),
            Field::Height => h.to_string(),
            Field::Transparent => yes(self.options.transparent),
            Field::Appearance => match self.options.appearance {
                Appearance::Dark => "dark".into(),
                Appearance::Light => "light".into(),
            },
            Field::Border => self.options.border.to_string(),
            Field::Grid => yes(self.options.grid),
        }
    }

    /// `h`/`l` on a choice.
    pub fn cycle(&mut self, delta: isize) {
        match self.field() {
            Field::Format => {
                let i = Format::ALL.iter().position(|f| *f == self.options.format).unwrap_or(0) as isize;
                let n = Format::ALL.len() as isize;
                self.set_format(Format::ALL[(i + delta).rem_euclid(n) as usize]);
            }
            Field::Style => {
                self.options.style = match self.options.style {
                    PicStyle::Terminal => PicStyle::Clean,
                    PicStyle::Clean => PicStyle::Terminal,
                };
                // The clean picture is paper by default, the terminal's is dark.
                self.options.appearance = match self.options.style {
                    PicStyle::Clean => Appearance::Light,
                    PicStyle::Terminal => Appearance::Dark,
                };
            }
            Field::Transparent => self.options.transparent = !self.options.transparent,
            Field::Appearance => {
                self.options.appearance = match self.options.appearance {
                    Appearance::Dark => Appearance::Light,
                    Appearance::Light => Appearance::Dark,
                }
            }
            Field::Grid => self.options.grid = !self.options.grid,
            Field::Zoom => self.options.zoom = (self.options.zoom as i64 + delta as i64 * 25).clamp(10, 800) as u32,
            Field::Border => self.options.border = (self.options.border as i64 + delta as i64).clamp(0, 20) as u32,
            _ => {}
        }
    }

    /// Changing the format changes the file's extension with it, unless the file was typed
    /// by hand to something else.
    fn set_format(&mut self, f: Format) {
        let old = self.options.format;
        self.options.format = f;
        if self.file.extension().map(|e| e.to_string_lossy().to_string()) == Some(old.extension().to_string()) {
            self.file.set_extension(f.extension());
        }
    }

    pub fn step_in(&mut self, doc: &Document) {
        if self.field().typed() {
            self.editing = Some(self.value(self.field(), doc));
        }
    }

    /// Commit what was typed. An error leaves the field open with the text as it was.
    pub fn commit(&mut self, text: &str, doc: &Document) -> Result<(), String> {
        let text = text.trim();
        let num = |what: &str, lo: u32, hi: u32| -> Result<u32, String> {
            let n: u32 = text.parse().map_err(|_| format!("{what} is a whole number, not {text:?}"))?;
            if n < lo || n > hi {
                return Err(format!("{what} runs from {lo} to {hi}"));
            }
            Ok(n)
        };
        match self.field() {
            Field::File => {
                if text.is_empty() {
                    return Err("a file name is needed".into());
                }
                let p = PathBuf::from(text);
                if let Some(f) = Format::of_path(&p) {
                    self.options.format = f;
                }
                self.file = p;
            }
            Field::Zoom => self.options.zoom = num("zoom", 10, 800)?,
            Field::Border => self.options.border = num("border", 0, 20)?,
            Field::Width => {
                let want = num("width", 16, 20000)?;
                let natural = Options { zoom: 100, ..self.options.clone() }.size_px(doc).0.max(1);
                self.options.zoom = ((want as f64 / natural as f64) * 100.0).round().clamp(10.0, 800.0) as u32;
            }
            Field::Height => {
                let want = num("height", 16, 20000)?;
                let natural = Options { zoom: 100, ..self.options.clone() }.size_px(doc).1.max(1);
                self.options.zoom = ((want as f64 / natural as f64) * 100.0).round().clamp(10.0, 800.0) as u32;
            }
            _ => {}
        }
        self.editing = None;
        Ok(())
    }
}

fn yes(b: bool) -> String {
    if b { "yes".into() } else { "no".into() }
}

/// A tab's name as a file name: lower-case, spaces to dashes, nothing a shell would mind.
fn slug(name: &str) -> String {
    let s: String = name.trim().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '-' }).collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "diagram".into() } else { s }
}

pub const WIDTH: u16 = 70;
pub const HEIGHT: u16 = Field::ALL.len() as u16 + 4;

pub struct Dialog<'a> {
    pub state: &'a State,
    pub doc: &'a Document,
}

impl Widget for Dialog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let inner = chrome::panel(buf, area, &format!("export — {}", s.options.format.tagline()), theme::t().green);
        let hint = match s.editing.is_some() {
            true => " type, then esc or enter to leave the field",
            false => " j/k field   i type   h/l cycle   enter export   esc",
        };
        let body = chrome::hint(buf, inner, hint);
        let mut lines: Vec<Line> = vec![Line::raw("")];
        for (i, f) in Field::ALL.iter().enumerate() {
            let on = i == s.sel;
            let name = format!("{}{:<12}", chrome::marker(on), f.name());
            let (value, vs) = match (&s.editing, on) {
                (Some(t), true) => (format!("{t}█"), Style::new().fg(theme::t().green).bold()),
                _ => (s.value(*f, self.doc), if on { Style::new().fg(theme::t().ink).bold() } else { Style::new().fg(theme::t().muted) }),
            };
            let ns = if on { Style::new().fg(theme::t().inverse).bg(theme::t().green).bold() } else { Style::new().fg(theme::t().green) };
            let room = (body.width as usize).saturating_sub(13 + 2 + 28);
            let value: String = if value.chars().count() > room { format!("…{}", value.chars().skip(value.chars().count() - room + 1).collect::<String>()) } else { value };
            let pad = room.saturating_sub(value.chars().count());
            lines.push(Line::from(vec![
                Span::styled(name, ns),
                Span::raw("  "),
                Span::styled(value, vs),
                Span::raw(" ".repeat(pad)),
                Span::styled(format!("  {}", f.unit()), Style::new().fg(theme::t().dim)),
            ]));
        }
        Paragraph::new(lines).render(body, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::ShapeKind;

    fn doc() -> Document {
        let mut d = Document::default();
        d.add(ShapeKind::Box, "a", 0.0, 0.0);
        d.add(ShapeKind::Box, "b", 40.0, 0.0);
        d
    }

    #[test]
    fn the_file_follows_the_tab_and_the_format_until_typed_by_hand() {
        let mut s = State::new("Claims Flow", true);
        assert_eq!(s.file, PathBuf::from("claims-flow.png"));
        s.cycle(1);
        assert_eq!((s.options.format, s.file.clone()), (Format::Svg, PathBuf::from("claims-flow.svg")));
        s.sel = 2;
        s.step_in(&doc());
        s.commit("out/picture.pdf", &doc()).unwrap();
        assert_eq!(s.options.format, Format::Pdf, "a typed extension picks the format");
        s.sel = 0;
        s.cycle(1);
        assert_eq!(s.options.format, Format::Xml);
        assert_eq!(s.file, PathBuf::from("out/picture.drawio"), "the extension follows the format when it was the old format's");
    }

    #[test]
    fn width_and_height_set_the_zoom_that_gives_them() {
        let mut s = State::new("t", false);
        let d = doc();
        let (w, _) = s.options.size_px(&d);
        s.sel = 4;
        s.commit(&(w * 2).to_string(), &d).unwrap();
        assert_eq!(s.options.zoom, 200);
        assert_eq!(s.value(Field::Width, &d), (w * 2).to_string());
        assert!(s.commit("abc", &d).unwrap_err().contains("whole number"));
        s.sel = 3;
        assert!(s.commit("5", &d).unwrap_err().contains("10 to 800"));
    }
}
