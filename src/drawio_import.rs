//! `:import <file.drawio>` — the reverse of `drawio_export`: a draw.io/diagrams.net file, read
//! back into a `Document`.
//!
//! Deliberately not pixel-faithful, the same way the export is not: a shape's paint, line and
//! opacity are not recovered, only its kind, its label and its box — position and size are what
//! a diagram is *made of*; how it is coloured is not. A cell this app itself wrote carries a
//! `vsKind`/`vsLabel` hint (see `drawio_export`'s doc comment) and comes back as the exact kind
//! and label it left as, even though several kinds share a stencil. A cell from a real
//! diagrams.net file has neither, so its kind is guessed at from its own stencil, generously —
//! an unrecognized shape becomes a plain box rather than failing the import, the same way a
//! refused relation is still drawn: the tool marks what it doesn't understand, it doesn't
//! refuse to open the file over it.
//!
//! A cell nested inside another (drawio's own grouping) is skipped rather than guessed at
//! wrongly — only what sits directly on the page comes in.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::drawio_export::{CELL_H, CELL_W};
use crate::model::{Document, ElementId};
use crate::ontology::{End, LineStyle, RelationKind, ShapeKind};

#[derive(Debug)]
pub enum ImportError {
    Io(std::io::Error),
    Parse(quick_xml::Error),
    Invalid(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::Io(e) => write!(f, "could not read file: {e}"),
            ImportError::Parse(e) => write!(f, "malformed draw.io XML: {e}"),
            ImportError::Invalid(why) => write!(f, "invalid draw.io file: {why}"),
        }
    }
}

pub fn import(path: &Path) -> Result<Document, ImportError> {
    let xml = std::fs::read_to_string(path).map_err(ImportError::Io)?;
    from_xml(&xml)
}

/// An `<mxCell>` as read, before it is known whether it becomes an element, a relation, or
/// neither (drawio's own root cells, or a row nested inside one of our own swimlane shapes).
struct Cell {
    id: String,
    parent: Option<String>,
    vs_kind: Option<String>,
    vs_label: Option<String>,
    value: Option<String>,
    style: String,
    vertex: bool,
    edge: bool,
    source: Option<String>,
    target: Option<String>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Cell {
    fn empty() -> Cell {
        Cell {
            id: String::new(),
            parent: None,
            vs_kind: None,
            vs_label: None,
            value: None,
            style: String::new(),
            vertex: false,
            edge: false,
            source: None,
            target: None,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        }
    }
}

pub fn from_xml(xml: &str) -> Result<Document, ImportError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut cells: Vec<Cell> = Vec::new();
    let mut current: Option<Cell> = None;
    loop {
        match reader.read_event().map_err(ImportError::Parse)? {
            Event::Eof => break,
            Event::Start(e) if e.name().as_ref() == "mxCell" => current = Some(read_cell(&e)?),
            Event::Empty(e) if e.name().as_ref() == "mxCell" => cells.push(read_cell(&e)?),
            Event::End(e) if e.name().as_ref() == "mxCell" => {
                if let Some(c) = current.take() {
                    cells.push(c);
                }
            }
            Event::Start(e) if e.name().as_ref() == "mxGeometry" => read_geometry(&e, &mut current)?,
            Event::Empty(e) if e.name().as_ref() == "mxGeometry" => read_geometry(&e, &mut current)?,
            _ => {}
        }
    }
    build(cells)
}

fn read_cell(e: &BytesStart) -> Result<Cell, ImportError> {
    let mut c = Cell::empty();
    for a in e.attributes() {
        let a = a.map_err(|e| ImportError::Invalid(format!("bad attribute: {e}")))?;
        let v = a.value.into_owned();
        match a.key.as_ref() {
            "id" => c.id = v,
            "vsKind" => c.vs_kind = Some(v),
            "vsLabel" => c.vs_label = Some(unescape(&v)),
            "value" => c.value = Some(unescape(&v)),
            "style" => c.style = v,
            "vertex" => c.vertex = v == "1",
            "edge" => c.edge = v == "1",
            "parent" => c.parent = Some(v),
            "source" => c.source = Some(v),
            "target" => c.target = Some(v),
            _ => {}
        }
    }
    Ok(c)
}

