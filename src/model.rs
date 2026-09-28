//! The document: elements, relations, and the little that is known about the diagram as a whole.
//!
//! This is what undo clones, what the file is, and what every view draws. It knows the
//! ontology's *names* — an element has a kind, a relation has a kind — and nothing about the
//! rules; asking whether a relation is allowed is [`lint`](Document::lint)'s job, and it asks
//! the ontology.
//!
//! Positions are in **cells**, world coordinates: `x` grows rightward, `y` grows downward, the
//! way rows do. The camera in the UI decides which part of the world is on screen; nothing
//! here knows the screen exists.

use serde::{Deserialize, Serialize};

use crate::ontology::{self, Colour, LineStyle, Notation, RelationKind, ShapeKind, View};

pub type ElementId = u32;
pub type RelationId = u32;

/// The file format of a whole workspace — every tab, and which one was open.
///
/// 3: tabs. A file is a workspace of diagrams, the way a draw.io file is a set of pages. A
///    version-2 file (one bare diagram) still opens, as a workspace of one tab.
pub const WORKSPACE_VERSION: u32 = 3;

/// What a file holds: one diagram per tab, plus the two things that belong to the whole
/// workspace rather than to any diagram — which tab was open, and whether the grid is shown.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Workspace {
    pub version: u32,
    /// Older files kept the grid here, for every tab at once. Read, applied to each tab's
    /// page on opening, and never written again.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub grid: bool,
    #[serde(default)]
    pub current: usize,
    pub tabs: Vec<Tab>,
}

fn yes() -> bool {
    true
}

/// One tab: a name, and the diagram in it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Tab {
    pub name: String,
    pub diagram: Document,
}

impl Workspace {
    /// A workspace of one fresh tab.
    pub fn new() -> Workspace {
        Workspace::single("diagram 1".into(), Document::default())
    }

    pub fn single(name: String, diagram: Document) -> Workspace {
        Workspace { version: WORKSPACE_VERSION, grid: true, current: 0, tabs: vec![Tab { name, diagram }] }
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace::new()
    }
}

/// The file format. Bumped when a saved file would no longer open the way it was saved.
///
/// 3: a relation's `elbow` is per cent of its first leg, not cells from the tail, so a bend
///    stays proportionally put as the two shapes move apart — `persistence::load` converts an
///    older file's cell count once, using the geometry the file itself still describes, so
///    nothing bent by hand moves the moment it is opened. (2 was the first ontology-typed
///    format — elements have a `ShapeKind`, relations a `RelationKind`, and the document a
///    `View`; 1 was a sketch with bare shapes and is not read, since nothing was ever saved in
///    it outside this repository.)
pub const CURRENT_VERSION: u32 = 3;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Document {
    pub version: u32,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub elements: Vec<Element>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    /// Back to front.
    #[serde(default = "default_layers", skip_serializing_if = "is_default_layers")]
    pub layers: Vec<LayerDef>,
    #[serde(default)]
    pub next_element_id: ElementId,
    #[serde(default)]
    pub next_relation_id: RelationId,
}

impl Default for Document {
    fn default() -> Self {
        Document {
            version: CURRENT_VERSION,
            metadata: Metadata::default(),
            elements: Vec::new(),
            relations: Vec::new(),
            layers: default_layers(),
            next_element_id: 0,
            next_relation_id: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// What kind of diagram this is — decides what the palette offers. See [`View`].
    #[serde(default)]
    pub view: View,
    /// The layer new shapes and relations go on — the one picked in the layer browser.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub layer: u32,
    /// The diagram's own look and page — what the sheet shows when it is on the diagram.
    #[serde(default, skip_serializing_if = "Page::is_default")]
    pub page: Page,
}

/// The diagram's own settings: the grid, the ground, the paper, and the three looks that
/// apply to everything on it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Page {
    /// The faint dots that mark the canvas. On unless turned off.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub grid: bool,
    /// Cells between grid marks across; down is half that, so the marks square up. Zero is
    /// the usual four.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub grid_size: u32,
    /// How the grid is marked: a dot at each crossing, or ruled lines.
    #[serde(default, skip_serializing_if = "GridStyle::is_default")]
    pub grid_style: GridStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid_color: Option<Colour>,
    /// The ground, on screen and in the file — blank is the terminal's own, or paper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Colour>,
    /// Draw the page's edge on the canvas, and frame exports to the page rather than to
    /// the diagram's bounds.
    #[serde(default, skip_serializing_if = "is_false")]
    pub page_view: bool,
    #[serde(default, skip_serializing_if = "Paper::is_default")]
    pub paper: Paper,
    #[serde(default, skip_serializing_if = "is_false")]
    pub landscape: bool,
    /// Every rectangle drawn with rounded corners.
    #[serde(default, skip_serializing_if = "is_false")]
    pub rounded: bool,
    /// Every outline drawn by an unsteady hand.
    #[serde(default, skip_serializing_if = "is_false")]
    pub sketch: bool,
    /// Every shape with a shadow beneath it, in the exports.
    #[serde(default, skip_serializing_if = "is_false")]
    pub shadow: bool,
}

impl Element {
    /// Whether a shadow falls from this shape: a box does; a grouping, a figure and bare
    /// text have no body to cast one.
    pub fn casts_shadow(&self) -> bool {
        use ontology::Shape;
        !self.kind.is_composite() && !matches!(self.kind.shape(), Shape::Text | Shape::StickFigure | Shape::Dashed)
    }
}

impl Default for Page {
    fn default() -> Page {
        Page {
            grid: true,
            grid_size: 0,
            grid_style: GridStyle::Auto,
            grid_color: None,
            background: None,
            page_view: false,
            paper: Paper::default(),
            landscape: false,
            rounded: false,
            sketch: false,
            shadow: false,
        }
    }
}

impl Page {
    fn is_default(&self) -> bool {
        *self == Page::default()
    }

    /// Cells between grid dots across, and down.
    pub fn grid_step(&self) -> (i64, i64) {
        let g = if self.grid_size == 0 { 4 } else { self.grid_size as i64 };
        (g, (g / 2).max(1))
    }

    /// The page in cells, the way round it is set.
    pub fn size(&self) -> (f64, f64) {
        self.paper.cells(self.landscape)
    }

    /// How a kind's shape is drawn on this diagram: rounded, every plain rectangle gets
    /// corners. One place, so the canvas and every export agree.
    pub fn shape(&self, kind: ShapeKind) -> ontology::Shape {
        match kind.shape() {
            ontology::Shape::Rectangle if self.rounded => ontology::Shape::RoundedRectangle,
            s => s,
        }
    }
}

fn is_true(b: &bool) -> bool {
    *b
}

/// A free key/value pair carried through from wherever an element or relation was imported —
/// Archi's own "Properties" tab, say. Distinct from [`Property`]: that is this app's own
/// domain-modelling concept, a field definition on an object type, and reusing it for arbitrary
/// source metadata would give one name two meanings. This app never writes a `Tag` itself.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub key: String,
    pub value: String,
}

/// One row of an ontology type: a property of an object type or interface, or a parameter of
/// an action type. Foundry's words, unchanged.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_name: Option<String>,
    #[serde(default, rename = "type")]
    pub base_type: BaseType,
    /// Holds many values.
    #[serde(default, skip_serializing_if = "is_false")]
    pub array: bool,
    /// The property that identifies an object — one per object type.
    #[serde(default, skip_serializing_if = "is_false")]
    pub primary_key: bool,
    /// The property an object is shown by.
    #[serde(default, skip_serializing_if = "is_false")]
    pub title: bool,
    /// A shared property: one definition, used on many object types.
    #[serde(default, skip_serializing_if = "is_false")]
    pub shared: bool,
    /// A parameter that must be given.
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    /// The value type it adopts, by name — its constraints and meaning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_type: Option<String>,
    #[serde(default, skip_serializing_if = "Status::is_default")]
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Property {
    pub fn new(name: impl Into<String>) -> Property {
        Property {
            name: name.into(),
            api_name: None,
            base_type: BaseType::String,
            array: false,
            primary_key: false,
            title: false,
            shared: false,
            required: false,
            value_type: None,
            status: Status::Active,
            description: None,
        }
    }

    /// The row in one breath, for a file that lays it out itself: `⚿ code: string`.
    pub fn compact(&self) -> String {
        let r = self.row(200);
        let (mark, rest) = r.split_at(r.char_indices().nth(2).map_or(r.len(), |(i, _)| i));
        let name = rest.split_whitespace().next().unwrap_or_default();
        let ty = rest.trim().strip_prefix(name).unwrap_or_default().trim();
        format!("{}{name}: {ty}", mark.trim_end().to_string() + if mark.trim().is_empty() { "" } else { " " })
    }

    /// The row's mark: symbols on the screen, letters on paper, where a sans face has no
    /// glyph for a key.
    pub fn mark(&self, plain: bool) -> &'static str {
        match (plain, self.primary_key, self.title, self.shared, self.status) {
            (false, true, ..) => "⚿ ",
            (false, _, true, ..) => "✎ ",
            (false, _, _, true, _) => "✱ ",
            (false, .., Status::Deprecated) => "✗ ",
            (true, true, ..) => "PK ",
            (true, _, true, ..) => "T  ",
            (true, _, _, true, _) => "S  ",
            (true, .., Status::Deprecated) => "x  ",
            (true, ..) => "   ",
            _ => "  ",
        }
    }

    /// The type, as the row prints it: the base type, `[]` for an array, the value type.
    pub fn type_text(&self) -> String {
        let mut ty = self.base_type.name().to_string();
        if self.array {
            ty.push_str("[]");
        }
        if let Some(v) = &self.value_type {
            ty.push_str(&format!(" ‹{v}›"));
        }
        ty
    }

    /// How many cells the row needs to print whole.
    pub fn natural_width(&self) -> usize {
        2 + self.name.chars().count() + 1 + 1 + self.type_text().chars().count()
    }

    /// The row as the box prints it: a mark, the name, and the type at the right edge —
    /// `⚿ code        string`. `width` is the room in cells.
    pub fn row(&self, width: usize) -> String {
        self.row_marked(width, false)
    }

    /// The same, with letters for marks where symbols will not print.
    pub fn row_marked(&self, width: usize, plain: bool) -> String {
        let mark = self.mark(plain);
        let ty = self.type_text();
        let name = if self.required { format!("{}*", self.name) } else { self.name.clone() };
        let room = width.saturating_sub(mark.chars().count() + ty.chars().count() + 1);
        let name: String = if name.chars().count() > room { name.chars().take(room.saturating_sub(1)).chain(['…']).collect() } else { name };
        format!("{mark}{name:<room$} {ty}", room = room)
    }
}

/// Foundry's property base types — the vocabulary of every property.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BaseType {
    #[default]
    String,
    Integer,
    Short,
    Long,
    Byte,
    Double,
    Float,
    Decimal,
    Boolean,
    Date,
    Timestamp,
    Geopoint,
    Geoshape,
    Vector,
    Struct,
    Attachment,
    MediaReference,
    TimeSeries,
    GeotemporalSeries,
    Marking,
    CipherText,
    /// A reference to an object of another type — what a parameter takes.
    ObjectReference,
}

