//! `:import <file.json>` — a Palantir Foundry ontology export, read into a resident [`Index`].
//!
//! Foundry's own ontology vocabulary is already this app's `Layer::Ontology` — object types,
//! interfaces, action types, datasources, link types, all with the same words (see
//! `ontology::mod`'s doc comment). What an export adds is scale: a real ontology runs to
//! hundreds of object types and action types, and drawing all of it at once is not a diagram,
//! it is a tangle — one box for every object type, action and datasource a type group
//! contains, on top of one another. So this reader does not draw anything on its own: it
//! parses the export once into an `Index` and hands `App` a tree of individually pickable
//! resources (`:tree`, one `ModelNode::Resource` per object type and action, grouped by
//! Foundry's own type groups); picking one starts a fresh tab with just that element, and
//! from there `e` (expand) lists what it is really connected to — in the export, not on the
//! diagram yet — so a diagram only ever grows to be exactly the one someone asked for.
//!
//! The export's own JSON is far larger than anything worth modelling as a strict schema —
//! hundreds of fields this app has no use for, and no promise they stay put across a Foundry
//! release. So this reads `serde_json::Value` by hand, field by field, exactly as lenient
//! about a field that has moved or gone missing as `archimate_import`'s XML reader is about
//! an attribute the file never wrote: default it, skip it, never panic on it.
//!
//! An action's rule tells us how it touches an object type — `addObjectRule` creates one,
//! `modifyObjectRule` / `addOrModifyObjectRuleV2` modifies one, `deleteObjectRule` deletes
//! one, `functionRule` calls a function — but `modifyObjectRule` and `deleteObjectRule` name
//! their target by a *parameter id*, not an object type id, so it takes a second lookup
//! (that parameter's own object-reference type) to resolve. Separately, an action's own
//! `entities.affectedObjectTypes` is Foundry's own declared list of what it touches,
//! independent of how — used here as the honest fallback edge, `Modifies`, for whatever a
//! rule didn't let us classify more precisely (most often a bare `functionRule`, which
//! touches an object type through code this export does not show us the inside of).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::Path;

use serde_json::Value;

use crate::archimate_import::ModelNode;
use crate::model::{BaseType, Document, ElementId, Property, RelationId, Status, Visibility};
use crate::ontology::{End, RelationKind, ShapeKind};

#[derive(Debug)]
pub enum ImportError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Invalid(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::Io(e) => write!(f, "could not read file: {e}"),
            ImportError::Json(e) => write!(f, "malformed JSON: {e}"),
            ImportError::Invalid(why) => write!(f, "invalid Foundry ontology export: {why}"),
        }
    }
}

pub fn import(path: &Path) -> Result<Index, ImportError> {
    let text = std::fs::read_to_string(path).map_err(ImportError::Io)?;
    from_json(&text)
}

// ─── the JSON, read leniently ──────────────────────────────────────────────

fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn opt_s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

fn path_at(v: &Value, path: &[&str]) -> Value {
    path.iter().fold(v.clone(), |acc, k| acc.get(k).cloned().unwrap_or(Value::Null))
}

fn status(v: &Value) -> Status {
    match s(v, "type") {
        "experimental" => Status::Experimental,
        "deprecated" => Status::Deprecated,
        _ => Status::Active,
    }
}

fn visibility(v: &Value) -> Visibility {
    match v.get("visibility").and_then(Value::as_str).unwrap_or("") {
        "PROMINENT" => Visibility::Prominent,
        "HIDDEN" => Visibility::Hidden,
        _ => Visibility::Normal,
    }
}

/// A property's `baseType.type` — `SCREAMING_SNAKE`, and `ARRAY`'s real type is one level
/// deeper, in `subType`.
fn property_base_type(bt: &Value) -> (BaseType, bool) {
    match s(bt, "type") {
        "ARRAY" => (scalar_base_type(s(bt.get("subType").unwrap_or(&Value::Null), "type")), true),
        t => (scalar_base_type(t), false),
    }
}

fn scalar_base_type(t: &str) -> BaseType {
    match t {
        "STRING" => BaseType::String,
        "INTEGER" => BaseType::Integer,
        "SHORT" => BaseType::Short,
        "LONG" => BaseType::Long,
        "BYTE" => BaseType::Byte,
        "DOUBLE" => BaseType::Double,
        "FLOAT" => BaseType::Float,
        "DECIMAL" => BaseType::Decimal,
        "BOOLEAN" => BaseType::Boolean,
        "DATE" => BaseType::Date,
        "TIMESTAMP" => BaseType::Timestamp,
        "GEOPOINT" => BaseType::Geopoint,
        "GEOSHAPE" => BaseType::Geoshape,
        "VECTOR" => BaseType::Vector,
        "STRUCT" => BaseType::Struct,
        "ATTACHMENT" => BaseType::Attachment,
        "MEDIA_REFERENCE" => BaseType::MediaReference,
        "TIMESERIES" => BaseType::TimeSeries,
        "GEOTEMPORAL_SERIES" => BaseType::GeotemporalSeries,
        "MARKING" => BaseType::Marking,
        "CIPHER_TEXT" => BaseType::CipherText,
        "OBJECT_REFERENCE" => BaseType::ObjectReference,
        _ => BaseType::String,
    }
}

/// A parameter's `type.type` — `camelCase`, and a `*List` suffix is this app's `array: true`
/// on the singular type rather than a type of its own.
fn parameter_base_type(t: &str) -> (BaseType, bool) {
    match t {
        "string" => (BaseType::String, false),
        "stringList" => (BaseType::String, true),
        "integer" => (BaseType::Integer, false),
        "integerList" => (BaseType::Integer, true),
        "long" => (BaseType::Long, false),
        "longList" => (BaseType::Long, true),
        "double" => (BaseType::Double, false),
        "decimal" => (BaseType::Decimal, false),
        "boolean" => (BaseType::Boolean, false),
        "date" => (BaseType::Date, false),
        "timestamp" => (BaseType::Timestamp, false),
        "attachment" => (BaseType::Attachment, false),
        "mediaReference" => (BaseType::MediaReference, false),
        "objectReference" => (BaseType::ObjectReference, false),
        "objectReferenceList" => (BaseType::ObjectReference, true),
        "objectSetRid" => (BaseType::ObjectReference, true),
        _ => (BaseType::String, false),
    }
}

