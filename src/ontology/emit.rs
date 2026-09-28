//! The ontology, emitted — `vim-shapes --ontology` — and a diagram, checked — `--check`.
//!
//! ## Why this is walked and not written
//!
//! A hand-written spec is a second copy of the truth, and a second copy of the truth is a lie
//! with a delay on it. Everything here is walked out of [`ShapeKind::ALL`],
//! [`RelationKind::ALL`] and [`allowed`](super::allowed) — the same tables the palette, the
//! picker and `:lint` read — so it cannot describe a kind that does not exist, cannot miss one
//! that does, and cannot state a rule the app would not enforce.
//!
//! ## Byte-stability is a feature
//!
//! The spec is meant to be the cached prefix of a prompt, and a prompt cache matches on exact
//! bytes: a key in a different order is a silent miss. So every object is a `serde_json::Map`,
//! which sorts its keys, and `preserve_order` must never be turned on. A test asserts both.

use super::{idiom, ShapeKind, Layer, RelationKind, View, RULES};
use crate::model::Document;
use serde_json::{json, Map, Value};

pub fn spec() -> Value {
    let mut top = Map::new();
    top.insert("format".into(), Value::from(1));
    top.insert(
        "layers".into(),
        Layer::ALL
            .iter()
            .map(|l| {
                let mut m = Map::new();
                m.insert("name".into(), json!(l.name()));
                m.insert("tagline".into(), json!(l.tagline()));
                m.insert("storey".into(), json!(l.storey()));
                Value::Object(m)
            })
            .collect(),
    );
    top.insert("elements".into(), ShapeKind::ALL.iter().map(|k| element(*k)).collect());
    top.insert("relations".into(), RelationKind::ALL.iter().map(|r| relation(*r)).collect());
    // The two vocabularies an element/relation kind has — `kind`, the short slug this app's
    // own commands and prose use, and `wire_kind`, the exact string a saved file's own `kind`
    // field holds — are easy to conflate into one guess. Say the file schema once, here,
    // instead of leaving it to be found by writing a guess and reading the error `--check`
    // gives back.
    top.insert(
        "property_fields".into(),
        field_schema(
            &["object_type", "interface", "action_type"],
            "the `properties` array on an element of one of these `wire_kind`s — an object type's or interface's properties, an action type's parameters",
            &[
                ("name", "string", "the property's own name — the only field with no default"),
                ("type", "string", "one of the base types (string, integer, boolean, date, ...); \"string\" if omitted"),
                ("primary_key", "boolean", "the property that identifies an object — one per object type"),
                ("title", "boolean", "the property an object is shown by — must be a string-typed property"),
                ("array", "boolean", "holds many values of `type` rather than one"),
                ("required", "boolean", "an action type's parameter that must be given"),
                ("shared", "boolean", "a shared property: one definition, used on many object types"),
                ("api_name", "string", "the name code reads this property by, if different from `name`"),
                ("value_type", "string", "a named value type this property adopts, for its constraints and meaning"),
                ("description", "string", "free text describing the property"),
            ],
        ),
    );
    top.insert(
        "relation_fields".into(),
        field_schema(
            &["every relation kind"],
            "text on the relation itself, independent of `kind` — a link type's name and its cardinality reading at each end, but present on any relation",
            &[
                ("label", "string", "the text at the relation's centre — a link type's own name"),
                ("tail_label", "string", "the text at the end nearest `from` — a link type's cardinality/role there"),
                ("head_label", "string", "the text at the end nearest `to` — a link type's cardinality/role there"),
            ],
        ),
    );
    top.insert(
        "views".into(),
        View::ALL
            .iter()
            .map(|v| {
                let mut m = Map::new();
                m.insert("name".into(), json!(v.name()));
                m.insert("tagline".into(), json!(v.tagline()));
                m.insert("layers".into(), v.layers().iter().map(|l| json!(l.name())).collect());
                Value::Object(m)
            })
            .collect(),
    );
    top.insert("rules".into(), RULES.iter().map(|r| json!(r)).collect());
    top.insert(
        "idioms".into(),
        idiom::IDIOMS
            .iter()
            .map(|i| {
                let mut m = Map::new();
                m.insert("name".into(), json!(i.name));
                m.insert("tagline".into(), json!(i.tagline));
                m.insert("story".into(), json!(i.story));
                m.insert(
                    "elements".into(),
                    i.elements
                        .iter()
                        .map(|(a, k, l)| {
                            let mut e = Map::new();
                            e.insert("alias".into(), json!(a));
                            e.insert("kind".into(), json!(k.slug()));
                            e.insert("label".into(), json!(l));
                            Value::Object(e)
                        })
                        .collect(),
                );
                m.insert(
                    "relations".into(),
                    i.relations
                        .iter()
                        .map(|(k, f, t)| {
                            let mut r = Map::new();
                            r.insert("kind".into(), json!(k.name()));
                            r.insert("from".into(), json!(f));
                            r.insert("to".into(), json!(t));
                            Value::Object(r)
                        })
                        .collect(),
                );
                Value::Object(m)
            })
            .collect(),
    );
    Value::Object(top)
}

