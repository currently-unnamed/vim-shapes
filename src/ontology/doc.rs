//! The ontology view's documentation: the diagram as a Markdown reference and as a plain
//! definition, and the checks a schema has to pass.
//!
//! The pictures show the ontology; this is what a team's wiki page about it says, generated
//! from the diagram so it cannot drift from the picture. Every name is Foundry's own, and the
//! definition's keys are the SDK's — `apiName`, `primaryKey` — so a script reads it straight.

use super::{RelationKind, ShapeKind};
use crate::model::{Document, Element, Relation, Status};
use serde_json::{json, Map, Value};
use std::fmt::Write as _;

/// Foundry's words for a link's cardinality, read off the ends the way an ER diagram does.
fn cardinality(r: &Relation) -> (&'static str, &'static str) {
    use super::End;
    let side = |e: End| if e == End::Crow { "many" } else { "one" };
    let n = r.notation();
    (side(n.tail), side(n.head))
}

fn api(e: &Element) -> String {
    e.api_name.clone().unwrap_or_else(|| {
        let mut out = String::new();
        for (i, w) in e.label.split_whitespace().enumerate() {
            let mut cs = w.chars();
            if let Some(c) = cs.next() {
                if i == 0 { out.extend(c.to_lowercase()) } else { out.extend(c.to_uppercase()) }
                out.push_str(cs.as_str());
            }
        }
        out
    })
}

fn of_kind(doc: &Document, k: ShapeKind) -> Vec<&Element> {
    doc.elements_in_order().into_iter().filter(|e| e.kind == k).collect()
}

fn relations_of(doc: &Document, id: u32, kind: RelationKind) -> Vec<&Relation> {
    doc.relations.iter().filter(|r| r.kind == kind && (r.from == id || r.to == id)).collect()
}

fn name(doc: &Document, id: u32) -> String {
    doc.element(id).map(|e| e.display()).unwrap_or_else(|| "?".into())
}