impl BaseType {
    pub const ALL: [BaseType; 22] = [
        BaseType::String,
        BaseType::Integer,
        BaseType::Short,
        BaseType::Long,
        BaseType::Byte,
        BaseType::Double,
        BaseType::Float,
        BaseType::Decimal,
        BaseType::Boolean,
        BaseType::Date,
        BaseType::Timestamp,
        BaseType::Geopoint,
        BaseType::Geoshape,
        BaseType::Vector,
        BaseType::Struct,
        BaseType::Attachment,
        BaseType::MediaReference,
        BaseType::TimeSeries,
        BaseType::GeotemporalSeries,
        BaseType::Marking,
        BaseType::CipherText,
        BaseType::ObjectReference,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BaseType::String => "string",
            BaseType::Integer => "integer",
            BaseType::Short => "short",
            BaseType::Long => "long",
            BaseType::Byte => "byte",
            BaseType::Double => "double",
            BaseType::Float => "float",
            BaseType::Decimal => "decimal",
            BaseType::Boolean => "boolean",
            BaseType::Date => "date",
            BaseType::Timestamp => "timestamp",
            BaseType::Geopoint => "geopoint",
            BaseType::Geoshape => "geoshape",
            BaseType::Vector => "vector",
            BaseType::Struct => "struct",
            BaseType::Attachment => "attachment",
            BaseType::MediaReference => "media reference",
            BaseType::TimeSeries => "time series",
            BaseType::GeotemporalSeries => "geotemporal series",
            BaseType::Marking => "marking",
            BaseType::CipherText => "cipher text",
            BaseType::ObjectReference => "object reference",
        }
    }

    /// A name, or a unique prefix of one — `int` is integer, `ti` says which.
    pub fn parse(s: &str) -> Option<BaseType> {
        let want = s.trim().to_ascii_lowercase().replace(['-', '_'], " ");
        if want.is_empty() {
            return None;
        }
        if let Some(b) = BaseType::ALL.into_iter().find(|b| b.name() == want) {
            return Some(b);
        }
        let mut hits = BaseType::ALL.into_iter().filter(|b| b.name().starts_with(&want));
        let first = hits.next()?;
        hits.next().map_or(Some(first), |_| None)
    }

    /// The next in the list, for cycling.
    pub fn next(self) -> BaseType {
        let i = BaseType::ALL.iter().position(|b| *b == self).unwrap_or(0);
        BaseType::ALL[(i + 1) % BaseType::ALL.len()]
    }
}

/// Foundry's development stages.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    #[default]
    Active,
    Experimental,
    Deprecated,
}

impl Status {
    pub const ALL: [Status; 3] = [Status::Active, Status::Experimental, Status::Deprecated];

    fn is_default(&self) -> bool {
        *self == Status::Active
    }

    pub fn name(self) -> &'static str {
        match self {
            Status::Active => "active",
            Status::Experimental => "experimental",
            Status::Deprecated => "deprecated",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        let want = s.trim().to_ascii_lowercase();
        Status::ALL.into_iter().find(|v| !want.is_empty() && v.name().starts_with(&want))
    }

    /// The mark a deprecated type or property wears: ✗ on the screen, x on paper, where a
    /// sans face may have no ✗. Every picture draws it in red — a dagger in the tag's grey
    /// read as a cross, or as a footnote, and was missed. `None` for anything still in use.
    pub fn mark(self, plain: bool) -> Option<&'static str> {
        match self {
            Status::Deprecated => Some(if plain { "x" } else { "✗" }),
            _ => None,
        }
    }
}

/// How prominently a type is shown.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Visibility {
    Prominent,
    #[default]
    Normal,
    Hidden,
}

impl Visibility {
    pub const ALL: [Visibility; 3] = [Visibility::Prominent, Visibility::Normal, Visibility::Hidden];

    fn is_default(&self) -> bool {
        *self == Visibility::Normal
    }

    pub fn name(self) -> &'static str {
        match self {
            Visibility::Prominent => "prominent",
            Visibility::Normal => "normal",
            Visibility::Hidden => "hidden",
        }
    }

    pub fn parse(s: &str) -> Option<Visibility> {
        let want = s.trim().to_ascii_lowercase();
        Visibility::ALL.into_iter().find(|v| !want.is_empty() && v.name().starts_with(&want))
    }
}

impl Element {
    /// Rows above the compartment: the kind's tag, the name, and the rule between.
    pub const HEADER: f64 = 3.0;

    /// Whether this kind carries rows at all — an object type's properties, an interface's,
    /// an action's parameters.
    pub fn takes_rows(&self) -> bool {
        matches!(self.kind, ShapeKind::ObjectType | ShapeKind::Interface | ShapeKind::ActionType)
    }

    /// Whether a compartment is drawn: the kind takes rows, and there are some.
    pub fn has_rows(&self) -> bool {
        self.takes_rows() && !self.properties.is_empty()
    }

    /// What the rows read: a parameter or a property.
    pub fn rows_word(&self) -> &'static str {
        if self.kind == ShapeKind::ActionType { "parameters" } else { "properties" }
    }

    /// The box as tall as its rows need: a border, the header, the rows, a border.
    pub fn rows_height(&self) -> f64 {
        if self.has_rows() { 2.0 + Element::HEADER + self.properties.len() as f64 } else { 0.0 }
    }

    /// Grow the box to hold its rows; never shrink it.
    pub fn fit_rows(&mut self) {
        let need = self.rows_height();
        if self.h < need {
            self.h = need;
        }
        let widest = self.properties.iter().map(|p| p.natural_width()).max().unwrap_or(0) as f64 + 4.0;
        if self.has_rows() && self.w < widest.min(60.0) {
            self.w = widest.min(60.0);
        }
    }

    /// The rows the box prints, each with the cell it starts on, top to bottom.
    pub fn row_lines(&self) -> Vec<(f64, f64, String)> {
        self.row_lines_marked(false)
    }

    /// The same, with letters for marks — for a picture set in a face without the symbols.
    pub fn row_lines_marked(&self, plain: bool) -> Vec<(f64, f64, String)> {
        if !self.has_rows() {
            return Vec::new();
        }
        let width = (self.w as usize).saturating_sub(4).max(6);
        let room = (self.h as usize).saturating_sub(1 + Element::HEADER as usize + 1);
        let mut out: Vec<(f64, f64, String)> = Vec::new();
        for (i, p) in self.properties.iter().enumerate() {
            let y = self.y + 1.0 + Element::HEADER + i as f64;
            if i + 1 == room && self.properties.len() > room {
                out.push((self.x + 2.0, y, format!("  … {} more", self.properties.len() - i)));
                break;
            }
            if i >= room {
                break;
            }
            out.push((self.x + 2.0, y, p.row_marked(width, plain)));
        }
        out
    }

    /// The rows as parts — the marked name and the type — for a picture that sets them
    /// against opposite edges itself. The overflow row has no type.
    pub fn row_parts(&self, plain: bool) -> Vec<(f64, f64, String, String)> {
        if !self.has_rows() {
            return Vec::new();
        }
        let room = (self.h as usize).saturating_sub(1 + Element::HEADER as usize + 1);
        let mut out = Vec::new();
        for (i, p) in self.properties.iter().enumerate() {
            let y = self.y + 1.0 + Element::HEADER + i as f64;
            if i + 1 == room && self.properties.len() > room {
                out.push((self.x + 2.0, y, format!("… {} more", self.properties.len() - i), String::new()));
                break;
            }
            if i >= room {
                break;
            }
            let name = if p.required { format!("{}*", p.name) } else { p.name.clone() };
            out.push((self.x + 2.0, y, format!("{}{name}", p.mark(plain).trim_end().to_string() + if p.mark(plain).trim().is_empty() { "" } else { " " }), p.type_text()));
        }
        out
    }

    /// The header rule's row: where the compartment begins.
    pub fn header_rule(&self) -> Option<f64> {
        self.has_rows().then_some(self.y + Element::HEADER)
    }

    /// The line the outline is drawn with: an experimental type is dashed, whatever it asked.
    pub fn drawn_line(&self) -> LineStyle {
        if self.status == Status::Experimental && self.line == LineStyle::Solid { LineStyle::Dashed } else { self.line }
    }

    /// How solid the shape is drawn: a deprecated type fades, whatever it asked.
    pub fn drawn_opacity(&self) -> u8 {
        if self.status == Status::Deprecated { self.opacity.min(50) } else { self.opacity }
    }

    /// The kind's tag as the box prints it: `object ✗` on a deprecated type, the mark last
    /// so a picture can find it and draw it red.
    pub fn tag(&self) -> String {
        self.tag_marked(false)
    }

    /// The same, with a letter for the mark where the face may not have the symbol.
    pub fn tag_marked(&self, plain: bool) -> String {
        match self.status.mark(plain) {
            Some(m) => format!("{} {m}", self.kind.short()),
            None => self.kind.short().to_string(),
        }
    }
}

/// How a grid is marked. `Auto` is each surface's own way — dots on the screen, where a rule
/// would fight the braille, and ruled on paper, the way a desktop tool draws it; `Dots` and
/// `Lines` say one way for every surface.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum GridStyle {
    #[default]
    Auto,
    Dots,
    Lines,
}

impl GridStyle {
    pub const ALL: [GridStyle; 3] = [GridStyle::Auto, GridStyle::Dots, GridStyle::Lines];

    fn is_default(&self) -> bool {
        *self == GridStyle::Auto
    }

    pub fn name(self) -> &'static str {
        match self {
            GridStyle::Auto => "auto",
            GridStyle::Dots => "dots",
            GridStyle::Lines => "lines",
        }
    }

    /// Ruled on the screen and in the terminal picture?
    pub fn ruled_on_screen(self) -> bool {
        self == GridStyle::Lines
    }

    /// Ruled on paper?
    pub fn ruled_on_paper(self) -> bool {
        self != GridStyle::Dots
    }

    pub fn parse(s: &str) -> Option<GridStyle> {
        let want = s.trim().to_ascii_lowercase();
        GridStyle::ALL.into_iter().find(|g| !want.is_empty() && g.name().starts_with(&want))
    }
}

/// A paper size, in the pixels of a 100 % export — a cell ten wide and twenty tall — so a
/// page is a whole number of cells.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Paper {
    #[default]
    UsLetter,
    UsLegal,
    UsTabloid,
    UsExecutive,
    A0,
    A1,
    A2,
    A3,
    A4,
    A5,
    A6,
    A7,
    B4,
    B5,
    Wide16x9,
    Wide16x10,
    Screen4x3,
    /// Width and height, in cells.
    Custom(u32, u32),
}

impl Paper {
    pub const ALL: [Paper; 17] = [
        Paper::UsLetter,
        Paper::UsLegal,
        Paper::UsTabloid,
        Paper::UsExecutive,
        Paper::A0,
        Paper::A1,
        Paper::A2,
        Paper::A3,
        Paper::A4,
        Paper::A5,
        Paper::A6,
        Paper::A7,
        Paper::B4,
        Paper::B5,
        Paper::Wide16x9,
        Paper::Wide16x10,
        Paper::Screen4x3,
    ];