fn element(k: ShapeKind) -> Value {
    let mut m = Map::new();
    m.insert("kind".into(), json!(k.slug()));
    // `kind` is the short slug `:add` and the manual use; `wire_kind` is the exact string a
    // saved file's own `"kind"` holds — the two differ (`component` vs `application_component`)
    // and a file written with the former fails to load with an error that names neither.
    m.insert("wire_kind".into(), serde_json::to_value(k).expect("ShapeKind serializes to a string"));
    m.insert("name".into(), json!(k.name()));
    m.insert("layer".into(), json!(k.layer().name()));
    m.insert("category".into(), json!(k.category().name()));
    m.insert("shape".into(), json!(k.shape().name()));
    m.insert("tagline".into(), json!(k.tagline()));
    m.insert("summary".into(), json!(k.summary()));
    if matches!(k, ShapeKind::ObjectType | ShapeKind::Interface | ShapeKind::ActionType) {
        // See top-level `property_fields` for what this array's own entries hold.
        m.insert("rows_field".into(), json!("properties"));
    }
    Value::Object(m)
}

fn relation(r: RelationKind) -> Value {
    let mut m = Map::new();
    m.insert("kind".into(), json!(r.name()));
    m.insert("wire_kind".into(), serde_json::to_value(r).expect("RelationKind serializes to a string"));
    m.insert("verb".into(), json!(r.verb()));
    m.insert("family".into(), json!(r.family().name()));
    m.insert("tagline".into(), json!(r.tagline()));
    m.insert("summary".into(), json!(r.summary()));
    // Which sources may take this relation to which targets — the rules, tabulated. A model
    // reads this rather than the prose, and the prose is pinned to it by the tests.
    let mut allowed = Map::new();
    for s in ShapeKind::ALL {
        let targets: Vec<Value> = ShapeKind::ALL
            .iter()
            .filter(|d| super::allowed(r, s, **d).is_ok())
            .map(|d| json!(d.slug()))
            .collect();
        if !targets.is_empty() {
            allowed.insert(s.slug().into(), Value::Array(targets));
        }
    }
    m.insert("allowed".into(), Value::Object(allowed));
    Value::Object(m)
}

/// A JSON-field schema `--ontology` documents rather than leaves to be found by writing a
/// guess and reading `--check`'s error. `applies_to` names the `wire_kind`s (or, for a
/// schema shared by every relation, says so in words) the fields below appear on.
fn field_schema(applies_to: &[&str], note: &str, fields: &[(&str, &str, &str)]) -> Value {
    let mut m = Map::new();
    m.insert("applies_to".into(), applies_to.iter().map(|a| json!(a)).collect());
    m.insert("note".into(), json!(note));
    m.insert(
        "fields".into(),
        fields
            .iter()
            .map(|(field, ty, note)| {
                let mut f = Map::new();
                f.insert("field".into(), json!(field));
                f.insert("type".into(), json!(ty));
                f.insert("note".into(), json!(note));
                Value::Object(f)
            })
            .collect(),
    );
    Value::Object(m)
}

/// A diagram, checked — what `--check` prints. Every refused relation, and every element
/// outside the diagram's view, one per line; and a last line saying so if there were none.
pub fn report(doc: &Document) -> String {
    let mut out = String::new();
    let name = |id| doc.element(id).map(|e| e.display()).unwrap_or_else(|| "?".into());
    for p in doc.lint() {
        if let Some(r) = doc.relation(p.relation) {
            out.push_str(&format!(
                "relation {}: {} {} {} — {}\n",
                r.id,
                name(r.from),
                r.kind.verb(),
                name(r.to),
                p.why
            ));
        }
    }
    for id in doc.out_of_view() {
        if let Some(e) = doc.element(id) {
            out.push_str(&format!(
                "element {}: {} is {} — outside a {} view\n",
                e.id,
                e.display(),
                e.kind.name(),
                doc.metadata.view.name()
            ));
        }
    }
    for w in super::doc::check(doc) {
        out.push_str(&format!("ontology: {w}\n"));
    }
    if out.is_empty() {
        out.push_str(&format!(
            "ok: {} elements, {} relations, nothing the rules refuse\n",
            doc.elements.len(),
            doc.relations.len()
        ));
    }
    out
}
