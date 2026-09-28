//! The tests that keep the ontology honest.
//!
//! One rule: **the ontology may not describe a diagram other than the one the app can draw.**
//! Every kind is walked; every one must be described; every idiom must build and lint clean;
//! and the emitted spec must be the same bytes twice.

use super::*;
use serde_json::Value;
use crate::ontology::ShapeKind::*;
use crate::ontology::RelationKind::*;

#[test]
fn every_element_kind_is_described() {
    for k in ShapeKind::ALL {
        assert!(!k.tagline().is_empty(), "{} has no tagline", k.name());
        assert!(!k.summary().is_empty(), "{} has no summary", k.name());
        assert!(!k.short().is_empty(), "{} has no short tag for its card", k.name());
    }
}

#[test]
fn every_tagline_fits_one_palette_row() {
    for k in ShapeKind::ALL {
        let t = k.tagline();
        assert!(!t.contains('\n'), "{}'s tagline wraps: {t:?}", k.name());
        assert!(t.chars().count() <= 84, "{}'s tagline is too long for a row: {t:?}", k.name());
    }
    for r in RelationKind::ALL {
        assert!(r.tagline().chars().count() <= 84, "{}'s tagline is too long", r.name());
    }
    for v in View::ALL {
        assert!(v.tagline().chars().count() <= 100, "{}'s tagline is too long", v.name());
    }
}

#[test]
fn short_tags_fit_inside_their_own_card() {
    // The tag sits on the first row inside the outline, a cell of margin either side.
    for k in ShapeKind::ALL {
        let (w, _) = k.default_size();
        assert!(k.short().chars().count() as f64 <= w - 3.0, "{}'s tag {:?} will not fit its card", k.name(), k.short());
    }
}

#[test]
fn slugs_are_unique_and_resolve_back_to_their_kind() {
    for k in ShapeKind::ALL {
        assert_eq!(ShapeKind::parse(k.slug()), Some(k), "{} does not round-trip", k.slug());
        assert_eq!(ShapeKind::parse(k.name()), Some(k), "{} by full name", k.name());
        let dup = ShapeKind::ALL.iter().filter(|o| o.slug() == k.slug()).count();
        assert_eq!(dup, 1, "{} is used twice", k.slug());
    }
    assert_eq!(ShapeKind::parse("comp"), Some(ApplicationComponent), "a unique prefix resolves");
    assert_eq!(ShapeKind::parse("app-"), None, "an ambiguous prefix does not");
}

#[test]
fn every_kind_lists_itself_exactly_once() {
    assert_eq!(ShapeKind::ALL.len(), 108);
    for (i, k) in ShapeKind::ALL.iter().enumerate() {
        assert!(!ShapeKind::ALL[i + 1..].contains(k), "{} appears twice", k.name());
    }
}

#[test]
fn the_palette_order_is_by_layer() {
    let mut last = Layer::Motivation;
    for k in ShapeKind::ALL {
        assert!(k.layer() >= last, "{} is filed out of layer order", k.name());
        last = k.layer();
    }
}