    fn is_default(&self) -> bool {
        *self == Paper::default()
    }

    pub fn name(self) -> String {
        match self {
            Paper::UsLetter => "us-letter".into(),
            Paper::UsLegal => "us-legal".into(),
            Paper::UsTabloid => "us-tabloid".into(),
            Paper::UsExecutive => "us-executive".into(),
            Paper::A0 => "a0".into(),
            Paper::A1 => "a1".into(),
            Paper::A2 => "a2".into(),
            Paper::A3 => "a3".into(),
            Paper::A4 => "a4".into(),
            Paper::A5 => "a5".into(),
            Paper::A6 => "a6".into(),
            Paper::A7 => "a7".into(),
            Paper::B4 => "b4".into(),
            Paper::B5 => "b5".into(),
            Paper::Wide16x9 => "16:9".into(),
            Paper::Wide16x10 => "16:10".into(),
            Paper::Screen4x3 => "4:3".into(),
            Paper::Custom(w, h) => format!("{w}x{h}"),
        }
    }

    /// The page in portrait, in pixels at 100 %.
    pub fn px(self) -> (u32, u32) {
        match self {
            Paper::UsLetter => (850, 1100),
            Paper::UsLegal => (850, 1400),
            Paper::UsTabloid => (1100, 1700),
            Paper::UsExecutive => (700, 1000),
            // The ISO sizes to the nearest whole cell — twenty pixels tall — so a page is
            // a whole number of rows; within a millimetre or two of the real sheet.
            Paper::A0 => (3300, 4680),
            Paper::A1 => (2340, 3300),
            Paper::A2 => (1650, 2340),
            Paper::A3 => (1170, 1660),
            Paper::A4 => (830, 1160),
            Paper::A5 => (580, 820),
            Paper::A6 => (410, 580),
            Paper::A7 => (290, 420),
            Paper::B4 => (980, 1400),
            Paper::B5 => (690, 980),
            Paper::Wide16x9 => (1600, 900),
            Paper::Wide16x10 => (1920, 1200),
            Paper::Screen4x3 => (1600, 1200),
            Paper::Custom(w, h) => (w * 10, h * 20),
        }
    }

    /// The page in cells, turned for landscape.
    pub fn cells(self, landscape: bool) -> (f64, f64) {
        let (w, h) = self.px();
        let (w, h) = ((w as f64 / 10.0).round(), (h as f64 / 20.0).round());
        if landscape { (h, w) } else { (w, h) }
    }

    /// A name, or `WxH` in cells for a custom page.
    pub fn parse(s: &str) -> Option<Paper> {
        let want = s.trim().to_ascii_lowercase();
        if let Some(p) = Paper::ALL.into_iter().find(|p| p.name() == want) {
            return Some(p);
        }
        if let Some((w, h)) = want.split_once('x')
            && let (Ok(w), Ok(h)) = (w.trim().parse::<u32>(), h.trim().parse::<u32>())
            && w >= 8
            && h >= 4
        {
            return Some(Paper::Custom(w, h));
        }
        let mut hits = Paper::ALL.into_iter().filter(|p| p.name().starts_with(&want));
        let first = hits.next()?;
        hits.next().map_or(Some(first), |_| None)
    }
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// A layer: a named stack that shapes and relations sit on, drawn in order — index 0 at the
/// back — and shown or hidden, unlocked or locked, as one. Most diagram tools have layers
/// and hide them behind "to front" and "to back"; here they are a thing you can see, in the
/// layer browser, the way a picture editor shows them.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LayerDef {
    pub id: u32,
    pub name: String,
    #[serde(default = "yes_", skip_serializing_if = "is_true")]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
}

fn yes_() -> bool {
    true
}


fn is_false(b: &bool) -> bool {
    !*b
}

/// The one layer every diagram starts with — and the reason `layers` stays out of a file that
/// never made another: a diagram with the default layer is byte-identical to one from before
/// layers existed.
pub fn default_layers() -> Vec<LayerDef> {
    vec![LayerDef { id: 0, name: "layer 1".into(), visible: true, locked: false }]
}

fn is_default_layers(v: &[LayerDef]) -> bool {
    *v == default_layers()
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Element {
    pub id: ElementId,
    pub kind: ShapeKind,
    #[serde(default)]
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// How the label is set: where in the box, and — for the rendering — in what font and
    /// size. Absent from the file while it is the default.
    #[serde(default, skip_serializing_if = "TextStyle::is_default")]
    pub text: TextStyle,
    /// A colour for the outline, instead of the layer's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Colour>,
    /// What the shape is filled with: the layer's own tint, nothing, or a colour.
    #[serde(default, skip_serializing_if = "Fill::is_auto")]
    pub fill: Fill,
    /// Whether the outline is drawn at all. Off, the fill and the label remain.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub outline: bool,
    /// How the outline is drawn: solid, dashed or dotted.
    #[serde(default, skip_serializing_if = "is_solid")]
    pub line: LineStyle,
    /// How solid the whole shape is, 10 to 100 %.
    #[serde(default = "full", skip_serializing_if = "is_full")]
    pub opacity: u8,
    /// The lean across: how many cells the bottom edge sits to the right of the top edge.
    /// A rectangle so leaned is a parallelogram; anything leans the same way.
    #[serde(default, skip_serializing_if = "is_flat")]
    pub skew: f64,
    /// The lean down: how many cells the right edge sits below the left edge.
    #[serde(default, skip_serializing_if = "is_flat")]
    pub skew_y: f64,
    /// The outline's thickness in braille dots: 1, 2 or 3.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub stroke: u8,
    /// Which ink this shape's own outline and fill draw in — `auto` follows the document's
    /// `:ink`. The relation reaching it, its ports, and its cursor highlight are never part of
    /// this: they stay in the document's ink regardless, so only the shape itself changes.
    #[serde(default, skip_serializing_if = "ElementInk::is_auto")]
    pub ink: ElementInk,
    /// An ontology type's rows: an object type's properties, an interface's, an action
    /// type's parameters. Drawn as a compartment under the header; the box grows to hold them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<Property>,
    /// The name code reads this type by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_name: Option<String>,
    /// The name shown for many of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural: Option<String>,
    #[serde(default, skip_serializing_if = "Status::is_default")]
    pub status: Status,
    #[serde(default, skip_serializing_if = "Visibility::is_default")]
    pub visibility: Visibility,
    /// Which layer it sits on — a [`LayerDef::id`].
    #[serde(default, skip_serializing_if = "is_zero")]
    pub layer: u32,
    /// Locked: it stays where it is and as it is until unlocked. The cursor can still land
    /// on it, and it can still be related to.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    /// Free text from the source this was imported from — Archi's own "Documentation" field,
    /// say. This app never writes one itself; only an import can set it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// Arbitrary key/value tags from the source. See [`Tag`] for why this is not `properties`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
}

fn one() -> u8 {
    1
}

fn full() -> u8 {
    100
}

fn is_full(n: &u8) -> bool {
    *n == 100
}

fn is_solid(l: &LineStyle) -> bool {
    *l == LineStyle::Solid
}

/// What a shape is filled with. Written as one string — `auto`, `none`, a colour — so the
/// file reads the way the sheet does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum Fill {
    /// The layer's own tint — in the exports, and, for anything but a plain sketch shape, on
    /// screen too: an architecture diagram reads filled the way the exports already draw it,
    /// while a freeform box stays the blank whiteboard shape it always was. See
    /// [`crate::ui::canvas::Scene::text`] for where "on screen" is decided.
    #[default]
    Auto,
    /// Nothing: the grid shows through.
    None,
    Colour(Colour),
}

impl Fill {
    fn is_auto(&self) -> bool {
        *self == Fill::Auto
    }

    pub fn name(self) -> String {
        match self {
            Fill::Auto => "auto".into(),
            Fill::None => "none".into(),
            Fill::Colour(c) => c.name(),
        }
    }

    /// `auto` (or blank), `none`, or anything `Colour` reads.
    pub fn parse(s: &str) -> Option<Fill> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Some(Fill::Auto),
            "none" | "no" | "transparent" => Some(Fill::None),
            _ => Colour::parse(s).map(Fill::Colour),
        }
    }

    /// The colour, if there is one to pick.
    pub fn colour(self) -> Option<Colour> {
        match self {
            Fill::Colour(c) => Some(c),
            _ => None,
        }
    }
}

impl From<Fill> for String {
    fn from(f: Fill) -> String {
        f.name()
    }
}

impl TryFrom<String> for Fill {
    type Error = String;
    fn try_from(s: String) -> Result<Fill, String> {
        Fill::parse(&s).ok_or_else(|| format!("not a fill: {s:?}"))
    }
}

impl Element {
    /// Which of the eight looks this shape wears, by its fill and line colour together —
    /// or `custom` when the pair is not one of them.
    pub fn look(&self) -> &'static str {
        ontology::LOOKS
            .iter()
            .find(|(_, fill, line)| self.fill == Fill::Colour(Colour::Hex(*fill)) && self.color == Some(Colour::Hex(*line)))
            .map(|(name, _, _)| *name)
            .unwrap_or("custom")
    }

    /// Put on a look: its fill and its line colour, together.
    pub fn set_look(&mut self, name: &str) -> Result<(), String> {
        let want = name.trim().to_ascii_lowercase();
        let (_, fill, line) = ontology::LOOKS
            .iter()
            .find(|(n, _, _)| *n == want)
            .ok_or_else(|| format!("no look called {name:?} — {}", ontology::LOOKS.iter().map(|l| l.0).collect::<Vec<_>>().join(", ")))?;
        self.fill = Fill::Colour(Colour::Hex(*fill));
        self.color = Some(Colour::Hex(*line));
        Ok(())
    }

    /// What the fill is, on a dark or a light ground: `auto` is the layer's pastel; `none` is
    /// nothing; a colour takes the ground's value.
    pub fn fill_on(&self, light: bool) -> Option<[u8; 3]> {
        match self.fill {
            Fill::Auto => self.kind.layer().pastel(),
            Fill::None => None,
            Fill::Colour(c) => Some(c.on(light)),
        }
    }
}

fn is_one(w: &u8) -> bool {
    *w == 1
}

/// Which ink a shape draws in. `Auto` is the document's own — [`crate::ui::wire::ink()`] — so
/// this lives here, in the model, rather than as that module's `Ink` with a third variant
/// bolted on: a file's `auto`/`lines`/`braille` should not change meaning if the global enum's
/// shape ever does.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ElementInk {
    #[default]
    Auto,
    Lines,
    Braille,
}

impl ElementInk {
    pub const ALL: [ElementInk; 3] = [ElementInk::Auto, ElementInk::Lines, ElementInk::Braille];

    fn is_auto(&self) -> bool {
        *self == ElementInk::Auto
    }