fn read_geometry(e: &BytesStart, current: &mut Option<Cell>) -> Result<(), ImportError> {
    let Some(c) = current.as_mut() else { return Ok(()) };
    for a in e.attributes() {
        let a = a.map_err(|e| ImportError::Invalid(format!("bad attribute: {e}")))?;
        let v = a.value.into_owned();
        match a.key.as_ref() {
            "x" => c.x = v.parse().unwrap_or(0.0),
            "y" => c.y = v.parse().unwrap_or(0.0),
            "width" => c.w = v.parse().unwrap_or(0.0),
            "height" => c.h = v.parse().unwrap_or(0.0),
            _ => {}
        }
    }
    Ok(())
}

fn build(cells: Vec<Cell>) -> Result<Document, ImportError> {
    let mut doc = Document::default();
    let mut ids: HashMap<String, ElementId> = HashMap::new();
    let mut edges: Vec<&Cell> = Vec::new();
    for c in &cells {
        // Only what sits directly on the page: a row nested inside one of our own swimlane
        // shapes, or a group in a foreign file, has some other cell as its parent, not "1".
        if c.vertex && c.parent.as_deref() == Some("1") {
            let kind = element_kind(&c.vs_kind, &c.style);
            let label = c.vs_label.clone().unwrap_or_else(|| strip_tags(c.value.as_deref().unwrap_or("")));
            let id = doc.add(kind, label, c.x / CELL_W, c.y / CELL_H);
            if c.w > 0.0 && c.h > 0.0 {
                let el = doc.element_mut(id).expect("just added");
                el.w = c.w / CELL_W;
                el.h = c.h / CELL_H;
            }
            ids.insert(c.id.clone(), id);
        } else if c.edge {
            edges.push(c);
        }
    }
    for c in edges {
        let from = c.source.as_deref().and_then(|s| ids.get(s));
        let to = c.target.as_deref().and_then(|s| ids.get(s));
        let (Some(&from), Some(&to)) = (from, to) else {
            return Err(ImportError::Invalid(format!("relation {} names an element that was not imported", c.id)));
        };
        let kind = relation_kind(&c.vs_kind, &c.style);
        let rid = doc.connect(kind, from, to).map_err(|why| ImportError::Invalid(why.to_string()))?;
        if let Some(label) = c.value.as_deref().filter(|s| !s.is_empty()) {
            doc.relation_mut(rid).expect("just connected").label = Some(label.to_string());
        }
    }
    Ok(doc)
}

/// The exact kind, if this cell is one this app wrote; else a generous guess from its stencil.
fn element_kind(vs_kind: &Option<String>, style: &str) -> ShapeKind {
    if let Some(k) = vs_kind.as_deref().and_then(ShapeKind::parse) {
        return k;
    }
    guess_shape(style)
}

/// The stencils this app's own export writes for the plain, rule-free shapes — recognized here
/// so a foreign file's own rectangles, circles and diamonds keep their look. Anything else is a
/// box: the label and the position are what matters, not the outline.
fn guess_shape(style: &str) -> ShapeKind {
    if style.contains("shape=cylinder") {
        ShapeKind::Cylinder
    } else if style.contains("shape=cloud") {
        ShapeKind::Cloud
    } else if style.contains("shape=hexagon") {
        ShapeKind::Hexagon
    } else if style.contains("rhombus") {
        ShapeKind::Diamond
    } else if style.contains("ellipse") {
        ShapeKind::Circle
    } else if style.contains("shape=parallelogram") {
        ShapeKind::Parallelogram
    } else if style.split(';').any(|k| k == "text") {
        ShapeKind::Text
    } else if style.contains("rounded=1") {
        ShapeKind::RoundedBox
    } else {
        ShapeKind::Box
    }
}

