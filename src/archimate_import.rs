//! `:import <file.xml>` — the Open Group's ArchiMate Model Exchange File Format, read into a
//! `Workspace`. What Archi writes with File > Export > Model to Open Exchange Format.
//!
//! The two vocabularies were never going to need a translation table written twice over: this
//! ontology's own element and relation kinds already line up with ArchiMate's, because they
//! were drawn from it. `BusinessActor`, `ApplicationComponent`, `TechnologyInterface` are this
//! ontology's own Rust names as much as they are ArchiMate's XML ones — split "BusinessActor"
//! at its capitals and it *is* `ShapeKind::parse`'s own key. So there is no hand-written map
//! from "BusinessActor" to `ShapeKind::BusinessActor`; there is a function that turns
//! "BusinessActor" into "business-actor" and asks the ontology whether that names anything,
//! the same question typing `:add business-actor` asks. A concept ArchiMate has and this
//! ontology does not — a newer 3.x type, a junction, Contract, Representation, Value, Meaning,
//! any of the Collaboration/Interaction pairs — answers no, and becomes a plain box carrying
//! its ArchiMate type in its label, so nothing is silently dropped. Relationships are simpler
//! still: all eleven of ArchiMate's are already this ontology's own relation names.
//!
//! A model's elements and relationships are the whole of it; a view is a picture of some of
//! them, and the format's `<views>` section (Archi's own extension to the standard, not every
//! exporter writes it) is optional. When there is one, each view becomes its own tab, placed
//! exactly where Archi drew it; whatever the file never put on a view — and everything, if the
//! file has no views at all — lands on its own tab, arranged by layer like `:layout` would.
//! Only a relationship actually drawn as a connection on a view is carried into that view's
//! tab; one connected on the model but never diagrammed anywhere does not appear twice by
//! guesswork — it is not lost, it is on the model's own tab if both ends ended up there.
//!
//! `:import <folder>` reads the *other* thing this can mean: a coArchi model repository — the
//! same model, kept as a folder of small XML files under git rather than one exchange file, so
//! a merge conflict is one element or one relationship, not the whole model. It is a different
//! dialect of XML entirely (Archi's own, not the Open Group's — see `coarchi` below), but it
//! resolves to the same intermediate shape as the exchange reader above and is built by the
//! same `build` function, so a model is a model whichever way it arrived.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::drawio_export::{CELL_H, CELL_W};
use crate::drawio_import::unescape;
use crate::layout;
use crate::model::{Document, ElementId, Tab, Workspace, WORKSPACE_VERSION};
use crate::ontology::{RelationKind, ShapeKind};

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
            ImportError::Parse(e) => write!(f, "malformed ArchiMate XML: {e}"),
            ImportError::Invalid(why) => write!(f, "invalid ArchiMate file: {why}"),
        }
    }
}

pub fn import(path: &Path) -> Result<Workspace, ImportError> {
    let xml = std::fs::read_to_string(path).map_err(ImportError::Io)?;
    let fallback = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "model".into());
    from_xml(&xml, &fallback)
}

struct RawElement {
    id: String,
    kind: String,
    name: String,
}

struct RawRelationship {
    id: String,
    kind: String,
    name: Option<String>,
    source: String,
    target: String,
}

struct RawNode {
    element_ref: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

struct RawView {
    name: String,
    nodes: Vec<RawNode>,
    connections: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    None,
    Element,
    Relationship,
    View,
}

pub fn from_xml(xml: &str, fallback_name: &str) -> Result<Workspace, ImportError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut model_name = String::new();
    let mut elements: Vec<RawElement> = Vec::new();
    let mut relationships: Vec<RawRelationship> = Vec::new();
    let mut views: Vec<RawView> = Vec::new();

    let mut section = Section::None;
    let mut in_name = false;
    let mut current_view: Option<RawView> = None;