    pub fn name(self) -> &'static str {
        match self {
            ElementInk::Auto => "auto",
            ElementInk::Lines => "lines",
            ElementInk::Braille => "braille",
        }
    }

    pub fn parse(s: &str) -> Option<ElementInk> {
        let want = s.trim().to_ascii_lowercase();
        ElementInk::ALL.into_iter().find(|i| !want.is_empty() && i.name().starts_with(&want))
    }
}

/// How a label is set inside its shape.
///
/// Alignment is honoured on screen. Font and size cannot be — a terminal has one font and one
/// size — so they are honoured where they can be, in the rendering, and shown in the panel so
/// the diagram carries them.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// A family this machine has, by name — see `fonts::families`. `None` is the rendering's
    /// own font.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    /// Pixels per em in the rendering. `None` is the rendering's own size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    #[serde(default, skip_serializing_if = "Align::is_default")]
    pub align: Align,
    #[serde(default, skip_serializing_if = "VAlign::is_default")]
    pub valign: VAlign,
    /// How far the label sits from where its alignment puts it, in cells — anywhere, the
    /// outside of the shape included. Set by dragging it (`T`).
    #[serde(default, skip_serializing_if = "is_still")]
    pub offset: (f64, f64),
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    /// The label's own colour; blank is the ink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Colour>,
    /// A filled band under the label — what makes one legible on a busy line or a grid.
    #[serde(default, skip_serializing_if = "is_false")]
    pub band: bool,
    /// Wrap to the width, or one line cut short with an ellipsis.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub wrap: bool,
    /// The width the label wraps to, in cells; blank is the shape's own inside.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Cells kept clear between the label and the shape's edge.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub padding: u32,
}

impl Default for TextStyle {
    fn default() -> TextStyle {
        TextStyle {
            font: None,
            size: None,
            align: Align::default(),
            valign: VAlign::default(),
            offset: (0.0, 0.0),
            bold: false,
            italic: false,
            underline: false,
            color: None,
            band: false,
            wrap: true,
            width: None,
            padding: 0,
        }
    }
}

fn is_flat(k: &f64) -> bool {
    *k == 0.0
}

fn is_still(o: &(f64, f64)) -> bool {
    *o == (0.0, 0.0)
}

fn all_still(o: &[(f64, f64); 3]) -> bool {
    o.iter().all(is_still)
}

impl TextStyle {
    pub fn is_default(&self) -> bool {
        *self == TextStyle::default()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    #[default]
    Centre,
    Right,
}

impl Align {
    pub const ALL: [Align; 3] = [Align::Left, Align::Centre, Align::Right];

    pub fn name(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Centre => "centre",
            Align::Right => "right",
        }
    }

    pub fn parse(s: &str) -> Option<Align> {
        let want = s.trim().to_ascii_lowercase();
        let want = if want == "center" { "centre".to_string() } else { want };
        Align::ALL.into_iter().find(|a| a.name() == want || (!want.is_empty() && a.name().starts_with(&want)))
    }

    fn is_default(&self) -> bool {
        *self == Align::default()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

impl VAlign {
    pub const ALL: [VAlign; 3] = [VAlign::Top, VAlign::Middle, VAlign::Bottom];

    pub fn name(self) -> &'static str {
        match self {
            VAlign::Top => "top",
            VAlign::Middle => "middle",
            VAlign::Bottom => "bottom",
        }
    }

    pub fn parse(s: &str) -> Option<VAlign> {
        let want = s.trim().to_ascii_lowercase();
        VAlign::ALL.into_iter().find(|a| a.name() == want || (!want.is_empty() && a.name().starts_with(&want)))
    }

    fn is_default(&self) -> bool {
        *self == VAlign::default()
    }
}

impl Element {
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    /// Whether a point lies inside this element's box.
    pub fn contains(&self, (px, py): (f64, f64)) -> bool {
        px >= self.x && px <= self.right() && py >= self.y && py <= self.bottom()
    }

    /// The eight handles of the element's box, clockwise from the top-left corner: TL, top,
    /// TR, right, BR, bottom, BL, left. A handle is a PORT: where a relation may attach.
    pub fn handles(&self) -> [(f64, f64); 8] {
        let (x, y, r, b) = (self.x, self.y, self.right(), self.bottom());
        let (mx, my) = ((x + r) / 2.0, (y + b) / 2.0);
        [(x, y), (mx, y), (r, y), (r, my), (r, b), (mx, b), (x, b), (x, my)]
    }

    /// The handle nearest a point on the element's edge — which port an unanchored relation
    /// is drawn from.
    pub fn nearest_handle(&self, p: (f64, f64)) -> usize {
        self.handles()
            .iter()
            .enumerate()
            .map(|(i, h)| (i, (h.0 - p.0).powi(2) + ((h.1 - p.1) * 2.0).powi(2)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map_or(0, |(i, _)| i)
    }

    /// The handle within `tol` cells of a point, if any — a mouse hit-test, where
    /// `nearest_handle` always answers with *something* even a screen width away.
    pub fn handle_at(&self, p: (f64, f64), tol: f64) -> Option<usize> {
        let i = self.nearest_handle(p);
        let h = self.handles()[i];
        ((h.0 - p.0).powi(2) + ((h.1 - p.1) * 2.0).powi(2) <= tol.powi(2)).then_some(i)
    }

    /// The eight points a hover arrow draws and is clicked at — one per handle, pushed
    /// `gap` further out along the same ray from the centre, so hovering a shape can offer
    /// "add a connected shape this way" without the arrow's own hit-zone overlapping the
    /// handle's (a handle resizes; an arrow, `gap` further out, opens a new one instead).
    /// The push is worked out in the space where a cell reads square — [`crate::shapes::ASPECT`]
    /// corrects it back — or a `gap` that looks right pushed sideways lands twice as far
    /// pushed up or down, exactly the "tight on the sides, loose top and bottom" a cell
    /// twice as tall as wide would otherwise draw.
    pub fn arrows(&self, gap: f64) -> [(f64, f64); 8] {
        let (cx, cy) = self.center();
        self.handles().map(|(hx, hy)| {
            let (dx, dy) = (hx - cx, (hy - cy) * crate::shapes::ASPECT);
            let len = dx.hypot(dy);
            if len == 0.0 { (hx, hy) } else { (hx + dx / len * gap, hy + dy / len * gap / crate::shapes::ASPECT) }
        })
    }

    /// Which of the eight regions around the box a point outside it falls in, if it is
    /// within `margin` of it — the box's own edges and corners extended straight out, not a
    /// circle around each arrow's own drawn position. A circle leaves dead ground between
    /// one arrow's reach and the next; a handle approached from an angle a circle's radius
    /// doesn't cover would lose hover before the mouse ever got there. Edges extended
    /// outward have no such gap: every point in the margin is in exactly one region, however
    /// it was reached, so an arrow can never go cold on the way to it. `margin` is a visual
    /// reach, the same one `arrows()`'s own `gap` is — vertically that is half as many cells,
    /// or the invisible gutter would reach twice as far above and below as it does to each
    /// side, the same mismatch [`Element::arrows`] corrects for the glyph itself.
    pub fn arrow_region(&self, (px, py): (f64, f64), margin: f64) -> Option<usize> {
        let (x, y, r, b) = (self.x, self.y, self.right(), self.bottom());
        let vmargin = margin / crate::shapes::ASPECT;
        if px < x - margin || px > r + margin || py < y - vmargin || py > b + vmargin {
            return None;
        }
        let col = if px < x { 0 } else if px > r { 2 } else { 1 };
        let row = if py < y { 0 } else if py > b { 2 } else { 1 };
        match (row, col) {
            (0, 0) => Some(0), // NW
            (0, 1) => Some(1), // N
            (0, 2) => Some(2), // NE
            (1, 2) => Some(3), // E
            (2, 2) => Some(4), // SE
            (2, 1) => Some(5), // S
            (2, 0) => Some(6), // SW
            (1, 0) => Some(7), // W
            (1, 1) => None,    // inside the box itself — not an arrow region at all
            _ => unreachable!(),
        }
    }

    /// The name a status line or picker calls it by: its label, or its kind when unlabelled.
    pub fn display(&self) -> String {
        match self.label.trim().is_empty() {
            true => format!("({})", self.kind.short()),
            false => self.label.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Relation {
    pub id: RelationId,
    pub kind: RelationKind,
    pub from: ElementId,
    pub to: ElementId,
    /// The label at the centre of the line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The labels at either end — a cardinality, a role, a port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_label: Option<String>,
    /// A look of its own, overriding the kind's notation. What a plain link is configured
    /// with; absent on every relation whose kind decides how it is drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Notation>,
    /// Which handle of each element the relation is attached to — see `Element::handles`.
    /// `None` is unanchored: the line leaves wherever the edge faces the other element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_port: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_port: Option<u8>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub layer: u32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    /// How far each node's label sits from its place on the line — tail, centre, head.
    #[serde(default, skip_serializing_if = "all_still")]
    pub offsets: [(f64, f64); 3],
    /// How the three labels are set: font, size, bold, italic, colour, band. Alignment and
    /// the offset are not used here — a relation's labels have places of their own.
    #[serde(default, skip_serializing_if = "TextStyle::is_default")]
    pub text: TextStyle,
    /// Where along the line the centre label sits, in per cent from the tail; blank is
    /// half way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_at: Option<u8>,
    /// Where an orthogonal route turns: per cent of the way along its first leg, from the
    /// tail; blank is half way. Kept as a fraction of the leg rather than a cell count so a
    /// bend stays proportionally where it was put as the two shapes move — a version-2
    /// file's own cell count is converted once, on load, by `Document::migrate_legacy_elbows`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elbow: Option<i64>,
    /// Free text from the source this was imported from — this app never writes one itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// Arbitrary key/value tags from the source. See [`Tag`] for why this is not `properties`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
}

impl Relation {
    pub fn offset_at(&self, node: Node) -> (f64, f64) {
        self.offsets[node.index()]
    }

    pub fn offset_mut(&mut self, node: Node) -> &mut (f64, f64) {
        &mut self.offsets[node.index()]
    }
}

/// Which way to move something in the drawing order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Raise {
    /// Above everything on its layer.
    Front,
    /// Beneath everything on its layer.
    Back,
    /// One step up.
    Forward,
    /// One step down.
    Backward,
}

/// The three nodes of a link: where it starts, its centre, and where it ends.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Node {
    Tail,
    #[default]
    Centre,
    Head,
}

impl Node {
    pub fn index(self) -> usize {
        match self {
            Node::Tail => 0,
            Node::Centre => 1,
            Node::Head => 2,
        }
    }

    pub const ALL: [Node; 3] = [Node::Tail, Node::Centre, Node::Head];

    pub fn name(self) -> &'static str {
        match self {
            Node::Tail => "tail",
            Node::Centre => "centre",
            Node::Head => "head",
        }
    }
}

impl Relation {
    /// How it is drawn: its own look if it has one, else its kind's.
    pub fn notation(&self) -> Notation {
        self.style.unwrap_or_else(|| self.kind.notation())
    }

    pub fn label_at(&self, node: Node) -> Option<&str> {
        match node {
            Node::Tail => self.tail_label.as_deref(),
            Node::Centre => self.label.as_deref(),
            Node::Head => self.head_label.as_deref(),
        }
    }

    pub fn set_label_at(&mut self, node: Node, text: &str) {
        let v = if text.trim().is_empty() { None } else { Some(text.trim().to_string()) };
        match node {
            Node::Tail => self.tail_label = v,
            Node::Centre => self.label = v,
            Node::Head => self.head_label = v,
        }
    }
}

/// One relation the rules refuse, and why — what `:lint` lists and the canvas marks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub relation: RelationId,
    pub why: &'static str,
}

impl Document {
    pub fn alloc_element_id(&mut self) -> ElementId {
        let id = self.next_element_id;
        self.next_element_id += 1;
        id
    }

