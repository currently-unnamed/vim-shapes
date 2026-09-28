//! The properties of a shape or a relation, as fields — and the one function that writes one.
//!
//! Every property, each in its own unit: a label is text, a position and a size are cells, a
//! kind is a kind. The sheet (`ui::sheet`) reads these; `apply` is the only thing that writes
//! them, so undo, the file, `:lint` and every export see one change made one way. Fields are
//! grouped by the sheet's three tabs — style, text, arrange — here, where they are defined,
//! rather than in the sheet, so a field added is a field the sheet shows.

use super::canvas::Target;
use crate::fonts;
use crate::model::{Align, Document, Element, ElementId, ElementInk, Fill, GridStyle, Node, Page, Paper, Raise, Relation, Status, VAlign, Visibility};
use crate::ontology::{Colour, End, EndSize, LineStyle, Paint, RelationKind, Route, ShapeKind, View, LOOKS};

/// What sort of value a field holds — which decides how it is typed, cycled and checked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unit {
    Text,
    /// A whole number of cells.
    Cells,
    ShapeKind,
    RelationKind,
    /// Solid, dashed or dotted.
    Line,
    /// What sits at an end of a link.
    End,
    /// A font family this machine has.
    Font,
    /// Pixels per em, in the rendering.
    Px,
    Align,
    VAlign,
    /// A colour from the palette, or blank for the default.
    Colour,
    /// A line's thickness, in braille dots: 1, 2 or 3.
    Width,
    /// A handle of the element a relation attaches to, or `auto`.
    Port,
    /// Not a value: something done when entered — reverse a link, bring a shape to the front.
    Action,
    /// A layer, by name.
    Layer,
    /// Yes or no; space flips it.
    YesNo,
    /// What kind of diagram: freeform, or one of the architecture views.
    View,
    /// A paper size by name, or `WxH` in cells.
    Paper,
    /// Portrait or landscape.
    Orientation,
    /// One of the eight looks: a fill and a line colour that go together.
    Look,
    /// `auto` (the layer's own), `none`, or a colour.
    Fill,
    /// `auto` (the layer's own) or `none`. An architecture shape's fill is a switch, not a
    /// palette: the colour is what tells you which layer it's in, so only a plain sketch
    /// shape — with no layer to say — may pick a colour of its own.
    LayerFill,
    /// Ten to a hundred per cent.
    Percent,
    /// Straight, orthogonal or curved.
    Route,
    /// Small, normal or large.
    EndSize,
    /// Dots or lines.
    GridStyle,
    /// Auto (the document's own `:ink`), lines or braille — this shape only.
    Ink,
    /// Active, experimental or deprecated.
    Status,
    /// Prominent, normal or hidden.
    Visibility,
}

impl Unit {
    pub fn name(self) -> &'static str {
        match self {
            Unit::Text => "text",
            Unit::Cells => "cells",
            Unit::ShapeKind => "kind of shape",
            Unit::RelationKind => "kind of relation",
            Unit::Line => "line",
            Unit::End => "end",
            Unit::Font => "font on this machine",
            Unit::Px => "px in the rendering",
            Unit::Align => "left / centre / right",
            Unit::VAlign => "top / middle / bottom",
            Unit::Colour => "a name, a hex — enter picks",
            Unit::Width => "dots: 1, 2 or 3",
            Unit::Port => "auto, or a handle: tl top tr right br bottom bl left",
            Unit::Action => "enter runs it",
            Unit::Layer => "a layer, by name (:layers)",
            Unit::YesNo => "yes / no — space flips it",
            Unit::View => "kind of diagram",
            Unit::Paper => "a paper size, or WxH in cells",
            Unit::Orientation => "portrait / landscape",
            Unit::Look => "a fill and line colour that go together",
            Unit::Fill => "auto (the layer's), none, a colour — enter picks",
            Unit::LayerFill => "auto (the layer's) or none — the colour marks the layer",
            Unit::Percent => "per cent, 10 to 100",
            Unit::Route => "straight / orthogonal / curved",
            Unit::EndSize => "small / normal / large",
            Unit::GridStyle => "auto (dots here, ruled on paper) / dots / lines",
            Unit::Ink => "auto (the document's), lines or braille — this shape only",
            Unit::Status => "active / experimental / deprecated",
            Unit::Visibility => "prominent / normal / hidden",
        }
    }

    /// What `h`/`l` cycle through, for a choice.
    pub fn choices(self) -> Option<Vec<String>> {
        Some(match self {
            Unit::ShapeKind => ShapeKind::ALL.iter().map(|k| k.slug().to_string()).collect(),
            Unit::RelationKind => RelationKind::ALL.iter().map(|k| k.name().to_string()).collect(),
            Unit::Line => LineStyle::ALL.iter().map(|l| l.name().to_string()).collect(),
            Unit::End => End::ALL.iter().map(|e| e.name().to_string()).collect(),
            // The rendering's own font first, so cycling back lands on "no preference".
            Unit::Font => std::iter::once(String::new()).chain(fonts::families().iter().map(|f| f.name.clone())).collect(),
            Unit::Align => Align::ALL.iter().map(|a| a.name().to_string()).collect(),
            Unit::VAlign => VAlign::ALL.iter().map(|a| a.name().to_string()).collect(),
            Unit::Colour => std::iter::once(String::new()).chain(Paint::ALL.iter().map(|p| p.name().to_string())).collect(),
            Unit::Width => ["1", "2", "3"].iter().map(|s| s.to_string()).collect(),
            Unit::Port => PORTS.iter().map(|s| s.to_string()).collect(),
            Unit::YesNo => ["no", "yes"].iter().map(|s| s.to_string()).collect(),
            Unit::View => View::ALL.iter().map(|v| v.name().to_string()).collect(),
            Unit::Paper => Paper::ALL.iter().map(|p| p.name()).collect(),
            Unit::Orientation => ["portrait", "landscape"].iter().map(|s| s.to_string()).collect(),
            Unit::Look => LOOKS.iter().map(|l| l.0.to_string()).collect(),
            Unit::Fill => ["auto", "none"].iter().map(|s| s.to_string()).chain(Paint::ALL.iter().map(|p| p.name().to_string())).collect(),
            Unit::LayerFill => ["auto", "none"].iter().map(|s| s.to_string()).collect(),
            Unit::Percent => (1..=10).map(|n| (n * 10).to_string()).collect(),
            Unit::Route => Route::ALL.iter().map(|r| r.name().to_string()).collect(),
            Unit::EndSize => EndSize::ALL.iter().map(|e| e.name().to_string()).collect(),
            Unit::GridStyle => GridStyle::ALL.iter().map(|g| g.name().to_string()).collect(),
            Unit::Ink => ElementInk::ALL.iter().map(|i| i.name().to_string()).collect(),
            Unit::Status => Status::ALL.iter().map(|v| v.name().to_string()).collect(),
            Unit::Visibility => Visibility::ALL.iter().map(|v| v.name().to_string()).collect(),
            // Layers are the document's, so the sheet asks for them with `layer_choices`.
            _ => return None,
        })
    }

    /// Whether `h`/`l` step this field rather than cycle it.
    pub fn is_number(self) -> bool {
        matches!(self, Unit::Cells | Unit::Px)
    }

    /// How many steps `H`/`L` take at once on a number: a page width of 2000 is two hundred
    /// presses at one, and twenty at ten, and ten is still small enough to land by eye.
    pub const BIG_STEP: usize = 10;

    /// What `h`/`l` step a number to: one more or one less, as text for [`apply`].
    ///
    /// Only the step lives here; the bounds stay in `apply`, so stepping past one says the
    /// same thing typing past it would ("width cannot be less than 4") rather than a second
    /// copy of each limit that could drift from the first. A blank number means "auto", and
    /// steps from where auto is: the rendering's 20 px for a size, and nothing for cells —
    /// `l` starts it at one, and `h` has nowhere below auto to go.
    pub fn step(self, value: &str, delta: isize) -> Option<String> {
        if !self.is_number() {
            return None;
        }
        let blank = value.trim().is_empty();
        let n: i64 = match self {
            Unit::Px if blank => 20,
            Unit::Cells if blank && delta < 0 => return None,
            Unit::Cells if blank => 0,
            _ => value.trim().parse().ok()?,
        };
        Some((n + delta as i64).to_string())
    }
}

/// The layer names, back to front — what the `layer` field cycles.
pub fn layer_choices(doc: &Document) -> Vec<String> {
    doc.layers.iter().map(|l| l.name.clone()).collect()
}

fn layer_name(doc: &Document, id: u32) -> String {
    doc.layer(id).map(|l| l.name.clone()).unwrap_or_else(|| format!("layer {id}"))
}