#[test]
fn the_rules_say_what_an_architect_would_expect() {
    let ok = |r, a, b| assert_eq!(allowed(r, a, b), Ok(()), "{a:?} {} {b:?} should be allowed", RelationKind::verb(r));
    let no = |r, a, b| assert!(allowed(r, a, b).is_err(), "{a:?} {} {b:?} should be refused", RelationKind::verb(r));

    ok(Realization, BusinessProcess, BusinessService);
    ok(Realization, ApplicationComponent, ApplicationService);
    ok(Realization, ApplicationComponent, BusinessProcess);
    ok(Realization, Node, ApplicationComponent);
    ok(Realization, DataObject, BusinessObject);
    ok(Realization, ApplicationComponent, Requirement);
    ok(Realization, Capability, Requirement);
    no(Realization, BusinessProcess, ApplicationComponent);
    no(Realization, BusinessService, BusinessProcess);

    ok(Serving, ApplicationService, BusinessProcess);
    ok(Serving, TechnologyService, ApplicationComponent);
    ok(Serving, BusinessService, BusinessActor);
    no(Serving, BusinessService, ApplicationComponent);
    no(Serving, BusinessObject, BusinessProcess);
    no(Serving, ApplicationService, Requirement);

    ok(Assignment, BusinessActor, BusinessRole);
    ok(Assignment, BusinessRole, BusinessProcess);
    ok(Assignment, ApplicationComponent, ApplicationFunction);
    ok(Assignment, Node, Artifact);
    no(Assignment, BusinessProcess, BusinessProcess);
    no(Assignment, ApplicationComponent, BusinessProcess);

    ok(Access, BusinessProcess, BusinessObject);
    ok(Access, ApplicationFunction, DataObject);
    ok(Access, ApplicationComponent, DataObject);
    no(Access, ApplicationComponent, BusinessObject);
    no(Access, BusinessProcess, BusinessProcess);

    ok(Composition, BusinessProcess, BusinessProcess);
    ok(Composition, ApplicationComponent, ApplicationComponent);
    ok(Aggregation, Grouping, Node);
    ok(Aggregation, Plateau, ApplicationComponent);
    no(Composition, BusinessProcess, Node);
    no(Composition, ApplicationComponent, ApplicationProcess);
    no(Composition, Goal, Goal);
    ok(Aggregation, Goal, Goal);

    ok(Triggering, BusinessEvent, BusinessProcess);
    ok(Flow, ApplicationComponent, ApplicationComponent);
    no(Triggering, BusinessProcess, ApplicationProcess);
    no(Flow, BusinessProcess, BusinessObject);

    ok(Influence, Assessment, Goal);
    ok(Influence, CourseOfAction, Goal);
    no(Influence, ApplicationComponent, Goal);

    ok(Specialization, BusinessActor, BusinessActor);
    no(Specialization, BusinessActor, BusinessRole);

    // Plain shapes: a line, an arrow, part-of — and nothing the ontology would have to vouch for.
    ok(Association, Box, ApplicationComponent);
    ok(Flow, Box, Circle);
    ok(Flow, Text, ApplicationComponent);
    ok(Composition, Box, Box);
    ok(Aggregation, Grouping, Box);
    no(Realization, Box, BusinessService);
    no(Serving, Cylinder, ApplicationComponent);
    no(Access, ApplicationFunction, Cylinder);

    ok(Realization, WorkPackage, Deliverable);
    ok(Realization, Deliverable, Plateau);
    ok(Triggering, WorkPackage, ImplementationEvent);
}

#[test]
fn every_refusal_carries_a_reason_someone_can_act_on() {
    for r in RelationKind::ALL {
        for a in ShapeKind::ALL {
            for b in ShapeKind::ALL {
                if let Err(why) = allowed(r, a, b) {
                    assert!(why.len() > 20, "{r:?} {a:?}→{b:?}: {why:?} teaches nothing");
                }
            }
        }
    }
}

#[test]
fn every_element_kind_can_take_at_least_one_specific_relation_somewhere() {
    // A kind that could only ever be associated would be a kind the rules had forgotten.
    for a in ShapeKind::ALL {
        let some = ShapeKind::ALL.iter().any(|b| {
            RelationKind::ALL.iter().any(|r| !matches!(r, Association | Link) && allowed(*r, a, *b).is_ok())
        }) || ShapeKind::ALL.iter().any(|b| {
            RelationKind::ALL.iter().any(|r| !matches!(r, Association | Link) && allowed(*r, *b, a).is_ok())
        });
        assert!(some, "{} can only ever be associated", a.name());
    }
}