/// The exact kind, if this edge is one this app wrote; else the `RelationKind` whose own
/// notation matches the line and arrowheads drawn — the vocabulary reused, not restated.
fn relation_kind(vs_kind: &Option<String>, style: &str) -> RelationKind {
    if let Some(k) = vs_kind.as_deref().and_then(RelationKind::parse) {
        return k;
    }
    guess_relation(style)
}

fn guess_relation(style: &str) -> RelationKind {
    let line = if !style.contains("dashed=1") {
        LineStyle::Solid
    } else if style.contains("dashPattern") {
        LineStyle::Dotted
    } else {
        LineStyle::Dashed
    };
    let tail = end_from_style(style, true);
    let head = end_from_style(style, false);
    RelationKind::ALL
        .into_iter()
        .find(|k| {
            let n = k.notation();
            n.line == line && n.tail == tail && n.head == head
        })
        .unwrap_or(RelationKind::Link)
}

fn end_from_style(style: &str, start: bool) -> End {
    let arrow = find_value(style, if start { "startArrow=" } else { "endArrow=" });
    let filled = find_value(style, if start { "startFill=" } else { "endFill=" }) != Some("0");
    match arrow {
        Some("none") => End::None,
        Some("block") => {
            if filled {
                End::Arrow
            } else {
                End::Triangle
            }
        }
        Some("open") => End::Open,
        Some("diamondThin") => {
            if filled {
                End::Diamond
            } else {
                End::HollowDiamond
            }
        }
        Some("oval") => {
            if filled {
                End::Dot
            } else {
                End::Circle
            }
        }
        Some("ERmany") => End::Crow,
        Some("ERone") => End::Bar,
        Some(_) => End::Arrow,
        // drawio's own default edge, when neither key is written: no tail, a plain head arrow.
        None => {
            if start {
                End::None
            } else {
                End::Arrow
            }
        }
    }
}

fn find_value<'a>(style: &'a str, key: &str) -> Option<&'a str> {
    style.split(';').find_map(|kv| kv.strip_prefix(key))
}

/// The inverse of `drawio_export::escape` — the four entities it writes, undone in the order
/// that keeps `&amp;` from swallowing the others: it is the only one of the four that can
/// appear as a *result* of escaping, so it must be the last one unescaped. Shared with
/// `archimate_import`: unescaping XML's four named entities is one rule, not two.
pub(crate) fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&gt;", ">").replace("&lt;", "<").replace("&amp;", "&")
}