    pub fn alloc_relation_id(&mut self) -> RelationId {
        let id = self.next_relation_id;
        self.next_relation_id += 1;
        id
    }

    /// Place an element of `kind` at `(x, y)`, at the kind's own default size.
    /// Where a relation node's label sits: its place along the line, plus wherever it has
    /// been dragged to. Shared by the screen and every export.
    pub fn label_point(&self, r: &Relation, node: Node) -> Option<(f64, f64)> {
        let pts = self.route(r)?;
        let t = match node {
            Node::Tail => 0.18,
            Node::Centre => r.label_at.map_or(0.5, |p| p as f64 / 100.0),
            Node::Head => 0.82,
        };
        let (x, y) = crate::shapes::along(&pts, t);
        let (dx, dy) = r.offset_at(node);
        Some((x + dx, y + dy))
    }

    /// The line a relation is drawn along, end to end: two points for a straight one, the
    /// corners of an orthogonal one, a sampled curve. Every surface draws from this.
    pub fn route(&self, r: &Relation) -> Option<Vec<(f64, f64)>> {
        let (p1, p2) = self.end_points(r)?;
        let across = self.leaves_across(r, p1, p2);
        let notation = r.notation();
        // An elbow the person set by hand (`:bend`, or dragging it) is theirs to keep — this
        // only picks one for them, the way `leaves_across`'s own guess is only a default too.
        let elbow = if r.elbow.is_none() && notation.route == ontology::Route::Orthogonal { self.clear_elbow(r, p1, p2, across) } else { r.elbow };
        Some(crate::shapes::route_from(p1, p2, notation.route, elbow, Some(across)))
    }

    /// Every element an orthogonal route's own legs must not cut through: not the relation's
    /// two ends, not a grouping or location — open by design, what is inside it has to stay
    /// visible — and not a plain label, which has no boundary of its own to speak of.
    fn route_obstacles(&self, r: &Relation) -> Vec<(f64, f64, f64, f64)> {
        self.elements
            .iter()
            .filter(|e| e.id != r.from && e.id != r.to && self.element_visible(e.id) && !e.kind.is_composite() && e.kind.shape() != ontology::Shape::Text)
            .map(|e| (e.x, e.y, e.w, e.h))
            .collect()
    }

    /// A little clearance so a route reads as going around a shape, not just grazing its
    /// edge — the same spirit as the gutter a fresh element is placed with.
    const ROUTE_CLEARANCE: f64 = 0.5;

    /// The elbow an *automatic* orthogonal route picks for itself: the plain midpoint if
    /// nothing sits in the way, or, if something does, the nearest one on either side that
    /// clears every obstacle — the least surprising bend that still doesn't cut through a
    /// third shape's own boundary. `None` (the midpoint) if nothing is in the way, or if
    /// nothing else clears every obstacle either — a route some shape simply cannot dodge is
    /// better drawn straight through, honestly, than nudged into an arbitrary-looking bend
    /// that fixes nothing. Searched in whole per cent points, the same unit a hand-set
    /// `elbow` is now kept in, so this never picks a value the person couldn't have dragged
    /// to themselves.
    fn clear_elbow(&self, r: &Relation, p1: (f64, f64), p2: (f64, f64), across: bool) -> Option<i64> {
        let obstacles: Vec<(f64, f64, f64, f64)> = self
            .route_obstacles(r)
            .into_iter()
            .map(|(x, y, w, h)| (x - Self::ROUTE_CLEARANCE, y - Self::ROUTE_CLEARANCE, w + 2.0 * Self::ROUTE_CLEARANCE, h + 2.0 * Self::ROUTE_CLEARANCE))
            .collect();
        if obstacles.is_empty() {
            return None;
        }
        let clear = |elbow: Option<i64>| {
            let pts = crate::shapes::route_from(p1, p2, ontology::Route::Orthogonal, elbow, Some(across));
            !obstacles.iter().any(|&rect| crate::shapes::route_crosses(&pts, rect))
        };
        if clear(None) {
            return None;
        }
        for step in 1..=50 {
            for cand in [50 + step, 50 - step] {
                if (0..=100).contains(&cand) && clear(Some(cand)) {
                    return Some(cand);
                }
            }
        }
        None
    }

    /// Which way an orthogonal route leaves its tail — across, from a side, or down, from a
    /// top or bottom edge — not along the edge itself. Resolved once here, the same way for
    /// drawing (`route`) and for deciding which way `bend` pulls, so the two can never
    /// disagree about which axis a relation's one turn runs along.
    fn leaves_across(&self, r: &Relation, p1: (f64, f64), p2: (f64, f64)) -> bool {
        let (dx, dy) = (p2.0 - p1.0, p2.1 - p1.1);
        let on_edge = self.element(r.from).and_then(|e| {
            let on_top_or_bottom = (p1.1 - e.y).abs() < 0.5 || (p1.1 - e.bottom()).abs() < 0.5;
            let on_side = (p1.0 - e.x).abs() < 0.5 || (p1.0 - e.right()).abs() < 0.5;
            match (on_side, on_top_or_bottom) {
                (true, false) => Some(true),
                (false, true) => Some(false),
                _ => None,
            }
        });
        on_edge.unwrap_or(dx.abs() >= dy.abs() * crate::shapes::ASPECT)
    }

    /// Whether — and which way — an orthogonal relation's one turn can be pulled: `Some(true)`
    /// if it leaves across, so the pull is left/right; `Some(false)` if it leaves down, so
    /// the pull is up/down; `None` for anything with no turn to speak of — a straight or
    /// curved route has neither `elbow` nor a middle leg to move.
    pub fn bendable(&self, r: &Relation) -> Option<bool> {
        if r.notation().route != ontology::Route::Orthogonal {
            return None;
        }
        let (p1, p2) = self.end_points(r)?;
        Some(self.leaves_across(r, p1, p2))
    }

    /// Slide an orthogonal relation's one turn by `n` percentage points of its own leg —
    /// always in the screen direction `bend`'s caller means (positive right or down),
    /// regardless of which of the two elements happens to be `from`: `elbow` itself is a
    /// fraction along the first leg, not a screen coordinate, so the sign it needs depends on
    /// which way that leg actually points. Clamped to the leg itself — pulling past either
    /// end would turn before the tail or after the head, a point the leg doesn't have.
    pub fn bend(&mut self, rid: RelationId, n: f64) {
        let Some(r) = self.relation(rid) else { return };
        let Some((p1, p2)) = self.end_points(r) else { return };
        if r.notation().route != ontology::Route::Orthogonal {
            return;
        }
        let across = self.leaves_across(r, p1, p2);
        let (dx, dy) = (p2.0 - p1.0, p2.1 - p1.1);
        let span = if across { dx } else { dy };
        let sign = if span == 0.0 { 1.0 } else { span.signum() };
        let Some(r) = self.relation_mut(rid) else { return };
        let base = r.elbow.unwrap_or(50);
        r.elbow = Some((base + (n * sign).round() as i64).clamp(0, 100));
    }

    /// A version-2 file's own `elbow` was cells from the tail; read into this version, that
    /// number is a per-cent one, so a bend inherited from an old file would land somewhere
    /// else on the very first draw. Converts every relation that had one, using the geometry
    /// the file itself still describes at the moment it opens, and marks the diagram current
    /// so a re-save never runs this twice. Called once, right after `persistence::load`
    /// deserializes an older file.
    pub(crate) fn migrate_legacy_elbows(&mut self) {
        if self.version >= CURRENT_VERSION {
            return;
        }
        let converted: Vec<(RelationId, i64)> = self
            .relations
            .iter()
            .filter_map(|r| {
                let cells = r.elbow?;
                let (p1, p2) = self.end_points(r)?;
                let across = self.leaves_across(r, p1, p2);
                let span = if across { p2.0 - p1.0 } else { p2.1 - p1.1 };
                let pct = if span.abs() < f64::EPSILON { 50 } else { ((cells as f64 / span.abs()) * 100.0).round().clamp(0.0, 100.0) as i64 };
                Some((r.id, pct))
            })
            .collect();
        for (id, pct) in converted {
            if let Some(r) = self.relation_mut(id) {
                r.elbow = Some(pct);
            }
        }
        self.version = CURRENT_VERSION;
    }

    pub fn add(&mut self, kind: ShapeKind, label: impl Into<String>, x: f64, y: f64) -> ElementId {
        let (w, h) = kind.default_size();
        let id = self.alloc_element_id();
        let layer = self.metadata.layer;
        self.elements.push(Element {
            id,
            kind,
            label: label.into(),
            x,
            y,
            w,
            h,
            text: TextStyle::default(),
            color: None,
            fill: Fill::Auto,
            outline: true,
            line: LineStyle::Solid,
            // A stick figure is head, arms and legs — all curve and diagonal — which line
            // art can only stair-step; braille draws the person it is meant to look like
            // even when the rest of the document reads better in lines.
            ink: if kind == ShapeKind::StickFigure { ElementInk::Braille } else { ElementInk::Auto },
            opacity: 100,
            skew: 0.0,
            skew_y: 0.0,
            properties: Vec::new(),
            api_name: None,
            plural: None,
            status: Status::Active,
            visibility: Visibility::Normal,
            stroke: 1,
            layer,
            locked: false,
            documentation: None,
            tags: Vec::new(),
        });
        id
    }

    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }

    pub fn element_mut(&mut self, id: ElementId) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id == id)
    }

    /// Remove an element and every relation that touched it — a dangling relation is a line
    /// to nowhere, and the file loader would rightly refuse it.
    pub fn remove_element(&mut self, id: ElementId) {
        self.elements.retain(|e| e.id != id);
        self.relations.retain(|r| r.from != id && r.to != id);
    }

    pub fn relation(&self, id: RelationId) -> Option<&Relation> {
        self.relations.iter().find(|r| r.id == id)
    }

    pub fn relation_mut(&mut self, id: RelationId) -> Option<&mut Relation> {
        self.relations.iter_mut().find(|r| r.id == id)
    }

    pub fn remove_relation(&mut self, id: RelationId) {
        self.relations.retain(|r| r.id != id);
    }

    /// Join two elements. Refuses a relation from an element to itself — the one line that can
    /// never mean anything — and a relation between elements that do not exist.
    pub fn connect(&mut self, kind: RelationKind, from: ElementId, to: ElementId) -> Result<RelationId, &'static str> {
        if from == to {
            return Err("an element cannot relate to itself");
        }
        if self.element(from).is_none() || self.element(to).is_none() {
            return Err("no such element");
        }
        let id = self.alloc_relation_id();
        let layer = self.metadata.layer;
        self.relations.push(Relation { id, kind, from, to, label: None, tail_label: None, head_label: None, style: None, from_port: None, to_port: None, layer, locked: false, offsets: [(0.0, 0.0); 3], text: TextStyle::default(), label_at: None, elbow: None, documentation: None, tags: Vec::new() });
        Ok(id)
    }

    /// Every relation touching an element: outgoing first, then incoming, each in file order.
    /// This is the order `Tab` walks them in, so it has to be stable.
    pub fn incident(&self, id: ElementId) -> Vec<RelationId> {
        let out = self.relations.iter().filter(|r| r.from == id).map(|r| r.id);
        let inc = self.relations.iter().filter(|r| r.to == id && r.from != id).map(|r| r.id);
        out.chain(inc).collect()
    }

    /// The far end of a relation, seen from `here`.
    pub fn other_end(&self, rel: RelationId, here: ElementId) -> Option<ElementId> {
        let r = self.relation(rel)?;
        Some(if r.from == here { r.to } else { r.from })
    }

    /// Whether the rules allow a relation as drawn.
    pub fn check(&self, r: &Relation) -> Result<(), &'static str> {
        let (Some(a), Some(b)) = (self.element(r.from), self.element(r.to)) else {
            return Err("a relation to an element that is not there");
        };
        ontology::allowed(r.kind, a.kind, b.kind)
    }

    /// Every relation the rules refuse, in file order.
    pub fn lint(&self) -> Vec<Problem> {
        self.relations
            .iter()
            .filter_map(|r| self.check(r).err().map(|why| Problem { relation: r.id, why }))
            .collect()
    }

    /// Where a relation's line starts and ends: at its ports where it has them, else where
    /// each element's edge faces the other. The one place this is decided.
    pub fn end_points(&self, r: &Relation) -> Option<((f64, f64), (f64, f64))> {
        let (a, b) = (self.element(r.from)?, self.element(r.to)?);
        let p1 = match r.from_port {
            Some(p) => a.handles()[(p as usize).min(7)],
            None => crate::shapes::edge_point(a.x, a.y, a.w, a.h, b.center()),
        };
        let p2 = match r.to_port {
            Some(p) => b.handles()[(p as usize).min(7)],
            None => crate::shapes::edge_point(b.x, b.y, b.w, b.h, a.center()),
        };
        Some((p1, p2))
    }

    /// Which handle of `el` the relation is on — its port, or the handle nearest where its
    /// unanchored line leaves. `None` if the relation does not touch the element.
    pub fn port_on(&self, r: &Relation, el: ElementId) -> Option<usize> {
        let e = self.element(el)?;
        let (p1, p2) = self.end_points(r)?;
        if r.from == el {
            Some(r.from_port.map_or_else(|| e.nearest_handle(p1), |p| p as usize))
        } else if r.to == el {
            Some(r.to_port.map_or_else(|| e.nearest_handle(p2), |p| p as usize))
        } else {
            None
        }
    }

    /// Every relation attached at handle `port` of `el`, outgoing first.
    pub fn at_port(&self, el: ElementId, port: usize) -> Vec<RelationId> {
        self.incident(el)
            .into_iter()
            .filter(|&rid| self.relation(rid).and_then(|r| self.port_on(r, el)) == Some(port))
            .collect()
    }

    /// Elements outside what the diagram's view is about — listed by `:lint`, never refused.
    pub fn out_of_view(&self) -> Vec<ElementId> {
        let v = self.metadata.view;
        self.elements.iter().filter(|e| !v.shows(e.kind)).map(|e| e.id).collect()
    }

    /// The elements drawn inside a composite — by position, not by a stored list. A grouping
    /// *is* the box, so what is in it is what is in the box: nothing to keep in sync, nothing
    /// that can go stale when something is dragged out.
    pub fn members(&self, group: ElementId) -> Vec<ElementId> {
        let Some(g) = self.element(group) else { return Vec::new() };
        if !g.kind.is_composite() {
            return Vec::new();
        }
        self.elements
            .iter()
            .filter(|e| e.id != group && g.contains(e.center()))
            // A composite inside a composite is a member; a composite *containing* this
            // one is not, even though its centre may well lie inside.
            .filter(|e| !(e.kind.is_composite() && e.contains(g.center()) && e.w * e.h >= g.w * g.h))
            .map(|e| e.id)
            .collect()
    }

    // ─── layers and drawing order ──────────────────────────────────────────

    pub fn layer(&self, id: u32) -> Option<&LayerDef> {
        self.layers.iter().find(|l| l.id == id)
    }

    /// Where a layer sits in the stack: 0 is the back.
    fn layer_index(&self, id: u32) -> usize {
        self.layers.iter().position(|l| l.id == id).unwrap_or(0)
    }

    /// Whether an element is drawn and can be stood on: its layer is visible.
    pub fn element_visible(&self, id: ElementId) -> bool {
        self.element(id).is_some_and(|e| self.layer(e.layer).is_none_or(|l| l.visible))
    }

    /// A relation shows when its own layer does and both its ends do.
    pub fn relation_visible(&self, id: RelationId) -> bool {
        self.relation(id).is_some_and(|r| {
            self.layer(r.layer).is_none_or(|l| l.visible) && self.element_visible(r.from) && self.element_visible(r.to)
        })
    }

    /// Whether an element may be changed: neither it nor its layer is locked.
    pub fn element_locked(&self, id: ElementId) -> bool {
        self.element(id).is_some_and(|e| e.locked || self.layer(e.layer).is_some_and(|l| l.locked))
    }

    pub fn relation_locked(&self, id: RelationId) -> bool {
        self.relation(id).is_some_and(|r| r.locked || self.layer(r.layer).is_some_and(|l| l.locked))
    }

    /// The elements in drawing order: back layer first, and within a layer in the order they
    /// sit in the document — which `raise` rearranges.
    pub fn elements_in_order(&self) -> Vec<&Element> {
        let mut v: Vec<&Element> = self.elements.iter().collect();
        v.sort_by_key(|e| self.layer_index(e.layer));
        v
    }

    /// The frontmost visible element under a point — what a mouse click lands on. A grouping
    /// is always drawn behind what it holds (`raise_element(.., Raise::Back)` on creation), so
    /// scanning drawing order back-to-front needs no separate case for "prefer the member over
    /// the box it sits in": the member is simply drawn later, and so is found first here.
    pub fn element_at(&self, p: (f64, f64)) -> Option<ElementId> {
        self.elements_in_order().into_iter().rev().find(|e| self.element_visible(e.id) && e.contains(p)).map(|e| e.id)
    }

    /// The frontmost visible element a point is inside, or — failing that — within its
    /// hover-arrow margin: what `hover` tracks, so the whole gutter around a shape's box
    /// stays live right up until the mouse actually leaves it, not the instant it crosses
    /// the border on the way to whichever arrow it's headed for.
    pub fn element_near(&self, p: (f64, f64), arrow_margin: f64) -> Option<ElementId> {
        self.elements_in_order()
            .into_iter()
            .rev()
            .find(|e| self.element_visible(e.id) && (e.contains(p) || e.arrow_region(p, arrow_margin).is_some()))
            .map(|e| e.id)
    }

    /// The frontmost visible element with a handle within `tol` of a point — a handle sits
    /// exactly on its box's border, so half its own hit-circle falls outside `contains`; a
    /// lookup gated on being inside the box first (as `element_at` is) would silently throw
    /// that outward half away; a click a hair short of the border would then fall through to
    /// the arrow gutter, not the handle it looks like it landed on. This asks each element's
    /// own `handle_at` instead of the point-in-box question `element_at` asks.
    pub fn element_for_handle(&self, p: (f64, f64), tol: f64) -> Option<(ElementId, usize)> {
        self.elements_in_order().into_iter().rev().filter(|e| self.element_visible(e.id)).find_map(|e| e.handle_at(p, tol).map(|h| (e.id, h)))
    }

    /// The relation whose route passes within `tol` cells of a point, if any — the nearest
    /// one, where more than one line crosses so close together.
    pub fn relation_at(&self, p: (f64, f64), tol: f64) -> Option<RelationId> {
        self.relations
            .iter()
            .filter(|r| self.relation_visible(r.id))
            .filter_map(|r| {
                let route = self.route(r)?;
                let d = route.windows(2).map(|seg| dist_to_segment(p, seg[0], seg[1])).fold(f64::INFINITY, f64::min);
                (d <= tol).then_some((r.id, d))
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(id, _)| id)
    }

    pub fn relations_in_order(&self) -> Vec<&Relation> {
        let mut v: Vec<&Relation> = self.relations.iter().collect();
        v.sort_by_key(|r| self.layer_index(r.layer));
        v
    }

    /// Move an element in the drawing order, among the elements of its own layer.
    pub fn raise_element(&mut self, id: ElementId, how: Raise) {
        let Some(pos) = self.elements.iter().position(|e| e.id == id) else { return };
        let layer = self.elements[pos].layer;
        let same: Vec<usize> = self.elements.iter().enumerate().filter(|(_, e)| e.layer == layer).map(|(i, _)| i).collect();
        let e = self.elements.remove(pos);
        let at = match how {
            Raise::Front => *same.last().unwrap_or(&pos),
            Raise::Back => *same.first().unwrap_or(&pos),
            Raise::Forward => same.iter().find(|&&i| i > pos).copied().unwrap_or(pos),
            Raise::Backward => same.iter().rev().find(|&&i| i < pos).copied().unwrap_or(pos),
        };
        self.elements.insert(at.min(self.elements.len()), e);
    }

    pub fn raise_relation(&mut self, id: RelationId, how: Raise) {
        let Some(pos) = self.relations.iter().position(|r| r.id == id) else { return };
        let layer = self.relations[pos].layer;
        let same: Vec<usize> = self.relations.iter().enumerate().filter(|(_, r)| r.layer == layer).map(|(i, _)| i).collect();
        let r = self.relations.remove(pos);
        let at = match how {
            Raise::Front => *same.last().unwrap_or(&pos),
            Raise::Back => *same.first().unwrap_or(&pos),
            Raise::Forward => same.iter().find(|&&i| i > pos).copied().unwrap_or(pos),
            Raise::Backward => same.iter().rev().find(|&&i| i < pos).copied().unwrap_or(pos),
        };
        self.relations.insert(at.min(self.relations.len()), r);
    }

    /// A new layer on top of the stack, named, and made current.
    pub fn add_layer(&mut self, name: &str) -> u32 {
        let id = self.layers.iter().map(|l| l.id + 1).max().unwrap_or(1);
        self.layers.push(LayerDef { id, name: name.to_string(), visible: true, locked: false });
        self.metadata.layer = id;
        id
    }

    /// Remove a layer; what was on it goes to the layer beneath (or above, for the bottom
    /// one). The last layer cannot go.
    pub fn remove_layer(&mut self, id: u32) -> Result<(), &'static str> {
        if self.layers.len() == 1 {
            return Err("the only layer — a diagram needs one");
        }
        let Some(i) = self.layers.iter().position(|l| l.id == id) else { return Err("no such layer") };
        let onto = if i > 0 { self.layers[i - 1].id } else { self.layers[1].id };
        for e in &mut self.elements {
            if e.layer == id {
                e.layer = onto;
            }
        }
        for r in &mut self.relations {
            if r.layer == id {
                r.layer = onto;
            }
        }
        self.layers.remove(i);
        if self.metadata.layer == id {
            self.metadata.layer = onto;
        }
        Ok(())
    }

    /// Move a layer one step up (`+1`) or down (`-1`) the stack.
    pub fn shift_layer(&mut self, id: u32, by: isize) {
        let Some(i) = self.layers.iter().position(|l| l.id == id) else { return };
        let j = i as isize + by;
        if j >= 0 && (j as usize) < self.layers.len() {
            self.layers.swap(i, j as usize);
        }
    }

    /// How many things sit on a layer.
    pub fn layer_count(&self, id: u32) -> (usize, usize) {
        (self.elements.iter().filter(|e| e.layer == id).count(), self.relations.iter().filter(|r| r.layer == id).count())
    }

    /// The bounding box of everything, or `None` when empty.
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut it = self.elements.iter();
        let first = it.next()?;
        let mut b = (first.x, first.y, first.right(), first.bottom());
        for e in it {
            b.0 = b.0.min(e.x);
            b.1 = b.1.min(e.y);
            b.2 = b.2.max(e.right());
            b.3 = b.3.max(e.bottom());
        }
        Some(b)
    }
}

