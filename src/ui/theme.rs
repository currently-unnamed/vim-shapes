//! The two palettes, and the only colours the app paints deliberately.
//!
//! Every colour on screen is one of two things. A ROLE is what a thing is — ordinary text, a
//! dim hint, a panel's ground, the cursor's tint — and each role has a value per mode. An
//! ACCENT has a meaning that must survive the switch — the cursor's yellow, a refusal's red, a
//! layer's colour, the ten named paints — and each has a dark value and a light one, because a
//! pastel that glows on a dark ground is invisible on paper. Nothing outside this file reaches
//! for a colour by value: `theme::t()` is the palette, and a test greps the interface for bare
//! greys so the next panel cannot regress.
//!
//! The two are gruvbox, dark and light: one design in two moods, so a diagram reads the same
//! way on either. The mode comes from `main` (a flag, the config file, the terminal's own
//! hint) or from `:theme`, and is read here through one atomic, so a switch is one store.

use crate::ontology::{Colour, Layer, Paint};
use ratatui::style::Color;
use std::sync::atomic::{AtomicU8, Ordering};

/// Which palette.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Dark,
    Light,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Dark => "dark",
            Mode::Light => "light",
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dark" | "d" => Some(Mode::Dark),
            "light" | "l" => Some(Mode::Light),
            _ => None,
        }
    }
}

/// Where the mode came from, for the debug panel: the answer to "why is it light".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Flag,
    Command,
    Config,
    Terminal,
    Default,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Flag => "--light / --dark",
            Source::Command => ":theme",
            Source::Config => "the config file",
            Source::Terminal => "COLORFGBG",
            Source::Default => "the default",
        }
    }
}

static MODE: AtomicU8 = AtomicU8::new(0);
static SOURCE: AtomicU8 = AtomicU8::new(4);

pub fn mode() -> Mode {
    if MODE.load(Ordering::Relaxed) == 1 { Mode::Light } else { Mode::Dark }
}

pub fn source() -> Source {
    match SOURCE.load(Ordering::Relaxed) {
        0 => Source::Flag,
        1 => Source::Command,
        2 => Source::Config,
        3 => Source::Terminal,
        _ => Source::Default,
    }
}

pub fn set_mode(m: Mode, from: Source) {
    MODE.store(if m == Mode::Light { 1 } else { 0 }, Ordering::Relaxed);
    SOURCE.store(
        match from {
            Source::Flag => 0,
            Source::Command => 1,
            Source::Config => 2,
            Source::Terminal => 3,
            Source::Default => 4,
        },
        Ordering::Relaxed,
    );
}

/// The mode to start in, first hit wins: a flag, the config file's word, the terminal's
/// `COLORFGBG` hint (`15;0` is light text on a dark ground), and dark when nothing says.
pub fn choose(flag: Option<Mode>, config: Option<&str>, colorfgbg: Option<&str>) -> (Mode, Source) {
    if let Some(m) = flag {
        return (m, Source::Flag);
    }
    if let Some(m) = config.and_then(Mode::parse) {
        return (m, Source::Config);
    }
    if let Some(bg) = colorfgbg.and_then(|s| s.rsplit(';').next()).and_then(|s| s.trim().parse::<u8>().ok()) {
        // The ground's number: 0–6 and 8 are dark colours, 7 and 15 and the rest light.
        return (if bg < 7 || bg == 8 { Mode::Dark } else { Mode::Light }, Source::Terminal);
    }
    (Mode::Dark, Source::Default)
}

/// One palette: the roles, then the accents.
pub struct Theme {
    /// Ordinary text.
    pub ink: Color,
    /// The brightest tier: a header, a focused relation.
    pub bright: Color,
    /// Hints, units, tags — the quiet tier.
    pub dim: Color,
    /// A value at rest in a list: between ink and dim.
    pub muted: Color,
    /// Text laid on an accent — the cursor's label on yellow.
    pub inverse: Color,
    /// A floating panel's ground.
    pub panel: Color,
    /// The ground under everything: painted on every cell before a frame is drawn, so the
    /// palette is whole and the terminal's own colours never show through it.
    pub ground: Color,
    /// The grid's dots.
    pub grid: Color,
    /// A relation at rest, a divider, a composite's outline.
    pub structure: Color,
    /// The tint under the shape the cursor is on: yellow dimmed to a ground a label can sit on
    /// in reverse. The cursor has to be unmissable — an outline alone was not.
    pub hilite: Color,
    /// A live / on / affirmative thing: the element a relation is drawn from, a picked one, a
    /// save that worked.
    pub green: Color,
    /// The cursor.
    pub yellow: Color,
    /// A warning or a refusal.
    pub red: Color,
    pub orange: Color,
    pub purple: Color,
    pub aqua: Color,
    /// A plain shape: neutral, so it never competes with a layer for the eye.
    pub sand: Color,
    /// The ten named paints, in `Paint::ALL` order.
    pub paints: [[u8; 3]; 10],
    /// The layers, in `Layer` order: motivation, strategy, business, application, technology,
    /// implementation, composite, sketch.
    pub layers: [Color; 9],
}