    loop {
        match reader.read_event().map_err(ImportError::Parse)? {
            Event::Eof => break,
            Event::Start(e) => match e.name().local_name().as_ref() {
                "element" => {
                    elements.push(read_element(&e)?);
                    section = Section::Element;
                }
                "relationship" => {
                    relationships.push(read_relationship(&e)?);
                    section = Section::Relationship;
                }
                "view" => {
                    current_view = Some(RawView { name: String::new(), nodes: Vec::new(), connections: Vec::new() });
                    section = Section::View;
                }
                "node" => {
                    if let (Some(v), Some(n)) = (current_view.as_mut(), read_node(&e)?) {
                        v.nodes.push(n);
                    }
                }
                "connection" => {
                    if let (Some(v), Some(r)) = (current_view.as_mut(), read_connection_ref(&e)) {
                        v.connections.push(r);
                    }
                }
                "name" => in_name = true,
                _ => {}
            },
            Event::Empty(e) => match e.name().local_name().as_ref() {
                "element" => elements.push(read_element(&e)?),
                "relationship" => relationships.push(read_relationship(&e)?),
                "node" => {
                    if let (Some(v), Some(n)) = (current_view.as_mut(), read_node(&e)?) {
                        v.nodes.push(n);
                    }
                }
                "connection" => {
                    if let (Some(v), Some(r)) = (current_view.as_mut(), read_connection_ref(&e)) {
                        v.connections.push(r);
                    }
                }
                _ => {}
            },
            Event::End(e) => match e.name().local_name().as_ref() {
                "element" | "relationship" => section = Section::None,
                "view" => {
                    if let Some(v) = current_view.take() {
                        views.push(v);
                    }
                    section = Section::None;
                }
                "name" => in_name = false,
                _ => {}
            },
            Event::Text(t) if in_name => {
                let text = unescape(t.as_ref());
                match section {
                    Section::Element => {
                        if let Some(last) = elements.last_mut().filter(|e| e.name.is_empty()) {
                            last.name = text;
                        }
                    }
                    Section::Relationship => {
                        if let Some(last) = relationships.last_mut().filter(|r| r.name.is_none()) {
                            last.name = Some(text);
                        }
                    }
                    Section::View => {
                        if let Some(v) = current_view.as_mut().filter(|v| v.name.is_empty()) {
                            v.name = text;
                        }
                    }
                    Section::None => {
                        if model_name.is_empty() {
                            model_name = text;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if elements.is_empty() {
        return Err(ImportError::Invalid("no elements — not an ArchiMate model file".into()));
    }

    let name = if model_name.is_empty() { fallback_name } else { &model_name };
    Ok(build(name, elements, relationships, views))
}

fn read_element(e: &BytesStart) -> Result<RawElement, ImportError> {
    let mut id = String::new();
    let mut kind = String::new();
    for a in e.attributes() {
        let a = a.map_err(|e| ImportError::Invalid(format!("bad attribute: {e}")))?;
        let v = a.value.into_owned();
        match a.key.local_name().as_ref() {
            "identifier" => id = v,
            "type" => kind = v,
            _ => {}
        }
    }
    Ok(RawElement { id, kind, name: String::new() })
}

fn read_relationship(e: &BytesStart) -> Result<RawRelationship, ImportError> {
    let mut r = RawRelationship { id: String::new(), kind: String::new(), name: None, source: String::new(), target: String::new() };
    for a in e.attributes() {
        let a = a.map_err(|e| ImportError::Invalid(format!("bad attribute: {e}")))?;
        let v = a.value.into_owned();
        match a.key.local_name().as_ref() {
            "identifier" => r.id = v,
            "type" => r.kind = v,
            "source" => r.source = v,
            "target" => r.target = v,
            _ => {}
        }
    }
    Ok(r)
}

/// `None` when the node has no `elementRef` — a pure visual container or a free-floating
/// label, neither of which names a model element. Its children (if it is a container) are
/// still walked, since a `Start` tag was seen either way.
fn read_node(e: &BytesStart) -> Result<Option<RawNode>, ImportError> {
    let mut element_ref: Option<String> = None;
    let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
    for a in e.attributes() {
        let a = a.map_err(|e| ImportError::Invalid(format!("bad attribute: {e}")))?;
        let v = a.value.into_owned();
        match a.key.local_name().as_ref() {
            "elementRef" => element_ref = Some(v),
            "x" => x = v.parse().unwrap_or(0.0),
            "y" => y = v.parse().unwrap_or(0.0),
            "w" => w = v.parse().unwrap_or(0.0),
            "h" => h = v.parse().unwrap_or(0.0),
            _ => {}
        }
    }
    Ok(element_ref.map(|element_ref| RawNode { element_ref, x, y, w, h }))
}

/// `None` for a connection with no `relationshipRef` — a plain visual line, not a relation.
fn read_connection_ref(e: &BytesStart) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.local_name().as_ref() == "relationshipRef").map(|a| a.value.into_owned())
}

fn build(model_name: &str, elements: Vec<RawElement>, relationships: Vec<RawRelationship>, views: Vec<RawView>) -> Workspace {
    let by_id: HashMap<&str, usize> = elements.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let rel_by_id: HashMap<&str, usize> = relationships.iter().enumerate().map(|(i, r)| (r.id.as_str(), i)).collect();

    if views.is_empty() {
        let mut doc = Document::default();
        let ids: HashMap<&str, ElementId> = elements.iter().map(|e| (e.id.as_str(), add_element(&mut doc, e))).collect();
        for r in &relationships {
            connect(&mut doc, &ids, r);
        }
        arrange(&mut doc);
        return Workspace::single(model_name.to_string(), doc);
    }

    let mut tabs = Vec::new();
    let mut placed: HashSet<usize> = HashSet::new();
    for (i, v) in views.iter().enumerate() {
        let mut doc = Document::default();
        let mut ids: HashMap<&str, ElementId> = HashMap::new();
        for n in &v.nodes {
            let Some(&ei) = by_id.get(n.element_ref.as_str()) else { continue };
            ids.insert(elements[ei].id.as_str(), add_node(&mut doc, &elements[ei], n));
            placed.insert(ei);
        }
        for cref in &v.connections {
            let Some(&ri) = rel_by_id.get(cref.as_str()) else { continue };
            connect(&mut doc, &ids, &relationships[ri]);
        }
        tabs.push(Tab { name: view_tab_name(&v.name, i), diagram: doc });
    }

    let unplaced: Vec<&RawElement> = (0..elements.len()).filter(|i| !placed.contains(i)).map(|i| &elements[i]).collect();
    if !unplaced.is_empty() {
        let mut doc = Document::default();
        let ids: HashMap<&str, ElementId> = unplaced.iter().map(|e| (e.id.as_str(), add_element(&mut doc, e))).collect();
        for r in &relationships {
            if ids.contains_key(r.source.as_str()) && ids.contains_key(r.target.as_str()) {
                connect(&mut doc, &ids, r);
            }
        }
        arrange(&mut doc);
        tabs.push(Tab { name: "(unplaced)".into(), diagram: doc });
    }
    Workspace { version: WORKSPACE_VERSION, grid: true, current: 0, tabs }
}

/// A view's tab name — its own, or a placeholder by position when the file left it blank.
/// Shared with the coArchi folder tree below, so a tree entry always names the tab `build`
/// actually gave that view, rather than guessing at the same rule a second time.
fn view_tab_name(name: &str, index: usize) -> String {
    if name.is_empty() { format!("diagram {}", index + 1) } else { name.to_string() }
}

fn kind_and_label(e: &RawElement) -> (ShapeKind, String) {
    match element_kind(&e.kind) {
        Some(k) => (k, e.name.clone()),
        None if e.name.is_empty() => (ShapeKind::Box, format!("[{}]", e.kind)),
        None => (ShapeKind::Box, format!("{} [{}]", e.name, e.kind)),
    }
}

fn add_element(doc: &mut Document, e: &RawElement) -> ElementId {
    let (kind, label) = kind_and_label(e);
    doc.add(kind, label, 0.0, 0.0)
}

fn add_node(doc: &mut Document, e: &RawElement, n: &RawNode) -> ElementId {
    let (kind, label) = kind_and_label(e);
    let id = doc.add(kind, label, n.x / CELL_W, n.y / CELL_H);
    if n.w > 0.0 && n.h > 0.0 {
        let el = doc.element_mut(id).expect("just added");
        el.w = n.w / CELL_W;
        el.h = n.h / CELL_H;
    }
    id
}

fn connect(doc: &mut Document, ids: &HashMap<&str, ElementId>, r: &RawRelationship) {
    let (Some(&from), Some(&to)) = (ids.get(r.source.as_str()), ids.get(r.target.as_str())) else { return };
    let Ok(rid) = doc.connect(relation_kind(&r.kind), from, to) else { return };
    if let Some(name) = r.name.as_deref().filter(|s| !s.is_empty()) {
        doc.relation_mut(rid).expect("just connected").label = Some(name.to_string());
    }
}

fn arrange(doc: &mut Document) {
    for (id, x, y) in layout::layers(doc) {
        if let Some(e) = doc.element_mut(id) {
            e.x = x;
            e.y = y;
        }
    }
}

/// The exact kind, when ArchiMate's own PascalCase type name, split at its capitals, names one
/// of this ontology's own — see the module doc comment for why that is not a coincidence.
/// `None` for a concept this ontology has no counterpart for.
fn element_kind(xsi_type: &str) -> Option<ShapeKind> {
    ShapeKind::parse(&kebab(xsi_type))
}

/// All eleven of ArchiMate's relationship names are already this ontology's own; anything else
/// — there is nothing else, in the standard — falls back to the plain, always-allowed link.
/// Archi's own file dialect (see `coarchi` below) spells these with a trailing "Relationship"
/// that the exchange format doesn't; stripping it is harmless either way.
fn relation_kind(xsi_type: &str) -> RelationKind {
    let base = xsi_type.strip_suffix("Relationship").unwrap_or(xsi_type);
    RelationKind::parse(&base.to_ascii_lowercase()).unwrap_or(RelationKind::Link)
}

/// "BusinessActor" -> "business-actor". Splits before every capital but the first, so it can
/// be handed to `ShapeKind::parse`, which matches on a name lowercased the same way — this is
/// the whole of the translation from ArchiMate's vocabulary to this ontology's own.
fn kebab(pascal: &str) -> String {
    let mut out = String::new();
    for (i, c) in pascal.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('-');
        }
        out.extend(c.to_lowercase());
    }
    out
}

// ─── coArchi ────────────────────────────────────────────────────────────────
//
// A coArchi model repository is a folder: `folder.xml` names the model, and every element,
// relationship and view is its own file, one tag deep, spread across a handful of
// category folders — so a git diff is one concept, not the whole model. The dialect (Archi's
// own, `xmlns:archimate="http://www.archimatetool.com/archimate"`) differs from the exchange
// format in three ways that matter here: a concept's name is its own root tag's attribute, not
// a child element; a relationship's source and target are `href`s to *another file*
// (`"BusinessActor_1a2b3c.xml#1a2b3c"` — the id is what follows the `#`), not a bare id in the
// same document; and a view's bounds are relative to whatever they are nested in, not to the
// page, since Archi lets one shape contain another on the canvas itself. That last one is why
// this reads each file into a small generic tree first rather than streaming it flat the way
// the exchange reader does — an offset has to accumulate down through nesting before a node's
// absolute position is known, and nothing about the file's own element order can be relied on
// to make that a single pass.

/// A handful of attributes and children, with no ArchiMate meaning of its own yet — general
/// enough to read any of coArchi's per-concept files the same way.
struct XmlNode {
    tag: String,
    attrs: HashMap<String, String>,
    children: Vec<XmlNode>,
}

impl XmlNode {
    fn attr(&self, key: &str) -> &str {
        self.attrs.get(key).map(String::as_str).unwrap_or("")
    }

    fn child(&self, tag: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.tag == tag)
    }
}

fn parse_tree(xml: &str) -> Result<XmlNode, ImportError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root: Option<XmlNode> = None;
    loop {
        match reader.read_event().map_err(ImportError::Parse)? {
            Event::Eof => break,
            Event::Start(e) => stack.push(xml_node(&e)?),
            Event::Empty(e) => attach(&mut stack, &mut root, xml_node(&e)?),
            Event::End(_) => {
                let node = stack.pop().ok_or_else(|| ImportError::Invalid("unbalanced XML".into()))?;
                attach(&mut stack, &mut root, node);
            }
            _ => {}
        }
    }
    root.ok_or_else(|| ImportError::Invalid("empty file".into()))
}