fn yes_no(b: bool) -> String {
    if b { "yes".into() } else { "no".into() }
}

fn parse_yes_no(s: &str) -> Result<bool, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "yes" | "y" | "true" | "on" => Ok(true),
        "no" | "n" | "false" | "off" | "" => Ok(false),
        other => Err(format!("yes or no, not {other:?}")),
    }
}

fn parse_raise(name: &str) -> Option<Raise> {
    match name {
        "to front" => Some(Raise::Front),
        "to back" => Some(Raise::Back),
        "bring forward" => Some(Raise::Forward),
        "send backward" => Some(Raise::Backward),
        _ => None,
    }
}

/// The arrange tab's common tail: the drawing order, the layer, the lock.
fn arrange_tail(doc: &Document, layer: u32, locked: bool) -> Vec<Field> {
    let f = |name, unit, value: String| Field { name, unit, value, tab: Tab::Arrange, mixed: false };
    vec![
        f("to front", Unit::Action, "above everything on its layer".into()),
        f("to back", Unit::Action, "beneath everything on its layer".into()),
        f("bring forward", Unit::Action, "one step up".into()),
        f("send backward", Unit::Action, "one step down".into()),
        f("layer", Unit::Layer, layer_name(doc, layer)),
        f("locked", Unit::YesNo, yes_no(locked)),
    ]
}

/// The eight handles by name, `auto` first — the order the sheet cycles them in.
pub const PORTS: [&str; 9] = ["auto", "tl", "top", "tr", "right", "br", "bottom", "bl", "left"];

fn port_name(p: Option<u8>) -> String {
    match p {
        None => "auto".into(),
        Some(i) => PORTS[(i as usize + 1).min(8)].into(),
    }
}

fn parse_port(s: &str) -> Result<Option<u8>, String> {
    let want = s.trim().to_ascii_lowercase();
    if want.is_empty() || want == "auto" {
        return Ok(None);
    }
    PORTS
        .iter()
        .position(|p| *p == want)
        .filter(|i| *i > 0)
        .map(|i| Some((i - 1) as u8))
        .ok_or_else(|| format!("a port is auto, or one of tl top tr right br bottom bl left, not {want:?}"))
}

/// The sheet's three tabs. Every field belongs to one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Style,
    Text,
    Arrange,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Style, Tab::Text, Tab::Arrange];

    pub fn name(self) -> &'static str {
        match self {
            Tab::Style => "style",
            Tab::Text => "text",
            Tab::Arrange => "arrange",
        }
    }
}

pub struct Field {
    pub name: &'static str,
    pub unit: Unit,
    pub value: String,
    pub tab: Tab,
    /// On a picked set: the shapes disagree, so there is no one value to show. Setting
    /// it sets them all.
    pub mixed: bool,
}

/// Every field of what the cursor is on, in sheet order. `None` when it is not there.
pub fn fields(doc: &Document, target: Target) -> Option<Vec<Field>> {
    fields_for(doc, target, &[])
}

/// The same, with the picked set for `Target::Picked`.
pub fn fields_for(doc: &Document, target: Target, picked: &[ElementId]) -> Option<Vec<Field>> {
    if target == Target::Picked {
        return set_fields(doc, picked);
    }
    Some(match target {
        Target::Picked => unreachable!("handled above"),
        Target::Element(id) => {
            let e = doc.element(id)?;
            let mut v = element_fields(e);
            v.push(Field { name: "snap to grid", unit: Unit::Action, value: "to the nearest grid dot".into(), tab: Tab::Arrange, mixed: false });
            v.extend(arrange_tail(doc, e.layer, e.locked));
            v
        }
        Target::Relation(id, _) => {
            let r = doc.relation(id)?;
            let mut v = relation_fields(r);
            v.extend(arrange_tail(doc, r.layer, r.locked));
            v
        }
        Target::Diagram => diagram_fields(doc),
    })
}

/// What a set of shapes shares: every field they all have, by name and unit, in the first
/// shape's order — with one value where they agree and `mixed` where they do not. Every
/// shape has the same fields, so today that is all of them; it is the intersection so it
/// stays true the day it is not.
fn set_fields(doc: &Document, picked: &[ElementId]) -> Option<Vec<Field>> {
    let mut each: Vec<Vec<Field>> = picked.iter().filter_map(|&id| fields(doc, Target::Element(id))).collect();
    if each.is_empty() {
        return None;
    }
    let rest = each.split_off(1);
    let first = each.remove(0);
    let mut out: Vec<Field> = first
        .into_iter()
        .filter(|f| rest.iter().all(|fs| fs.iter().any(|g| g.name == f.name && g.unit == f.unit)))
        .map(|mut f| {
            let agree = rest.iter().all(|fs| fs.iter().any(|g| g.name == f.name && g.value == f.value));
            if !agree && f.unit != Unit::Action {
                f.value = String::new();
                f.mixed = true;
            }
            f
        })
        .collect();
    // What only a set can do: line up, space out, size alike — to the first picked shape,
    // which is the one the cursor was on when picking began.
    let a = |name, value: &str| Field { name, unit: Unit::Action, value: value.into(), tab: Tab::Arrange, mixed: false };
    for (name, what) in SET_ACTIONS {
        out.push(a(name, what));
    }
    Some(out)
}

/// The arrangements of a picked set, and what each does — to the first picked shape.
pub const SET_ACTIONS: [(&str, &str); 10] = [
    ("align left", "left edges to the first picked"),
    ("align centre", "centres to the first picked, across"),
    ("align right", "right edges to the first picked"),
    ("align top", "top edges to the first picked"),
    ("align middle", "middles to the first picked, down"),
    ("align bottom", "bottom edges to the first picked"),
    ("distribute across", "equal gaps between them, left to right"),
    ("distribute down", "equal gaps between them, top to bottom"),
    ("match width", "as wide as the first picked"),
    ("match height", "as tall as the first picked"),
];

/// One of the set's own actions, on the unlocked shapes of the set.
fn arrange_set(doc: &mut Document, name: &str, ids: &[ElementId]) -> Result<(), String> {
    let first = doc.element(ids[0]).ok_or("no such shape")?.clone();
    let mut es: Vec<Element> = ids.iter().filter_map(|&id| doc.element(id).cloned()).collect();
    match name {
        "align left" => es.iter_mut().for_each(|e| e.x = first.x),
        "align centre" => es.iter_mut().for_each(|e| e.x = (first.x + first.w / 2.0 - e.w / 2.0).round()),
        "align right" => es.iter_mut().for_each(|e| e.x = first.right() - e.w),
        "align top" => es.iter_mut().for_each(|e| e.y = first.y),
        "align middle" => es.iter_mut().for_each(|e| e.y = (first.y + first.h / 2.0 - e.h / 2.0).round()),
        "align bottom" => es.iter_mut().for_each(|e| e.y = first.bottom() - e.h),
        "match width" => es.iter_mut().for_each(|e| e.w = first.w),
        "match height" => es.iter_mut().for_each(|e| e.h = first.h),
        // Spread out: the first and last stay, the rest share the room between them so
        // every gap is the same.
        "distribute across" | "distribute down" => {
            if es.len() < 3 {
                return Err("distributing takes three or more — the ends stay, the rest space out".into());
            }
            let across = name == "distribute across";
            es.sort_by(|a, b| if across { a.x.total_cmp(&b.x) } else { a.y.total_cmp(&b.y) });
            let span = if across { es[es.len() - 1].right() - es[0].x } else { es[es.len() - 1].bottom() - es[0].y };
            let sizes: f64 = es.iter().map(|e| if across { e.w } else { e.h }).sum();
            let gap = ((span - sizes) / (es.len() - 1) as f64).max(0.0);
            let mut at = if across { es[0].x } else { es[0].y };
            for e in es.iter_mut() {
                if across {
                    e.x = at.round();
                    at += e.w + gap;
                } else {
                    e.y = at.round();
                    at += e.h + gap;
                }
            }
        }
        other => return Err(format!("no field {other:?}")),
    }
    for e in es {
        if let Some(slot) = doc.element_mut(e.id) {
            *slot = e;
        }
    }
    Ok(())
}

