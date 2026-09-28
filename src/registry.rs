//! The workbench's element registry: read every diagram file under a workbench root once,
//! and compile a single cross-diagram index of every distinct element — the way Archi's own
//! model tree holds every element once, and a view merely places some of them. `ui::workbench`
//! shows this as a virtual "elements" section, browsable and pickable, alongside the real
//! folder tree; `App`'s `e` (expand) reads its edges the same way it already reads a Foundry
//! import's, through the shared `foundry_import::Edge` currency — nothing downstream of "here
//! is a `Vec<Edge>`" needs to know whether it came from one Foundry export or many diagrams.
//!
//! Identity, decided once here: two elements are the same entry if they share an `api_name`
//! (Foundry-sourced ones do) or, failing that, a display name — always namespaced by
//! `ShapeKind`, so a `Customer` object type and an unrelated `Customer` business actor are
//! never folded into one ([`Key::of`]). Two diagrams that disagree about one entry's own
//! properties are resolved silently here — the sighting with the most properties wins, first
//! seen breaks a tie — since that drift is not the "resolve the conflict" this module means:
//! only a fresh ontology `:import` disagreeing with what is already known gets the
//! interactive resolution (`ui::conflictpick`, which reads this module's `Entry` too). A
//! `:import` naming something the registry has never seen is not a conflict at all —
//! [`Registry::absorb`] adds it as a real `Entry` on the spot, `seen_in: 0`, so it appears in
//! the workbench's elements the moment the export is read, with nothing drawn yet.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::foundry_import::Edge;
use crate::model::{Document, Element, ElementId, Property, Status};
use crate::ontology::ShapeKind;
use crate::persistence;
use crate::workbench::Node;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ident {
    Api(String),
    Name(String),
}

/// An element's identity across every diagram in a workbench — see the module doc for why
/// this is `api_name`-or-name, namespaced by kind.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub kind: ShapeKind,
    pub ident: Ident,
}

impl Key {
    pub fn of(e: &Element) -> Key {
        let ident = match e.api_name.as_deref() {
            Some(a) if !a.is_empty() => Ident::Api(a.to_string()),
            _ => Ident::Name(e.display()),
        };
        Key { kind: e.kind, ident }
    }

    /// A stable string form — what goes into `App.ontology_ids` and into a
    /// `foundry_import::Edge`'s `from`/`to`, exactly parallel to a raw Foundry id, so every
    /// consumer of that id (`ontology_connections`, `add_ontology_connection`, `expandpick`)
    /// needs no change to work with a registry-sourced element too.
    pub fn token(&self) -> String {
        let (form, ident) = match &self.ident {
            Ident::Api(a) => ("api", a.as_str()),
            Ident::Name(n) => ("name", n.as_str()),
        };
        format!("registry:{}:{form}:{ident}", self.kind.slug())
    }
}

/// One distinct element, as the registry currently understands it — the representative data
/// is whichever sighting had the most properties (the most complete definition), first seen
/// breaking a tie. `seen_in` is how many diagrams draw it at all — shown in the workbench row
/// so more than one is a visible hint that it might be worth checking for drift by hand.
/// `seen_paths` is the coarser, file-level twin of that count — every distinct *file* (not
/// tab) it turns up in, sorted, for `:grep`'s "which diagrams have this" to open straight
/// from, the same granularity `workbench_open_diagram` already opens at.
#[derive(Clone, Debug)]
pub struct Entry {
    pub key: Key,
    pub name: String,
    pub api_name: Option<String>,
    pub properties: Vec<Property>,
    pub status: Status,
    pub seen_in: usize,
    pub seen_paths: Vec<PathBuf>,
}

impl Entry {
    /// Whether a `:grep` query names this entry — its label, its kind, its `api_name`, or one
    /// of its properties' names or `api_name`s, so "which diagrams touch `orderId`" answers
    /// with the object type that has it, not just an entry literally called `orderId`.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_ascii_lowercase();
        if q.is_empty() {
            return false;
        }
        self.name.to_ascii_lowercase().contains(&q)
            || self.key.kind.name().to_ascii_lowercase().contains(&q)
            || self.api_name.as_deref().is_some_and(|a| a.to_ascii_lowercase().contains(&q))
            || self.properties.iter().any(|p| p.name.to_ascii_lowercase().contains(&q) || p.api_name.as_deref().is_some_and(|a| a.to_ascii_lowercase().contains(&q)))
    }
}