/// The reference, as a wiki page: object types first, then interfaces, actions, and the
/// value types and shared properties last.
pub fn markdown(doc: &Document) -> String {
    let mut out = String::new();
    let title = doc.metadata.title.clone().unwrap_or_else(|| "Ontology".into());
    let _ = writeln!(out, "# {title}\n");
    let status = |s: Status| if s == Status::Active { String::new() } else { format!(" *({})*", s.name()) };

    let objects = of_kind(doc, ShapeKind::ObjectType);
    if !objects.is_empty() {
        let _ = writeln!(out, "## Object types\n");
    }
    for e in &objects {
        let _ = writeln!(out, "### {}{}\n", e.display(), status(e.status));
        let _ = writeln!(out, "- API name: `{}`", api(e));
        if let Some(p) = &e.plural {
            let _ = writeln!(out, "- Plural: {p}");
        }
        if let Some(pk) = e.properties.iter().find(|p| p.primary_key) {
            let _ = writeln!(out, "- Primary key: `{}`", pk.name);
        }
        if let Some(t) = e.properties.iter().find(|p| p.title) {
            let _ = writeln!(out, "- Title: `{}`", t.name);
        }
        for r in relations_of(doc, e.id, RelationKind::BackedBy) {
            let _ = writeln!(out, "- Backed by: {}", name(doc, r.to));
        }
        let groups: Vec<String> = doc.elements.iter().filter(|g| g.kind == ShapeKind::ObjectTypeGroup && doc.members(g.id).contains(&e.id)).map(|g| g.display()).collect();
        if !groups.is_empty() {
            let _ = writeln!(out, "- Groups: {}", groups.join(", "));
        }
        out.push('\n');
        if !e.properties.is_empty() {
            let _ = writeln!(out, "| property | type | key | title | shared | value type |");
            let _ = writeln!(out, "| --- | --- | --- | --- | --- | --- |");
            for p in &e.properties {
                let ty = format!("{}{}", p.base_type.name(), if p.array { "[]" } else { "" });
                let _ = writeln!(out, "| `{}` | {} | {} | {} | {} | {} |", p.name, ty, if p.primary_key { "⚿" } else { "" }, if p.title { "✎" } else { "" }, if p.shared { "✱" } else { "" }, p.value_type.clone().unwrap_or_default());
            }
            out.push('\n');
        }
        let links = relations_of(doc, e.id, RelationKind::LinkType);
        if !links.is_empty() {
            let _ = writeln!(out, "Link types:\n");
            for r in links {
                let (tail, head) = cardinality(r);
                let (other, here_card, there_card, side_api) = if r.from == e.id { (r.to, tail, head, r.head_label.clone()) } else { (r.from, head, tail, r.tail_label.clone()) };
                let _ = writeln!(out, "- **{}** — {} {} to {} {}{}", r.label.clone().unwrap_or_else(|| name(doc, other)), here_card, e.display(), there_card, name(doc, other), side_api.map(|a| format!(" (`{a}`)")).unwrap_or_default());
            }
            out.push('\n');
        }
        let ifaces: Vec<String> = relations_of(doc, e.id, RelationKind::Implements).iter().filter(|r| r.from == e.id).map(|r| name(doc, r.to)).collect();
        if !ifaces.is_empty() {
            let _ = writeln!(out, "Implements: {}\n", ifaces.join(", "));
        }
        let mut verbs: Vec<String> = Vec::new();
        for r in doc.relations.iter().filter(|r| r.to == e.id && matches!(r.kind, RelationKind::Creates | RelationKind::Modifies | RelationKind::Deletes | RelationKind::Links | RelationKind::Unlinks)) {
            verbs.push(format!("{} {}", name(doc, r.from), r.kind.verb()));
        }
        if !verbs.is_empty() {
            let _ = writeln!(out, "Actions: {}\n", verbs.join("; "));
        }
    }

    let ifaces = of_kind(doc, ShapeKind::Interface);
    if !ifaces.is_empty() {
        let _ = writeln!(out, "## Interfaces\n");
    }
    for e in &ifaces {
        let _ = writeln!(out, "### {}{}\n", e.display(), status(e.status));
        let _ = writeln!(out, "- API name: `{}`", api(e));
        let parents: Vec<String> = relations_of(doc, e.id, RelationKind::Extends).iter().filter(|r| r.from == e.id).map(|r| name(doc, r.to)).collect();
        if !parents.is_empty() {
            let _ = writeln!(out, "- Extends: {}", parents.join(", "));
        }
        let impls: Vec<String> = relations_of(doc, e.id, RelationKind::Implements).iter().filter(|r| r.to == e.id).map(|r| name(doc, r.from)).collect();
        if !impls.is_empty() {
            let _ = writeln!(out, "- Implemented by: {}", impls.join(", "));
        }
        out.push('\n');
        if !e.properties.is_empty() {
            let _ = writeln!(out, "| property | type | shared | value type |");
            let _ = writeln!(out, "| --- | --- | --- | --- |");
            for p in &e.properties {
                let _ = writeln!(out, "| `{}` | {}{} | {} | {} |", p.name, p.base_type.name(), if p.array { "[]" } else { "" }, if p.shared { "✱" } else { "" }, p.value_type.clone().unwrap_or_default());
            }
            out.push('\n');
        }
    }

    let actions = of_kind(doc, ShapeKind::ActionType);
    if !actions.is_empty() {
        let _ = writeln!(out, "## Action types\n");
    }
    for e in &actions {
        let _ = writeln!(out, "### {}{}\n", e.display(), status(e.status));
        let _ = writeln!(out, "- API name: `{}`", api(e));
        out.push('\n');
        if !e.properties.is_empty() {
            let _ = writeln!(out, "| parameter | type | required |");
            let _ = writeln!(out, "| --- | --- | --- |");
            for p in &e.properties {
                let _ = writeln!(out, "| `{}` | {}{} | {} |", p.name, p.base_type.name(), if p.array { "[]" } else { "" }, if p.required { "yes" } else { "" });
            }
            out.push('\n');
        }
        let rules: Vec<String> = doc.relations.iter().filter(|r| r.from == e.id && r.kind.is_ontology()).map(|r| format!("{} {}", r.kind.verb(), name(doc, r.to))).collect();
        if !rules.is_empty() {
            let _ = writeln!(out, "Rules: {}\n", rules.join("; "));
        }
    }

    for (kind, heading) in [(ShapeKind::ValueType, "Value types"), (ShapeKind::SharedProperty, "Shared properties"), (ShapeKind::Function, "Functions"), (ShapeKind::Datasource, "Datasources")] {
        let es = of_kind(doc, kind);
        if es.is_empty() {
            continue;
        }
        let _ = writeln!(out, "## {heading}\n");
        for e in es {
            let _ = writeln!(out, "- **{}**{}", e.display(), status(e.status));
        }
        out.push('\n');
    }
    out
}