/// Write one field on every shape of a picked set — or on the one target, as `apply`.
/// Checked on every shape before anything is written, so a value one of them refuses
/// changes none of them; a locked shape is passed over, and only a set that is locked
/// throughout refuses.
pub fn apply_for(doc: &mut Document, target: Target, name: &str, value: &str, picked: &[ElementId]) -> Result<(), String> {
    if target != Target::Picked {
        return apply(doc, target, name, value);
    }
    let open: Vec<ElementId> = picked.iter().copied().filter(|&id| !doc.element_locked(id)).collect();
    if open.is_empty() {
        return Err(if picked.is_empty() { "nothing picked — space".into() } else { LOCKED.into() });
    }
    if SET_ACTIONS.iter().any(|(n, _)| *n == name) {
        return arrange_set(doc, name, &open);
    }
    let mut trial = doc.clone();
    for &id in &open {
        apply(&mut trial, Target::Element(id), name, value)?;
    }
    *doc = trial;
    Ok(())
}

/// The diagram's own fields. Style is the three looks and the two grounds; text is what
/// the diagram is; arrange is the grid and the page.
fn diagram_fields(doc: &Document) -> Vec<Field> {
    let f = |tab, name, unit, value: String| Field { name, unit, value, tab, mixed: false };
    use Tab::*;
    let p: &Page = &doc.metadata.page;
    let colour = |c: Option<Colour>| c.map(Colour::name).unwrap_or_default();
    let (pw, ph) = p.size();
    vec![
        f(Style, "rounded", Unit::YesNo, yes_no(p.rounded)),
        f(Style, "sketch", Unit::YesNo, yes_no(p.sketch)),
        f(Style, "shadow", Unit::YesNo, yes_no(p.shadow)),
        f(Style, "background", Unit::Colour, colour(p.background)),
        f(Style, "grid colour", Unit::Colour, colour(p.grid_color)),
        f(Text, "title", Unit::Text, doc.metadata.title.clone().unwrap_or_default()),
        f(Text, "view", Unit::View, doc.metadata.view.name().to_string()),
        f(Arrange, "grid", Unit::YesNo, yes_no(p.grid)),
        f(Arrange, "grid style", Unit::GridStyle, p.grid_style.name().to_string()),
        f(Arrange, "grid size", Unit::Cells, p.grid_step().0.to_string()),
        f(Arrange, "page view", Unit::YesNo, yes_no(p.page_view)),
        f(Arrange, "paper", Unit::Paper, p.paper.name()),
        f(Arrange, "orientation", Unit::Orientation, if p.landscape { "landscape".into() } else { "portrait".into() }),
        f(Arrange, "page width", Unit::Cells, format!("{pw}")),
        f(Arrange, "page height", Unit::Cells, format!("{ph}")),
    ]
}

fn apply_diagram(doc: &mut Document, name: &str, value: &str) -> Result<(), String> {
    let p = &mut doc.metadata.page;
    let whole = |what: &str, lo: u32, hi: u32| -> Result<u32, String> {
        let n: u32 = value.parse().map_err(|_| format!("{what} is a whole number of cells, not {value:?}"))?;
        if !(lo..=hi).contains(&n) {
            return Err(format!("{what} runs from {lo} to {hi} cells"));
        }
        Ok(n)
    };
    match name {
        "rounded" => p.rounded = parse_yes_no(value)?,
        "sketch" => p.sketch = parse_yes_no(value)?,
        "shadow" => p.shadow = parse_yes_no(value)?,
        "background" => p.background = colour_of(value)?,
        "grid colour" | "grid color" => p.grid_color = colour_of(value)?,
        "grid" => p.grid = parse_yes_no(value)?,
        "grid size" => p.grid_size = whole("grid size", 2, 40)?,
        "grid style" => p.grid_style = GridStyle::parse(value).ok_or_else(|| format!("grid style is auto, dots or lines, not {value:?}"))?,
        "page view" => p.page_view = parse_yes_no(value)?,
        "paper" => p.paper = Paper::parse(value).ok_or_else(|| format!("no paper called {value:?} — {}, or WxH in cells", Paper::ALL.iter().map(|p| p.name()).collect::<Vec<_>>().join(", ")))?,
        "orientation" => {
            p.landscape = match value.to_ascii_lowercase().as_str() {
                "landscape" | "l" | "wide" => true,
                "portrait" | "p" | "tall" => false,
                _ => return Err(format!("orientation is portrait or landscape, not {value:?}")),
            }
        }
        // A width or height of its own makes the page a custom one, the other side kept.
        "page width" | "page height" => {
            let (w, h) = p.size();
            let (mut w, mut h) = (w as u32, h as u32);
            if name == "page width" { w = whole("page width", 8, 2000)? } else { h = whole("page height", 4, 1000)? }
            let (w, h) = if p.landscape { (h, w) } else { (w, h) };
            p.paper = Paper::Custom(w, h);
        }
        "title" => doc.metadata.title = if value.is_empty() { None } else { Some(value.to_string()) },
        "view" => doc.metadata.view = View::parse(value).ok_or_else(|| format!("no view called {value:?} — {}", View::ALL.iter().map(|v| v.name()).collect::<Vec<_>>().join(", ")))?,
        _ => return Err(format!("the diagram has no field called {name:?}")),
    }
    Ok(())
}

/// Whether a target may be changed at all — the lock, on it or on its layer.
pub fn locked(doc: &Document, target: Target) -> bool {
    match target {
        Target::Element(id) => doc.element_locked(id),
        Target::Relation(id, _) => doc.relation_locked(id),
        Target::Diagram => false,
        Target::Picked => false,
    }
}

pub const LOCKED: &str = "locked — unlock it on the arrange tab, or its layer in :layers";

/// The field a relation node's label lives in.
pub fn node_field(node: Node) -> &'static str {
    match node {
        Node::Tail => "tail label",
        Node::Centre => "label",
        Node::Head => "head label",
    }
}

fn element_fields(e: &Element) -> Vec<Field> {
    let f = |tab, name, unit, value: String| Field { name, unit, value, tab, mixed: false };
    use Tab::*;
    // An ontology type carries Foundry's own metadata too; the rows have a browser (P).
    let mut ontology = Vec::new();
    if e.kind.layer() == crate::ontology::Layer::Ontology {
        ontology.push(f(Text, "api name", Unit::Text, e.api_name.clone().unwrap_or_default()));
        ontology.push(f(Text, "plural", Unit::Text, e.plural.clone().unwrap_or_default()));
        ontology.push(f(Style, "status", Unit::Status, e.status.name().to_string()));
        ontology.push(f(Style, "visibility", Unit::Visibility, e.visibility.name().to_string()));
        if e.takes_rows() {
            ontology.push(f(Arrange, e.rows_word(), Unit::Action, format!("{} — P opens the browser", e.properties.len())));
        }
    }
    let mut v = vec![
        f(Style, "look", Unit::Look, e.look().to_string()),
        f(Style, "fill", if e.kind.is_sketch() { Unit::Fill } else { Unit::LayerFill }, e.fill.name()),
        f(Style, "outline", Unit::YesNo, yes_no(e.outline)),
        f(Style, "colour", Unit::Colour, e.color.map(|p| p.name().to_string()).unwrap_or_default()),
        f(Style, "line", Unit::Line, e.line.name().to_string()),
        f(Style, "stroke", Unit::Width, e.stroke.to_string()),
        f(Style, "opacity", Unit::Percent, e.opacity.to_string()),
        f(Style, "ink", Unit::Ink, e.ink.name().to_string()),
        f(Text, "label", Unit::Text, e.label.clone()),
        f(Text, "font", Unit::Font, e.text.font.clone().unwrap_or_default()),
        f(Text, "size", Unit::Px, e.text.size.map(|s| s.to_string()).unwrap_or_default()),
        f(Text, "bold", Unit::YesNo, yes_no(e.text.bold)),
        f(Text, "italic", Unit::YesNo, yes_no(e.text.italic)),
        f(Text, "underline", Unit::YesNo, yes_no(e.text.underline)),
        f(Text, "text colour", Unit::Colour, e.text.color.map(Colour::name).unwrap_or_default()),
        f(Text, "label band", Unit::YesNo, yes_no(e.text.band)),
        f(Text, "wrap", Unit::YesNo, yes_no(e.text.wrap)),
        f(Text, "label width", Unit::Cells, e.text.width.map(|w| w.to_string()).unwrap_or_default()),
        f(Text, "padding", Unit::Cells, e.text.padding.to_string()),
        f(Text, "align", Unit::Align, e.text.align.name().to_string()),
        f(Text, "valign", Unit::VAlign, e.text.valign.name().to_string()),
        f(Arrange, "kind", Unit::ShapeKind, e.kind.slug().to_string()),
        f(Arrange, "x", Unit::Cells, format!("{}", e.x.round() as i64)),
        f(Arrange, "y", Unit::Cells, format!("{}", e.y.round() as i64)),
        f(Arrange, "width", Unit::Cells, format!("{}", e.w.round() as i64)),
        f(Arrange, "height", Unit::Cells, format!("{}", e.h.round() as i64)),
        f(Arrange, "skew x", Unit::Cells, format!("{}", e.skew.round() as i64)),
        f(Arrange, "skew y", Unit::Cells, format!("{}", e.skew_y.round() as i64)),
    ];
    v.append(&mut ontology);
    v
}

