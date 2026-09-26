//! Idioms: shapes of diagram that are *answers*, not merely legal.
//!
//! The rules say what may be drawn. An idiom says what is worth drawing — the seven or eight
//! small diagrams every architect draws over and over, each a worked example of how the
//! layers meet. `:idiom <name>` stamps one into the diagram you are working on, so a model
//! can begin from a known-good shape rather than from a blank canvas.
//!
//! Every idiom is **built and linted by the tests**: each one becomes a real document, and
//! the rules must pass every relation in it. An idiom that quietly taught a forbidden line
//! would be a lesson in the wrong direction, and the build refuses it.

use super::{ShapeKind, RelationKind};
use crate::layout;
use crate::model::{Document, ElementId};

pub struct Idiom {
    /// What `:idiom` calls it.
    pub name: &'static str,
    /// One line for the completion list and the manual.
    pub tagline: &'static str,
    /// Why this shape, and what it fixes.
    pub story: &'static str,
    /// `(alias, kind, label)` — aliases are how the relations name their ends.
    pub elements: &'static [(&'static str, ShapeKind, &'static str)],
    /// `(kind, from alias, to alias)`.
    pub relations: &'static [(RelationKind, &'static str, &'static str)],
}

use ShapeKind::*;
use RelationKind::*;

pub static IDIOMS: &[Idiom] = &[
    Idiom {
        name: "object-link",
        tagline: "two object types joined one-to-many",
        story: "The smallest ontology: an Airport and the Flights that leave it. The link type's \
                ends are its cardinality — a bar at the one, a crow's foot at the many — and the \
                labels at each end are the API names each side reads the link by.",
        elements: &[("airport", ObjectType, "Airport"), ("flight", ObjectType, "Flight")],
        relations: &[(LinkType, "airport", "flight")],
    },
    Idiom {
        name: "interface",
        tagline: "an interface and two object types that implement it",
        story: "Polymorphism: a Vehicle interface, implemented by Truck and Van. What is drawn \
                against the interface — a link constraint, an action — holds for every \
                implementer, which is the whole reason to draw the abstract shape.",
        elements: &[("vehicle", Interface, "Vehicle"), ("truck", ObjectType, "Truck"), ("van", ObjectType, "Van")],
        relations: &[(Implements, "truck", "vehicle"), (Implements, "van", "vehicle")],
    },
    Idiom {
        name: "action",
        tagline: "an action type and the object type it modifies",
        story: "An action is a node of its own: Assign driver takes parameters and, by its rule, \
                modifies a Delivery. Drawn as a shape joined by its verb, its parameters have a \
                compartment and every other type it touches gets a verb of its own.",
        elements: &[("action", ActionType, "Assign driver"), ("delivery", ObjectType, "Delivery")],
        relations: &[(Modifies, "action", "delivery")],
    },
    Idiom {
        name: "service",
        tagline: "a component, the service it promises, and the process that uses it",
        story: "The seam between application and business. The process never sees the \
                component: it uses a SERVICE, which the component realizes. Swap the component \
                and the process does not notice — which is the whole point of drawing the \
                service rather than an arrow straight from the box to the box.",
        elements: &[
            ("process", BusinessProcess, "Handle claim"),
            ("service", ApplicationService, "Claims API"),
            ("component", ApplicationComponent, "Claims system"),
        ],
        relations: &[(Realization, "component", "service"), (Serving, "service", "process")],
    },
    Idiom {
        name: "stack",
        tagline: "the full layered stack, from the actor down to the node",
        story: "One column through every core layer. An actor takes a role; the role is \
                assigned to a process; the process realizes the business service the actor \
                actually experiences. Underneath, an application service serves the process \
                and a component realizes it; underneath that, a technology service serves the \
                component and a node realizes it. Every crossing of a layer goes through a \
                service — that is the pattern.",
        elements: &[
            ("actor", BusinessActor, "Customer"),
            ("role", BusinessRole, "Account holder"),
            ("process", BusinessProcess, "Open account"),
            ("bsvc", BusinessService, "Account opening"),
            ("asvc", ApplicationService, "Account API"),
            ("component", ApplicationComponent, "Core banking"),
            ("tsvc", TechnologyService, "Database service"),
            ("node", Node, "DB cluster"),
        ],
        relations: &[
            (Assignment, "actor", "role"),
            (Assignment, "role", "process"),
            (Realization, "process", "bsvc"),
            (Serving, "bsvc", "actor"),
            (Serving, "asvc", "process"),
            (Realization, "component", "asvc"),
            (Serving, "tsvc", "component"),
            (Realization, "node", "tsvc"),
        ],
    },
    Idiom {
        name: "motivation",
        tagline: "from a stakeholder's concern down to the capability that answers it",
        story: "Why anything on the diagram exists. A stakeholder has a driver; an assessment \
                of that driver influences a goal; the goal is made measurable by an outcome \
                and concrete by a requirement; and a capability realizes the requirement — \
                which is the hook every component on every other diagram can hang from.",
        elements: &[
            ("stakeholder", Stakeholder, "CFO"),
            ("driver", Driver, "Run cost"),
            ("assessment", Assessment, "Licence costs rising 15%/yr"),
            ("goal", Goal, "Cut run cost 20%"),
            ("outcome", Outcome, "Cloud bill down by Q4"),
            ("requirement", Requirement, "Move to managed database"),
            ("capability", Capability, "Cloud operations"),
        ],
        relations: &[
            (Association, "stakeholder", "driver"),
            (Association, "driver", "assessment"),
            (Influence, "assessment", "goal"),
            (Realization, "outcome", "goal"),
            (Realization, "requirement", "goal"),
            (Realization, "capability", "requirement"),
        ],
    },
    Idiom {
        name: "process",
        tagline: "an event, the processes it sets off, and the object they work on",
        story: "Time, drawn left to right. An event triggers a process, which triggers the \
                next, which raises the event that ends it. The object the processes read and \
                write hangs off them with ACCESS — the one relation that says data without \
                saying sequence.",
        elements: &[
            ("start", BusinessEvent, "Order received"),
            ("validate", BusinessProcess, "Validate order"),
            ("ship", BusinessProcess, "Ship order"),
            ("end", BusinessEvent, "Order shipped"),
            ("order", BusinessObject, "Order"),
        ],
        relations: &[
            (Triggering, "start", "validate"),
            (Triggering, "validate", "ship"),
            (Triggering, "ship", "end"),
            (Access, "validate", "order"),
            (Access, "ship", "order"),
        ],
    },
    Idiom {
        name: "data",
        tagline: "a function on its data, the business object it stands for, and where it runs",
        story: "How data crosses the layers. A business object is what the business talks \
                about; a data object realizes it; an application function accesses the data \
                object and a component is assigned to the function; an artifact realizes the \
                component and is assigned to the node it is deployed on. Read it top to bottom \
                and it answers 'where does the invoice actually live'.",
        elements: &[
            ("bobj", BusinessObject, "Invoice"),
            ("dobj", DataObject, "invoice"),
            ("function", ApplicationFunction, "Render invoice"),
            ("component", ApplicationComponent, "Billing"),
            ("artifact", Artifact, "billing.jar"),
            ("node", Node, "App server"),
        ],
        relations: &[
            (Realization, "dobj", "bobj"),
            (Access, "function", "dobj"),
            (Assignment, "component", "function"),
            (Realization, "artifact", "component"),
            (Assignment, "node", "artifact"),
        ],
    },
    Idiom {
        name: "deployment",
        tagline: "a component, the artifact that ships it, the node it runs on, and the network",
        story: "The technology view of one service. The component is realized by an artifact \
                — the image, the jar — which is assigned to the node that runs it; the node \
                runs system software and sits on a network. Draw this once per deployable and \
                the infrastructure diagram writes itself.",
        elements: &[
            ("component", ApplicationComponent, "Orders service"),
            ("artifact", Artifact, "orders-svc:1.4"),
            ("node", Node, "k8s cluster"),
            ("sysw", SystemSoftware, "Kubernetes"),
            ("net", CommunicationNetwork, "VPC"),
        ],
        relations: &[
            (Realization, "artifact", "component"),
            (Assignment, "node", "artifact"),
            (Assignment, "node", "sysw"),
            (Association, "net", "node"),
        ],
    },
    Idiom {
        name: "migration",
        tagline: "two plateaus, the gap between them, and the work that closes it",
        story: "When. The as-is plateau aggregates what exists today; the to-be plateau, what \
                will; the gap names the difference. A work package realizes the deliverable \
                that realizes the to-be plateau, and a go-live event marks the moment. Time \
                runs left to right, like a process.",
        elements: &[
            ("asis", Plateau, "As-is"),
            ("legacy", ApplicationComponent, "Legacy CRM"),
            ("gap", Gap, "Replace CRM"),
            ("tobe", Plateau, "To-be"),
            ("work", WorkPackage, "CRM migration"),
            ("deliverable", Deliverable, "New CRM live"),
            ("golive", ImplementationEvent, "Go-live"),
        ],
        relations: &[
            (Aggregation, "asis", "legacy"),
            (Association, "asis", "gap"),
            (Association, "gap", "tobe"),
            (Realization, "work", "deliverable"),
            (Realization, "deliverable", "tobe"),
            (Triggering, "work", "golive"),
        ],
    },
];