pub struct Registry {
    pub entries: HashMap<Key, Entry>,
    pub edges: Vec<Edge>,
}

impl Registry {
    /// Linear, not a second index keyed by token — the registry is at most a few hundred
    /// distinct elements, and a token round-trips through a plain string anywhere this app
    /// already threads a raw Foundry id, so there is no format to keep in sync by parsing it
    /// back into a `Key` as well.
    pub fn by_token(&self, token: &str) -> Option<&Entry> {
        self.entries.values().find(|e| e.key.token() == token)
    }

    /// Same shape as `foundry_import::Index::connections` — every edge touching `token` whose
    /// far end is not already `placed`.
    pub fn connections(&self, token: &str, placed: &HashSet<&str>) -> Vec<Edge> {
        self.edges
            .iter()
            .filter(|e| (e.from == token && e.from != e.to && !placed.contains(e.to.as_str())) || (e.to == token && e.from != e.to && !placed.contains(e.from.as_str())))
            .cloned()
            .collect()
    }

    /// Draws one entry onto `doc`, copying its representative properties — the registry's
    /// counterpart to `foundry_import::add_object_type` and friends, for a `Key` that never
    /// came from any one Foundry export.
    pub fn place(doc: &mut Document, entry: &Entry) -> ElementId {
        let id = doc.add(entry.key.kind, entry.name.clone(), 0.0, 0.0);
        if let Some(el) = doc.element_mut(id) {
            el.properties = entry.properties.clone();
            el.api_name = entry.api_name.clone();
            el.status = entry.status;
        }
        id
    }
}

/// One object type, interface or action type a fresh `:import` disagrees with the registry
/// about — same `Key`, different `properties`.
#[derive(Clone, Debug)]
pub struct Conflict {
    pub key: Key,
    pub name: String,
    pub existing: Vec<Property>,
    pub incoming: Vec<Property>,
}

impl Registry {
    /// Every object type, interface or action type a fresh Foundry import redefines
    /// differently from what this registry already has under the same `Key` — nothing for a
    /// `Key` the registry has never seen (that is a new element, not a conflict) or one whose
    /// properties came back unchanged (a harmless re-import).
    pub fn conflicts_with(&self, index: &crate::foundry_import::Index) -> Vec<Conflict> {
        index
            .defined()
            .into_iter()
            .filter_map(|d| {
                let key = Key { kind: d.kind, ident: Ident::Api(d.api_name) };
                let existing = self.entries.get(&key)?;
                (existing.properties != d.properties).then(|| Conflict { key, name: d.display_name, existing: existing.properties.clone(), incoming: d.properties })
            })
            .collect()
    }

    /// `ui::conflictpick`'s "take the incoming one" — replaces this entry's properties with
    /// the fresh import's, in memory only (see the module doc: nothing already drawn is
    /// rewritten).
    pub fn apply_incoming(&mut self, key: &Key, properties: Vec<Property>) {
        if let Some(e) = self.entries.get_mut(key) {
            e.properties = properties;
        }
    }

    /// A fresh Foundry import's other half of `conflicts_with`: every `Key` it defines that
    /// this registry has never seen becomes a real `Entry` right away — `seen_in: 0`, since
    /// nothing is drawn yet, just known — so it shows up in the workbench's elements section
    /// and is placeable (`Registry::place`) without anyone walking the tree by hand first. A
    /// `Key` already known but redefined differently is left alone here; it comes back as a
    /// `Conflict` instead, for `ui::conflictpick` to resolve into `apply_incoming`.
    pub fn absorb(&mut self, index: &crate::foundry_import::Index) -> Vec<Conflict> {
        let conflicts = self.conflicts_with(index);
        for d in index.defined() {
            let key = Key { kind: d.kind, ident: Ident::Api(d.api_name.clone()) };
            self.entries.entry(key.clone()).or_insert_with(|| Entry {
                key,
                name: d.display_name,
                api_name: Some(d.api_name),
                properties: d.properties,
                status: d.status,
                seen_in: 0,
                seen_paths: Vec::new(),
            });
        }
        conflicts
    }
}

/// Every `.json` file under a scanned tree, depth-first — reuses `workbench::scan`'s own walk
/// rather than a second directory-walking function.
fn diagram_paths(nodes: &[Node]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for n in nodes {
        match n {
            Node::Folder { children, .. } => out.extend(diagram_paths(children)),
            Node::Diagram { path, .. } => out.push(path.clone()),
        }
    }
    out
}