fn attach(stack: &mut [XmlNode], root: &mut Option<XmlNode>, node: XmlNode) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(node),
        None => *root = Some(node),
    }
}

fn xml_node(e: &BytesStart) -> Result<XmlNode, ImportError> {
    let tag = e.name().local_name().as_ref().to_string();
    let mut attrs = HashMap::new();
    for a in e.attributes() {
        let a = a.map_err(|e| ImportError::Invalid(format!("bad attribute: {e}")))?;
        attrs.insert(a.key.local_name().as_ref().to_string(), unescape(&a.value));
    }
    Ok(XmlNode { tag, attrs, children: Vec::new() })
}

/// The id half of an Archi cross-file reference — `"BusinessActor_1a2b3c.xml#1a2b3c"` names
/// the same concept its own file's `id` attribute does; the filename is redundant with it.
fn href_id(href: &str) -> String {
    href.rsplit('#').next().unwrap_or(href).to_string()
}

/// A folder in a coArchi repository's own organization — its name from that folder's own
/// `folder.xml`, not the (often meaningless) directory name — shown by the model tree
/// (`ui::tree`). Ephemeral: built once at import time from the folders on disk, never part of
/// the workspace this app saves, since the file format has no notion of folders at all and
/// nothing about *how a model came in* belongs in what it *is*.
#[derive(Debug)]
pub enum ModelNode {
    Folder { name: String, children: Vec<ModelNode>, expanded: bool },
    /// `tab_index` is the position `build` gives this view's tab among the *views'* tabs —
    /// stable because every view becomes exactly one tab, in the same order it was found in.
    View { name: String, tab_index: usize },
}