#[test]
fn every_idiom_builds_and_lints_clean() {
    for i in idiom::IDIOMS {
        let doc = idiom::build(i);
        assert_eq!(doc.elements.len(), i.elements.len(), "idiom {}", i.name);
        assert_eq!(doc.relations.len(), i.relations.len(), "idiom {}", i.name);
        let problems = doc.lint();
        assert!(
            problems.is_empty(),
            "idiom {} teaches a refused relation: {:?}",
            i.name,
            problems.iter().map(|p| p.why).collect::<Vec<_>>()
        );
        assert!(
            i.relations.iter().any(|(k, _, _)| !matches!(k, Association | Link)),
            "idiom {} says nothing — every relation is an association",
            i.name
        );
        // Laid out, nothing overlaps.
        for a in &doc.elements {
            for b in &doc.elements {
                if a.id < b.id {
                    let apart = a.right() <= b.x || b.right() <= a.x || a.bottom() <= b.y || b.bottom() <= a.y;
                    assert!(apart, "idiom {}: {} overlaps {}", i.name, a.display(), b.display());
                }
            }
        }
    }
}

#[test]
fn idioms_have_unique_names_that_resolve() {
    for i in idiom::IDIOMS {
        assert_eq!(idiom::find(i.name).map(|f| f.name), Some(i.name));
        assert_eq!(idiom::IDIOMS.iter().filter(|o| o.name == i.name).count(), 1);
    }
}

#[test]
fn the_spec_is_the_same_bytes_twice_and_sorted() {
    let a = serde_json::to_string(&emit::spec()).unwrap();
    let b = serde_json::to_string(&emit::spec()).unwrap();
    assert_eq!(a, b);
    let Value::Object(top) = emit::spec() else { panic!("an object") };
    let keys: Vec<&String> = top.keys().collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "keys are sorted, so the prompt cache hits");
    assert!(top.contains_key("elements") && top.contains_key("relations") && top.contains_key("rules"));
}

#[test]
fn the_spec_tabulates_the_same_rules_the_app_enforces() {
    let spec = emit::spec();
    let rels = spec["relations"].as_array().unwrap();
    let comp = rels.iter().find(|r| r["kind"] == "composition").unwrap();
    let targets = comp["allowed"]["process"].as_array().unwrap();
    assert!(targets.iter().any(|t| t == "process"));
    assert!(!targets.iter().any(|t| t == "node"));
}

#[test]
fn the_spec_names_the_wire_form_next_to_the_slug_so_a_file_need_not_be_guessed() {
    // A file's own `"kind"` is the serde name (`application_component`), not the short slug
    // `kind` gives (`component`) — writing the slug into a file is a runtime error away from
    // being caught. `wire_kind` says the real string up front.
    let spec = emit::spec();
    let elements = spec["elements"].as_array().unwrap();
    let component = elements.iter().find(|e| e["kind"] == "component" && e["layer"] == "application").unwrap();
    assert_eq!(component["wire_kind"], "application_component");
    let app_function = elements.iter().find(|e| e["kind"] == "app-function").unwrap();
    assert_eq!(app_function["wire_kind"], "application_function");
    let relations = spec["relations"].as_array().unwrap();
    let link_type = relations.iter().find(|r| r["kind"] == "link-type").unwrap();
    assert_eq!(link_type["wire_kind"], "link_type");
    for e in elements {
        let k: ShapeKind = serde_json::from_value(e["wire_kind"].clone()).expect("wire_kind must parse back to a ShapeKind");
        assert_eq!(k.slug(), e["kind"].as_str().unwrap(), "wire_kind must round-trip to the same kind slug");
    }
}