pub const DARK: Theme = Theme {
    ink: Color::Rgb(235, 219, 178),
    bright: Color::Rgb(251, 241, 199),
    dim: Color::Rgb(146, 131, 116),
    muted: Color::Rgb(189, 174, 147),
    inverse: Color::Rgb(0, 0, 0),
    panel: Color::Rgb(29, 32, 33),
    ground: Color::Rgb(40, 40, 40),
    grid: Color::Rgb(60, 56, 54),
    structure: Color::Rgb(146, 131, 116),
    hilite: Color::Rgb(84, 66, 22),
    green: Color::Rgb(142, 192, 124),
    yellow: Color::Rgb(215, 153, 33),
    red: Color::Rgb(204, 36, 29),
    orange: Color::Rgb(254, 128, 25),
    purple: Color::Rgb(211, 134, 155),
    aqua: Color::Rgb(131, 165, 152),
    sand: Color::Rgb(168, 153, 132),
    paints: crate::ontology::PAINTS_DARK,
    layers: [
        Color::Rgb(211, 134, 155),
        Color::Rgb(215, 153, 33),
        Color::Rgb(254, 128, 25),
        Color::Rgb(131, 165, 152),
        Color::Rgb(142, 192, 124),
        Color::Rgb(251, 73, 52),
        Color::Rgb(69, 133, 136),
        Color::Rgb(146, 131, 116),
        Color::Rgb(168, 153, 132),
    ],
};

pub const LIGHT: Theme = Theme {
    ink: Color::Rgb(60, 56, 54),
    bright: Color::Rgb(40, 40, 40),
    dim: Color::Rgb(124, 111, 100),
    muted: Color::Rgb(102, 92, 84),
    inverse: Color::Rgb(251, 241, 199),
    panel: Color::Rgb(242, 229, 188),
    ground: Color::Rgb(251, 241, 199),
    grid: Color::Rgb(213, 196, 161),
    structure: Color::Rgb(124, 111, 100),
    hilite: Color::Rgb(250, 227, 150),
    green: Color::Rgb(121, 116, 14),
    yellow: Color::Rgb(181, 118, 20),
    red: Color::Rgb(157, 0, 6),
    orange: Color::Rgb(175, 58, 3),
    purple: Color::Rgb(143, 63, 113),
    aqua: Color::Rgb(66, 123, 88),
    sand: Color::Rgb(124, 111, 100),
    paints: crate::ontology::PAINTS_LIGHT,
    layers: [
        Color::Rgb(143, 63, 113),
        Color::Rgb(181, 118, 20),
        Color::Rgb(175, 58, 3),
        Color::Rgb(66, 123, 88),
        Color::Rgb(121, 116, 14),
        Color::Rgb(204, 36, 29),
        Color::Rgb(7, 102, 120),
        Color::Rgb(124, 111, 100),
        Color::Rgb(102, 92, 84),
    ],
};

/// The palette in use.
pub fn t() -> &'static Theme {
    match mode() {
        Mode::Dark => &DARK,
        Mode::Light => &LIGHT,
    }
}

/// What a named colour from the palette looks like on screen: this mode's value for it.
pub fn paint(p: Paint) -> Color {
    let c = t().paints[Paint::ALL.iter().position(|q| *q == p).unwrap_or(0)];
    Color::Rgb(c[0], c[1], c[2])
}

/// What any colour looks like on the terminal: a name is the mode's accent, a hex is itself.
pub fn colour(c: Colour) -> Color {
    match c {
        Colour::Named(p) => paint(p),
        Colour::Hex([r, g, b]) => Color::Rgb(r, g, b),
    }
}

/// A colour at `opacity` per cent over `ground` — what a half-solid shape's outline looks
/// like on the terminal.
pub fn fade(c: Color, opacity: u8, ground: Color) -> Color {
    match (c, ground) {
        (Color::Rgb(r, g, b), Color::Rgb(gr, gg, gb)) if opacity < 100 => {
            let m = crate::ontology::mix([r, g, b], [gr, gg, gb], 1.0 - opacity as f64 / 100.0);
            Color::Rgb(m[0], m[1], m[2])
        }
        _ => c,
    }
}

/// The band a label sits on when it asks for one: a step off the ground, either way, so the
/// label reads over a line or the grid.
pub fn band(ground: Color) -> Color {
    match ground {
        Color::Rgb(r, g, b) if (r as u32 + g as u32 + b as u32) < 384 => Color::Rgb(r.saturating_add(34), g.saturating_add(30), b.saturating_add(28)),
        Color::Rgb(r, g, b) => Color::Rgb(r.saturating_sub(24), g.saturating_sub(24), b.saturating_sub(24)),
        _ => t().grid,
    }
}