fn relation_fields(r: &Relation) -> Vec<Field> {
    let f = |tab, name, unit, value: String| Field { name, unit, value, tab, mixed: false };
    use Tab::*;
    // Every relation's look can be set: a kind gives it a default, and any setting made here
    // becomes the relation's own. The rules still judge it by its kind.
    let n = r.notation();
    vec![
        f(Style, "kind", Unit::RelationKind, r.kind.name().to_string()),
        f(Style, "look", Unit::Look, LOOKS.iter().find(|l| n.color == Some(Colour::Hex(l.2))).map_or("custom", |l| l.0).to_string()),
        f(Style, "route", Unit::Route, n.route.name().to_string()),
        f(Style, "line", Unit::Line, n.line.name().to_string()),
        f(Style, "width", Unit::Width, n.width.to_string()),
        f(Style, "tail", Unit::End, n.tail.name().to_string()),
        f(Style, "head", Unit::End, n.head.name().to_string()),
        f(Style, "end size", Unit::EndSize, n.end_size.name().to_string()),
        f(Style, "colour", Unit::Colour, n.color.map(|p| p.name().to_string()).unwrap_or_default()),
        f(Style, "opacity", Unit::Percent, n.opacity.to_string()),
        f(Text, "tail label", Unit::Text, r.tail_label.clone().unwrap_or_default()),
        f(Text, "label", Unit::Text, r.label.clone().unwrap_or_default()),
        f(Text, "head label", Unit::Text, r.head_label.clone().unwrap_or_default()),
        f(Text, "font", Unit::Font, r.text.font.clone().unwrap_or_default()),
        f(Text, "size", Unit::Px, r.text.size.map(|s| s.to_string()).unwrap_or_default()),
        f(Text, "bold", Unit::YesNo, yes_no(r.text.bold)),
        f(Text, "italic", Unit::YesNo, yes_no(r.text.italic)),
        f(Text, "text colour", Unit::Colour, r.text.color.map(Colour::name).unwrap_or_default()),
        f(Text, "label band", Unit::YesNo, yes_no(r.text.band)),
        f(Text, "label at", Unit::Percent, r.label_at.map_or("50".into(), |p| p.to_string())),
        f(Arrange, "from port", Unit::Port, port_name(r.from_port)),
        f(Arrange, "to port", Unit::Port, port_name(r.to_port)),
        f(Arrange, "elbow", Unit::Cells, r.elbow.map(|e| e.to_string()).unwrap_or_default()),
        f(Arrange, "reverse", Unit::Action, "swap the two ends".into()),
    ]
}

/// A colour field's value: blank for none, a palette name, or a hex.
pub fn colour_of(value: &str) -> Result<Option<Colour>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    Colour::parse(value)
        .map(Some)
        .ok_or_else(|| format!("no colour called {value:?} — a hex like #4a90d9, or {}", Paint::ALL.iter().map(|p| p.name()).collect::<Vec<_>>().join(", ")))
}