struct RawProperty {
    id: String,
    api_name: String,
    display_name: String,
    base_type: BaseType,
    array: bool,
    shared: bool,
    status: Status,
}

fn parse_property(v: &Value) -> RawProperty {
    let (base_type, array) = property_base_type(v.get("baseType").unwrap_or(&Value::Null));
    RawProperty {
        id: s(v, "id").to_string(),
        api_name: s(v, "apiName").to_string(),
        display_name: opt_s(v.get("displayMetadata").unwrap_or(&Value::Null), "displayName").unwrap_or_else(|| s(v, "apiName").to_string()),
        base_type,
        array,
        shared: v.get("sharedPropertyTypeRid").is_some_and(|r| !r.is_null()),
        status: status(v.get("status").unwrap_or(&Value::Null)),
    }
}

pub(crate) struct RawObjectType {
    id: String,
    api_name: String,
    display_name: String,
    plural: Option<String>,
    status: Status,
    visibility: Visibility,
    groups: Vec<String>,
    interfaces: Vec<String>,
    primary_keys: Vec<String>,
    title: Option<String>,
    properties: Vec<RawProperty>,
    datasources: Vec<String>,
}

fn parse_object_type(v: &Value) -> RawObjectType {
    let dm = v.get("displayMetadata").cloned().unwrap_or(Value::Null);
    RawObjectType {
        id: s(v, "id").to_string(),
        api_name: s(v, "apiName").to_string(),
        display_name: opt_s(&dm, "displayName").unwrap_or_else(|| s(v, "apiName").to_string()),
        plural: opt_s(&dm, "pluralDisplayName"),
        status: status(v.get("status").unwrap_or(&Value::Null)),
        visibility: visibility(&dm),
        groups: arr(v, "typeGroups").iter().filter_map(Value::as_str).map(str::to_string).collect(),
        interfaces: arr(v, "interfaces").iter().map(|i| s(i, "interfaceTypeRid").to_string()).filter(|s| !s.is_empty()).collect(),
        primary_keys: arr(v, "primaryKeys").iter().filter_map(Value::as_str).map(str::to_string).collect(),
        title: opt_s(v, "titlePropertyId"),
        properties: arr(v, "properties").iter().map(parse_property).collect(),
        datasources: arr(v, "datasources").iter().map(|d| s(d, "backingResourceRid").to_string()).filter(|s| !s.is_empty()).collect(),
    }
}

pub(crate) struct RawInterface {
    rid: String,
    api_name: String,
    display_name: String,
    status: Status,
    extends: Vec<String>,
    properties: Vec<RawProperty>,
}

fn parse_interface(v: &Value) -> RawInterface {
    let dm = v.get("displayMetadata").cloned().unwrap_or(Value::Null);
    RawInterface {
        rid: s(v, "rid").to_string(),
        api_name: s(v, "apiName").to_string(),
        display_name: opt_s(&dm, "displayName").unwrap_or_else(|| s(v, "apiName").to_string()),
        status: status(v.get("status").unwrap_or(&Value::Null)),
        extends: arr(v, "extendsInterfaces").iter().filter_map(Value::as_str).map(str::to_string).collect(),
        properties: arr(v, "properties")
            .iter()
            .map(|p| RawProperty {
                id: s(p, "apiName").to_string(),
                api_name: s(p, "apiName").to_string(),
                display_name: opt_s(p.get("displayMetadata").unwrap_or(&Value::Null), "displayName").unwrap_or_else(|| s(p, "apiName").to_string()),
                base_type: BaseType::String,
                array: false,
                shared: p.get("isSharedProperty").and_then(Value::as_bool).unwrap_or(false),
                status: Status::Active,
            })
            .collect(),
    }
}

struct RawGroup {
    rid: String,
    display_name: String,
}

fn parse_group(v: &Value) -> RawGroup {
    RawGroup {
        rid: s(v, "rid").to_string(),
        display_name: opt_s(v.get("displayMetadata").unwrap_or(&Value::Null), "displayName").unwrap_or_else(|| s(v, "rid").to_string()),
    }
}

/// A link type between two object types. `from`/`to` carry the direction the cardinality
/// notation is drawn in — the "one" or "A" side is `from`, the "many" or "B" side is `to` —
/// and each side's own label is the API name *it* uses to reach the other, which is where
/// Foundry's own `*ToB*`/`manyToOne*` metadata names sit.
struct RawLink {
    from: String,
    to: String,
    from_label: String,
    to_label: String,
    center: Option<String>,
    many_to_many: bool,
    /// The many side's own property id that carries the one side's key — only a one-to-many
    /// link has one; a many-to-many's pairing lives in a join table this diagram never draws,
    /// so nothing on either object type is "the" foreign key for it.
    fk_property_id: Option<String>,
}