/// What a layer looks like. The one place the app paints by *what an element is*: the fills
/// architects already read the layers in, translated to this palette. Yellow is taken by the
/// cursor, so business — yellow everywhere else — is orange here, and the eye learns it in a
/// minute.
pub fn layer_color(l: Layer) -> Color {
    t().layers[layer_index(l)]
}

fn layer_index(l: Layer) -> usize {
    match l {
        Layer::Motivation => 0,
        Layer::Strategy => 1,
        Layer::Business => 2,
        Layer::Application => 3,
        Layer::Technology => 4,
        Layer::Implementation => 5,
        Layer::Ontology => 6,
        Layer::Composite => 7,
        Layer::Sketch => 8,
    }
}

/// Relative luminance, the way WCAG measures it.
#[cfg(test)]
fn luminance(c: Color) -> f64 {
    let Color::Rgb(r, g, b) = c else { return 0.5 };
    let lin = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// The contrast between two colours, 1 to 21 — what a test holds every role to.
#[cfg(test)]
pub fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_is_chosen_flag_first_then_config_then_the_terminal_then_dark() {
        assert_eq!(choose(Some(Mode::Light), Some("dark"), Some("15;0")), (Mode::Light, Source::Flag));
        assert_eq!(choose(None, Some("light"), Some("15;0")), (Mode::Light, Source::Config));
        assert_eq!(choose(None, Some("auto"), Some("15;0")), (Mode::Dark, Source::Terminal), "auto defers to the terminal");
        assert_eq!(choose(None, None, Some("0;15")), (Mode::Light, Source::Terminal));
        assert_eq!(choose(None, None, Some("0;7")), (Mode::Light, Source::Terminal));
        assert_eq!(choose(None, None, Some("garbage")), (Mode::Dark, Source::Default));
        assert_eq!(choose(None, None, None), (Mode::Dark, Source::Default));
        assert_eq!(Mode::parse("L"), Some(Mode::Light));
    }

    #[test]
    fn every_role_reads_on_its_ground_and_every_accent_differs_between_the_modes() {
        for th in [&DARK, &LIGHT] {
            for (name, c) in [("ink", th.ink), ("bright", th.bright), ("dim", th.dim), ("muted", th.muted), ("structure", th.structure)] {
                assert!(contrast(c, th.ground) >= 3.0, "{name} on the ground: {:.2}", contrast(c, th.ground));
                assert!(contrast(c, th.panel) >= 3.0, "{name} on a panel: {:.2}", contrast(c, th.panel));
            }
            for (name, c) in [("green", th.green), ("yellow", th.yellow), ("red", th.red), ("aqua", th.aqua), ("purple", th.purple), ("orange", th.orange)] {
                assert!(contrast(th.inverse, c) >= 3.0, "inverse text on {name}: {:.2}", contrast(th.inverse, c));
            }
            assert!(contrast(th.ink, th.hilite) >= 4.5, "the ink reads on the cursor's tint: {:.2}", contrast(th.ink, th.hilite));
            assert!(contrast(th.inverse, th.yellow) >= 3.0, "the cursor's label reads on its yellow: {:.2}", contrast(th.inverse, th.yellow));
        }
        for i in 0..10 {
            assert_ne!(DARK.paints[i], LIGHT.paints[i], "paint {i} has a value per mode");
        }
        for i in 0..9 {
            assert_ne!(DARK.layers[i], LIGHT.layers[i], "layer {i} has a value per mode");
        }
    }

    /// The fourth check, beside warnings, clippy and the private names: nothing in the
    /// interface paints a neutral by value, so a light terminal cannot be handed a dark grey.
    #[test]
    fn no_panel_paints_a_bare_neutral() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui");
        let bare = ["Color::White", "Color::Black", "Color::DarkGray", "Color::Gray", "Color::LightBlue", "Color::Cyan", "Color::Magenta"];
        for entry in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
            let p = entry.path();
            if p.file_name().is_some_and(|n| n == "theme.rs") || p.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for (i, line) in text.lines().enumerate() {
                for b in bare {
                    assert!(!line.contains(b), "{}:{}: {b} — use a role from theme::t()", p.display(), i + 1);
                }
            }
        }
    }

    #[test]
    fn a_named_paint_is_the_mode_s_value_and_a_hex_is_itself() {
        set_mode(Mode::Dark, Source::Default);
        assert_eq!(paint(Paint::Aqua), Color::Rgb(131, 165, 152));
        assert_eq!(colour(Colour::Hex([1, 2, 3])), Color::Rgb(1, 2, 3));
        assert_eq!(LIGHT.paints[4], [66, 123, 88], "aqua on paper is the deep one");
        assert!(contrast(Color::Rgb(255, 255, 255), Color::Rgb(0, 0, 0)) > 20.0);
    }
}