/// Write one field's value into the diagram, parsed and checked by its unit. The caller has
/// taken a checkpoint. Returns what went wrong, in words, when nothing was written.
pub fn apply(doc: &mut Document, target: Target, name: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    // The lock: nothing changes on a locked thing but the lock itself — and its layer, since
    // a layer's lock is undone in the browser and a thing has to be able to leave one.
    if locked(doc, target) && !matches!(name, "locked" | "layer") {
        return Err(LOCKED.into());
    }
    if target == Target::Diagram {
        return apply_diagram(doc, name, value);
    }
    if target == Target::Picked {
        return Err("a picked set is written with apply_for".into());
    }
    // The arrange tail, the same for both.
    if let Some(how) = parse_raise(name) {
        match target {
            Target::Element(id) => doc.raise_element(id, how),
            Target::Relation(id, _) => doc.raise_relation(id, how),
            Target::Diagram | Target::Picked => {}
        }
        return Ok(());
    }
    if name == "layer" {
        let want = value.to_ascii_lowercase();
        let id = doc
            .layers
            .iter()
            .find(|l| l.name.to_ascii_lowercase() == want)
            .map(|l| l.id)
            .ok_or_else(|| format!("no layer called {value:?} — {}", layer_choices(doc).join(", ")))?;
        match target {
            Target::Element(eid) => doc.element_mut(eid).ok_or("no such shape")?.layer = id,
            Target::Relation(rid, _) => doc.relation_mut(rid).ok_or("no such relation")?.layer = id,
            Target::Diagram | Target::Picked => {}
        }
        return Ok(());
    }
    if name == "locked" {
        let b = parse_yes_no(value)?;
        match target {
            Target::Element(eid) => doc.element_mut(eid).ok_or("no such shape")?.locked = b,
            Target::Relation(rid, _) => doc.relation_mut(rid).ok_or("no such relation")?.locked = b,
            Target::Diagram | Target::Picked => {}
        }
        return Ok(());
    }
    let cells = |what: &str, min: i64| -> Result<f64, String> {
        let n: i64 = value.parse().map_err(|_| format!("{what} is a whole number of cells, not {value:?}"))?;
        if n < min {
            return Err(format!("{what} cannot be less than {min}"));
        }
        Ok(n as f64)
    };
    match target {
        Target::Element(id) => {
            let (gx, gy) = doc.metadata.page.grid_step();
            let e = doc.element_mut(id).ok_or("no such shape")?;
            match name {
                "label" => e.label = value.to_string(),
                "api name" => e.api_name = if value.is_empty() { None } else { Some(value.to_string()) },
                "plural" => e.plural = if value.is_empty() { None } else { Some(value.to_string()) },
                "status" => e.status = Status::parse(value).ok_or_else(|| format!("status is active, experimental or deprecated, not {value:?}"))?,
                "visibility" => e.visibility = Visibility::parse(value).ok_or_else(|| format!("visibility is prominent, normal or hidden, not {value:?}"))?,
                "properties" | "parameters" => return Err("the rows have a browser of their own — P on the shape, or :props".into()),
                "kind" => {
                    e.kind = ShapeKind::parse(value)
                        .ok_or_else(|| format!("no kind of shape called {value:?} — :help layers lists them"))?
                }
                "x" => e.x = cells("x", i64::MIN)?,
                "y" => e.y = cells("y", i64::MIN)?,
                "width" => e.w = cells("width", 4)?,
                "height" => e.h = cells("height", 2)?,
                "skew x" | "skew" => e.skew = cells("skew x", i64::MIN)?.clamp(-e.w, e.w),
                "skew y" => e.skew_y = cells("skew y", i64::MIN)?.clamp(-e.h, e.h),
                // Empty means the rendering's own font; anything else has to be a family this
                // machine has, and a prefix of one is enough.
                "font" => e.text.font = if value.is_empty() { None } else { Some(fonts::resolve(value)?.name.clone()) },
                "size" => {
                    e.text.size = if value.is_empty() || value == "0" {
                        None
                    } else {
                        let n: u32 = value.parse().map_err(|_| format!("size is whole pixels per em, not {value:?} — blank for the rendering's own"))?;
                        if !(6..=200).contains(&n) {
                            return Err("size runs from 6 to 200 px".into());
                        }
                        Some(n)
                    }
                }
                "bold" => e.text.bold = parse_yes_no(value)?,
                "italic" => e.text.italic = parse_yes_no(value)?,
                "underline" => e.text.underline = parse_yes_no(value)?,
                "text colour" | "text color" => e.text.color = colour_of(value)?,
                "label band" => e.text.band = parse_yes_no(value)?,
                "wrap" => e.text.wrap = parse_yes_no(value)?,
                "label width" => e.text.width = if value.is_empty() { None } else { Some(cells("label width", 1)? as u32) },
                "padding" => e.text.padding = cells("padding", 0)? as u32,
                "align" => e.text.align = Align::parse(value).ok_or_else(|| format!("align is left, centre or right, not {value:?}"))?,
                "valign" => e.text.valign = VAlign::parse(value).ok_or_else(|| format!("valign is top, middle or bottom, not {value:?}"))?,
                "colour" | "color" => e.color = colour_of(value)?,
                "snap to grid" => {
                    let (gx, gy) = (gx as f64, gy as f64);
                    e.x = (e.x / gx).round() * gx;
                    e.y = (e.y / gy).round() * gy;
                }
                "stroke" | "line width" => {
                    e.stroke = match value {
                        "1" | "2" | "3" => value.parse().unwrap_or(1),
                        _ => return Err(format!("stroke is 1, 2 or 3 dots, not {value:?}")),
                    }
                }
                "look" => {
                    if !e.kind.is_sketch() {
                        return Err("an architecture shape has no look of its own — fill is auto or none, and the layer sets the rest; a plain sketch shape can pick one of the eight looks".into());
                    }
                    e.set_look(value)?
                }
                "fill" => {
                    let f = Fill::parse(value).ok_or_else(|| format!("fill is auto, none, or a colour — a name or a hex — not {value:?}"))?;
                    if matches!(f, Fill::Colour(_)) && !e.kind.is_sketch() {
                        return Err("an architecture shape's fill is auto or none — the colour is what says which layer it's in; a plain sketch shape can take any colour".into());
                    }
                    e.fill = f;
                }
                "outline" => e.outline = parse_yes_no(value)?,
                "line" => e.line = LineStyle::parse(value).ok_or_else(|| format!("line is solid, dashed or dotted, not {value:?}"))?,
                "opacity" => {
                    let n: u8 = value.trim_end_matches('%').trim().parse().map_err(|_| format!("opacity is a percentage, not {value:?}"))?;
                    if !(10..=100).contains(&n) {
                        return Err("opacity runs from 10 to 100 %".into());
                    }
                    e.opacity = n;
                }
                "ink" => e.ink = ElementInk::parse(value).ok_or_else(|| format!("ink is auto, lines or braille, not {value:?}"))?,
                other => return Err(format!("no field {other:?}")),
            }
        }
        Target::Relation(id, _) => {
            let r = doc.relation_mut(id).ok_or("no such relation")?;
            // A look of its own starts from what the link draws now, so changing one end
            // leaves the other two settings as they were.
            let mut look = r.notation();
            match name {
                "kind" => {
                    let kind = RelationKind::parse(value)
                        .ok_or_else(|| format!("no kind of relation called {value:?} — :help relation-kinds"))?;
                    r.kind = kind;
                    // A kind decides its own look; only a plain link keeps one of its own.
                    if kind != RelationKind::Link {
                        r.style = None;
                    }
                    return Ok(());
                }
                "line" => look.line = LineStyle::parse(value).ok_or_else(|| format!("a line is solid, dashed or dotted, not {value:?}"))?,
                "width" => {
                    look.width = match value {
                        "1" | "2" | "3" => value.parse().unwrap_or(1),
                        _ => return Err(format!("width is 1, 2 or 3 dots, not {value:?}")),
                    }
                }
                "colour" | "color" => look.color = colour_of(value)?,
                "look" => {
                    let want = value.to_ascii_lowercase();
                    let (_, _, line) = LOOKS.iter().find(|l| l.0 == want).ok_or_else(|| format!("no look called {value:?} — {}", LOOKS.iter().map(|l| l.0).collect::<Vec<_>>().join(", ")))?;
                    look.color = Some(Colour::Hex(*line));
                }
                "route" => look.route = Route::parse(value).ok_or_else(|| format!("route is straight, orthogonal or curved, not {value:?}"))?,
                "end size" => look.end_size = EndSize::parse(value).ok_or_else(|| format!("end size is small, normal or large, not {value:?}"))?,
                "opacity" => {
                    let n: u8 = value.trim_end_matches('%').trim().parse().map_err(|_| format!("opacity is a percentage, not {value:?}"))?;
                    if !(10..=100).contains(&n) {
                        return Err("opacity runs from 10 to 100 %".into());
                    }
                    look.opacity = n;
                }
                "elbow" => {
                    r.elbow = if value.is_empty() { None } else { Some(value.parse().map_err(|_| format!("elbow is cells from the tail along the first leg, not {value:?} — blank for half way"))?) };
                    return Ok(());
                }
                "tail" => look.tail = End::parse(value).ok_or_else(|| format!("no end called {value:?} — none, arrow, open-arrow, triangle, diamond, hollow-diamond, dot, crow, bar, circle"))?,
                "head" => look.head = End::parse(value).ok_or_else(|| format!("no end called {value:?} — none, arrow, open-arrow, triangle, diamond, hollow-diamond, dot, crow, bar, circle"))?,
                "from port" => {
                    r.from_port = parse_port(value)?;
                    return Ok(());
                }
                "to port" => {
                    r.to_port = parse_port(value)?;
                    return Ok(());
                }
                // The one-key fix for a relation drawn the wrong way round: the ends swap,
                // ports and end labels with them. The rules then judge the new direction.
                "reverse" => {
                    std::mem::swap(&mut r.from, &mut r.to);
                    std::mem::swap(&mut r.from_port, &mut r.to_port);
                    std::mem::swap(&mut r.tail_label, &mut r.head_label);
                    return Ok(());
                }
                "tail label" => {
                    r.set_label_at(Node::Tail, value);
                    return Ok(());
                }
                "label" => {
                    r.set_label_at(Node::Centre, value);
                    return Ok(());
                }
                "head label" => {
                    r.set_label_at(Node::Head, value);
                    return Ok(());
                }
                "font" => {
                    r.text.font = if value.is_empty() { None } else { Some(fonts::resolve(value)?.name.clone()) };
                    return Ok(());
                }
                "size" => {
                    r.text.size = if value.is_empty() || value == "0" {
                        None
                    } else {
                        let n: u32 = value.parse().map_err(|_| format!("size is whole pixels per em, not {value:?} — blank for the rendering's own"))?;
                        if !(6..=200).contains(&n) {
                            return Err("size runs from 6 to 200 px".into());
                        }
                        Some(n)
                    };
                    return Ok(());
                }
                "bold" => {
                    r.text.bold = parse_yes_no(value)?;
                    return Ok(());
                }
                "italic" => {
                    r.text.italic = parse_yes_no(value)?;
                    return Ok(());
                }
                "text colour" | "text color" => {
                    r.text.color = colour_of(value)?;
                    return Ok(());
                }
                "label band" => {
                    r.text.band = parse_yes_no(value)?;
                    return Ok(());
                }
                "label at" => {
                    let n: u8 = value.trim_end_matches('%').trim().parse().map_err(|_| format!("label at is per cent along the line, not {value:?}"))?;
                    if !(5..=95).contains(&n) {
                        return Err("label at runs from 5 to 95 % — the ends have labels of their own".into());
                    }
                    r.label_at = if n == 50 { None } else { Some(n) };
                    return Ok(());
                }
                other => return Err(format!("no field {other:?}")),
            }
            // The relation's own look is a full notation, so a kind change resetting it is
            // one field going away and not five.
            r.style = Some(look);
        }
        Target::Diagram | Target::Picked => unreachable!("handled above"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_has_its_fields_in_three_tabs_each_in_its_unit() {
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::Node, "db", 3.0, 4.0);
        let fs = fields(&doc, Target::Element(id)).unwrap();
        let by = |t: Tab| fs.iter().filter(|f| f.tab == t).map(|f| f.name).collect::<Vec<_>>();
        assert_eq!(by(Tab::Style), ["look", "fill", "outline", "colour", "line", "stroke", "opacity", "ink"]);
        assert_eq!(by(Tab::Text), ["label", "font", "size", "bold", "italic", "underline", "text colour", "label band", "wrap", "label width", "padding", "align", "valign"]);
        assert_eq!(by(Tab::Arrange), ["kind", "x", "y", "width", "height", "skew x", "skew y", "snap to grid", "to front", "to back", "bring forward", "send backward", "layer", "locked"]);
        assert_eq!(fs.iter().find(|f| f.name == "kind").unwrap().value, "node");
        assert_eq!(fs.iter().find(|f| f.name == "width").unwrap().unit, Unit::Cells);
    }

    #[test]
    fn apply_parses_by_unit_and_refuses_what_does_not_fit() {
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::Node, "db", 3.0, 4.0);
        let t = Target::Element(id);
        assert!(apply(&mut doc, t, "width", "20").is_ok());
        assert_eq!(doc.element(id).unwrap().w, 20.0);
        assert!(apply(&mut doc, t, "width", "twenty").unwrap_err().contains("whole number"));
        assert!(apply(&mut doc, t, "width", "1").unwrap_err().contains("less than 4"));
        assert!(apply(&mut doc, t, "kind", "comp").is_ok());
        assert_eq!(doc.element(id).unwrap().kind, ShapeKind::ApplicationComponent);
        assert!(apply(&mut doc, t, "kind", "nonsense").is_err());
        assert!(apply(&mut doc, t, "x", "-5").is_ok(), "a shape may sit left of the origin");
    }

    #[test]
    fn typography_is_set_in_its_own_units_and_cleared_by_a_blank() {
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::Box, "hi", 0.0, 0.0);
        let t = Target::Element(id);
        assert!(apply(&mut doc, t, "align", "r").is_ok());
        assert_eq!(doc.element(id).unwrap().text.align, Align::Right);
        assert!(apply(&mut doc, t, "valign", "bottom").is_ok());
        assert!(apply(&mut doc, t, "valign", "sideways").unwrap_err().contains("top, middle or bottom"));
        assert!(apply(&mut doc, t, "size", "28").is_ok());
        assert_eq!(doc.element(id).unwrap().text.size, Some(28));
        assert!(apply(&mut doc, t, "size", "3").unwrap_err().contains("6 to 200"));
        assert!(apply(&mut doc, t, "size", "").is_ok());
        assert_eq!(doc.element(id).unwrap().text.size, None, "blank is the rendering's own");
        assert!(apply(&mut doc, t, "font", "no-such-font-zzz").unwrap_err().contains("no font"));
        assert!(apply(&mut doc, t, "font", "").is_ok());
        assert_eq!(doc.element(id).unwrap().text.font, None);
        assert_eq!(Unit::VAlign.choices().unwrap()[0], "top");
    }

    #[test]
    fn a_plain_link_exposes_its_look_and_a_typed_relation_does_not() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Box, "", 0.0, 0.0);
        let b = doc.add(ShapeKind::Box, "", 30.0, 0.0);
        let r = doc.connect(RelationKind::Link, a, b).unwrap();
        let fs = fields(&doc, Target::Relation(r, Node::Head)).unwrap();
        let names: Vec<&str> = fs.iter().map(|f| f.name).collect();
        assert_eq!(&names[..10], ["kind", "look", "route", "line", "width", "tail", "head", "end size", "colour", "opacity"]);
        assert_eq!(&names[10..20], ["tail label", "label", "head label", "font", "size", "bold", "italic", "text colour", "label band", "label at"]);
        assert_eq!(&names[20..24], ["from port", "to port", "elbow", "reverse"]);
        assert_eq!(node_field(Node::Head), "head label");
        assert!(apply(&mut doc, Target::Relation(r, Node::Centre), "head", "crow").is_ok());
        assert!(apply(&mut doc, Target::Relation(r, Node::Centre), "line", "dash").is_ok());
        let n = doc.relation(r).unwrap().notation();
        assert_eq!((n.line, n.tail, n.head), (LineStyle::Dashed, End::None, End::Crow), "each setting kept the others");
        assert!(apply(&mut doc, Target::Relation(r, Node::Centre), "kind", "flow").is_ok());
        assert!(doc.relation(r).unwrap().style.is_none(), "a typed relation's look is its kind's until it is set");
        let fs = fields(&doc, Target::Relation(r, Node::Centre)).unwrap();
        assert_eq!(fs.iter().find(|f| f.name == "line").unwrap().value, "dashed", "and the sheet shows the kind's look");
        assert!(apply(&mut doc, Target::Relation(r, Node::Centre), "width", "3").is_ok());
        assert!(apply(&mut doc, Target::Relation(r, Node::Centre), "colour", "pur").is_ok());
        let n = doc.relation(r).unwrap().notation();
        assert_eq!((n.line, n.width, n.color), (LineStyle::Dashed, 3, Some(Paint::Purple.into())), "set on a typed relation, its kind's line survives");
        assert!(apply(&mut doc, Target::Relation(r, Node::Centre), "width", "9").unwrap_err().contains("1, 2 or 3"));
        assert!(apply(&mut doc, Target::Relation(r, Node::Centre), "colour", "").is_ok());
        assert_eq!(doc.relation(r).unwrap().notation().color, None, "blank is the default grey");
    }

    #[test]
    fn a_shape_takes_a_colour_from_the_palette_by_prefix() {
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::Node, "db", 0.0, 0.0);
        assert!(apply(&mut doc, Target::Element(id), "colour", "gr").unwrap_err().contains("no colour"), "green or grey: ambiguous");
        assert!(apply(&mut doc, Target::Element(id), "colour", "gree").is_ok());
        assert_eq!(doc.element(id).unwrap().color, Some(Paint::Green.into()));
        assert!(apply(&mut doc, Target::Element(id), "color", "gray").is_ok(), "either spelling, either spelling");
        assert_eq!(doc.element(id).unwrap().color, Some(Paint::Grey.into()));
        assert!(apply(&mut doc, Target::Element(id), "stroke", "3").is_ok());
        assert_eq!(doc.element(id).unwrap().stroke, 3);
        assert!(apply(&mut doc, Target::Element(id), "stroke", "4").unwrap_err().contains("1, 2 or 3"));
    }

    #[test]
    fn a_number_steps_by_one_and_auto_steps_from_where_auto_is() {
        assert_eq!(Unit::Cells.step("12", 1).as_deref(), Some("13"));
        assert_eq!(Unit::Cells.step("-3", -1).as_deref(), Some("-4"), "x and skew go negative");
        assert_eq!(Unit::Px.step("", 1).as_deref(), Some("21"), "blank size is the rendering's 20");
        assert_eq!(Unit::Cells.step("", 1).as_deref(), Some("1"));
        assert_eq!(Unit::Cells.step("", -1), None, "nothing below auto");
        // The floor is apply's: a step past it is refused in the words typing would get.
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::Box, "a", 0.0, 0.0);
        doc.element_mut(id).unwrap().w = 4.0;
        let below = Unit::Cells.step("4", -1).unwrap();
        assert!(apply(&mut doc, Target::Element(id), "width", &below).unwrap_err().contains("less than 4"));
    }

    #[test]
    fn a_kind_field_cycles_and_a_number_field_does_not() {
        assert_eq!(Unit::ShapeKind.choices().unwrap()[1], "driver");
        assert!(Unit::Cells.choices().is_none());
        assert_eq!(Unit::ShapeKind.step("driver", 1), None, "a choice cycles; it does not step");
        assert_eq!(Unit::Align.choices().unwrap(), ["left", "centre", "right"]);
        let fonts = Unit::Font.choices().unwrap();
        assert_eq!(fonts[0], "", "the default comes first");
    }

    #[test]
    fn the_arrange_tail_moves_things_between_layers_and_a_lock_refuses_everything_else() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Box, "a", 3.0, 3.0);
        let notes = doc.add_layer("notes");
        let t = Target::Element(a);
        assert!(apply(&mut doc, t, "layer", "notes").is_ok());
        assert_eq!(doc.element(a).unwrap().layer, notes);
        assert!(apply(&mut doc, t, "layer", "nowhere").unwrap_err().contains("no layer called"));
        assert!(apply(&mut doc, t, "snap to grid", "").is_ok());
        assert_eq!((doc.element(a).unwrap().x, doc.element(a).unwrap().y), (4.0, 4.0));
        assert!(apply(&mut doc, t, "locked", "yes").is_ok());
        assert_eq!(apply(&mut doc, t, "width", "20").unwrap_err(), LOCKED);
        assert!(apply(&mut doc, t, "layer", "layer 1").is_ok(), "a locked thing may still change layer");
        assert!(apply(&mut doc, t, "locked", "no").is_ok());
        assert!(apply(&mut doc, t, "width", "20").is_ok());
        doc.layers[0].locked = true;
        assert_eq!(apply(&mut doc, t, "width", "24").unwrap_err(), LOCKED, "a layer's lock covers what is on it");
    }

    #[test]
    fn the_diagram_has_its_looks_its_grounds_its_grid_and_its_page_and_takes_a_hex() {
        let mut doc = Document::default();
        let fs = fields(&doc, Target::Diagram).unwrap();
        let names = |t: Tab| fs.iter().filter(|f| f.tab == t).map(|f| f.name).collect::<Vec<_>>();
        assert_eq!(names(Tab::Style), ["rounded", "sketch", "shadow", "background", "grid colour"]);
        assert_eq!(names(Tab::Text), ["title", "view"]);
        assert_eq!(names(Tab::Arrange), ["grid", "grid style", "grid size", "page view", "paper", "orientation", "page width", "page height"]);
        assert!(!locked(&doc, Target::Diagram));
        apply(&mut doc, Target::Diagram, "rounded", "yes").unwrap();
        apply(&mut doc, Target::Diagram, "background", "#F4F4F4").unwrap();
        apply(&mut doc, Target::Diagram, "grid colour", "aq").unwrap();
        apply(&mut doc, Target::Diagram, "grid size", "8").unwrap();
        apply(&mut doc, Target::Diagram, "grid style", "lin").unwrap();
        assert_eq!(doc.metadata.page.grid_style, GridStyle::Lines);
        assert!(apply(&mut doc, Target::Diagram, "grid style", "cross").unwrap_err().contains("auto, dots or lines"));
        apply(&mut doc, Target::Diagram, "paper", "a4").unwrap();
        apply(&mut doc, Target::Diagram, "orientation", "landscape").unwrap();
        apply(&mut doc, Target::Diagram, "title", "Order flow").unwrap();
        apply(&mut doc, Target::Diagram, "view", "bus").unwrap();
        let p = &doc.metadata.page;
        assert!(p.rounded && !p.sketch && !p.shadow);
        assert_eq!(p.background, Some(Colour::Hex([244, 244, 244])));
        assert_eq!(p.grid_color, Some(Colour::Named(Paint::Aqua)));
        assert_eq!(p.grid_step(), (8, 4));
        assert_eq!((p.paper, p.landscape), (Paper::A4, true));
        assert_eq!((doc.metadata.title.as_deref(), doc.metadata.view), (Some("Order flow"), View::Business));
        let fs = fields(&doc, Target::Diagram).unwrap();
        let value = |n: &str| fs.iter().find(|f| f.name == n).unwrap().value.clone();
        assert_eq!((value("background"), value("grid colour"), value("page width"), value("page height")), ("#f4f4f4".into(), "aqua".into(), "58".into(), "83".into()));
        // A width of its own makes the page custom, the height kept.
        apply(&mut doc, Target::Diagram, "page width", "150").unwrap();
        assert_eq!(doc.metadata.page.paper, Paper::Custom(83, 150), "stored in portrait: 83 wide, 150 tall, shown landscape");
        assert_eq!(doc.metadata.page.size(), (150.0, 83.0));
        assert!(apply(&mut doc, Target::Diagram, "grid size", "1").unwrap_err().contains("2 to 40"));
        assert!(apply(&mut doc, Target::Diagram, "paper", "b9").unwrap_err().contains("no paper"));
        assert!(apply(&mut doc, Target::Diagram, "background", "#12").unwrap_err().contains("no colour"));
        apply(&mut doc, Target::Diagram, "background", "").unwrap();
        assert_eq!(doc.metadata.page.background, None, "blank clears a colour");
        assert!(apply(&mut doc, Target::Diagram, "to front", "").is_err(), "the diagram has no drawing order");
    }

    #[test]
    fn a_shape_has_a_fill_an_outline_a_line_pattern_an_opacity_and_eight_looks() {
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::Box, "a", 0.0, 0.0);
        let value = |doc: &Document, n: &str| fields(doc, Target::Element(id)).unwrap().into_iter().find(|f| f.name == n).unwrap().value;
        assert_eq!((value(&doc, "look"), value(&doc, "fill"), value(&doc, "outline"), value(&doc, "opacity")), ("custom".into(), "auto".into(), "yes".into(), "100".into()));
        apply(&mut doc, Target::Element(id), "look", "blue").unwrap();
        let e = doc.element(id).unwrap();
        assert_eq!((e.fill, e.color), (Fill::Colour(Colour::Hex([218, 232, 252])), Some(Colour::Hex([108, 142, 191]))));
        assert_eq!(value(&doc, "look"), "blue", "a look is read back from the pair it set");
        apply(&mut doc, Target::Element(id), "fill", "none").unwrap();
        assert_eq!((doc.element(id).unwrap().fill, value(&doc, "look")), (Fill::None, "custom".into()));
        apply(&mut doc, Target::Element(id), "fill", "#abc").unwrap();
        assert_eq!(doc.element(id).unwrap().fill_on(true), Some([170, 187, 204]));
        apply(&mut doc, Target::Element(id), "fill", "").unwrap();
        assert_eq!(doc.element(id).unwrap().fill, Fill::Auto, "blank is auto");
        apply(&mut doc, Target::Element(id), "outline", "no").unwrap();
        apply(&mut doc, Target::Element(id), "line", "dot").unwrap();
        apply(&mut doc, Target::Element(id), "opacity", "40%").unwrap();
        let e = doc.element(id).unwrap();
        assert_eq!((e.outline, e.line, e.opacity), (false, LineStyle::Dotted, 40));
        assert!(apply(&mut doc, Target::Element(id), "opacity", "5").unwrap_err().contains("10 to 100"));
        assert!(apply(&mut doc, Target::Element(id), "look", "teal").unwrap_err().contains("no look"));
        assert!(apply(&mut doc, Target::Element(id), "fill", "zzz").unwrap_err().contains("fill is"));
        assert_eq!(Unit::Fill.choices().unwrap()[..2], ["auto", "none"]);
        assert_eq!(Unit::Percent.choices().unwrap().len(), 10);
        // An architecture shape's auto fill is its layer's pastel; a plain one's is paper.
        let app = doc.add(ShapeKind::ApplicationComponent, "b", 0.0, 10.0);
        assert_eq!(doc.element(app).unwrap().fill_on(true), Some([181, 255, 255]));
        assert_eq!(doc.element(id).unwrap().fill_on(true), Some([255, 255, 255]));
    }

    #[test]
    fn an_architecture_shapes_fill_is_auto_or_none_never_a_colour() {
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::ApplicationComponent, "svc", 0.0, 0.0);
        let field = |doc: &Document, n: &str| fields(doc, Target::Element(id)).unwrap().into_iter().find(|f| f.name == n).unwrap();
        assert_eq!(field(&doc, "fill").unit, Unit::LayerFill, "an architecture shape's fill is a switch, not a picker");
        apply(&mut doc, Target::Element(id), "fill", "none").unwrap();
        assert_eq!(doc.element(id).unwrap().fill, Fill::None);
        apply(&mut doc, Target::Element(id), "fill", "").unwrap();
        assert_eq!(doc.element(id).unwrap().fill, Fill::Auto, "blank is auto");
        assert!(apply(&mut doc, Target::Element(id), "fill", "purple").unwrap_err().contains("the colour is what says which layer"));
        assert!(apply(&mut doc, Target::Element(id), "fill", "#abc").unwrap_err().contains("the colour is what says which layer"));
        assert!(apply(&mut doc, Target::Element(id), "look", "blue").unwrap_err().contains("has no look of its own"));
        assert_eq!(doc.element(id).unwrap().fill, Fill::Auto, "a refused fill leaves it as it was");
        assert_eq!(Unit::LayerFill.choices().unwrap(), ["auto", "none"]);
    }

    #[test]
    fn a_picked_set_shows_what_its_shapes_share_and_a_value_set_there_lands_on_all_of_them() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Box, "one", 0.0, 0.0);
        let b = doc.add(ShapeKind::Ellipse, "two", 30.0, 6.0);
        let picked = [a, b];
        let fs = fields_for(&doc, Target::Picked, &picked).unwrap();
        let get = |n: &str| fs.iter().find(|f| f.name == n).unwrap();
        assert_eq!(fs.len(), fields(&doc, Target::Element(a)).unwrap().len() + SET_ACTIONS.len(), "every field is shared by two shapes, and the set's own arrangements follow");
        assert!(get("label").mixed && get("label").value.is_empty(), "labels differ");
        assert!(get("kind").mixed);
        assert!(!get("stroke").mixed && get("stroke").value == "1", "strokes agree");
        assert!(!get("to front").mixed, "an action is never mixed");
        apply_for(&mut doc, Target::Picked, "colour", "red", &picked).unwrap();
        apply_for(&mut doc, Target::Picked, "x", "12", &picked).unwrap();
        assert!(picked.iter().all(|&id| doc.element(id).unwrap().color == Some(Colour::Named(Paint::Red)) && doc.element(id).unwrap().x == 12.0), "set on both — x is an align");
        let fs = fields_for(&doc, Target::Picked, &picked).unwrap();
        assert_eq!(fs.iter().find(|f| f.name == "colour").unwrap().value, "red");
        // A value one refuses changes none; a locked shape is passed over.
        doc.element_mut(a).unwrap().w = 10.0;
        assert!(apply_for(&mut doc, Target::Picked, "width", "2", &picked).is_err());
        assert_eq!(doc.element(a).unwrap().w, 10.0);
        doc.element_mut(b).unwrap().locked = true;
        apply_for(&mut doc, Target::Picked, "stroke", "3", &picked).unwrap();
        assert_eq!((doc.element(a).unwrap().stroke, doc.element(b).unwrap().stroke), (3, 1));
        doc.element_mut(a).unwrap().locked = true;
        assert_eq!(apply_for(&mut doc, Target::Picked, "stroke", "2", &picked).unwrap_err(), LOCKED);
        assert!(apply_for(&mut doc, Target::Picked, "stroke", "2", &[]).unwrap_err().contains("nothing picked"));
        assert!(fields_for(&doc, Target::Picked, &[]).is_none());
    }

    #[test]
    fn a_label_has_a_look_a_colour_a_band_a_wrap_a_width_and_a_padding_and_a_link_s_labels_too() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Box, "one two three four", 0.0, 0.0);
        let b = doc.add(ShapeKind::Box, "b", 40.0, 0.0);
        let r = doc.connect(RelationKind::Link, a, b).unwrap();
        for (name, value) in [("bold", "yes"), ("italic", "y"), ("underline", "on"), ("text colour", "#123456"), ("label band", "yes"), ("wrap", "no"), ("label width", "8"), ("padding", "2")] {
            apply(&mut doc, Target::Element(a), name, value).unwrap();
        }
        let t = &doc.element(a).unwrap().text;
        assert!(t.bold && t.italic && t.underline && t.band && !t.wrap);
        assert_eq!((t.color, t.width, t.padding), (Some(Colour::Hex([0x12, 0x34, 0x56])), Some(8), 2));
        assert!(apply(&mut doc, Target::Element(a), "label width", "0").unwrap_err().contains("less than 1"));
        apply(&mut doc, Target::Element(a), "label width", "").unwrap();
        assert_eq!(doc.element(a).unwrap().text.width, None, "blank is the shape's own inside");
        for (name, value) in [("bold", "yes"), ("text colour", "red"), ("label band", "yes"), ("label at", "25%"), ("size", "20")] {
            apply(&mut doc, Target::Relation(r, Node::Centre), name, value).unwrap();
        }
        let rel = doc.relation(r).unwrap();
        assert!(rel.text.bold && rel.text.band && rel.text.color == Some(Colour::Named(Paint::Red)) && rel.text.size == Some(20));
        assert_eq!(rel.label_at, Some(25));
        assert!(apply(&mut doc, Target::Relation(r, Node::Centre), "label at", "2").unwrap_err().contains("5 to 95"));
        apply(&mut doc, Target::Relation(r, Node::Centre), "label at", "50").unwrap();
        assert_eq!(doc.relation(r).unwrap().label_at, None, "half way is the default, and stays out of the file");
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"bold\":true") && json.contains("\"wrap\":false") && !json.contains("label_at"), "{json}");
        assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), doc);
        assert!(!serde_json::to_string(&Document::default()).unwrap().contains("wrap"), "a default text style stays out of the file");
    }

    #[test]
    fn a_link_has_a_route_an_end_size_an_opacity_a_look_and_an_elbow() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Box, "", 0.0, 0.0);
        let b = doc.add(ShapeKind::Box, "", 40.0, 20.0);
        let r = doc.connect(RelationKind::Link, a, b).unwrap();
        let t = Target::Relation(r, Node::Centre);
        apply(&mut doc, t, "route", "orth").unwrap();
        apply(&mut doc, t, "end size", "large").unwrap();
        apply(&mut doc, t, "opacity", "40").unwrap();
        apply(&mut doc, t, "look", "blue").unwrap();
        apply(&mut doc, t, "elbow", "6").unwrap();
        let rel = doc.relation(r).unwrap();
        let n = rel.notation();
        assert_eq!((n.route, n.end_size, n.opacity, n.color), (Route::Orthogonal, EndSize::Large, 40, Some(Colour::Hex([108, 142, 191]))));
        assert_eq!(rel.elbow, Some(6));
        let value = |doc: &Document, n: &str| fields(doc, t).unwrap().into_iter().find(|f| f.name == n).unwrap().value;
        assert_eq!((value(&doc, "look"), value(&doc, "route"), value(&doc, "elbow")), ("blue".into(), "orthogonal".into(), "6".into()));
        let pts = doc.route(rel).unwrap();
        assert_eq!(pts.len(), 4, "an orthogonal route has two turns");
        let leg = (pts[1].0 - pts[0].0).abs() + (pts[1].1 - pts[0].1).abs();
        assert!((leg - 6.0).abs() < 1e-9, "the first leg is the elbow long: {pts:?}");
        assert!(pts[1].0 == pts[0].0 || pts[1].1 == pts[0].1, "and square to its edge");
        apply(&mut doc, t, "elbow", "").unwrap();
        assert_eq!(doc.relation(r).unwrap().elbow, None, "blank is half way");
        assert!(apply(&mut doc, t, "route", "zig").unwrap_err().contains("straight, orthogonal or curved"));
        assert!(apply(&mut doc, t, "opacity", "0").unwrap_err().contains("10 to 100"));
        apply(&mut doc, t, "route", "curved").unwrap();
        let pts = doc.route(doc.relation(r).unwrap()).unwrap();
        assert!(pts.len() > 8, "a curve is sampled");
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"route\":\"curved\"") && json.contains("\"end_size\":\"large\"") && !json.contains("elbow"), "{json}");
        assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), doc);
    }

    #[test]
    fn a_picked_set_lines_up_spaces_out_and_sizes_alike_to_the_first_picked() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Box, "a", 10.0, 4.0);
        let b = doc.add(ShapeKind::Box, "b", 30.0, 9.0);
        let c = doc.add(ShapeKind::Box, "c", 70.0, 1.0);
        doc.element_mut(a).unwrap().w = 20.0;
        doc.element_mut(a).unwrap().h = 8.0;
        let picked = [a, b, c];
        let get = |doc: &Document, id| doc.element(id).unwrap().clone();
        apply_for(&mut doc, Target::Picked, "align left", "", &picked).unwrap();
        assert!(picked.iter().all(|&id| get(&doc, id).x == 10.0));
        apply_for(&mut doc, Target::Picked, "align middle", "", &picked).unwrap();
        let mid = |e: &Element| e.y + e.h / 2.0;
        assert!(picked.iter().all(|&id| (mid(&get(&doc, id)) - mid(&get(&doc, a))).abs() <= 0.5));
        apply_for(&mut doc, Target::Picked, "match width", "", &picked).unwrap();
        apply_for(&mut doc, Target::Picked, "match height", "", &picked).unwrap();
        assert!(picked.iter().all(|&id| (get(&doc, id).w, get(&doc, id).h) == (20.0, 8.0)));
        doc.element_mut(b).unwrap().x = 35.0;
        doc.element_mut(c).unwrap().x = 100.0;
        apply_for(&mut doc, Target::Picked, "distribute across", "", &picked).unwrap();
        let (ea, eb, ec) = (get(&doc, a), get(&doc, b), get(&doc, c));
        assert_eq!((ea.x, ec.x), (10.0, 100.0), "the ends stay");
        assert_eq!(eb.x - ea.right(), ec.x - eb.right(), "one gap");
        assert!(apply_for(&mut doc, Target::Picked, "distribute down", "", &[a, b]).unwrap_err().contains("three or more"));
        let fs = fields_for(&doc, Target::Picked, &picked).unwrap();
        assert!(fs.iter().any(|f| f.name == "distribute across" && f.tab == Tab::Arrange && f.unit == Unit::Action));
    }

    #[test]
    fn a_relation_has_ports_and_a_reverse_action_on_its_arrange_tab() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::Box, "a", 0.0, 0.0);
        let b = doc.add(ShapeKind::Box, "b", 30.0, 0.0);
        let r = doc.connect(RelationKind::Link, a, b).unwrap();
        doc.relation_mut(r).unwrap().tail_label = Some("1".into());
        let t = Target::Relation(r, Node::Centre);
        let fs = fields(&doc, t).unwrap();
        let arrange: Vec<&str> = fs.iter().filter(|f| f.tab == Tab::Arrange).map(|f| f.name).collect();
        assert_eq!(arrange, ["from port", "to port", "elbow", "reverse", "to front", "to back", "bring forward", "send backward", "layer", "locked"]);
        assert!(apply(&mut doc, t, "from port", "top").is_ok());
        assert_eq!(doc.relation(r).unwrap().from_port, Some(1));
        assert!(apply(&mut doc, t, "to port", "sideways").unwrap_err().contains("a port is auto"));
        assert!(apply(&mut doc, t, "to port", "auto").is_ok());
        assert!(apply(&mut doc, t, "reverse", "").is_ok());
        let rel = doc.relation(r).unwrap();
        assert_eq!((rel.from, rel.to), (b, a), "the ends swapped");
        assert_eq!((rel.from_port, rel.to_port), (None, Some(1)), "and the ports with them");
        assert_eq!(rel.head_label.as_deref(), Some("1"), "and the end labels");
    }
}