/// The diagram as a plain definition, in the SDK's casing.
pub fn definition(doc: &Document) -> Value {
    let mut top = Map::new();
    top.insert("title".into(), json!(doc.metadata.title));
    let props = |e: &Element| -> Value {
        e.properties
            .iter()
            .map(|p| {
                let mut m = Map::new();
                m.insert("displayName".into(), json!(p.name));
                m.insert("apiName".into(), json!(p.api_name.clone().unwrap_or_else(|| p.name.clone())));
                m.insert("baseType".into(), json!(p.base_type.name()));
                if p.array {
                    m.insert("array".into(), json!(true));
                }
                if p.primary_key {
                    m.insert("primaryKey".into(), json!(true));
                }
                if p.title {
                    m.insert("title".into(), json!(true));
                }
                if p.shared {
                    m.insert("shared".into(), json!(true));
                }
                if p.required {
                    m.insert("required".into(), json!(true));
                }
                if let Some(v) = &p.value_type {
                    m.insert("valueType".into(), json!(v));
                }
                if p.status != Status::Active {
                    m.insert("status".into(), json!(p.status.name()));
                }
                Value::Object(m)
            })
            .collect()
    };
    let head = |e: &Element| -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("displayName".into(), json!(e.label));
        m.insert("apiName".into(), json!(api(e)));
        if let Some(p) = &e.plural {
            m.insert("pluralDisplayName".into(), json!(p));
        }
        m.insert("status".into(), json!(e.status.name()));
        m.insert("visibility".into(), json!(e.visibility.name()));
        m
    };
    top.insert(
        "objectTypes".into(),
        of_kind(doc, ShapeKind::ObjectType)
            .into_iter()
            .map(|e| {
                let mut m = head(e);
                m.insert("primaryKey".into(), json!(e.properties.iter().find(|p| p.primary_key).map(|p| p.api_name.clone().unwrap_or_else(|| p.name.clone()))));
                m.insert("titleProperty".into(), json!(e.properties.iter().find(|p| p.title).map(|p| p.api_name.clone().unwrap_or_else(|| p.name.clone()))));
                m.insert("properties".into(), props(e));
                m.insert("implements".into(), relations_of(doc, e.id, RelationKind::Implements).iter().filter(|r| r.from == e.id).map(|r| json!(doc.element(r.to).map(api))).collect());
                m.insert("datasources".into(), relations_of(doc, e.id, RelationKind::BackedBy).iter().map(|r| json!(name(doc, r.to))).collect());
                Value::Object(m)
            })
            .collect(),
    );
    top.insert(
        "linkTypes".into(),
        doc.relations
            .iter()
            .filter(|r| r.kind == RelationKind::LinkType)
            .map(|r| {
                let (tail, head) = cardinality(r);
                let mut m = Map::new();
                m.insert("displayName".into(), json!(r.label));
                m.insert("from".into(), json!({ "objectType": doc.element(r.from).map(api), "cardinality": tail, "apiName": r.tail_label }));
                m.insert("to".into(), json!({ "objectType": doc.element(r.to).map(api), "cardinality": head, "apiName": r.head_label }));
                Value::Object(m)
            })
            .collect(),
    );
    top.insert(
        "interfaces".into(),
        of_kind(doc, ShapeKind::Interface)
            .into_iter()
            .map(|e| {
                let mut m = head(e);
                m.insert("properties".into(), props(e));
                m.insert("extends".into(), relations_of(doc, e.id, RelationKind::Extends).iter().filter(|r| r.from == e.id).map(|r| json!(doc.element(r.to).map(api))).collect());
                Value::Object(m)
            })
            .collect(),
    );
    top.insert(
        "actionTypes".into(),
        of_kind(doc, ShapeKind::ActionType)
            .into_iter()
            .map(|e| {
                let mut m = head(e);
                m.insert("parameters".into(), props(e));
                m.insert(
                    "rules".into(),
                    doc.relations
                        .iter()
                        .filter(|r| r.from == e.id && r.kind.is_ontology())
                        .map(|r| json!({ "rule": r.kind.name(), "target": doc.element(r.to).map(api) }))
                        .collect(),
                );
                Value::Object(m)
            })
            .collect(),
    );
    for (kind, key) in [(ShapeKind::ValueType, "valueTypes"), (ShapeKind::SharedProperty, "sharedProperties"), (ShapeKind::Function, "functions")] {
        top.insert(key.into(), of_kind(doc, kind).into_iter().map(|e| Value::Object(head(e))).collect());
    }
    Value::Object(top)
}