#[test]
fn the_spec_documents_the_fields_that_take_no_kind_of_their_own() {
    // `properties`/parameters and the three label fields are real JSON structure that no
    // element/relation kind entry otherwise mentions — undocumented until read out of
    // model.rs, which is exactly the gap this closes.
    let spec = emit::spec();
    let object_type = spec["elements"].as_array().unwrap().iter().find(|e| e["wire_kind"] == "object_type").unwrap();
    assert_eq!(object_type["rows_field"], "properties");
    let action_type = spec["elements"].as_array().unwrap().iter().find(|e| e["wire_kind"] == "action_type").unwrap();
    assert_eq!(action_type["rows_field"], "properties");
    let component = spec["elements"].as_array().unwrap().iter().find(|e| e["wire_kind"] == "application_component").unwrap();
    assert!(component.get("rows_field").is_none(), "a component has no rows");

    let prop_fields: Vec<&str> = spec["property_fields"]["fields"].as_array().unwrap().iter().map(|f| f["field"].as_str().unwrap()).collect();
    for name in ["name", "type", "primary_key", "title", "array", "required", "shared", "api_name", "value_type", "description"] {
        assert!(prop_fields.contains(&name), "property_fields is missing `{name}`");
    }
    let rel_fields: Vec<&str> = spec["relation_fields"]["fields"].as_array().unwrap().iter().map(|f| f["field"].as_str().unwrap()).collect();
    assert_eq!(rel_fields, ["label", "tail_label", "head_label"]);
}

#[test]
fn the_bundled_example_help_points_at_shows_what_help_claims_it_shows() {
    let ws = crate::persistence::load(std::path::Path::new("examples/ontology-schema.json")).expect("the example --help points at must parse");
    let d = &ws.tabs[0].diagram;
    assert!(d.elements.iter().any(|e| e.kind == ObjectType && !e.properties.is_empty()), "an object type with properties");
    assert!(d.elements.iter().any(|e| e.kind == Interface), "an interface");
    assert!(
        d.relations.iter().any(|r| r.kind == LinkType && r.label.as_deref().map(|l| !l.is_empty()).unwrap_or(false)),
        "a named link type"
    );
}

#[test]
fn a_view_narrows_the_palette_and_free_does_not() {
    assert!(View::Technology.shows(Node));
    assert!(!View::Technology.shows(BusinessActor));
    assert!(View::Technology.shows(Grouping), "a box is never out of place");
    assert!(View::Business.shows(Text), "and neither is a note");
    for k in ShapeKind::ALL {
        assert!(View::Free.shows(k), "the architecture shows every layer, the ontology's included: {}", k.name());
        assert_eq!(View::Freeform.shows(k), k.is_sketch() || k.layer() == Layer::Composite, "{}", k.name());
        assert_eq!(View::Ontology.shows(k), matches!(k.layer(), Layer::Ontology | Layer::Business | Layer::Application | Layer::Composite | Layer::Sketch), "{}", k.name());
    }
}