/// `dir` may be the model repository's root (coArchi's own layout: `model/folder.xml` under
/// it) or the `model` folder itself — whichever the user typed, so `:import` on the checkout
/// works the same as `:import` on its `model` subfolder.
pub fn import_coarchi(dir: &Path) -> Result<(Workspace, Vec<ModelNode>), ImportError> {
    let root = if dir.join("folder.xml").is_file() { dir.to_path_buf() } else { dir.join("model") };
    if !root.join("folder.xml").is_file() {
        return Err(ImportError::Invalid(format!("no folder.xml under {} — not a coArchi model", dir.display())));
    }
    let model = parse_tree(&std::fs::read_to_string(root.join("folder.xml")).map_err(ImportError::Io)?)?;
    let model_name = model.attr("name").to_string();

    let mut elements = Vec::new();
    let mut relationships = Vec::new();
    let mut views = Vec::new();
    let tree = walk_coarchi_dir(&root, &mut elements, &mut relationships, &mut views)?;

    if elements.is_empty() {
        return Err(ImportError::Invalid(format!("no elements found under {}", root.display())));
    }
    let name = if model_name.is_empty() { root.display().to_string() } else { model_name };
    Ok((build(&name, elements, relationships, views), tree))
}

/// Walks one directory: a `folder.xml` names it (or the directory's own name, if it has
/// none); every other `.xml` file is an element, a relationship, or a view, sorted the same
/// as any file browser would so the tree reads the way the repository looks on disk. A
/// sub-directory becomes a child folder, recursively — but only if it actually has anything
/// in it, so an empty category folder from a model that never used it does not show up as a
/// dead end.
fn walk_coarchi_dir(dir: &Path, elements: &mut Vec<RawElement>, relationships: &mut Vec<RawRelationship>, views: &mut Vec<RawView>) -> Result<Vec<ModelNode>, ImportError> {
    let mut nodes = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(nodes) };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            let children = walk_coarchi_dir(&path, elements, relationships, views)?;
            if !children.is_empty() {
                let name = read_folder_name(&path).unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                nodes.push(ModelNode::Folder { name, children, expanded: false });
            }
            continue;
        }
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")) || path.file_name().is_some_and(|n| n == "folder.xml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(ImportError::Io)?;
        let tree = parse_tree(&text)?;
        if tree.tag == "ArchimateDiagramModel" {
            let view = read_coarchi_view(&tree);
            let index = views.len();
            nodes.push(ModelNode::View { name: view_tab_name(&view.name, index), tab_index: index });
            views.push(view);
        } else if tree.child("source").is_some() {
            relationships.push(RawRelationship {
                id: tree.attr("id").to_string(),
                kind: tree.tag.clone(),
                name: Some(tree.attr("name").to_string()).filter(|s| !s.is_empty()),
                source: tree.child("source").map(|s| href_id(s.attr("href"))).unwrap_or_default(),
                target: tree.child("target").map(|t| href_id(t.attr("href"))).unwrap_or_default(),
            });
        } else {
            elements.push(RawElement { id: tree.attr("id").to_string(), kind: tree.tag.clone(), name: tree.attr("name").to_string() });
        }
    }
    Ok(nodes)
}