fn parse_link(v: &Value) -> Option<RawLink> {
    let def = v.get("definition")?;
    let center = opt_s(v, "description");
    match s(def, "type") {
        "oneToMany" => {
            let d = def.get("oneToMany")?;
            Some(RawLink {
                from: s(d, "objectTypeIdOneSide").to_string(),
                to: s(d, "objectTypeIdManySide").to_string(),
                from_label: s(&path_at(d, &["oneToManyLinkMetadata"]), "apiName").to_string(),
                to_label: s(&path_at(d, &["manyToOneLinkMetadata"]), "apiName").to_string(),
                center,
                many_to_many: false,
                fk_property_id: opt_s(d, "manySideForeignKeyPropertyId"),
            })
        }
        "manyToMany" => {
            let d = def.get("manyToMany")?;
            Some(RawLink {
                from: s(d, "objectTypeIdA").to_string(),
                to: s(d, "objectTypeIdB").to_string(),
                from_label: s(&path_at(d, &["objectTypeAToBLinkMetadata"]), "apiName").to_string(),
                to_label: s(&path_at(d, &["objectTypeBToALinkMetadata"]), "apiName").to_string(),
                center,
                many_to_many: true,
                fk_property_id: None,
            })
        }
        // An intermediary many-to-many (through a join object type this diagram does not
        // draw separately) still names its A/B sides directly — collapses to the same shape.
        "intermediary" => {
            let d = def.get("intermediary")?;
            Some(RawLink {
                from: s(d, "objectTypeIdA").to_string(),
                to: s(d, "objectTypeIdB").to_string(),
                from_label: s(&path_at(d, &["objectTypeAToBLinkMetadata"]), "apiName").to_string(),
                to_label: s(&path_at(d, &["objectTypeBToALinkMetadata"]), "apiName").to_string(),
                center,
                many_to_many: true,
                fk_property_id: None,
            })
        }
        _ => None,
    }
}

struct RawParameter {
    id: String,
    display_name: String,
    base_type: BaseType,
    array: bool,
    required: bool,
    object_type_id: Option<String>,
}

fn parse_parameter(v: &Value) -> RawParameter {
    let p = v.get("parameter").cloned().unwrap_or(Value::Null);
    let ty = p.get("type").cloned().unwrap_or(Value::Null);
    let (base_type, array) = parameter_base_type(s(&ty, "type"));
    let object_type_id = opt_s(ty.get("objectReference").unwrap_or(&Value::Null), "objectTypeId");
    let required = s(
        &path_at(&p, &["validation", "defaultValidation", "validation", "required"]),
        "type",
    ) == "required";
    RawParameter {
        id: s(&p, "id").to_string(),
        display_name: opt_s(p.get("displayMetadata").unwrap_or(&Value::Null), "displayName").unwrap_or_else(|| s(&p, "id").to_string()),
        base_type,
        array,
        required,
        object_type_id,
    }
}

/// How an action's rule touches an object type, or calls out to a function — resolved to a
/// concrete target, not the parameter indirection Foundry's own `modifyObjectRule` and
/// `deleteObjectRule` store it as.
enum RuleEdge {
    Creates(String),
    Modifies(String),
    Deletes(String),
    Calls(String),
}

pub(crate) struct RawActionType {
    api_name: String,
    display_name: String,
    status: Status,
    parameters: Vec<RawParameter>,
    edges: Vec<RuleEdge>,
    /// Foundry's own declared list of what this action touches, independent of how — the
    /// `Modifies` fallback for whatever no rule classified, and how the tree files an action
    /// under the groups its object types belong to (an action has no group of its own).
    affected_object_types: Vec<String>,
}

fn parse_action_type(v: &Value) -> RawActionType {
    let m = v.get("metadata").cloned().unwrap_or(Value::Null);
    let dm = m.get("displayMetadata").cloned().unwrap_or(Value::Null);
    let parameters: Vec<RawParameter> = arr(&m, "formContentInOrder")
        .iter()
        .filter(|item| s(item, "type") == "parameter")
        .map(parse_parameter)
        .collect();
    let by_param_id: HashMap<&str, &RawParameter> = parameters.iter().map(|p| (p.id.as_str(), p)).collect();
    let resolve = |param_id: &str| -> Option<String> { by_param_id.get(param_id).and_then(|p| p.object_type_id.clone()) };

    let logic = path_at(v, &["actionTypeLogic", "logic"]);
    let mut edges = Vec::new();
    for rule in arr(&logic, "rules") {
        match s(rule, "type") {
            "addObjectRule" => {
                if let Some(id) = opt_s(rule.get("addObjectRule").unwrap_or(&Value::Null), "objectTypeId") {
                    edges.push(RuleEdge::Creates(id));
                }
            }
            "modifyObjectRule" => {
                if let Some(id) = opt_s(rule.get("modifyObjectRule").unwrap_or(&Value::Null), "objectToModify").and_then(|p| resolve(&p)) {
                    edges.push(RuleEdge::Modifies(id));
                }
            }
            "addOrModifyObjectRuleV2" => {
                if let Some(id) = opt_s(rule.get("addOrModifyObjectRuleV2").unwrap_or(&Value::Null), "objectToModify").and_then(|p| resolve(&p)) {
                    edges.push(RuleEdge::Modifies(id));
                }
            }
            "deleteObjectRule" => {
                if let Some(id) = opt_s(rule.get("deleteObjectRule").unwrap_or(&Value::Null), "objectToDelete").and_then(|p| resolve(&p)) {
                    edges.push(RuleEdge::Deletes(id));
                }
            }
            "functionRule" => {
                if let Some(rid) = opt_s(rule.get("functionRule").unwrap_or(&Value::Null), "functionRid") {
                    edges.push(RuleEdge::Calls(rid));
                }
            }
            _ => {}
        }
    }

    RawActionType {
        api_name: s(&m, "apiName").to_string(),
        display_name: opt_s(&dm, "displayName").unwrap_or_else(|| s(&m, "apiName").to_string()),
        status: status(m.get("status").unwrap_or(&Value::Null)),
        parameters,
        edges,
        affected_object_types: arr(&path_at(&m, &["entities"]), "affectedObjectTypes").iter().filter_map(Value::as_str).map(str::to_string).collect(),
    }
}