#[test]
fn the_ontology_view_has_its_own_grammar_in_foundry_s_words() {
    let ok = |r, a, b| assert_eq!(allowed(r, a, b), Ok(()), "{a:?} {} {b:?} should be allowed", RelationKind::verb(r));
    let no = |r, a, b| assert!(allowed(r, a, b).is_err(), "{a:?} {} {b:?} should be refused", RelationKind::verb(r));
    ok(LinkType, ObjectType, ObjectType);
    ok(LinkType, ObjectType, Interface);
    no(LinkType, ObjectType, ActionType);
    ok(Implements, ObjectType, Interface);
    no(Implements, Interface, ObjectType);
    ok(Extends, Interface, Interface);
    no(Extends, ObjectType, Interface);
    for verb in [Creates, Modifies, Deletes, Links, Unlinks] {
        ok(verb, ActionType, ObjectType);
        ok(verb, ActionType, Interface);
        no(verb, ObjectType, ObjectType);
    }
    ok(Calls, ActionType, Function);
    no(Calls, ObjectType, Function);
    ok(Uses, ObjectType, SharedProperty);
    ok(Uses, SharedProperty, ValueType);
    no(Uses, SharedProperty, ObjectType);
    ok(BackedBy, ObjectType, Datasource);
    no(BackedBy, Datasource, ObjectType);
    ok(Aggregation, ObjectTypeGroup, ObjectType);
    no(Composition, ObjectType, ObjectType);
    no(Specialization, ObjectType, ObjectType);
    ok(Association, ObjectType, ActionType);
    ok(Link, ObjectType, Box);
    // The two vocabularies do not mix, and each refusal says which side to fix.
    assert!(allowed(Realization, ObjectType, Interface).unwrap_err().contains("architecture relation"), "between two ontology types the architecture's words are refused");
    assert!(allowed(LinkType, ApplicationComponent, DataObject).unwrap_err().contains("ontology relation"));
    assert!(allowed(LinkType, ObjectType, Box).unwrap_err().contains("ontology relation"));
    assert_eq!(allowed_kinds(ObjectType, ObjectType)[0], LinkType, "the link type is the first thing offered between two object types");
    assert_eq!(allowed_kinds(ActionType, ObjectType)[0], Creates);
    assert_eq!(LinkType.notation().tail, End::Bar);
    assert_eq!(LinkType.notation().head, End::Crow);
    assert_eq!(RelationKind::parse("link-t"), Some(LinkType));
    assert_eq!(ShapeKind::parse("object_type"), Some(ObjectType));
    assert_eq!(ShapeKind::parse("object_t"), None, "object type or object type group: ambiguous");
    assert_eq!(View::parse("ont"), Some(View::Ontology));
    assert_eq!(View::Ontology.badge(), "architecture · ontology");
    // The seam: where the ontology meets the architecture it sits in.
    ok(Realization, ObjectType, BusinessObject);
    ok(Realization, Datasource, DataObject);
    no(Realization, ObjectType, DataObject);
    no(Realization, DataObject, ObjectType);
    ok(Access, ApplicationComponent, ObjectType);
    ok(Access, ApplicationFunction, Datasource);
    no(Access, BusinessProcess, ObjectType);
    ok(Serving, ActionType, BusinessProcess);
    ok(Serving, Function, ApplicationComponent);
    no(Serving, ObjectType, BusinessProcess);
    ok(Aggregation, Grouping, ObjectType);
    no(Flow, ApplicationComponent, ObjectType);
    ok(Association, Node, ObjectType);
}

#[test]
fn the_alignment_layers_ground_object_types_in_the_common_core() {
    let ok = |r, a, b| assert_eq!(allowed(r, a, b), Ok(()), "{a:?} {} {b:?} should be allowed", RelationKind::verb(r));
    let no = |r, a, b| assert!(allowed(r, a, b).is_err(), "{a:?} {} {b:?} should be refused", RelationKind::verb(r));
    // The upper ontology's own hierarchy: each class has one real parent.
    ok(SubsumedBy, BfoMaterialEntity, BfoIndependentContinuant);
    no(SubsumedBy, BfoMaterialEntity, BfoContinuant);
    ok(SubsumedBy, BfoRole, BfoRealizableEntity);
    no(SubsumedBy, BfoRole, BfoDisposition);
    ok(SubsumedBy, BfoFunction, BfoDisposition);
    // The common core's own hierarchy, and its grounding in BFO.
    ok(SubsumedBy, CcoPerson, CcoAgent);
    no(SubsumedBy, CcoPerson, BfoMaterialEntity);
    ok(SubsumedBy, CcoAgent, BfoMaterialEntity);
    ok(SubsumedBy, CcoGeospatialRegion, BfoImmaterialEntity);
    no(SubsumedBy, CcoGeospatialRegion, BfoMaterialEntity);
    ok(SubsumedBy, CcoFacility, CcoArtifact);
    // The seam: an object type or interface reaches the common core, never the upper ontology
    // straight, and nothing else in the ontology layer reaches either.
    ok(SubsumedBy, ObjectType, CcoPerson);
    ok(SubsumedBy, Interface, CcoAgent);
    assert!(allowed(SubsumedBy, ObjectType, BfoMaterialEntity).unwrap_err().contains("common core"), "no straight line to the top");
    no(SubsumedBy, Datasource, CcoPerson);
    no(SubsumedBy, BusinessObject, CcoPerson);
    no(SubsumedBy, ApplicationComponent, BfoMaterialEntity);
    // Nothing but subsumed-by, association and link mean anything up here.
    no(Realization, ObjectType, CcoPerson);
    no(Access, ApplicationComponent, CcoPerson);
    ok(Association, ObjectType, CcoPerson);
    ok(Link, CcoPerson, BfoMaterialEntity);
    // The flagship case: two object types from different sources, interoperable because both
    // ground in the same common-core class.
    ok(SubsumedBy, ObjectType, CcoPerson);
    assert_eq!(RelationKind::parse("subsumed-by"), Some(SubsumedBy));
    assert_eq!(View::parse("align"), Some(View::Alignment));
    assert_eq!(View::Alignment.badge(), "architecture · alignment");
    assert_eq!(SubsumedBy.notation().head, End::Triangle);
}