fn read_folder_name(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("folder.xml")).ok()?;
    let tree = parse_tree(&text).ok()?;
    Some(tree.attr("name").to_string()).filter(|s| !s.is_empty())
}

fn read_coarchi_view(tree: &XmlNode) -> RawView {
    let mut nodes = Vec::new();
    let mut connections = Vec::new();
    for child in &tree.children {
        walk_coarchi_node(child, (0.0, 0.0), &mut nodes, &mut connections);
    }
    RawView { name: tree.attr("name").to_string(), nodes, connections }
}

/// A `<children>` node's own place is its bounds *plus* whatever it sits inside; its own
/// nested `<children>` need that sum to find theirs in turn, which is why this carries it down
/// rather than reading every node's bounds as if it were the page's own corner.
fn walk_coarchi_node(node: &XmlNode, offset: (f64, f64), nodes: &mut Vec<RawNode>, connections: &mut Vec<String>) {
    if node.tag == "sourceConnections" {
        if let Some(rel) = node.child("archimateRelationship") {
            connections.push(href_id(rel.attr("href")));
        }
        return;
    }
    if node.tag != "children" {
        return;
    }
    let (bx, by, w, h) = match node.child("bounds") {
        Some(b) => (attr_f64(b, "x"), attr_f64(b, "y"), attr_f64(b, "width"), attr_f64(b, "height")),
        None => (0.0, 0.0, 0.0, 0.0),
    };
    let here = (offset.0 + bx, offset.1 + by);
    if let Some(el) = node.child("archimateElement") {
        nodes.push(RawNode { element_ref: href_id(el.attr("href")), x: here.0, y: here.1, w, h });
    }
    for child in &node.children {
        walk_coarchi_node(child, here, nodes, connections);
    }
}