/// Read every diagram under `root` once and compile the registry — called when a workbench
/// opens and after every rescan. A file that fails to load (mid-edit, or not really this
/// app's format despite the `.json` extension) is skipped, not refused — the same leniency
/// `workbench::scan` already reads the folder with.
pub fn build(root: &Path) -> Registry {
    let mut entries: HashMap<Key, Entry> = HashMap::new();
    let mut edges = Vec::new();
    for path in diagram_paths(&crate::workbench::scan(root)) {
        let Ok(ws) = persistence::load(&path) else { continue };
        for tab in ws.tabs {
            let doc = &tab.diagram;
            let mut local: HashMap<ElementId, Key> = HashMap::new();
            for e in &doc.elements {
                let key = Key::of(e);
                local.insert(e.id, key.clone());
                match entries.get_mut(&key) {
                    Some(existing) => {
                        if e.properties.len() > existing.properties.len() {
                            existing.name = e.display();
                            existing.api_name = e.api_name.clone();
                            existing.properties = e.properties.clone();
                            existing.status = e.status;
                        }
                        existing.seen_in += 1;
                        if !existing.seen_paths.contains(&path) {
                            existing.seen_paths.push(path.clone());
                        }
                    }
                    None => {
                        entries.insert(
                            key.clone(),
                            Entry {
                                key,
                                name: e.display(),
                                api_name: e.api_name.clone(),
                                properties: e.properties.clone(),
                                status: e.status,
                                seen_in: 1,
                                seen_paths: vec![path.clone()],
                            },
                        );
                    }
                }
            }
            for r in &doc.relations {
                let (Some(from_key), Some(to_key)) = (local.get(&r.from), local.get(&r.to)) else { continue };
                edges.push(Edge {
                    kind: r.kind,
                    from: from_key.token(),
                    to: to_key.token(),
                    tail_label: r.tail_label.clone(),
                    head_label: r.head_label.clone(),
                    center_label: r.label.clone(),
                    many_to_many: false,
                    fk_property_api_name: None,
                });
            }
        }
    }
    Registry { entries, edges }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Workspace;
    use crate::ontology::{RelationKind, ShapeKind};

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("vim-shapes-registry-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_doc(path: &Path, build: impl FnOnce(&mut Document)) {
        let mut doc = Document::default();
        build(&mut doc);
        persistence::save(&Workspace::single("t".into(), doc), path).unwrap();
    }

    #[test]
    fn two_diagrams_sharing_an_api_name_merge_to_one_entry() {
        let root = tmp("api");
        write_doc(&root.join("a.json"), |d| {
            let id = d.add(ShapeKind::ObjectType, "Customer", 0.0, 0.0);
            d.element_mut(id).unwrap().api_name = Some("customer".into());
        });
        write_doc(&root.join("b.json"), |d| {
            let id = d.add(ShapeKind::ObjectType, "Customer (renamed label)", 0.0, 0.0);
            let el = d.element_mut(id).unwrap();
            el.api_name = Some("customer".into());
            el.properties = vec![Property::new("id")];
        });
        let reg = build(&root);
        assert_eq!(reg.entries.len(), 1, "one api_name, one entry, despite the differing label");
        let entry = reg.entries.values().next().unwrap();
        assert_eq!(entry.properties.len(), 1, "the richer sighting's properties won");
        assert_eq!(entry.seen_in, 2);
        let mut paths = entry.seen_paths.clone();
        paths.sort();
        assert_eq!(paths, vec![root.join("a.json"), root.join("b.json")], "one file each, not two sightings of the same one");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn seen_paths_counts_files_not_tabs_or_elements() {
        let root = tmp("paths");
        // Two sightings in the same file — a second tab, and a second element in it — must
        // still be one path, the granularity `workbench_open_diagram` opens at.
        let mut ws = Workspace::single("first".into(), Document::default());
        let mut second = Document::default();
        second.add(ShapeKind::ObjectType, "Customer", 0.0, 0.0);
        second.add(ShapeKind::ObjectType, "Customer", 10.0, 0.0);
        ws.tabs.push(crate::model::Tab { name: "second".into(), diagram: second });
        persistence::save(&ws, &root.join("only.json")).unwrap();
        let reg = build(&root);
        let entry = reg.entries.values().find(|e| e.name == "Customer").unwrap();
        assert_eq!(entry.seen_in, 2, "two sightings");
        assert_eq!(entry.seen_paths, vec![root.join("only.json")], "but one file");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_entry_matches_a_grep_on_its_label_kind_api_name_or_a_property() {
        let e = Entry {
            key: Key { kind: ShapeKind::ObjectType, ident: Ident::Api("cust".into()) },
            name: "Customer".into(),
            api_name: Some("cust".into()),
            properties: vec![Property { api_name: Some("orderId".into()), ..Property::new("Order Id") }],
            status: Status::Active,
            seen_in: 1,
            seen_paths: Vec::new(),
        };
        assert!(e.matches("custom"), "label, case-insensitively");
        assert!(e.matches("OBJECT TYPE"), "the kind's own name");
        assert!(e.matches("cust"), "the api_name");
        assert!(e.matches("orderid"), "a property's api_name, not just its display name");
        assert!(!e.matches("nothing here"));
        assert!(!e.matches(""), "an empty query names nothing, rather than everything");
    }

    #[test]
    fn matching_names_merge_when_neither_has_an_api_name_but_different_kinds_never_do() {
        let root = tmp("name");
        write_doc(&root.join("a.json"), |d| {
            d.add(ShapeKind::BusinessActor, "Customer", 0.0, 0.0);
        });
        write_doc(&root.join("b.json"), |d| {
            d.add(ShapeKind::BusinessActor, "Customer", 10.0, 10.0);
            d.add(ShapeKind::ApplicationComponent, "Customer", 20.0, 20.0);
        });
        let reg = build(&root);
        assert_eq!(reg.entries.len(), 2, "the actor merges by name; the component is a different kind entirely");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_relation_in_one_diagram_becomes_a_real_edge_once_scanned() {
        let root = tmp("edge");
        write_doc(&root.join("only.json"), |d| {
            let a = d.add(ShapeKind::ObjectType, "Order", 0.0, 0.0);
            let b = d.add(ShapeKind::ObjectType, "Customer", 10.0, 0.0);
            d.connect(RelationKind::LinkType, a, b).unwrap();
        });
        let reg = build(&root);
        assert_eq!(reg.edges.len(), 1);
        let order = reg.entries.values().find(|e| e.name == "Order").unwrap();
        let placed: HashSet<&str> = HashSet::new();
        let conns = reg.connections(&order.key.token(), &placed);
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].kind, RelationKind::LinkType);
        std::fs::remove_dir_all(&root).ok();
    }

    fn foundry_fixture(json: &str) -> crate::foundry_import::Index {
        let path = std::env::temp_dir().join(format!("vim-shapes-registry-conflict-{}-{}", std::process::id(), json.len()));
        std::fs::write(&path, json).unwrap();
        let index = crate::foundry_import::import(&path).unwrap();
        std::fs::remove_file(&path).ok();
        index
    }

    #[test]
    fn a_fresh_import_redefining_a_known_object_type_is_a_conflict_a_new_one_or_an_unchanged_one_is_not() {
        let key = Key { kind: ShapeKind::ObjectType, ident: Ident::Api("customer".into()) };
        let mut entries = HashMap::new();
        entries.insert(key.clone(), Entry { key, name: "Customer".into(), api_name: Some("customer".into()), properties: vec![Property::new("id")], status: Status::Active, seen_in: 1, seen_paths: Vec::new() });
        let reg = Registry { entries, edges: Vec::new() };

        let index = foundry_fixture(
            r#"{
                "version": 2,
                "objectTypes": [
                    {
                        "id": "ot.customer", "apiName": "customer",
                        "displayMetadata": {"displayName": "Customer"},
                        "status": {"type": "active"}, "typeGroups": [], "interfaces": [],
                        "primaryKeys": ["p.id"], "titlePropertyId": null,
                        "properties": [
                            {"id": "p.id", "apiName": "id", "displayMetadata": {"displayName": "Id", "visibility": "NORMAL"}, "status": {"type": "active"}, "baseType": {"type": "STRING"}},
                            {"id": "p.name", "apiName": "name", "displayMetadata": {"displayName": "Name", "visibility": "NORMAL"}, "status": {"type": "active"}, "baseType": {"type": "STRING"}}
                        ],
                        "datasources": []
                    },
                    {
                        "id": "ot.new", "apiName": "brand_new",
                        "displayMetadata": {"displayName": "Brand New"},
                        "status": {"type": "active"}, "typeGroups": [], "interfaces": [],
                        "primaryKeys": [], "titlePropertyId": null, "properties": [], "datasources": []
                    }
                ],
                "interfaces": [], "relations": [], "actionTypes": [], "sharedProperties": []
            }"#,
        );

        let conflicts = reg.conflicts_with(&index);
        assert_eq!(conflicts.len(), 1, "customer's properties changed; brand_new is new, not a conflict");
        assert_eq!(conflicts[0].name, "Customer");
        assert_eq!(conflicts[0].existing.len(), 1);
        assert_eq!(conflicts[0].incoming.len(), 2);

        // Re-checking against a registry already holding the import's own properties finds
        // nothing to flag — a harmless re-import, not a conflict.
        let mut settled = reg;
        settled.apply_incoming(&conflicts[0].key, conflicts[0].incoming.clone());
        assert!(settled.conflicts_with(&index).is_empty());
    }

    #[test]
    fn absorb_adds_every_new_defined_type_without_touching_a_conflicting_one() {
        let key = Key { kind: ShapeKind::ObjectType, ident: Ident::Api("customer".into()) };
        let mut entries = HashMap::new();
        entries.insert(key.clone(), Entry { key: key.clone(), name: "Customer".into(), api_name: Some("customer".into()), properties: vec![Property::new("id")], status: Status::Active, seen_in: 1, seen_paths: Vec::new() });
        let mut reg = Registry { entries, edges: Vec::new() };

        let index = foundry_fixture(
            r#"{
                "version": 2,
                "objectTypes": [
                    {
                        "id": "ot.customer", "apiName": "customer",
                        "displayMetadata": {"displayName": "Customer"},
                        "status": {"type": "active"}, "typeGroups": [], "interfaces": [],
                        "primaryKeys": ["p.id"], "titlePropertyId": null,
                        "properties": [
                            {"id": "p.id", "apiName": "id", "displayMetadata": {"displayName": "Id", "visibility": "NORMAL"}, "status": {"type": "active"}, "baseType": {"type": "STRING"}},
                            {"id": "p.name", "apiName": "name", "displayMetadata": {"displayName": "Name", "visibility": "NORMAL"}, "status": {"type": "active"}, "baseType": {"type": "STRING"}}
                        ],
                        "datasources": []
                    },
                    {
                        "id": "ot.brandnewabsorbed", "apiName": "brand_new_absorbed_type",
                        "displayMetadata": {"displayName": "Brand New Absorbed Type"},
                        "status": {"type": "active"}, "typeGroups": [], "interfaces": [],
                        "primaryKeys": [], "titlePropertyId": null, "properties": [], "datasources": []
                    }
                ],
                "interfaces": [], "relations": [], "actionTypes": [], "sharedProperties": []
            }"#,
        );

        let conflicts = reg.absorb(&index);
        assert_eq!(conflicts.len(), 1, "customer's properties changed — a conflict, resolved separately");
        assert_eq!(conflicts[0].name, "Customer");

        let existing = reg.entries.get(&key).unwrap();
        assert_eq!(existing.properties.len(), 1, "absorb never overwrites a conflicting entry itself — only apply_incoming does");

        let new_key = Key { kind: ShapeKind::ObjectType, ident: Ident::Api("brand_new_absorbed_type".into()) };
        let added = reg.entries.get(&new_key).expect("this key had no conflict, so it is a real entry now");
        assert_eq!(added.name, "Brand New Absorbed Type");
        assert_eq!(added.seen_in, 0, "known, but nothing has drawn it yet");
        assert!(added.seen_paths.is_empty());
    }

    #[test]
    fn apply_incoming_updates_the_entry_in_place() {
        let key = Key { kind: ShapeKind::ObjectType, ident: Ident::Api("customer".into()) };
        let mut entries = HashMap::new();
        entries.insert(key.clone(), Entry { key: key.clone(), name: "Customer".into(), api_name: Some("customer".into()), properties: vec![Property::new("id")], status: Status::Active, seen_in: 1, seen_paths: Vec::new() });
        let mut reg = Registry { entries, edges: Vec::new() };
        reg.apply_incoming(&key, vec![Property::new("id"), Property::new("name")]);
        assert_eq!(reg.entries.get(&key).unwrap().properties.len(), 2);
    }
}