/// A point's distance to a line segment — `relation_at`'s hit test, since a route is a chain
/// of these end to end.
fn dist_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let len2 = abx * abx + aby * aby;
    if len2 == 0.0 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    let t = (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0);
    let (cx, cy) = (a.0 + t * abx, a.1 + t * aby);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::ShapeKind::*;
    use super::Document;

    #[test]
    fn document_round_trips_through_json() {
        let mut doc = Document::default();
        let a = doc.add(ApplicationComponent, "CRM", 1.0, 2.0);
        let d = doc.add(DataObject, "customers", 20.0, 2.0);
        doc.connect(RelationKind::Access, a, d).unwrap();
        doc.metadata.view = View::Application;

        let json = serde_json::to_string(&doc).expect("serialize");
        let restored: Document = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, doc);
        assert!(json.contains("\"application_component\""), "kinds are written by name: {json}");
    }

    #[test]
    fn a_fresh_stick_figure_defaults_to_braille_ink_and_everything_else_to_auto() {
        let mut doc = Document::default();
        let figure = doc.add(StickFigure, "Customer", 0.0, 0.0);
        let box_ = doc.add(Box, "note", 10.0, 0.0);
        assert_eq!(doc.element(figure).unwrap().ink, ElementInk::Braille, "all curve and diagonal — lines can only stair-step it");
        assert_eq!(doc.element(box_).unwrap().ink, ElementInk::Auto, "everything else still follows the document's own :ink");
    }

    #[test]
    fn a_default_text_style_stays_out_of_the_file_and_a_set_one_round_trips() {
        let mut doc = Document::default();
        let a = doc.add(BusinessActor, "Customer", 0.0, 0.0);
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("\"text\""), "nothing to say, nothing written: {json}");
        let e = doc.element_mut(a).unwrap();
        e.text.align = Align::Left;
        e.text.font = Some("Menlo".into());
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"align\":\"left\"") && json.contains("\"font\":\"Menlo\""), "{json}");
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn a_partial_file_still_loads_via_serde_default() {
        let old = r#"{ "version": 2, "elements": [
            { "id": 0, "kind": "node", "x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0 } ] }"#;
        let doc: Document = serde_json::from_str(old).expect("deserialize");
        assert_eq!(doc.elements[0].label, "");
        assert_eq!(doc.metadata.view, View::Free);
    }

    #[test]
    fn removing_an_element_removes_the_relations_that_touched_it() {
        let mut doc = Document::default();
        let a = doc.add(BusinessProcess, "", 0.0, 0.0);
        let b = doc.add(BusinessService, "", 10.0, 0.0);
        doc.connect(RelationKind::Realization, a, b).unwrap();
        doc.remove_element(a);
        assert_eq!(doc.elements.len(), 1);
        assert!(doc.relations.is_empty(), "no line to nowhere survives");
    }

    #[test]
    fn a_relation_to_itself_is_refused() {
        let mut doc = Document::default();
        let a = doc.add(BusinessProcess, "", 0.0, 0.0);
        assert!(doc.connect(RelationKind::Flow, a, a).is_err());
    }

    #[test]
    fn lint_names_the_relation_the_rules_refuse_and_why() {
        let mut doc = Document::default();
        let p = doc.add(BusinessProcess, "", 0.0, 0.0);
        let n = doc.add(Node, "", 30.0, 0.0);
        let bad = doc.connect(RelationKind::Composition, p, n).unwrap();
        let ok = doc.connect(RelationKind::Association, p, n).unwrap();
        let problems = doc.lint();
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].relation, bad);
        assert!(!problems.iter().any(|p| p.relation == ok));
        assert!(problems[0].why.contains("same layer"), "{}", problems[0].why);
    }

    #[test]
    fn the_page_stays_out_of_the_file_until_set_and_a_paper_has_a_size_in_cells() {
        let mut doc = Document::default();
        doc.add(Node, "a", 0.0, 0.0);
        assert!(!serde_json::to_string(&doc).unwrap().contains("\"page\""));
        doc.metadata.page.paper = Paper::A4;
        doc.metadata.page.landscape = true;
        doc.metadata.page.grid_color = Colour::parse("#e6e6e6");
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"paper\":\"a4\"") && json.contains("\"grid_color\":\"#e6e6e6\""), "{json}");
        assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), doc);
        assert_eq!(Paper::A4.cells(false), (83.0, 58.0));
        assert_eq!(Paper::A4.cells(true), (58.0, 83.0));
        assert_eq!(Paper::Custom(42, 150).cells(true), (150.0, 42.0), "custom is stored in cells, portrait");
        assert_eq!(Paper::parse("16:9"), Some(Paper::Wide16x9));
        assert_eq!(Paper::parse("120x40"), Some(Paper::Custom(120, 40)));
        assert_eq!(Paper::parse("us-l"), None, "letter or legal: ambiguous");
        assert_eq!(Paper::parse("us-le"), None);
        assert_eq!(Paper::parse("us-lett"), Some(Paper::UsLetter));
    }

    #[test]
    fn a_shape_s_look_stays_out_of_the_file_until_set_and_reads_back_as_strings() {
        let mut doc = Document::default();
        let id = doc.add(Node, "a", 0.0, 0.0);
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("fill") && !json.contains("outline") && !json.contains("opacity") && !json.contains("\"line\""), "{json}");
        let e = doc.element_mut(id).unwrap();
        e.fill = Fill::Colour(Colour::Hex([1, 2, 3]));
        e.outline = false;
        e.line = LineStyle::Dashed;
        e.opacity = 50;
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"fill\":\"#010203\"") && json.contains("\"outline\":false") && json.contains("\"line\":\"dashed\"") && json.contains("\"opacity\":50"), "{json}");
        assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), doc);
        assert_eq!(Fill::parse("none"), Some(Fill::None));
        assert_eq!(Fill::parse("Auto"), Some(Fill::Auto));
        assert_eq!(Fill::parse("red"), Some(Fill::Colour(Colour::Named(crate::ontology::Paint::Red))));
    }

    #[test]
    fn layers_stay_out_of_a_file_until_a_second_one_exists() {
        let mut doc = Document::default();
        doc.add(Node, "a", 0.0, 0.0);
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("\"layers\"") && !json.contains("\"layer\""), "{json}");
        let top = doc.add_layer("annotations");
        let b = doc.add(Text, "note", 0.0, 10.0);
        assert_eq!(doc.element(b).unwrap().layer, top, "new shapes go on the current layer");
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("annotations"));
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn raising_reorders_within_a_layer_only_and_hidden_layers_hide_their_relations() {
        let mut doc = Document::default();
        let a = doc.add(Box, "a", 0.0, 0.0);
        let b = doc.add(Box, "b", 0.0, 0.0);
        let c = doc.add(Box, "c", 0.0, 0.0);
        doc.raise_element(a, Raise::Front);
        let order: Vec<ElementId> = doc.elements_in_order().iter().map(|e| e.id).collect();
        assert_eq!(order, vec![b, c, a]);
        doc.raise_element(a, Raise::Backward);
        let order: Vec<ElementId> = doc.elements_in_order().iter().map(|e| e.id).collect();
        assert_eq!(order, vec![b, a, c]);
        doc.raise_element(c, Raise::Back);
        let order: Vec<ElementId> = doc.elements_in_order().iter().map(|e| e.id).collect();
        assert_eq!(order, vec![c, b, a]);
        // A second layer sits above the first whatever the document order says.
        let top = doc.add_layer("top");
        let d = doc.add(Box, "d", 0.0, 0.0);
        doc.raise_element(d, Raise::Back);
        let order: Vec<ElementId> = doc.elements_in_order().iter().map(|e| e.id).collect();
        assert_eq!(order.last(), Some(&d), "back of the top layer is still above the bottom layer");
        let r = doc.connect(RelationKind::Link, a, d).unwrap();
        assert!(doc.relation_visible(r));
        doc.layers.iter_mut().find(|l| l.id == top).unwrap().visible = false;
        assert!(!doc.element_visible(d) && doc.element_visible(a));
        assert!(!doc.relation_visible(r), "a relation to a hidden shape is hidden");
        // Removing the top layer drops its things onto the one beneath.
        doc.remove_layer(top).unwrap();
        assert_eq!(doc.element(d).unwrap().layer, 0);
        assert!(doc.remove_layer(0).is_err(), "the last layer stays");
    }

    #[test]
    fn a_grouping_holds_what_is_drawn_inside_it_by_position() {
        let mut doc = Document::default();
        let g = doc.add(Grouping, "crm", 0.0, 0.0);
        let inside = doc.add(ApplicationComponent, "", 2.0, 2.0);
        let outside = doc.add(ApplicationComponent, "", 100.0, 2.0);
        assert_eq!(doc.members(g), vec![inside]);
        doc.element_mut(outside).unwrap().x = 4.0;
        assert_eq!(doc.members(g).len(), 2, "moving in is joining");
        assert!(doc.members(inside).is_empty(), "only a composite has members");
    }

    #[test]
    fn a_click_inside_a_grouping_lands_on_the_member_drawn_in_front_of_it() {
        let mut doc = Document::default();
        let g = doc.add(Grouping, "crm", 0.0, 0.0);
        let inside = doc.add(ApplicationComponent, "", 2.0, 2.0);
        assert_eq!(doc.element_at((3.0, 3.0)), Some(inside), "the member, not the box behind it");
        assert_eq!(doc.element_at((0.5, 0.5)), Some(g), "ground the grouping has and the member does not");
        assert_eq!(doc.element_at((-5.0, -5.0)), None, "off every box");
    }

    #[test]
    fn a_handle_hit_test_has_a_radius() {
        let mut doc = Document::default();
        let a = doc.add(Box, "", 0.0, 0.0);
        let e = doc.element(a).unwrap();
        let top_left = e.handles()[0];
        assert_eq!(e.handle_at(top_left, 0.5), Some(0));
        assert_eq!(e.handle_at((top_left.0 + 0.1, top_left.1), 0.5), Some(0), "close enough");
        assert_eq!(e.handle_at(e.center(), 0.5), None, "the middle is nobody's handle");
    }

    #[test]
    fn every_hover_arrow_sits_the_same_visual_distance_from_its_own_handle() {
        let mut doc = Document::default();
        let a = doc.add(Box, "", 0.0, 0.0);
        let e = doc.element(a).unwrap().clone();
        let (handles, arrows) = (e.handles(), e.arrows(2.0));
        for i in 0..8 {
            let (hx, hy) = handles[i];
            let (ax, ay) = arrows[i];
            // A cell reads twice as tall as it is wide, so the y difference is doubled
            // before measuring — the same correction `arrows()` itself makes, without
            // which top and bottom would land twice as far out as the sides did.
            let visual = ((ax - hx).powi(2) + ((ay - hy) * crate::shapes::ASPECT).powi(2)).sqrt();
            assert!((visual - 2.0).abs() < 0.001, "handle {i}: pushed {visual} visual units, wanted 2.0");
        }
    }

    #[test]
    fn arrow_region_covers_the_whole_gutter_with_no_gap_between_directions() {
        let mut doc = Document::default();
        let a = doc.add(Box, "", 10.0, 10.0);
        let e = doc.element(a).unwrap().clone();
        let margin = 3.0;
        let vmargin = margin / crate::shapes::ASPECT;
        assert_eq!(e.arrow_region(e.center(), margin), None, "inside the box is nobody's arrow");
        assert_eq!(e.arrow_region((e.x, e.y), margin), None, "the border itself is still the box, not the gutter");
        // Every point on a straight walk outward, in each of the eight directions, lands in
        // that direction's own region the whole way to the margin — the bug this guards
        // against: a circle around each arrow's own drawn point left gaps a vertical or
        // diagonal approach could fall through, going cold before the mouse ever arrived.
        // The margin itself reads as a visual reach, not a cell count: half as many cells
        // up or down as side to side, so a diagonal ray leaves through whichever axis is
        // narrower first.
        let mid = ((e.x + e.right()) / 2.0, (e.y + e.bottom()) / 2.0);
        // (start point, unit direction, direction index)
        type Ray = ((f64, f64), (f64, f64), usize);
        let rays: [Ray; 8] = [
            ((e.x, e.y), (-1.0, -1.0), 0),             // NW
            ((mid.0, e.y), (0.0, -1.0), 1),            // N
            ((e.right(), e.y), (1.0, -1.0), 2),        // NE
            ((e.right(), mid.1), (1.0, 0.0), 3),       // E
            ((e.right(), e.bottom()), (1.0, 1.0), 4),  // SE
            ((mid.0, e.bottom()), (0.0, 1.0), 5),      // S
            ((e.x, e.bottom()), (-1.0, 1.0), 6),       // SW
            ((e.x, mid.1), (-1.0, 0.0), 7),            // W
        ];
        for ((bx, by), (ux, uy), dir) in rays {
            // How far this ray can walk before it leaves the margin on whichever axis it
            // moves in — both, for a diagonal, the narrower one winning.
            let max_s = match (ux != 0.0, uy != 0.0) {
                (true, true) => (margin / ux.abs()).min(vmargin / uy.abs()),
                (true, false) => margin,
                (false, true) => vmargin,
                (false, false) => unreachable!(),
            };
            for frac in [0.2, 0.4, 0.6, 0.8, 0.98] {
                let s = max_s * frac;
                let p = (bx + ux * s, by + uy * s);
                assert_eq!(e.arrow_region(p, margin), Some(dir), "direction {dir}, {frac} of the way to its own margin: {p:?}");
            }
        }
    }

    #[test]
    fn a_relation_hit_test_follows_its_route() {
        let mut doc = Document::default();
        let a = doc.add(Box, "", 0.0, 0.0);
        let b = doc.add(Box, "", 40.0, 0.0);
        let r = doc.connect(RelationKind::Link, a, b).unwrap();
        let rel = doc.relation(r).unwrap().clone();
        let (p1, p2) = doc.end_points(&rel).unwrap();
        let mid = ((p1.0 + p2.0) / 2.0, (p1.1 + p2.1) / 2.0);
        assert_eq!(doc.relation_at(mid, 0.5), Some(r));
        assert_eq!(doc.relation_at((mid.0, mid.1 + 20.0), 0.5), None, "far off the line");
    }

    #[test]
    fn a_relation_leaves_from_its_port_when_it_has_one_and_the_facing_edge_when_not() {
        let mut doc = Document::default();
        let a = doc.add(Box, "", 0.0, 0.0);
        let b = doc.add(Box, "", 40.0, 0.0);
        let r = doc.connect(RelationKind::Link, a, b).unwrap();
        let rel = doc.relation(r).unwrap().clone();
        let (p1, _) = doc.end_points(&rel).unwrap();
        assert_eq!(p1, (20.0, 3.0), "unanchored: the right edge, facing b");
        assert_eq!(doc.port_on(&rel, a), Some(3), "…which is the right handle");
        assert_eq!(doc.at_port(a, 3), vec![r]);
        doc.relation_mut(r).unwrap().from_port = Some(1);
        let rel = doc.relation(r).unwrap().clone();
        assert_eq!(doc.end_points(&rel).unwrap().0, (10.0, 0.0), "anchored: the top handle");
        assert_eq!(doc.at_port(a, 3), Vec::<RelationId>::new());
        assert_eq!(doc.at_port(a, 1), vec![r]);
    }

    #[test]
    fn bend_pulls_an_orthogonal_route_s_one_turn_along_whichever_axis_it_leaves() {
        let mut doc = Document::default();
        let a = doc.add(Box, "", 0.0, 0.0);
        let b = doc.add(Box, "", 40.0, 0.0);
        let r = doc.connect(RelationKind::Link, a, b).unwrap();
        let rel = doc.relation(r).unwrap().clone();
        assert_eq!(doc.bendable(&rel), Some(true), "side to side: the pull is left/right");
        let (p1, p2) = doc.end_points(&rel).unwrap();
        let span = p2.0 - p1.0;
        doc.bend(r, 3.0);
        assert_eq!(doc.relation(r).unwrap().elbow, Some(53), "half way (50) plus 3 percentage points, toward b");
        let pts = doc.route(doc.relation(r).unwrap()).unwrap();
        assert!((pts[1].0 - (p1.0 + span * 0.53)).abs() < 1e-9, "53% of the way along the leg: {pts:?}");
        doc.bend(r, -5.0);
        assert_eq!(doc.relation(r).unwrap().elbow, Some(48), "3 - 5 more percentage points from half way");
        let pts = doc.route(doc.relation(r).unwrap()).unwrap();
        assert!((pts[1].0 - (p1.0 + span * 0.48)).abs() < 1e-9, "48% of the way along the leg: {pts:?}");

        let mut vert = Document::default();
        let c = vert.add(Box, "", 0.0, 0.0);
        let d = vert.add(Box, "", 0.0, 40.0);
        let r2 = vert.connect(RelationKind::Link, c, d).unwrap();
        let rel2 = vert.relation(r2).unwrap().clone();
        assert_eq!(vert.bendable(&rel2), Some(false), "top to bottom: the pull is up/down");
        vert.bend(r2, 4.0);
        let pts2 = vert.route(vert.relation(r2).unwrap()).unwrap();
        assert_eq!(pts2[1].1, pts2[2].1, "still square — only the middle leg's y moved");

        // A straight or curved route has no turn at all to pull.
        let mut straight = Document::default();
        let e = straight.add(Box, "", 0.0, 0.0);
        let f = straight.add(Box, "", 40.0, 0.0);
        let r3 = straight.connect(RelationKind::Link, e, f).unwrap();
        straight.relation_mut(r3).unwrap().style =
            Some(crate::ontology::Notation { route: crate::ontology::Route::Straight, ..straight.relation(r3).unwrap().notation() });
        let rel3 = straight.relation(r3).unwrap().clone();
        assert_eq!(straight.bendable(&rel3), None);
        straight.bend(r3, 3.0);
        assert_eq!(straight.relation(r3).unwrap().elbow, None, "nothing to bend, so nothing changed");
    }

    #[test]
    fn an_automatic_orthogonal_route_bends_around_a_third_shape_in_its_way_but_a_hand_set_elbow_is_never_second_guessed() {
        let mut doc = Document::default();
        let a = doc.add(Box, "", 0.0, 0.0);
        let b = doc.add(Box, "", 40.0, 20.0);
        let r = doc.connect(RelationKind::Link, a, b).unwrap();
        let before = doc.route(doc.relation(r).unwrap()).unwrap();

        // An obstacle placed right where the plain, un-bent route already runs.
        let c = doc.add(Box, "", 25.0, 10.0);
        doc.element_mut(c).unwrap().w = 10.0;
        doc.element_mut(c).unwrap().h = 5.0;
        let c_rect = (25.0, 10.0, 10.0, 5.0);
        assert!(crate::shapes::route_crosses(&before, c_rect), "the obstacle really is in the plain route's own way: {before:?}");

        let after = doc.route(doc.relation(r).unwrap()).unwrap();
        assert!(!crate::shapes::route_crosses(&after, c_rect), "the automatic route now bends around it instead: {after:?}");
        assert_eq!((after[0], after[after.len() - 1]), (before[0], before[before.len() - 1]), "the two ends it actually joins never move");

        // Once bent by hand — even back to nothing — an elbow is the person's to keep, so
        // an obstacle is never allowed to second-guess it.
        doc.bend(r, 0.0);
        let held = doc.route(doc.relation(r).unwrap()).unwrap();
        assert!(crate::shapes::route_crosses(&held, c_rect), "a hand-set elbow is drawn where it was put, obstacle or not: {held:?}");
    }

    #[test]
    fn incident_lists_outgoing_before_incoming_in_a_stable_order() {
        let mut doc = Document::default();
        let a = doc.add(BusinessActor, "", 0.0, 0.0);
        let r = doc.add(BusinessRole, "", 20.0, 0.0);
        let l = doc.add(Location, "", 0.0, 20.0);
        let inc = doc.connect(RelationKind::Aggregation, l, a).unwrap();
        let out = doc.connect(RelationKind::Assignment, a, r).unwrap();
        assert_eq!(doc.incident(a), vec![out, inc]);
        assert_eq!(doc.other_end(inc, a), Some(l));
    }
}
