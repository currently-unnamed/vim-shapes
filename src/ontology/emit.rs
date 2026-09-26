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
    m.insert("name".into(), json!(k.name()));
    m.insert("layer".into(), json!(k.layer().name()));
    m.insert("category".into(), json!(k.category().name()));
    m.insert("shape".into(), json!(k.shape().name()));
    m.insert("tagline".into(), json!(k.tagline()));
    m.insert("summary".into(), json!(k.summary()));
    Value::Object(m)
}

fn relation(r: RelationKind) -> Value {
    let mut m = Map::new();
    m.insert("kind".into(), json!(r.name()));
    m.insert("verb".into(), json!(r.verb()));
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