fn from_json(text: &str) -> Result<Index, ImportError> {
    let v: Value = serde_json::from_str(text).map_err(ImportError::Json)?;
    let object_types: Vec<RawObjectType> = arr(&v, "objectTypes").iter().map(parse_object_type).collect();
    if object_types.is_empty() {
        return Err(ImportError::Invalid("no object types — not a Foundry ontology export".into()));
    }
    let groups: Vec<RawGroup> = arr(&v, "typeGroups").iter().map(parse_group).collect();
    let interfaces: Vec<RawInterface> = arr(&v, "interfaces").iter().map(parse_interface).collect();
    let links: Vec<RawLink> = arr(&v, "relations").iter().filter_map(parse_link).collect();
    let action_types: Vec<RawActionType> = arr(&v, "actionTypes").iter().map(parse_action_type).collect();
    Ok(Index::new(groups, object_types, interfaces, links, action_types))
}

// ─── the resident index: what `:tree` offers and `e` expands ───────────────

/// One real connection between two resources in the export — the whole of what `e` (expand)
/// can add. `from`/`to` are always the direction the relation actually means (a link type's
/// cardinality, an action's verb): which one is "the far end" from wherever the cursor
/// started depends on which of the two `id` equals, not on `from`/`to` themselves.
#[derive(Clone)]
pub struct Edge {
    pub kind: RelationKind,
    pub from: String,
    pub to: String,
    pub tail_label: Option<String>,
    pub head_label: Option<String>,
    pub center_label: Option<String>,
    pub many_to_many: bool,
    /// Set only on a `LinkType` edge whose one-to-many foreign key names a real property on
    /// `to` (the many side) — its API name, the same name the property browser shows, so a
    /// row there can be matched back to this edge without knowing Foundry's own property id.
    pub fk_property_api_name: Option<String>,
}

fn plain_edge(kind: RelationKind, from: String, to: String) -> Edge {
    Edge { kind, from, to, tail_label: None, head_label: None, center_label: None, many_to_many: false, fk_property_api_name: None }
}

impl Edge {
    /// A real export's `description` routinely runs to a full sentence, and the tail/head
    /// API names run long too — too long for a line drawn straight across a gutter no wider
    /// than `layout::GUT_X`. Squeezed into that, the text bleeds into the boxes on either
    /// side; worse, once two placed elements are no longer side by side at all, the line
    /// between them goes diagonal and the label smears across it at an angle, unreadable
    /// either way. A long label gets the room a straight vertical line's label has instead —
    /// `place_expanded` is the other half of this, deciding where the far end lands.
    pub fn prefers_vertical(&self) -> bool {
        [self.tail_label.as_deref(), self.head_label.as_deref(), self.center_label.as_deref()]
            .into_iter()
            .flatten()
            .any(|s| s.chars().count() as f64 > crate::layout::GUT_X)
    }
}

/// One object type, interface or action type this export defines, read back out for a
/// conflict check and for `registry::Registry::absorb` — see `Index::defined`.
pub struct Defined {
    pub kind: ShapeKind,
    pub api_name: String,
    pub display_name: String,
    pub properties: Vec<Property>,
    pub status: Status,
}

/// The whole export, parsed once and kept — never drawn in full. `:tree` walks its resources
/// by group; `e` walks its edges from whichever one is already on the diagram.
pub struct Index {
    object_types: HashMap<String, RawObjectType>,
    interfaces: HashMap<String, RawInterface>,
    actions: HashMap<String, RawActionType>,
    groups: Vec<RawGroup>,
    edges: Vec<Edge>,
}

impl Index {
    fn new(groups: Vec<RawGroup>, object_types: Vec<RawObjectType>, interfaces: Vec<RawInterface>, links: Vec<RawLink>, actions: Vec<RawActionType>) -> Index {
        let object_types: HashMap<String, RawObjectType> = object_types.into_iter().map(|o| (o.id.clone(), o)).collect();
        let interfaces: HashMap<String, RawInterface> = interfaces.into_iter().map(|i| (i.rid.clone(), i)).collect();
        let actions: HashMap<String, RawActionType> = actions.into_iter().map(|a| (a.api_name.clone(), a)).collect();
        let edges = build_edges(&object_types, &interfaces, &actions, &links);
        Index { object_types, interfaces, actions, groups, edges }
    }

    /// Every group, sorted by name, holding one node per object type it contains, each in
    /// turn holding its own actions and link types — Group > Object Type > Action Type or
    /// Link Type, the same hierarchy the export itself has. `(ungrouped)` last, for what
    /// belongs to none.
    pub fn tree(&self) -> Vec<ModelNode> {
        let mut sorted_groups: Vec<&RawGroup> = self.groups.iter().collect();
        sorted_groups.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        let group_index: HashMap<&str, usize> = sorted_groups.iter().enumerate().map(|(i, g)| (g.rid.as_str(), i)).collect();

        let mut buckets: Vec<Vec<&str>> = vec![Vec::new(); sorted_groups.len()];
        let mut ungrouped: Vec<&str> = Vec::new();
        for (id, o) in &self.object_types {
            let idxs: Vec<usize> = o.groups.iter().filter_map(|g| group_index.get(g.as_str()).copied()).collect();
            if idxs.is_empty() {
                ungrouped.push(id.as_str());
            } else {
                for i in idxs {
                    buckets[i].push(id.as_str());
                }
            }
        }

        let mut out = Vec::new();
        for (group, ids) in sorted_groups.iter().zip(buckets) {
            if ids.is_empty() {
                continue;
            }
            out.push(ModelNode::Folder { name: group.display_name.clone(), children: self.object_type_nodes(ids), expanded: false });
        }
        if !ungrouped.is_empty() {
            out.push(ModelNode::Folder { name: "(ungrouped)".into(), children: self.object_type_nodes(ungrouped), expanded: false });
        }
        out
    }

    fn object_type_nodes(&self, ids: Vec<&str>) -> Vec<ModelNode> {
        let mut nodes: Vec<(&str, ModelNode)> = ids
            .into_iter()
            .filter_map(|id| {
                let o = self.object_types.get(id)?;
                let children = self.resource_children(id);
                let expanded = false;
                Some((o.display_name.as_str(), ModelNode::Resource { name: o.display_name.clone(), id: id.to_string(), children, expanded }))
            })
            .collect();
        nodes.sort_by(|a, b| a.0.cmp(b.0));
        nodes.into_iter().map(|(_, n)| n).collect()
    }