/// A generous label reader for a foreign cell's HTML `value`: not a sanitizer, just enough to
/// keep drawio's own `<br>`/`<font>` decoration out of a label this app didn't write.
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::RelationKind as RK;
    use crate::ontology::ShapeKind as SK;

    #[test]
    fn a_document_survives_an_export_then_import_round_trip() {
        let mut doc = Document::default();
        let a = doc.add(SK::ApplicationComponent, "CRM", 1.0, 1.0);
        let b = doc.add(SK::ApplicationService, "Contacts", 20.0, 3.0);
        doc.element_mut(b).unwrap().w = 8.0;
        doc.element_mut(b).unwrap().h = 5.0;
        let r = doc.connect(RK::Realization, a, b).unwrap();
        doc.relation_mut(r).unwrap().label = Some("provides".into());

        let xml = crate::drawio_export::to_xml(&doc);
        let back = from_xml(&xml).expect("imports");

        assert_eq!(back.elements.len(), 2);
        let a2 = &back.elements[0];
        assert_eq!(a2.kind, SK::ApplicationComponent, "the exact kind, not just its shape");
        assert_eq!(a2.label, "CRM");
        assert_eq!((a2.x, a2.y), (1.0, 1.0));
        let b2 = &back.elements[1];
        assert_eq!(b2.kind, SK::ApplicationService);
        assert_eq!(b2.label, "Contacts");
        assert_eq!((b2.x, b2.y, b2.w, b2.h), (20.0, 3.0, 8.0, 5.0));

        assert_eq!(back.relations.len(), 1);
        let r2 = &back.relations[0];
        assert_eq!(r2.kind, RK::Realization, "the exact relation kind, recovered from its hint");
        assert_eq!(r2.label.as_deref(), Some("provides"));
        assert_eq!((r2.from, r2.to), (a2.id, b2.id));
    }

    #[test]
    fn a_label_with_markup_round_trips_through_the_double_escape() {
        let mut doc = Document::default();
        doc.add(SK::Box, "CRM <v2> & \"friends\"", 0.0, 0.0);
        let xml = crate::drawio_export::to_xml(&doc);
        let back = from_xml(&xml).expect("imports");
        assert_eq!(back.elements[0].label, "CRM <v2> & \"friends\"");
    }

    #[test]
    fn a_genuine_diagrams_net_file_imports_generously() {
        // A minimal file with none of this app's own hints — the shape of a real export.
        const FOREIGN: &str = r#"<mxfile host="app.diagrams.net">
  <diagram name="Page-1" id="p1">
    <mxGraphModel dx="800" dy="600" grid="1" gridSize="10" page="1">
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <mxCell id="2" value="Server" style="rounded=0;whiteSpace=wrap;html=1;" vertex="1" parent="1">
          <mxGeometry x="40" y="40" width="120" height="60" as="geometry"/>
        </mxCell>
        <mxCell id="3" value="Cache" style="ellipse;whiteSpace=wrap;html=1;" vertex="1" parent="1">
          <mxGeometry x="240" y="40" width="80" height="80" as="geometry"/>
        </mxCell>
        <mxCell id="4" value="reads from" style="edgeStyle=orthogonalEdgeStyle;html=1;" edge="1" parent="1" source="2" target="3">
          <mxGeometry relative="1" as="geometry"/>
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>"#;
        let doc = from_xml(FOREIGN).expect("a real diagrams.net file imports");
        assert_eq!(doc.elements.len(), 2);
        assert_eq!(doc.elements[0].kind, SK::Box, "an unrecognized stencil is still a box");
        assert_eq!(doc.elements[0].label, "Server");
        assert_eq!(doc.elements[1].kind, SK::Circle, "an ellipse stencil is recognized");
        assert_eq!((doc.elements[0].x, doc.elements[0].y), (4.0, 2.0));
        assert_eq!(doc.relations.len(), 1);
        assert_eq!(doc.relations[0].label.as_deref(), Some("reads from"));
    }

    #[test]
    fn an_edge_to_an_element_that_was_never_imported_is_refused_whole() {
        const BROKEN: &str = r#"<mxfile><diagram><mxGraphModel><root>
          <mxCell id="0"/><mxCell id="1" parent="0"/>
          <mxCell id="2" value="A" style="" vertex="1" parent="1"><mxGeometry x="0" y="0" width="10" height="10" as="geometry"/></mxCell>
          <mxCell id="3" edge="1" parent="1" source="2" target="9"><mxGeometry as="geometry"/></mxCell>
        </root></mxGraphModel></diagram></mxfile>"#;
        match from_xml(BROKEN) {
            Err(ImportError::Invalid(why)) => assert!(why.contains("was not imported"), "{why}"),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn import_reads_the_file_a_path_names() {
        let mut doc = Document::default();
        doc.add(SK::Box, "X", 0.0, 0.0);
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-drawio-import-{}.drawio", std::process::id()));
        std::fs::write(&path, crate::drawio_export::to_xml(&doc)).unwrap();
        let back = import(&path).expect("imports");
        std::fs::remove_file(&path).ok();
        assert_eq!(back.elements.len(), 1);
        assert_eq!(back.elements[0].label, "X");
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        match import(std::path::Path::new("/nonexistent/vim-shapes-does-not-exist.drawio")) {
            Err(ImportError::Io(_)) => {}
            other => panic!("expected Io error, got {other:?}"),
        }
    }

    #[test]
    fn malformed_xml_is_a_parse_error() {
        match from_xml("<mxfile><diagram") {
            Err(ImportError::Parse(_)) => {}
            other => panic!("expected Parse error, got {other:?}"),
        }
    }
}