/// What a schema has to get right, each with the fix in words — wherever the diagram has
/// ontology types on it.
pub fn check(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    if !doc.elements.iter().any(|e| e.kind.layer() == super::Layer::Ontology) {
        return out;
    }
    for e in doc.elements_in_order() {
        let who = e.display();
        match e.kind {
            ShapeKind::ObjectType => {
                let keys = e.properties.iter().filter(|p| p.primary_key).count();
                if e.properties.is_empty() {
                    out.push(format!("{who} has no properties — give it a `properties` array (P adds one in the app)"));
                } else if keys == 0 {
                    out.push(format!("{who} has no primary key — set `primary_key: true` on one property (p on a row in the property browser)"));
                } else if keys > 1 {
                    out.push(format!("{who} has {keys} properties with `primary_key: true` — one identifies an object"));
                }
                if e.properties.iter().any(|p| p.title && !matches!(p.base_type, crate::model::BaseType::String)) {
                    out.push(format!("{who} has a `title: true` property whose `type` is not `string` — a title is what an object is shown by"));
                }
                if !e.properties.is_empty() && !e.properties.iter().any(|p| p.title) {
                    out.push(format!("{who} has no property with `title: true` — a title is what an object is shown by (l on a row in the app)"));
                }
            }
            ShapeKind::Interface => {
                if !doc.relations.iter().any(|r| r.kind == RelationKind::Implements && r.to == e.id) {
                    out.push(format!("nothing implements {who} — an interface is the shape object types share"));
                }
            }
            ShapeKind::ActionType if !doc.relations.iter().any(|r| r.from == e.id && matches!(r.kind, RelationKind::Creates | RelationKind::Modifies | RelationKind::Deletes | RelationKind::Links | RelationKind::Unlinks | RelationKind::Calls)) => {
                out.push(format!("{who} has no rule — an action creates, modifies, deletes, links or calls something"));
            }
            _ => {}
        }
    }
    for r in &doc.relations {
        if r.kind == RelationKind::LinkType && r.label.as_deref().unwrap_or("").is_empty() {
            out.push(format!("the link type between {} and {} has no `label` — that's its name (t on its centre node)", name(doc, r.from), name(doc, r.to)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BaseType, Property};
    use crate::ontology::View;

    fn sample() -> Document {
        let mut d = Document::default();
        d.metadata.view = View::Ontology;
        d.metadata.title = Some("Flights".into());
        let a = d.add(ShapeKind::ObjectType, "Airport", 0.0, 0.0);
        let f = d.add(ShapeKind::ObjectType, "Flight", 40.0, 0.0);
        let i = d.add(ShapeKind::Interface, "Place", 0.0, 20.0);
        let act = d.add(ShapeKind::ActionType, "Delay flight", 40.0, 20.0);
        let e = d.element_mut(a).unwrap();
        let mut code = Property::new("code");
        code.primary_key = true;
        let mut nm = Property::new("name");
        nm.title = true;
        nm.value_type = Some("Name".into());
        let mut loc = Property::new("location");
        loc.base_type = BaseType::Geopoint;
        e.properties = vec![code, nm, loc];
        e.api_name = Some("airport".into());
        let e = d.element_mut(act).unwrap();
        let mut minutes = Property::new("minutes");
        minutes.base_type = BaseType::Integer;
        minutes.required = true;
        e.properties = vec![minutes];
        let l = d.connect(RelationKind::LinkType, a, f).unwrap();
        let rel = d.relation_mut(l).unwrap();
        rel.label = Some("departures".into());
        rel.tail_label = Some("origin".into());
        rel.head_label = Some("departures".into());
        d.connect(RelationKind::Implements, a, i).unwrap();
        d.connect(RelationKind::Modifies, act, f).unwrap();
        d
    }

    #[test]
    fn the_markdown_reference_reads_like_a_wiki_page_in_foundry_s_words() {
        let md = markdown(&sample());
        assert!(md.starts_with("# Flights\n"));
        assert!(md.contains("## Object types\n\n### Airport\n\n- API name: `airport`\n- Primary key: `code`\n- Title: `name`"), "{md}");
        assert!(md.contains("| `location` | geopoint |"));
        assert!(md.contains("- **departures** — one Airport to many Flight (`departures`)"), "{md}");
        assert!(md.contains("Implements: Place"));
        assert!(md.contains("Actions: Delay flight modifies"));
        assert!(md.contains("## Interfaces\n\n### Place\n\n- API name: `place`\n- Implemented by: Airport"));
        assert!(md.contains("## Action types\n\n### Delay flight\n\n- API name: `delayFlight`\n\n| parameter | type | required |\n| --- | --- | --- |\n| `minutes` | integer | yes |"), "{md}");
        assert!(md.contains("Rules: modifies Flight"));
    }

    #[test]
    fn the_definition_uses_the_sdk_s_casing() {
        let v = definition(&sample());
        assert_eq!(v["objectTypes"][0]["apiName"], "airport");
        assert_eq!(v["objectTypes"][0]["primaryKey"], "code");
        assert_eq!(v["objectTypes"][0]["titleProperty"], "name");
        assert_eq!(v["objectTypes"][0]["properties"][2]["baseType"], "geopoint");
        assert_eq!(v["objectTypes"][0]["properties"][1]["valueType"], "Name");
        assert_eq!(v["objectTypes"][0]["implements"][0], "place");
        assert_eq!(v["linkTypes"][0]["from"]["cardinality"], "one");
        assert_eq!(v["linkTypes"][0]["to"]["cardinality"], "many");
        assert_eq!(v["linkTypes"][0]["to"]["apiName"], "departures");
        assert_eq!(v["actionTypes"][0]["parameters"][0]["required"], true);
        assert_eq!(v["actionTypes"][0]["rules"][0]["rule"], "modifies");
        assert_eq!(v["actionTypes"][0]["rules"][0]["target"], "flight");
    }

    #[test]
    fn the_checks_name_what_a_schema_is_missing_and_say_nothing_outside_the_view() {
        let mut d = sample();
        let warnings = check(&d);
        assert!(warnings.iter().any(|w| w.starts_with("Flight has no properties")), "{warnings:?}");
        assert!(!warnings.iter().any(|w| w.starts_with("Airport")), "{warnings:?}");
        assert!(!warnings.iter().any(|w| w.starts_with("nothing implements")), "Place is implemented");
        let f = d.elements.iter().find(|e| e.label == "Flight").unwrap().id;
        let e = d.element_mut(f).unwrap();
        let mut a = Property::new("a");
        a.primary_key = true;
        let mut b = Property::new("b");
        b.primary_key = true;
        b.title = true;
        b.base_type = BaseType::Integer;
        e.properties = vec![a, b];
        let warnings = check(&d);
        assert!(warnings.iter().any(|w| w.contains("2 properties with `primary_key: true`")), "{warnings:?}");
        assert!(warnings.iter().any(|w| w.contains("`title: true` property whose `type` is not `string`")), "{warnings:?}");
        d.metadata.view = View::Free;
        assert!(!check(&d).is_empty(), "the checks follow the types, not the view");
        assert!(check(&Document::default()).is_empty(), "and say nothing without them");
    }
}