    /// An object type's own actions (`Creates`/`Modifies`/`Deletes` edges that name it) and
    /// link types (either end), sorted together by name — everything else an edge can touch
    /// (an interface, a datasource, a function) stays `e`-only, since neither has its own
    /// place in this hierarchy.
    fn resource_children(&self, ot_id: &str) -> Vec<ModelNode> {
        let mut seen_actions: HashSet<&str> = HashSet::new();
        let mut children: Vec<(String, ModelNode)> = Vec::new();
        for edge in &self.edges {
            match edge.kind {
                RelationKind::Creates | RelationKind::Modifies | RelationKind::Deletes if edge.to == ot_id => {
                    if !seen_actions.insert(edge.from.as_str()) {
                        continue;
                    }
                    let Some(a) = self.actions.get(&edge.from) else { continue };
                    children.push((a.display_name.clone(), ModelNode::Resource { name: a.display_name.clone(), id: edge.from.clone(), children: Vec::new(), expanded: false }));
                }
                RelationKind::LinkType if edge.from == ot_id || edge.to == ot_id => {
                    let other_id = if edge.from == ot_id { &edge.to } else { &edge.from };
                    let Some(other) = self.object_types.get(other_id) else { continue };
                    let name = format!("→ {}", other.display_name);
                    children.push((name.clone(), ModelNode::Link { name, from: edge.from.clone(), to: edge.to.clone() }));
                }
                _ => {}
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        children.into_iter().map(|(_, n)| n).collect()
    }

    /// The exact edge a tree `Link` node names — `from`/`to` there are already the export's
    /// own direction, so this is a direct lookup, not a search either way round.
    pub fn find_link(&self, from: &str, to: &str) -> Option<Edge> {
        self.edges.iter().find(|e| e.kind == RelationKind::LinkType && e.from == from && e.to == to).cloned()
    }

    /// The link a foreign-key property on `ot_id` (named by its API name, the same name the
    /// property browser shows) backs, if any — what `e` on that row in `props` expands.
    pub fn foreign_key_edge(&self, ot_id: &str, property_api_name: &str) -> Option<Edge> {
        self.edges.iter().find(|e| e.kind == RelationKind::LinkType && e.to == ot_id && e.fk_property_api_name.as_deref() == Some(property_api_name)).cloned()
    }

    pub fn object_type(&self, id: &str) -> Option<&RawObjectType> {
        self.object_types.get(id)
    }

    pub fn interface(&self, id: &str) -> Option<&RawInterface> {
        self.interfaces.get(id)
    }

    pub fn action(&self, id: &str) -> Option<&RawActionType> {
        self.actions.get(id)
    }

    /// Every object type, interface and action type this export defines — kind, api_name,
    /// display name, status, and its properties already converted to this app's own
    /// `Property` — so a workbench's registry can check for a conflict, or absorb a brand new
    /// one, against what it already knows without this module's own `Raw*` types ever leaving
    /// it.
    pub fn defined(&self) -> Vec<Defined> {
        let mut out: Vec<Defined> = self
            .object_types
            .values()
            .map(|o| Defined { kind: ShapeKind::ObjectType, api_name: o.api_name.clone(), display_name: o.display_name.clone(), properties: properties_from(&o.properties, &o.primary_keys, &o.title), status: o.status })
            .collect();
        out.extend(self.interfaces.values().map(|i| Defined { kind: ShapeKind::Interface, api_name: i.api_name.clone(), display_name: i.display_name.clone(), properties: properties_from(&i.properties, &[], &None), status: i.status }));
        out.extend(self.actions.values().map(|a| Defined { kind: ShapeKind::ActionType, api_name: a.api_name.clone(), display_name: a.display_name.clone(), properties: properties_from_parameters(&a.parameters), status: a.status }));
        out
    }

    /// The kind a resource id names, for a row that has not been placed yet — an object
    /// type, an interface or an action are named in the export; anything else is an opaque
    /// id an edge points at (a function's, from `Calls`; a datasource's, from `BackedBy`),
    /// told apart by which relation is asking.
    pub fn kind_of(&self, id: &str, via: RelationKind) -> ShapeKind {
        if self.object_types.contains_key(id) {
            ShapeKind::ObjectType
        } else if self.interfaces.contains_key(id) {
            ShapeKind::Interface
        } else if self.actions.contains_key(id) {
            ShapeKind::ActionType
        } else if via == RelationKind::Calls {
            ShapeKind::Function
        } else {
            ShapeKind::Datasource
        }
    }

    pub fn display_name(&self, id: &str, via: RelationKind) -> String {
        if let Some(o) = self.object_type(id) {
            return o.display_name.clone();
        }
        if let Some(i) = self.interface(id) {
            return i.display_name.clone();
        }
        if let Some(a) = self.action(id) {
            return a.display_name.clone();
        }
        let _ = via;
        id.rsplit('.').next().unwrap_or(id).to_string()
    }

    /// Every edge touching `id` whose far end is not already in `placed` — the whole of what
    /// `e` has left to offer from here.
    pub fn connections<'a>(&'a self, id: &str, placed: &HashSet<&str>) -> Vec<&'a Edge> {
        self.edges
            .iter()
            .filter(|e| (e.from == id && e.from != e.to && !placed.contains(e.to.as_str())) || (e.to == id && e.from != e.to && !placed.contains(e.from.as_str())))
            .collect()
    }
}