#[test]
fn an_object_type_group_is_a_composite_that_holds_and_moves_its_types() {
    let mut doc = crate::model::Document::default();
    let g = doc.add(ObjectTypeGroup, "Logistics", 0.0, 0.0);
    let a = doc.add(ObjectType, "Truck", 4.0, 3.0);
    let outside = doc.add(ObjectType, "Invoice", 60.0, 3.0);
    assert!(ObjectTypeGroup.is_composite() && Grouping.is_composite() && !ObjectType.is_composite());
    assert_eq!(doc.members(g), vec![a], "what sits inside belongs to it");
    assert!(!doc.members(g).contains(&outside));
    assert!(!doc.element(g).unwrap().casts_shadow(), "a boundary has no body");
}

#[test]
fn the_check_report_names_what_is_wrong() {
    let mut doc = crate::model::Document::default();
    let p = doc.add(BusinessProcess, "Pay", 0.0, 0.0);
    let n = doc.add(Node, "db01", 30.0, 0.0);
    doc.connect(Composition, p, n).unwrap();
    doc.metadata.view = View::Business;
    let r = emit::report(&doc);
    assert!(r.contains("Pay is composed of db01"), "{r}");
    assert!(r.contains("db01 is Node — outside a business view"), "{r}");
    let service = idiom::IDIOMS.iter().find(|i| i.name == "service").unwrap();
    let clean = emit::report(&idiom::build(service));
    assert!(clean.starts_with("ok:"), "{clean}");
    // An ontology idiom is a start, not a schema: the report says what is still to do.
    let started = emit::report(&idiom::build(&idiom::IDIOMS[0]));
    assert!(started.contains("ontology: Airport has no properties"), "{started}");
}

#[test]
fn the_relations_run_family_by_family_with_the_catch_alls_last() {
    // The picker and the palette put a heading wherever the family changes, so a family
    // split across ALL would be headed twice — and its halves read as two families.
    let order: Vec<usize> = RelationKind::ALL.iter().map(|r| Family::ALL.iter().position(|f| *f == r.family()).unwrap()).collect();
    assert!(order.windows(2).all(|w| w[0] <= w[1]), "ALL is out of family order: {order:?}");
    assert_eq!(RelationKind::ALL[RelationKind::ALL.len() - 2..], [Association, Link], "the two that say least, last");
}

#[test]
fn an_ontology_shape_suggests_its_schema_and_not_what_fans_out() {
    let s = suggestions(ObjectType, View::Ontology);
    assert!(s.contains(&(Implements, Interface)) && s.contains(&(BackedBy, Datasource)) && s.contains(&(LinkType, ObjectType)));
    assert!(s.iter().all(|(r, k)| r.is_specific() && allowed(*r, ObjectType, *k).is_ok()), "only allowed, specific lines");
    // A function serves any behaviour of two layers: allowed, offered below, but not a suggestion.
    assert!(allowed(Serving, Function, BusinessProcess).is_ok());
    assert!(suggestions(Function, View::Ontology).is_empty());
    assert!(suggestions(Grouping, View::Free).iter().all(|(r, _)| *r != Aggregation), "a composite aggregates anything");
}