pub fn find(name: &str) -> Option<&'static Idiom> {
    let want = name.trim().to_ascii_lowercase();
    IDIOMS.iter().find(|i| i.name == want).or_else(|| {
        let mut hits = IDIOMS.iter().filter(|i| i.name.starts_with(&want));
        let first = hits.next()?;
        hits.next().map_or(Some(first), |_| None)
    })
}

/// Build an idiom as a document of its own, laid out by layer.
#[cfg(test)]
pub fn build(idiom: &Idiom) -> Document {
    let mut doc = Document::default();
    stamp(&mut doc, idiom, (0.0, 0.0));
    doc
}

/// Add an idiom's elements and relations to `doc`, laid out by layer with its top-left at
/// `origin`. Returns the new elements' ids, in the idiom's own order.
pub fn stamp(doc: &mut Document, idiom: &Idiom, origin: (f64, f64)) -> Vec<ElementId> {
    // Lay the idiom out on its own first, so its shape does not depend on what else is in
    // the diagram, then translate it into place.
    let mut scratch = Document::default();
    let mut alias: Vec<(&str, ElementId)> = Vec::new();
    for (a, kind, label) in idiom.elements {
        let id = scratch.add(*kind, *label, 0.0, 0.0);
        alias.push((a, id));
    }
    let id_of = |a: &str| alias.iter().find(|(n, _)| *n == a).map(|(_, id)| *id).expect("idiom alias");
    for (kind, from, to) in idiom.relations {
        scratch.connect(*kind, id_of(from), id_of(to)).expect("idiom relation");
    }
    let pos = layout::layers(&scratch);

    let mut out = Vec::new();
    let mut map: Vec<(ElementId, ElementId)> = Vec::new();
    for e in &scratch.elements {
        let (_, x, y) = pos.iter().find(|p| p.0 == e.id).copied().unwrap_or((e.id, 0.0, 0.0));
        let id = doc.add(e.kind, e.label.clone(), origin.0 + x, origin.1 + y);
        map.push((e.id, id));
        out.push(id);
    }
    let new = |old: ElementId| map.iter().find(|(o, _)| *o == old).map(|(_, n)| *n).expect("mapped");
    for r in &scratch.relations {
        doc.connect(r.kind, new(r.from), new(r.to)).expect("stamped relation");
    }
    out
}