fn build_edges(object_types: &HashMap<String, RawObjectType>, interfaces: &HashMap<String, RawInterface>, actions: &HashMap<String, RawActionType>, links: &[RawLink]) -> Vec<Edge> {
    let mut edges = Vec::new();
    for link in links {
        if !object_types.contains_key(&link.from) || !object_types.contains_key(&link.to) {
            continue;
        }
        let fk_property_api_name = link.fk_property_id.as_ref().and_then(|fk_id| {
            object_types.get(&link.to)?.properties.iter().find(|p| &p.id == fk_id).map(|p| p.api_name.clone())
        });
        edges.push(Edge {
            kind: RelationKind::LinkType,
            from: link.from.clone(),
            to: link.to.clone(),
            tail_label: Some(link.from_label.clone()).filter(|s| !s.is_empty()),
            head_label: Some(link.to_label.clone()).filter(|s| !s.is_empty()),
            center_label: link.center.clone(),
            many_to_many: link.many_to_many,
            fk_property_api_name,
        });
    }
    for (oid, o) in object_types {
        for iface_rid in &o.interfaces {
            if interfaces.contains_key(iface_rid) {
                edges.push(plain_edge(RelationKind::Implements, oid.clone(), iface_rid.clone()));
            }
        }
        for ds in &o.datasources {
            edges.push(plain_edge(RelationKind::BackedBy, oid.clone(), ds.clone()));
        }
    }
    for (rid, iface) in interfaces {
        for parent in &iface.extends {
            if interfaces.contains_key(parent) {
                edges.push(plain_edge(RelationKind::Extends, rid.clone(), parent.clone()));
            }
        }
    }
    for (aid, a) in actions {
        let mut done: BTreeSet<&str> = BTreeSet::new();
        for edge in &a.edges {
            match edge {
                RuleEdge::Creates(oid) if object_types.contains_key(oid) => {
                    edges.push(plain_edge(RelationKind::Creates, aid.clone(), oid.clone()));
                    done.insert(oid);
                }
                RuleEdge::Modifies(oid) if object_types.contains_key(oid) => {
                    edges.push(plain_edge(RelationKind::Modifies, aid.clone(), oid.clone()));
                    done.insert(oid);
                }
                RuleEdge::Deletes(oid) if object_types.contains_key(oid) => {
                    edges.push(plain_edge(RelationKind::Deletes, aid.clone(), oid.clone()));
                    done.insert(oid);
                }
                RuleEdge::Calls(fn_rid) => edges.push(plain_edge(RelationKind::Calls, aid.clone(), fn_rid.clone())),
                _ => {}
            }
        }
        for oid in &a.affected_object_types {
            if done.contains(oid.as_str()) {
                continue;
            }
            if object_types.contains_key(oid) {
                edges.push(plain_edge(RelationKind::Modifies, aid.clone(), oid.clone()));
            }
        }
    }
    edges
}

// ─── placing one resource on a document ────────────────────────────────────

/// A property's own conversion, factored out of `add_object_type`/`add_interface` so
/// `Index::defined` can build the same `Property`s for a conflict check without ever placing
/// anything on a `Document` — the one place Foundry's property shape becomes this app's.
fn properties_from(props: &[RawProperty], primary_keys: &[String], title: &Option<String>) -> Vec<Property> {
    props
        .iter()
        .map(|p| Property {
            name: p.display_name.clone(),
            api_name: Some(p.api_name.clone()),
            base_type: p.base_type,
            array: p.array,
            primary_key: primary_keys.iter().any(|k| k == &p.id),
            title: title.as_deref() == Some(p.id.as_str()),
            shared: p.shared,
            required: false,
            value_type: None,
            status: p.status,
            description: None,
        })
        .collect()
}

/// An action type's parameters, the same idea as `properties_from` for an object type's
/// properties — this app has one row shape for both.
fn properties_from_parameters(params: &[RawParameter]) -> Vec<Property> {
    params
        .iter()
        .map(|p| Property {
            name: p.display_name.clone(),
            api_name: Some(p.id.clone()),
            base_type: p.base_type,
            array: p.array,
            primary_key: false,
            title: false,
            shared: false,
            required: p.required,
            value_type: None,
            status: Status::Active,
            description: None,
        })
        .collect()
}

pub(crate) fn add_object_type(doc: &mut Document, o: &RawObjectType) -> ElementId {
    let id = doc.add(ShapeKind::ObjectType, o.display_name.clone(), 0.0, 0.0);
    let el = doc.element_mut(id).expect("just added");
    el.properties = properties_from(&o.properties, &o.primary_keys, &o.title);
    el.api_name = Some(o.api_name.clone());
    el.plural = o.plural.clone();
    el.status = o.status;
    el.visibility = o.visibility;
    id
}

pub(crate) fn add_interface(doc: &mut Document, i: &RawInterface) -> ElementId {
    let id = doc.add(ShapeKind::Interface, i.display_name.clone(), 0.0, 0.0);
    let el = doc.element_mut(id).expect("just added");
    el.properties = properties_from(&i.properties, &[], &None);
    el.api_name = Some(i.api_name.clone());
    el.status = i.status;
    id
}

pub(crate) fn add_action_type(doc: &mut Document, a: &RawActionType) -> ElementId {
    let id = doc.add(ShapeKind::ActionType, a.display_name.clone(), 0.0, 0.0);
    let el = doc.element_mut(id).expect("just added");
    el.properties = properties_from_parameters(&a.parameters);
    el.api_name = Some(a.api_name.clone());
    el.status = a.status;
    id
}

/// Only the rid survives to here — this export names the function it calls but never its
/// display name, so the box is labelled by the rid's last, most legible segment.
pub(crate) fn add_function(doc: &mut Document, rid: &str) -> ElementId {
    let label = rid.rsplit('.').next().unwrap_or(rid);
    doc.add(ShapeKind::Function, label.to_string(), 0.0, 0.0)
}

pub(crate) fn add_datasource(doc: &mut Document, rid: &str) -> ElementId {
    let label = rid.rsplit('.').next().unwrap_or(rid);
    doc.add(ShapeKind::Datasource, label.to_string(), 0.0, 0.0)
}