fn attr_f64(node: &XmlNode, key: &str) -> f64 {
    node.attr(key).parse().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::RelationKind as RK;
    use crate::ontology::ShapeKind as SK;

    const MINIMAL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<model xmlns="http://www.opengroup.org/xsd/archimate/3.0/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" identifier="id-model">
  <name xml:lang="en">Test Model</name>
  <elements>
    <element identifier="id-1" xsi:type="ApplicationComponent"><name xml:lang="en">CRM</name></element>
    <element identifier="id-2" xsi:type="ApplicationService"><name xml:lang="en">Contacts</name></element>
    <element identifier="id-3" xsi:type="Path"><name xml:lang="en">Backbone</name></element>
  </elements>
  <relationships>
    <relationship identifier="id-10" xsi:type="Realization" source="id-1" target="id-2"><name xml:lang="en">provides</name></relationship>
    <relationship identifier="id-11" xsi:type="Serving" source="id-3" target="id-1" />
  </relationships>
</model>"#;

    #[test]
    fn a_model_with_no_views_becomes_one_tab_arranged_by_layer() {
        let ws = from_xml(MINIMAL, "fallback").expect("imports");
        assert_eq!(ws.tabs.len(), 1);
        assert_eq!(ws.tabs[0].name, "Test Model");
        let doc = &ws.tabs[0].diagram;
        assert_eq!(doc.elements.len(), 3);
        assert_eq!(doc.elements[0].kind, SK::ApplicationComponent);
        assert_eq!(doc.elements[0].label, "CRM");
        assert_eq!(doc.elements[1].kind, SK::ApplicationService);
        // Path has no counterpart in this ontology: a box, tagged with its own ArchiMate type.
        assert_eq!(doc.elements[2].kind, SK::Box);
        assert_eq!(doc.elements[2].label, "Backbone [Path]");
        assert_eq!(doc.relations.len(), 2);
        assert_eq!(doc.relations[0].kind, RK::Realization);
        assert_eq!(doc.relations[0].label.as_deref(), Some("provides"));
        assert_eq!(doc.relations[1].kind, RK::Serving);
    }

    #[test]
    fn a_view_places_its_nodes_where_the_file_said_and_only_its_own_connections() {
        const WITH_VIEW: &str = r#"<model xmlns="http://www.opengroup.org/xsd/archimate/3.0/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" identifier="id-model">
  <elements>
    <element identifier="id-1" xsi:type="ApplicationComponent"><name>CRM</name></element>
    <element identifier="id-2" xsi:type="ApplicationService"><name>Contacts</name></element>
    <element identifier="id-3" xsi:type="ApplicationComponent"><name>Billing</name></element>
  </elements>
  <relationships>
    <relationship identifier="id-10" xsi:type="Realization" source="id-1" target="id-2" />
    <relationship identifier="id-11" xsi:type="Association" source="id-1" target="id-3" />
  </relationships>
  <views>
    <diagrams>
      <view identifier="id-v1" xsi:type="Diagram">
        <name>Main View</name>
        <node identifier="id-n1" xsi:type="Element" elementRef="id-1" x="10" y="20" w="120" h="60" />
        <node identifier="id-n2" xsi:type="Element" elementRef="id-2" x="200" y="20" w="120" h="60" />
        <connection identifier="id-c1" xsi:type="Relationship" relationshipRef="id-10" source="id-n1" target="id-n2" />
      </view>
    </diagrams>
  </views>
</model>"#;
        let ws = from_xml(WITH_VIEW, "fallback").expect("imports");
        assert_eq!(ws.tabs.len(), 2, "the view's own tab, and one for what it left out");
        assert_eq!(ws.tabs[0].name, "Main View");
        let placed = &ws.tabs[0].diagram;
        assert_eq!(placed.elements.len(), 2);
        assert_eq!((placed.elements[0].x, placed.elements[0].y), (1.0, 1.0));
        assert_eq!((placed.elements[0].w, placed.elements[0].h), (12.0, 3.0));
        assert_eq!(placed.relations.len(), 1, "only the connection the view actually drew");
        assert_eq!(placed.relations[0].kind, RK::Realization);

        assert_eq!(ws.tabs[1].name, "(unplaced)");
        let unplaced = &ws.tabs[1].diagram;
        assert_eq!(unplaced.elements.len(), 1);
        assert_eq!(unplaced.elements[0].label, "Billing");
        assert!(unplaced.relations.is_empty(), "Billing's own relation had no view to be on, and its other end isn't here either");
    }

    #[test]
    fn import_reads_the_file_a_path_names() {
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-archimate-import-{}.xml", std::process::id()));
        std::fs::write(&path, MINIMAL).unwrap();
        let ws = import(&path).expect("imports");
        std::fs::remove_file(&path).ok();
        assert_eq!(ws.tabs[0].name, "Test Model");
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        match import(std::path::Path::new("/nonexistent/vim-shapes-does-not-exist.xml")) {
            Err(ImportError::Io(_)) => {}
            other => panic!("expected Io error, got {other:?}"),
        }
    }

    #[test]
    fn a_file_that_is_not_an_archimate_model_is_refused() {
        match from_xml("<model><documentation>no elements here</documentation></model>", "x") {
            Err(ImportError::Invalid(_)) => {}
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn malformed_xml_is_a_parse_error() {
        match from_xml("<model><elements", "x") {
            Err(ImportError::Parse(_)) => {}
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    /// A minimal coArchi repository, hand-built to the shape Archi's own serializer writes:
    /// `model/folder.xml` naming the model, one file per element and per relationship, and one
    /// per view — a grouping nested inside another, to prove a child's bounds are read
    /// relative to its parent's, not to the page.
    fn write_coarchi_fixture(root: &std::path::Path) {
        let model = root.join("model");
        std::fs::create_dir_all(model.join("application")).unwrap();
        std::fs::create_dir_all(model.join("relations")).unwrap();
        std::fs::create_dir_all(model.join("diagrams")).unwrap();

        std::fs::write(
            model.join("folder.xml"),
            r#"<archimate:ArchimateModel xmlns:archimate="http://www.archimatetool.com/archimate" name="Test Model" id="m1"/>"#,
        )
        .unwrap();
        std::fs::write(
            model.join("application/ApplicationComponent_a1.xml"),
            r#"<archimate:ApplicationComponent xmlns:archimate="http://www.archimatetool.com/archimate" name="CRM" id="a1"/>"#,
        )
        .unwrap();
        std::fs::write(
            model.join("application/ApplicationService_a2.xml"),
            r#"<archimate:ApplicationService xmlns:archimate="http://www.archimatetool.com/archimate" name="Contacts" id="a2"/>"#,
        )
        .unwrap();
        std::fs::write(
            model.join("relations/RealizationRelationship_r1.xml"),
            r#"<archimate:RealizationRelationship xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:archimate="http://www.archimatetool.com/archimate" id="r1">
  <source xsi:type="archimate:ApplicationComponent" href="ApplicationComponent_a1.xml#a1"/>
  <target xsi:type="archimate:ApplicationService" href="ApplicationService_a2.xml#a2"/>
</archimate:RealizationRelationship>"#,
        )
        .unwrap();
        std::fs::write(
            model.join("diagrams/ArchimateDiagramModel_v1.xml"),
            r#"<archimate:ArchimateDiagramModel xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:archimate="http://www.archimatetool.com/archimate" name="Main View" id="v1">
  <children xsi:type="archimate:DiagramModelArchimateObject" id="n1" targetConnections="c1">
    <bounds x="100" y="50" width="120" height="55"/>
    <archimateElement xsi:type="archimate:ApplicationComponent" href="ApplicationComponent_a1.xml#a1"/>
  </children>
  <children xsi:type="archimate:DiagramModelArchimateObject" id="grp">
    <bounds x="300" y="200" width="200" height="150"/>
    <children xsi:type="archimate:DiagramModelArchimateObject" id="n2">
      <sourceConnections xsi:type="archimate:DiagramModelArchimateConnection" id="c1" source="n2" target="n1">
        <archimateRelationship xsi:type="archimate:RealizationRelationship" href="RealizationRelationship_r1.xml#r1"/>
      </sourceConnections>
      <bounds x="10" y="20" width="120" height="55"/>
      <archimateElement xsi:type="archimate:ApplicationService" href="ApplicationService_a2.xml#a2"/>
    </children>
  </children>
</archimate:ArchimateDiagramModel>"#,
        )
        .unwrap();
    }

    #[test]
    fn a_coarchi_folder_places_a_nested_node_relative_to_its_parent() {
        let mut root = std::env::temp_dir();
        root.push(format!("vim-shapes-coarchi-{}", std::process::id()));
        write_coarchi_fixture(&root);

        // The repository root (no folder.xml directly in it) and its `model` subfolder both
        // resolve to the same model — whichever one a user happens to type.
        let (ws, tree) = import_coarchi(&root).expect("imports from the repo root");
        let (ws2, tree2) = import_coarchi(&root.join("model")).expect("imports from the model folder directly");
        std::fs::remove_dir_all(&root).ok();

        for (ws, tree) in [(ws, tree), (ws2, tree2)] {
            assert_eq!(ws.tabs.len(), 1);
            assert_eq!(ws.tabs[0].name, "Main View");
            let doc = &ws.tabs[0].diagram;
            assert_eq!(doc.elements.len(), 2);
            let crm = doc.elements.iter().find(|e| e.label == "CRM").expect("CRM");
            assert_eq!((crm.x, crm.y), (10.0, 2.5), "n1's bounds are already relative to the page");
            let contacts = doc.elements.iter().find(|e| e.label == "Contacts").expect("Contacts");
            assert_eq!((contacts.x, contacts.y), (31.0, 11.0), "n2's (10,20) plus its parent's (300,200), in cells");
            assert_eq!(doc.relations.len(), 1);
            assert_eq!(doc.relations[0].kind, RK::Realization, "the suffix on Archi's own relationship type is stripped");

            // The tree has only what leads to a view: `application` and `relations` hold no
            // views of their own, so only `diagrams` — carrying the one view — shows up.
            assert_eq!(tree.len(), 1);
            match &tree[0] {
                ModelNode::Folder { name, children, .. } => {
                    assert_eq!(name, "diagrams");
                    assert_eq!(children.len(), 1);
                    match &children[0] {
                        ModelNode::View { name, tab_index } => {
                            assert_eq!(name, "Main View");
                            assert_eq!(*tab_index, 0);
                        }
                        other => panic!("expected a view, got {other:?}"),
                    }
                }
                other => panic!("expected a folder, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_folder_with_no_folder_xml_is_not_a_coarchi_model() {
        let mut root = std::env::temp_dir();
        root.push(format!("vim-shapes-not-coarchi-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let result = import_coarchi(&root);
        std::fs::remove_dir_all(&root).ok();
        match result {
            Err(ImportError::Invalid(why)) => assert!(why.contains("folder.xml"), "{why}"),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