/// A `LinkType` edge's tail/head labels, centre label and many-to-many cardinality — the
/// notation `Index::connections` cannot carry on the relation itself, applied after
/// `Document::connect` the same way a freshly drawn one would be configured by hand.
pub(crate) fn apply_link_notation(doc: &mut Document, rel: RelationId, edge: &Edge) {
    let Some(rel) = doc.relation_mut(rel) else { return };
    rel.tail_label = edge.tail_label.clone();
    rel.head_label = edge.head_label.clone();
    rel.label = edge.center_label.clone();
    if edge.many_to_many {
        let mut notation = RelationKind::LinkType.notation();
        notation.tail = End::Crow;
        notation.head = End::Crow;
        rel.style = Some(notation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        r#"{
            "version": 2,
            "typeGroups": [
                {"rid": "g.crm", "displayMetadata": {"displayName": "CRM"}},
                {"rid": "g.billing", "displayMetadata": {"displayName": "Billing"}}
            ],
            "objectTypes": [
                {
                    "id": "ot.customer", "apiName": "Customer",
                    "displayMetadata": {"displayName": "Customer", "pluralDisplayName": "Customers"},
                    "status": {"type": "active"}, "typeGroups": ["g.crm"], "interfaces": [],
                    "primaryKeys": ["p.id"], "titlePropertyId": "p.name",
                    "properties": [
                        {"id": "p.id", "apiName": "id", "displayMetadata": {"displayName": "Id", "visibility": "NORMAL"}, "status": {"type": "active"}, "baseType": {"type": "STRING"}},
                        {"id": "p.name", "apiName": "name", "displayMetadata": {"displayName": "Name", "visibility": "PROMINENT"}, "status": {"type": "active"}, "baseType": {"type": "STRING"}}
                    ],
                    "datasources": [{"backingResourceRid": "ri.foundry.main.dataset.customers"}]
                },
                {
                    "id": "ot.order", "apiName": "Order",
                    "displayMetadata": {"displayName": "Order"},
                    "status": {"type": "experimental"}, "typeGroups": ["g.billing"], "interfaces": [],
                    "primaryKeys": ["p.oid"], "titlePropertyId": null,
                    "properties": [
                        {"id": "p.oid", "apiName": "id", "displayMetadata": {"displayName": "Id", "visibility": "NORMAL"}, "status": {"type": "active"}, "baseType": {"type": "STRING"}}
                    ],
                    "datasources": []
                },
                {
                    "id": "ot.loose", "apiName": "Loose",
                    "displayMetadata": {"displayName": "Loose"},
                    "status": {"type": "active"}, "typeGroups": [], "interfaces": [],
                    "primaryKeys": [], "titlePropertyId": null, "properties": [], "datasources": []
                }
            ],
            "interfaces": [
                {
                    "rid": "iface.identifiable", "apiName": "Identifiable",
                    "displayMetadata": {"displayName": "Identifiable"}, "status": {"type": "active"},
                    "extendsInterfaces": [], "properties": []
                }
            ],
            "relations": [
                {
                    "description": null,
                    "definition": {"type": "oneToMany", "oneToMany": {
                        "objectTypeIdOneSide": "ot.customer", "objectTypeIdManySide": "ot.order",
                        "oneToManyLinkMetadata": {"apiName": "orders"},
                        "manyToOneLinkMetadata": {"apiName": "customer"}
                    }}
                }
            ],
            "actionTypes": [
                {
                    "metadata": {
                        "rid": "at.create-order", "apiName": "createOrder",
                        "displayMetadata": {"displayName": "Create Order"},
                        "status": {"type": "active"},
                        "entities": {"affectedObjectTypes": ["ot.order"]},
                        "formContentInOrder": [
                            {"type": "parameter", "parameter": {"id": "customer", "displayMetadata": {"displayName": "Customer"}, "type": {"type": "objectReference", "objectReference": {"objectTypeId": "ot.customer"}}, "validation": {"defaultValidation": {"validation": {"required": {"type": "required"}}}}}}
                        ]
                    },
                    "actionTypeLogic": {"logic": {"rules": [
                        {"type": "addObjectRule", "addObjectRule": {"objectTypeId": "ot.order"}}
                    ]}}
                },
                {
                    "metadata": {
                        "rid": "at.close-order", "apiName": "closeOrder",
                        "displayMetadata": {"displayName": "Close Order"},
                        "status": {"type": "active"},
                        "entities": {"affectedObjectTypes": ["ot.order"]},
                        "formContentInOrder": [
                            {"type": "parameter", "parameter": {"id": "order", "displayMetadata": {"displayName": "Order"}, "type": {"type": "objectReference", "objectReference": {"objectTypeId": "ot.order"}}, "validation": {"defaultValidation": {"validation": {"required": {"type": "required"}}}}}}
                        ]
                    },
                    "actionTypeLogic": {"logic": {"rules": [
                        {"type": "modifyObjectRule", "modifyObjectRule": {"objectToModify": "order"}}
                    ]}}
                },
                {
                    "metadata": {
                        "rid": "at.delete-order", "apiName": "deleteOrder",
                        "displayMetadata": {"displayName": "Delete Order"},
                        "status": {"type": "active"},
                        "entities": {"affectedObjectTypes": ["ot.order"]},
                        "formContentInOrder": [
                            {"type": "parameter", "parameter": {"id": "order", "displayMetadata": {"displayName": "Order"}, "type": {"type": "objectReference", "objectReference": {"objectTypeId": "ot.order"}}, "validation": {"defaultValidation": {"validation": {"required": {"type": "required"}}}}}}
                        ]
                    },
                    "actionTypeLogic": {"logic": {"rules": [
                        {"type": "deleteObjectRule", "deleteObjectRule": {"objectToDelete": "order"}}
                    ]}}
                },
                {
                    "metadata": {
                        "rid": "at.recalc", "apiName": "recalc",
                        "displayMetadata": {"displayName": "Recalculate"},
                        "status": {"type": "active"},
                        "entities": {"affectedObjectTypes": ["ot.order"]},
                        "formContentInOrder": []
                    },
                    "actionTypeLogic": {"logic": {"rules": [
                        {"type": "functionRule", "functionRule": {"functionRid": "ri.function-registry.main.function.abc"}}
                    ]}}
                }
            ],
            "sharedProperties": []
        }"#
        .to_string()
    }

    #[test]
    fn a_malformed_or_unrelated_file_is_refused_not_panicked_on() {
        assert!(matches!(from_json("not json"), Err(ImportError::Json(_))));
        assert!(matches!(from_json("{}"), Err(ImportError::Invalid(_))));
    }

    #[test]
    fn the_tree_is_group_over_object_type_over_its_own_actions_and_links() {
        let index = from_json(&fixture()).unwrap();
        let tree = index.tree();
        let names: Vec<&str> = tree
            .iter()
            .map(|n| match n {
                ModelNode::Folder { name, .. } => name.as_str(),
                _ => panic!("expected a folder"),
            })
            .collect();
        assert_eq!(names, vec!["Billing", "CRM", "(ungrouped)"], "sorted by name, ungrouped last");

        let object_types = |folder: &str| -> Vec<&ModelNode> {
            let ModelNode::Folder { children, .. } = tree.iter().find(|n| matches!(n, ModelNode::Folder { name, .. } if name == folder)).unwrap() else { unreachable!() };
            children.iter().collect()
        };
        let billing = object_types("Billing");
        assert_eq!(billing.len(), 1, "just Order — its four actions are nested under it, not siblings of it");
        let ModelNode::Resource { name, id, children: order_children, .. } = billing[0] else { panic!("expected a resource") };
        assert_eq!((name.as_str(), id.as_str()), ("Order", "ot.order"));
        // Its four actions, plus the same link Customer has — the many side of it too.
        let child_names: Vec<&str> = order_children
            .iter()
            .map(|c| match c {
                ModelNode::Resource { name, .. } | ModelNode::Link { name, .. } => name.as_str(),
                _ => panic!("expected an action or a link"),
            })
            .collect();
        assert_eq!(child_names, vec!["Close Order", "Create Order", "Delete Order", "Recalculate", "→ Customer"], "sorted by name, the link last");

        let crm = object_types("CRM");
        let ModelNode::Resource { name: cname, children: customer_children, .. } = crm[0] else { panic!("expected a resource") };
        assert_eq!(cname, "Customer");
        assert!(matches!(&customer_children[0], ModelNode::Link { name, .. } if name == "→ Order"), "Customer's own link to Order, nested under it");

        let ungrouped = object_types("(ungrouped)");
        assert!(matches!(ungrouped[0], ModelNode::Resource { name, .. } if name == "Loose"));
    }

    #[test]
    fn a_link_type_only_shows_up_once_both_its_object_types_are_placed() {
        let index = from_json(&fixture()).unwrap();
        let none: HashSet<&str> = HashSet::new();
        let from_customer = index.connections("ot.customer", &none);
        assert!(from_customer.iter().any(|e| e.kind == RelationKind::LinkType && e.to == "ot.order"), "the link is there before anything else is placed");

        let mut placed = HashSet::new();
        placed.insert("ot.order");
        assert!(index.connections("ot.customer", &placed).iter().all(|e| e.kind != RelationKind::LinkType), "once the other end is already on the diagram, expanding stops offering it again");
    }

    #[test]
    fn an_action_s_rules_resolve_to_the_precise_verb_and_a_function_rule_falls_back_honestly() {
        let index = from_json(&fixture()).unwrap();
        let none: HashSet<&str> = HashSet::new();
        let verb = |action_id: &str| -> Vec<(RelationKind, String)> {
            index.connections(action_id, &none).into_iter().filter(|e| e.from == action_id).map(|e| (e.kind, e.to.clone())).collect()
        };
        assert_eq!(verb("createOrder"), vec![(RelationKind::Creates, "ot.order".to_string())]);
        assert_eq!(verb("closeOrder"), vec![(RelationKind::Modifies, "ot.order".to_string())]);
        assert_eq!(verb("deleteOrder"), vec![(RelationKind::Deletes, "ot.order".to_string())]);
        assert_eq!(
            verb("recalc"),
            vec![(RelationKind::Calls, "ri.function-registry.main.function.abc".to_string()), (RelationKind::Modifies, "ot.order".to_string())],
            "a bare function rule still gets the honest fallback to the object type Foundry declared it affects"
        );
        assert_eq!(index.kind_of("ri.function-registry.main.function.abc", RelationKind::Calls), ShapeKind::Function);
    }

    #[test]
    fn placing_a_resource_carries_its_real_properties() {
        let index = from_json(&fixture()).unwrap();
        let o = index.object_type("ot.customer").unwrap();
        let mut doc = Document::default();
        let id = add_object_type(&mut doc, o);
        let el = doc.element(id).unwrap();
        assert_eq!(el.display(), "Customer");
        assert_eq!(el.properties.len(), 2);
        assert!(el.properties.iter().any(|p| p.name == "Name" && p.title));
    }

    #[test]
    fn a_long_description_or_api_name_prefers_a_vertical_line() {
        let short = plain_edge(RelationKind::LinkType, "a".into(), "b".into());
        assert!(!short.prefers_vertical(), "short or absent labels fit a horizontal gutter fine");

        let mut long_center = short.clone();
        long_center.center_label = Some("Connects a purchase order to the individual tagged assets acquired under its expected hardware commitment.".into());
        assert!(long_center.prefers_vertical());

        let mut long_end = short.clone();
        long_end.tail_label = Some("acquiringPurchaseOrderIdentifier".into());
        assert!(long_end.prefers_vertical(), "a long API name alone is enough, not just the description");
    }
}
