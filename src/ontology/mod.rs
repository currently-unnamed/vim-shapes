//! The ontology: what an architecture diagram *is*, written down once.
//!
//! Every diagramming tool lets you draw a box and a line. What makes a drawing an
//! architecture model rather than a picture is that the boxes are *kinds of thing* and the
//! lines are *kinds of relationship*, and that some pairings mean something while others are
//! nonsense — a business process cannot be *composed of* a server. This module is where those
//! facts live, and it is the only place they live: the palette lists it, the relation picker
//! reads it, `:lint` checks against it, the manual is generated from it, and
//! `vim-shapes --ontology` emits it for a model that has never seen the app.
//!
//! ## The one rule this file exists to keep
//!
//! **Nothing here may describe a kind that does not exist, and no kind may exist that is not
//! described.** `ShapeKind::ALL` and `RelationKind::ALL` are walked by the tests, and every
//! entry must have a layer, a shape, a tagline and a summary. Add a kind and forget its prose,
//! and the build tells you.
//!
//! ## The layers
//!
//! | | |
//! | --- | --- |
//! | 1. **layers** | motivation, strategy, business, application, technology, implementation — and the composites that box them |
//! | 2. **elements** | the kinds of thing in each layer, each with a category: active, behaviour, passive |
//! | 3. **relations** | the eleven ways two elements can be joined, and what each one *means* |
//! | 4. **rules** | which relation is allowed between which pair — [`allowed`], one function, walked by the tests |
//! | 5. **[idioms](idiom)** | shapes that are answers, not merely legal. **Built and linted by the tests.** |
//! | 6. **views** | which layers a kind of diagram is about, so a palette offers possibilities rather than choices |
//!
//! The vocabulary follows the common enterprise-architecture modelling convention of layered
//! structure/behaviour/passive elements, because that is what an architect already reads; the
//! names are kept plain so a diagram made here is legible to someone who has never used the
//! language it borrows from.

pub mod doc;
pub mod emit;
pub mod idiom;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

// ─── layers ─────────────────────────────────────────────────────────────────

/// Which storey of the architecture an element sits on.
///
/// The core three — business, application, technology — are ordered: each is *realized* by
/// the one below and *serves* the one above. Motivation and strategy sit over the top, saying
/// why; implementation sits alongside, saying when; composites are boxes drawn around any of
/// it.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    Motivation,
    Strategy,
    Business,
    Application,
    Technology,
    Implementation,
    /// The types a data platform's ontology is built from — object types, link types, action
    /// types, interfaces — the vocabulary of Palantir Foundry's Ontology, used unchanged. A
    /// layer of the architecture: the digital twin sits beside the business it models and
    /// the applications that read it.
    Ontology,
    Composite,
    /// Plain shapes with no architectural meaning: a box, a circle, an arrow's worth of
    /// diamond. For the parts of a picture the ontology has no word for.
    Sketch,
}

impl Layer {
    /// The fill architects already read a layer in — the pastel every desktop tool uses —
    /// for a shape whose fill is `auto`. A composite is a boundary and has none; a plain
    /// shape is paper.
    pub fn pastel(self) -> Option<[u8; 3]> {
        match self {
            Layer::Motivation => Some([204, 204, 255]),
            Layer::Strategy => Some([245, 222, 170]),
            Layer::Business => Some([255, 255, 181]),
            Layer::Application => Some([181, 255, 255]),
            Layer::Technology => Some([201, 231, 183]),
            Layer::Implementation => Some([255, 224, 224]),
            Layer::Ontology => Some([226, 238, 250]),
            Layer::Composite => None,
            Layer::Sketch => Some([255, 255, 255]),
        }
    }

    /// Palette and layout order: the *why* on top, the stack beneath it, the *when* under that,
    /// and the plain shapes last.
    pub const ALL: [Layer; 9] = [
        Layer::Motivation,
        Layer::Strategy,
        Layer::Business,
        Layer::Application,
        Layer::Technology,
        Layer::Implementation,
        Layer::Ontology,
        Layer::Composite,
        Layer::Sketch,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Layer::Motivation => "motivation",
            Layer::Strategy => "strategy",
            Layer::Business => "business",
            Layer::Application => "application",
            Layer::Technology => "technology",
            Layer::Implementation => "implementation",
            Layer::Ontology => "ontology",
            Layer::Composite => "composite",
            Layer::Sketch => "sketch",
        }
    }

    pub fn tagline(self) -> &'static str {
        match self {
            Layer::Motivation => "why — who wants what, and the goals and requirements that follow",
            Layer::Strategy => "what the enterprise can do, and how it means to get there",
            Layer::Business => "the organisation: people, roles, processes, the services they offer",
            Layer::Application => "the software: components, the services they expose, the data they hold",
            Layer::Technology => "the infrastructure: nodes, devices, networks, what runs on them",
            Layer::Implementation => "when — the work packages, deliverables and plateaus of getting there",
            Layer::Ontology => "a data platform's ontology: object types with their properties, link types, interfaces, action types",
            Layer::Composite => "boxes drawn around any of it: a grouping, a place",
            Layer::Sketch => "plain shapes — a box, a circle, a note — for what the ontology has no word for",
        }
    }

    /// Where in the core stack a layer sits. Business is *above* application, which is above
    /// technology: a lower layer realizes the one over it, and serves it. `None` for the layers
    /// that stand aside from the stack.
    pub fn storey(self) -> Option<u8> {
        match self {
            Layer::Business => Some(0),
            Layer::Application => Some(1),
            Layer::Technology => Some(2),
            _ => None,
        }
    }
}

/// What *sort* of thing an element is, across every layer.
///
/// This is the axis the rules actually turn on. A layer says where an element lives; a category
/// says what it is for — something that acts, something that happens, or something that is
/// acted upon — and it is the pairing of categories that decides which relation makes sense.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Something that *acts*: an actor, a component, a node. Structure that does things.
    Active,
    /// Something that *happens*: a process, a function, a service, an event.
    Behaviour,
    /// Something that is *acted upon*: an object, a data object, an artifact.
    Passive,
    /// A reason: a stakeholder, a goal, a requirement.
    Motivation,
    /// A piece of the plan: a work package, a deliverable, a plateau.
    Implementation,
    /// A box around other elements.
    Composite,
    /// A plain shape. It means whatever its label says, and the rules leave it alone.
    Shape,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Active => "active structure",
            Category::Behaviour => "behaviour",
            Category::Passive => "passive structure",
            Category::Motivation => "motivation",
            Category::Implementation => "implementation",
            Category::Composite => "composite",
            Category::Shape => "plain shape",
        }
    }
}

// ─── shapes ─────────────────────────────────────────────────────────────────

/// The geometry a kind is drawn with. What `shapes.rs` turns into braille.
///
/// A terminal cannot draw an icon in the corner of a box the way a desktop tool does, so the
/// *outline* has to carry the distinction: a rounded box is behaviour, a square one is
/// structure, a cylinder is data, a cloud is a network. The kind's short tag is printed inside
/// as well, so nothing rests on the reader knowing the code.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Diamond,
    Cylinder,
    Cloud,
    Parallelogram,
    Hexagon,
    /// A dashed box — a grouping. Drawn open, so the things inside it stay legible.
    Dashed,
    /// No outline at all: a label standing on its own.
    Text,
    // The rest of the general palette, as any diagramming tool draws it.
    Triangle,
    PredefinedProcess,
    Document,
    InternalStorage,
    Cube,
    Step,
    Trapezoid,
    Tape,
    Note,
    Card,
    Callout,
    StickFigure,
    DataStorage,
    Delay,
    Display,
    ManualInput,
    OffPage,
    BlockArrow,
    DoubleArrow,
    And,
    Or,
}

impl Shape {
    pub fn name(self) -> &'static str {
        match self {
            Shape::Rectangle => "rectangle",
            Shape::RoundedRectangle => "rounded rectangle",
            Shape::Ellipse => "ellipse",
            Shape::Diamond => "diamond",
            Shape::Cylinder => "cylinder",
            Shape::Cloud => "cloud",
            Shape::Parallelogram => "parallelogram",
            Shape::Hexagon => "hexagon",
            Shape::Dashed => "dashed box",
            Shape::Text => "text",
            Shape::Triangle => "triangle",
            Shape::PredefinedProcess => "predefined process",
            Shape::Document => "document",
            Shape::InternalStorage => "internal storage",
            Shape::Cube => "cube",
            Shape::Step => "step",
            Shape::Trapezoid => "trapezoid",
            Shape::Tape => "tape",
            Shape::Note => "note",
            Shape::Card => "card",
            Shape::Callout => "callout",
            Shape::StickFigure => "stick figure",
            Shape::DataStorage => "data storage",
            Shape::Delay => "delay",
            Shape::Display => "display",
            Shape::ManualInput => "manual input",
            Shape::OffPage => "off-page connector",
            Shape::BlockArrow => "block arrow",
            Shape::DoubleArrow => "double arrow",
            Shape::And => "and",
            Shape::Or => "or",
        }
    }
}

// ─── elements ───────────────────────────────────────────────────────────────

/// Every kind of element a diagram can hold.
///
/// Serialized by its snake-case name, so a saved file reads `"kind": "business_process"` and can
/// be edited by hand.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    // motivation
    Stakeholder,
    Driver,
    Assessment,
    Goal,
    Outcome,
    Principle,
    Requirement,
    Constraint,
    // strategy
    Resource,
    Capability,
    ValueStream,
    CourseOfAction,
    // business
    BusinessActor,
    BusinessRole,
    BusinessInterface,
    BusinessProcess,
    BusinessFunction,
    BusinessEvent,
    BusinessService,
    BusinessObject,
    // application
    ApplicationComponent,
    ApplicationInterface,
    ApplicationFunction,
    ApplicationProcess,
    ApplicationEvent,
    ApplicationService,
    DataObject,
    // technology
    Node,
    Device,
    SystemSoftware,
    TechnologyInterface,
    CommunicationNetwork,
    TechnologyFunction,
    TechnologyProcess,
    TechnologyService,
    Artifact,
    // implementation
    WorkPackage,
    Deliverable,
    ImplementationEvent,
    Plateau,
    Gap,
    // ── the ontology layer: a data platform's types ─────────────────────────────
    /// "The schema definition of a real-world entity or event": what an object is an instance
    /// of. Carries its properties, one of them the primary key and one the title.
    ObjectType,
    /// "An Ontology type that describes the shape of an object type and its capabilities":
    /// abstract, never instantiated; object types implement it, interfaces extend it.
    Interface,
    /// "The schema definition of a set of changes or edits to objects, property values, and
    /// links that a user can take at once": parameters in, rules out.
    ActionType,
    /// "A piece of code-based logic that takes in input parameters and returns an output"; an
    /// action can be backed by one.
    Function,
    /// "A property that can be used on multiple object types": one definition, many owners.
    SharedProperty,
    /// "Semantic wrappers around a field type comprised of metadata and constraints": an
    /// Email, a PercentOfCapacity.
    ValueType,
    /// The dataset or model an object type is backed by.
    Datasource,
    /// "A label that helps you categorize your object types": a box around the object types
    /// it groups.
    ObjectTypeGroup,
    // composite
    Grouping,
    Location,
    // sketch
    Box,
    RoundedBox,
    Circle,
    Diamond,
    Cylinder,
    Cloud,
    Parallelogram,
    Hexagon,
    Text,
    Square,
    Ellipse,
    Triangle,
    PredefinedProcess,
    Document,
    InternalStorage,
    Cube,
    Step,
    Trapezoid,
    Tape,
    Note,
    Card,
    Callout,
    StickFigure,
    DataStorage,
    Delay,
    Display,
    ManualInput,
    OffPage,
    BlockArrow,
    DoubleArrow,
    And,
    Or,
}

use ShapeKind::*;

impl ShapeKind {
    /// Palette order: by layer, and within a layer active → behaviour → passive.
    pub const ALL: [ShapeKind; 83] = [
        Stakeholder, Driver, Assessment, Goal, Outcome, Principle, Requirement, Constraint,
        Resource, Capability, ValueStream, CourseOfAction,
        BusinessActor, BusinessRole, BusinessInterface, BusinessProcess, BusinessFunction,
        BusinessEvent, BusinessService, BusinessObject,
        ApplicationComponent, ApplicationInterface, ApplicationFunction, ApplicationProcess,
        ApplicationEvent, ApplicationService, DataObject,
        Node, Device, SystemSoftware, TechnologyInterface, CommunicationNetwork,
        TechnologyFunction, TechnologyProcess, TechnologyService, Artifact,
        WorkPackage, Deliverable, ImplementationEvent, Plateau, Gap,
        ObjectType, Interface, ActionType, Function, SharedProperty, ValueType, Datasource, ObjectTypeGroup,
        Grouping, Location,
        Box, RoundedBox, Circle, Diamond, Cylinder, Cloud, Parallelogram, Hexagon, Text,
        Square, Ellipse, Triangle, PredefinedProcess, Document, InternalStorage, Cube, Step, Trapezoid, Tape, Note, Card, Callout, StickFigure, DataStorage, Delay, Display, ManualInput, OffPage, BlockArrow, DoubleArrow, And, Or,
    ];

    /// The full name, as the palette and the manual print it.
    pub fn name(self) -> &'static str {
        match self {
            Stakeholder => "Stakeholder",
            Driver => "Driver",
            Assessment => "Assessment",
            Goal => "Goal",
            Outcome => "Outcome",
            Principle => "Principle",
            Requirement => "Requirement",
            Constraint => "Constraint",
            Resource => "Resource",
            Capability => "Capability",
            ValueStream => "Value Stream",
            CourseOfAction => "Course of Action",
            BusinessActor => "Business Actor",
            BusinessRole => "Business Role",
            BusinessInterface => "Business Interface",
            BusinessProcess => "Business Process",
            BusinessFunction => "Business Function",
            BusinessEvent => "Business Event",
            BusinessService => "Business Service",
            BusinessObject => "Business Object",
            ApplicationComponent => "Application Component",
            ApplicationInterface => "Application Interface",
            ApplicationFunction => "Application Function",
            ApplicationProcess => "Application Process",
            ApplicationEvent => "Application Event",
            ApplicationService => "Application Service",
            DataObject => "Data Object",
            Node => "Node",
            Device => "Device",
            SystemSoftware => "System Software",
            TechnologyInterface => "Technology Interface",
            CommunicationNetwork => "Communication Network",
            TechnologyFunction => "Technology Function",
            TechnologyProcess => "Technology Process",
            TechnologyService => "Technology Service",
            Artifact => "Artifact",
            WorkPackage => "Work Package",
            Deliverable => "Deliverable",
            ImplementationEvent => "Implementation Event",
            Plateau => "Plateau",
            Gap => "Gap",
            ObjectType => "Object Type",
            Interface => "Interface",
            ActionType => "Action Type",
            Function => "Function",
            SharedProperty => "Shared Property",
            ValueType => "Value Type",
            Datasource => "Datasource",
            ObjectTypeGroup => "Object Type Group",
            Grouping => "Grouping",
            Location => "Location",
            Box => "Box",
            RoundedBox => "Rounded Box",
            Circle => "Circle",
            Diamond => "Diamond",
            Cylinder => "Cylinder",
            Cloud => "Cloud",
            Parallelogram => "Parallelogram",
            Hexagon => "Hexagon",
            Text => "Text",
            Square => "Square",
            Ellipse => "Ellipse",
            Triangle => "Triangle",
            PredefinedProcess => "Predefined Process",
            Document => "Document",
            InternalStorage => "Internal Storage",
            Cube => "Cube",
            Step => "Step",
            Trapezoid => "Trapezoid",
            Tape => "Tape",
            Note => "Note",
            Card => "Card",
            Callout => "Callout",
            StickFigure => "Stick Figure",
            DataStorage => "Data Storage",
            Delay => "Delay",
            Display => "Display",
            ManualInput => "Manual Input",
            OffPage => "Off-page Connector",
            BlockArrow => "Block Arrow",
            DoubleArrow => "Double Arrow",
            And => "And",
            Or => "Or",
        }
    }

    /// The word printed inside the card — short enough to fit beside a label on a small box,
    /// and the thing `:add` matches on when you cannot remember the full name.
    pub fn short(self) -> &'static str {
        match self {
            Stakeholder => "stakeholder",
            Driver => "driver",
            Assessment => "assessment",
            Goal => "goal",
            Outcome => "outcome",
            Principle => "principle",
            Requirement => "requirement",
            Constraint => "constraint",
            Resource => "resource",
            Capability => "capability",
            ValueStream => "value stream",
            CourseOfAction => "course",
            BusinessActor => "actor",
            BusinessRole => "role",
            BusinessInterface => "interface",
            BusinessProcess => "process",
            BusinessFunction => "function",
            BusinessEvent => "event",
            BusinessService => "service",
            BusinessObject => "object",
            ApplicationComponent => "component",
            ApplicationInterface => "interface",
            ApplicationFunction => "function",
            ApplicationProcess => "process",
            ApplicationEvent => "event",
            ApplicationService => "service",
            DataObject => "data",
            Node => "node",
            Device => "device",
            SystemSoftware => "sys sw",
            TechnologyInterface => "interface",
            CommunicationNetwork => "network",
            TechnologyFunction => "function",
            TechnologyProcess => "process",
            TechnologyService => "service",
            Artifact => "artifact",
            WorkPackage => "work pkg",
            Deliverable => "deliverable",
            ImplementationEvent => "event",
            Plateau => "plateau",
            Gap => "gap",
            ObjectType => "object",
            Interface => "interface",
            ActionType => "action",
            Function => "function",
            SharedProperty => "shared",
            ValueType => "value type",
            Datasource => "dataset",
            ObjectTypeGroup => "group",
            Grouping => "group",
            Location => "location",
            Box => "box",
            RoundedBox => "rounded",
            Circle => "circle",
            Diamond => "diamond",
            Cylinder => "cylinder",
            Cloud => "cloud",
            Parallelogram => "slant",
            Hexagon => "hexagon",
            Text => "text",
            Square => "square",
            Ellipse => "ellipse",
            Triangle => "triangle",
            PredefinedProcess => "pre-process",
            Document => "document",
            InternalStorage => "int storage",
            Cube => "cube",
            Step => "step",
            Trapezoid => "trapezoid",
            Tape => "tape",
            Note => "note",
            Card => "card",
            Callout => "callout",
            StickFigure => "figure",
            DataStorage => "data store",
            Delay => "delay",
            Display => "display",
            ManualInput => "manual in",
            OffPage => "off-page",
            BlockArrow => "arrow",
            DoubleArrow => "dbl arrow",
            And => "and",
            Or => "or",
        }
    }

    /// The name a file, `:add` and the manual use — lowercase, hyphenated, unambiguous.
    pub fn slug(self) -> &'static str {
        match self {
            Stakeholder => "stakeholder",
            Driver => "driver",
            Assessment => "assessment",
            Goal => "goal",
            Outcome => "outcome",
            Principle => "principle",
            Requirement => "requirement",
            Constraint => "constraint",
            Resource => "resource",
            Capability => "capability",
            ValueStream => "value-stream",
            CourseOfAction => "course-of-action",
            BusinessActor => "actor",
            BusinessRole => "role",
            BusinessInterface => "business-interface",
            BusinessProcess => "process",
            BusinessFunction => "business-function",
            BusinessEvent => "business-event",
            BusinessService => "business-service",
            BusinessObject => "business-object",
            ApplicationComponent => "component",
            ApplicationInterface => "app-interface",
            ApplicationFunction => "app-function",
            ApplicationProcess => "app-process",
            ApplicationEvent => "app-event",
            ApplicationService => "app-service",
            DataObject => "data-object",
            Node => "node",
            Device => "device",
            SystemSoftware => "system-software",
            TechnologyInterface => "tech-interface",
            CommunicationNetwork => "network",
            TechnologyFunction => "tech-function",
            TechnologyProcess => "tech-process",
            TechnologyService => "tech-service",
            Artifact => "artifact",
            WorkPackage => "work-package",
            Deliverable => "deliverable",
            ImplementationEvent => "implementation-event",
            Plateau => "plateau",
            Gap => "gap",
            ObjectType => "object_type",
            Interface => "interface",
            ActionType => "action_type",
            Function => "function",
            SharedProperty => "shared_property",
            ValueType => "value_type",
            Datasource => "datasource",
            ObjectTypeGroup => "object_type_group",
            Grouping => "grouping",
            Location => "location",
            Box => "box",
            RoundedBox => "rounded-box",
            Circle => "circle",
            Diamond => "diamond",
            Cylinder => "cylinder",
            Cloud => "cloud",
            Parallelogram => "parallelogram",
            Hexagon => "hexagon",
            Text => "text",
            Square => "square",
            Ellipse => "ellipse",
            Triangle => "triangle",
            PredefinedProcess => "predefined-process",
            Document => "document",
            InternalStorage => "internal-storage",
            Cube => "cube",
            Step => "step",
            Trapezoid => "trapezoid",
            Tape => "tape",
            Note => "note",
            Card => "card",
            Callout => "callout",
            StickFigure => "stick-figure",
            DataStorage => "data-storage",
            Delay => "delay",
            Display => "display",
            ManualInput => "manual-input",
            OffPage => "off-page",
            BlockArrow => "block-arrow",
            DoubleArrow => "double-arrow",
            And => "and",
            Or => "or",
        }
    }

    /// Resolve what somebody typed — a slug, a full name, or a unique prefix of either.
    pub fn parse(s: &str) -> Option<ShapeKind> {
        let want = s.trim().to_ascii_lowercase().replace(' ', "-");
        if want.is_empty() {
            return None;
        }
        if let Some(k) = ShapeKind::ALL.iter().find(|k| {
            k.slug() == want || k.name().to_ascii_lowercase().replace(' ', "-") == want
        }) {
            return Some(*k);
        }
        let mut hits = ShapeKind::ALL.iter().filter(|k| {
            k.slug().starts_with(&want)
                || k.name().to_ascii_lowercase().replace(' ', "-").starts_with(&want)
        });
        let first = hits.next()?;
        match hits.next() {
            None => Some(*first),
            Some(_) => None,
        }
    }

    pub fn layer(self) -> Layer {
        match self {
            Stakeholder | Driver | Assessment | Goal | Outcome | Principle | Requirement
            | Constraint => Layer::Motivation,
            Resource | Capability | ValueStream | CourseOfAction => Layer::Strategy,
            BusinessActor | BusinessRole | BusinessInterface | BusinessProcess
            | BusinessFunction | BusinessEvent | BusinessService | BusinessObject => Layer::Business,
            ApplicationComponent | ApplicationInterface | ApplicationFunction
            | ApplicationProcess | ApplicationEvent | ApplicationService | DataObject => {
                Layer::Application
            }
            Node | Device | SystemSoftware | TechnologyInterface | CommunicationNetwork
            | TechnologyFunction | TechnologyProcess | TechnologyService | Artifact => {
                Layer::Technology
            }
            WorkPackage | Deliverable | ImplementationEvent | Plateau | Gap => Layer::Implementation,
            ObjectType | Interface | ActionType | Function | SharedProperty | ValueType | Datasource | ObjectTypeGroup => Layer::Ontology,
            Grouping | Location => Layer::Composite,
            Box | RoundedBox | Circle | Diamond | Cylinder | Cloud | Parallelogram | Hexagon | Text => Layer::Sketch,
            Square | Ellipse | Triangle | PredefinedProcess | Document | InternalStorage | Cube | Step | Trapezoid | Tape | Note | Card | Callout | StickFigure | DataStorage | Delay | Display | ManualInput | OffPage | BlockArrow | DoubleArrow | And | Or => Layer::Sketch,
        }
    }

    pub fn category(self) -> Category {
        match self {
            Stakeholder | Driver | Assessment | Goal | Outcome | Principle | Requirement
            | Constraint => Category::Motivation,
            Resource | BusinessActor | BusinessRole | BusinessInterface | ApplicationComponent
            | ApplicationInterface | Node | Device | SystemSoftware | TechnologyInterface
            | CommunicationNetwork => Category::Active,
            Capability | ValueStream | CourseOfAction | BusinessProcess | BusinessFunction
            | BusinessEvent | BusinessService | ApplicationFunction | ApplicationProcess
            | ApplicationEvent | ApplicationService | TechnologyFunction | TechnologyProcess
            | TechnologyService => Category::Behaviour,
            BusinessObject | DataObject | Artifact => Category::Passive,
            WorkPackage | Deliverable | ImplementationEvent | Plateau | Gap => {
                Category::Implementation
            }
            ObjectType | Interface | SharedProperty | ValueType | Datasource => Category::Passive,
            ActionType | Function => Category::Behaviour,
            ObjectTypeGroup => Category::Composite,
            Grouping | Location => Category::Composite,
            Box | RoundedBox | Circle | Diamond | Cylinder | Cloud | Parallelogram | Hexagon | Text => Category::Shape,
            Square | Ellipse | Triangle | PredefinedProcess | Document | InternalStorage | Cube | Step | Trapezoid | Tape | Note | Card | Callout | StickFigure | DataStorage | Delay | Display | ManualInput | OffPage | BlockArrow | DoubleArrow | And | Or => Category::Shape,
        }
    }

    pub fn shape(self) -> Shape {
        match self {
            // Motivation: the people are round, the intentions are boxes, the rules are
            // slanted — a requirement leans on you.
            Stakeholder | Driver => Shape::Ellipse,
            Assessment | Goal | Outcome => Shape::RoundedRectangle,
            Principle => Shape::Rectangle,
            Requirement | Constraint => Shape::Parallelogram,
            // Strategy: a capability is a hexagon (it has facets), a value stream leans the
            // way it flows.
            Resource => Shape::Rectangle,
            Capability => Shape::Hexagon,
            ValueStream => Shape::Parallelogram,
            CourseOfAction => Shape::RoundedRectangle,
            // The core stack: structure is square, behaviour is rounded, an actor is round, an
            // event is a diamond, data is a cylinder, a network is a cloud.
            BusinessActor => Shape::Ellipse,
            BusinessRole | BusinessInterface | ApplicationInterface | TechnologyInterface => {
                Shape::RoundedRectangle
            }
            BusinessProcess | BusinessFunction | BusinessService | ApplicationFunction
            | ApplicationProcess | ApplicationService | TechnologyFunction | TechnologyProcess
            | TechnologyService => Shape::RoundedRectangle,
            BusinessEvent | ApplicationEvent | ImplementationEvent => Shape::Diamond,
            BusinessObject | Artifact | Deliverable => Shape::Rectangle,
            DataObject => Shape::Cylinder,
            ApplicationComponent | Node | Device | SystemSoftware => Shape::Rectangle,
            CommunicationNetwork => Shape::Cloud,
            WorkPackage => Shape::RoundedRectangle,
            Plateau => Shape::Rectangle,
            Gap => Shape::Parallelogram,
            ObjectType | Interface => Shape::Rectangle,
            ActionType => Shape::Step,
            Function => Shape::RoundedRectangle,
            SharedProperty => Shape::Card,
            ValueType => Shape::Note,
            Datasource => Shape::Cylinder,
            ObjectTypeGroup => Shape::Dashed,
            Grouping | Location => Shape::Dashed,
            // The plain shapes are their own outline, and nothing else.
            Box => Shape::Rectangle,
            RoundedBox => Shape::RoundedRectangle,
            Circle => Shape::Ellipse,
            Diamond => Shape::Diamond,
            Cylinder => Shape::Cylinder,
            Cloud => Shape::Cloud,
            Parallelogram => Shape::Parallelogram,
            Hexagon => Shape::Hexagon,
            Text => Shape::Text,
            Square => Shape::Rectangle,
            Ellipse => Shape::Ellipse,
            Triangle => Shape::Triangle,
            PredefinedProcess => Shape::PredefinedProcess,
            Document => Shape::Document,
            InternalStorage => Shape::InternalStorage,
            Cube => Shape::Cube,
            Step => Shape::Step,
            Trapezoid => Shape::Trapezoid,
            Tape => Shape::Tape,
            Note => Shape::Note,
            Card => Shape::Card,
            Callout => Shape::Callout,
            StickFigure => Shape::StickFigure,
            DataStorage => Shape::DataStorage,
            Delay => Shape::Delay,
            Display => Shape::Display,
            ManualInput => Shape::ManualInput,
            OffPage => Shape::OffPage,
            BlockArrow => Shape::BlockArrow,
            DoubleArrow => Shape::DoubleArrow,
            And => Shape::And,
            Or => Shape::Or,
        }
    }

    /// The size a fresh element is placed at, in cells. Groupings are big because they are
    /// meant to have things inside them.
    pub fn default_size(self) -> (f64, f64) {
        match self {
            Grouping | Location => (30.0, 12.0),
            ObjectType | Interface => (22.0, 6.0),
            ActionType => (20.0, 5.0),
            SharedProperty | ValueType => (16.0, 4.0),
            Datasource => (14.0, 5.0),
            ObjectTypeGroup => (36.0, 14.0),
            Circle => (12.0, 6.0),
            Cloud => (16.0, 7.0),
            Text => (14.0, 3.0),
            Square => (10.0, 5.0),
            Ellipse => (18.0, 6.0),
            Triangle => (14.0, 6.0),
            PredefinedProcess => (18.0, 5.0),
            Document => (16.0, 6.0),
            InternalStorage => (16.0, 6.0),
            Cube => (16.0, 6.0),
            Tape => (16.0, 6.0),
            Note => (16.0, 6.0),
            Callout => (16.0, 6.0),
            StickFigure => (10.0, 6.0),
            OffPage => (12.0, 5.0),
            DoubleArrow => (18.0, 5.0),
            And => (12.0, 5.0),
            Or => (12.0, 5.0),

            CommunicationNetwork => (16.0, 6.0),
            DataObject => (12.0, 6.0),
            BusinessEvent | ApplicationEvent | ImplementationEvent => (14.0, 5.0),
            _ => (16.0, 5.0),
        }
    }

    pub fn is_service(self) -> bool {
        matches!(self, BusinessService | ApplicationService | TechnologyService)
    }

    /// A box drawn around other elements — a grouping, a location, an object type group:
    /// open, so what is inside stays legible, and moving it moves what is inside.
    pub fn is_composite(self) -> bool {
        self.category() == Category::Composite
    }

    /// A plain shape — one the rules leave alone, and one whose kind is not worth printing
    /// in its corner.
    pub fn is_sketch(self) -> bool {
        self.layer() == Layer::Sketch
    }

    pub fn is_interface(self) -> bool {
        matches!(
            self,
            BusinessInterface | ApplicationInterface | TechnologyInterface
        )
    }

    /// One line for the palette row, beside the name. What it *is*, not what it is called —
    /// typing `server` finds Node, because that is what you would have called it.
    pub fn tagline(self) -> &'static str {
        match self {
            Stakeholder => "a person or group with an interest in the outcome",
            Driver => "a condition that motivates change — a market, a regulation, a cost",
            Assessment => "a finding about a driver: a SWOT line, an audit result",
            Goal => "an intended end state, stated by a stakeholder",
            Outcome => "a measurable result a goal has been turned into",
            Principle => "a general rule the architecture should honour",
            Requirement => "a statement of what must be realized",
            Constraint => "a requirement that limits how, not what",
            Resource => "an asset the enterprise owns — people, money, an installed base",
            Capability => "an ability the enterprise possesses — what it can do",
            ValueStream => "a sequence of stages that together produce value for someone",
            CourseOfAction => "a strategy or tactic — a way of using capabilities",
            BusinessActor => "an organisational entity that acts: a person, a department, a customer",
            BusinessRole => "a responsibility an actor can take on",
            BusinessInterface => "a channel through which a business service is reached",
            BusinessProcess => "a sequence of activities that produces a business result",
            BusinessFunction => "a grouping of behaviour by skill or resource, not by sequence",
            BusinessEvent => "something that happens and influences business behaviour",
            BusinessService => "a defined behaviour the organisation exposes to its environment",
            BusinessObject => "a concept used in the business — an order, a claim, a contract",
            ApplicationComponent => "a deployable unit of software — a system, a service, a module",
            ApplicationInterface => "a point of access to an application: an API, a screen, a queue",
            ApplicationFunction => "automated behaviour, grouped by what it does",
            ApplicationProcess => "automated behaviour, in sequence",
            ApplicationEvent => "an application state change worth reacting to",
            ApplicationService => "automated behaviour exposed for others to use — an API's promise",
            DataObject => "data an application holds — a table, a document, a message; a database",
            Node => "a computing resource — a server, a virtual machine, a container host",
            Device => "physical hardware — a laptop, a phone, a sensor",
            SystemSoftware => "software a node runs — an OS, a runtime, a database engine",
            TechnologyInterface => "a point of access to a node's services — a port, a protocol",
            CommunicationNetwork => "a network linking nodes — a LAN, a VPC, the internet",
            TechnologyFunction => "behaviour a node performs, grouped by what it does",
            TechnologyProcess => "behaviour a node performs, in sequence — a job, a pipeline",
            TechnologyService => "what a node offers to the layer above — storage, compute, messaging",
            Artifact => "a physical piece of data — a binary, a file, a container image",
            WorkPackage => "a unit of work — a project, a sprint, a migration step",
            Deliverable => "a precisely defined result of a work package",
            ImplementationEvent => "a milestone: a go-live, a decommission, a decision",
            Plateau => "a stable state of the architecture at a point in time — as-is, to-be",
            Gap => "the difference between two plateaus — what has to change",
            ObjectType => "the schema of an entity or event — an Airport, an Order — with its properties",
            Interface => "the shape object types share — abstract, implemented, extended",
            ActionType => "a set of edits made at once — parameters in, create / modify / delete rules out",
            Function => "code-based logic with inputs and an output — what a function-backed action calls",
            SharedProperty => "one property definition used on many object types",
            ValueType => "a field type with meaning and constraints — an Email, a PercentOfCapacity",
            Datasource => "the dataset or model an object type is backed by",
            ObjectTypeGroup => "a label that groups object types — a box around them",
            Grouping => "a box around related elements, of any kind — a domain, a system boundary",
            Location => "a place — a data centre, a region, an office",
            Box => "a plain rectangle — a box that means whatever you write in it",
            RoundedBox => "a plain rounded rectangle",
            Circle => "a plain ellipse — a circle, a node in a sketch",
            Diamond => "a plain diamond — a decision, a junction",
            Cylinder => "a plain cylinder — a database, a store",
            Cloud => "a plain cloud — the internet, somewhere else",
            Parallelogram => "a plain parallelogram — an input, an output",
            Hexagon => "a plain hexagon",
            Text => "a label on its own, with no outline — a note, a title, a caption",
            Square => "a plain square",
            Ellipse => "a plain ellipse, wider than it is tall",
            Triangle => "a plain triangle, point up",
            PredefinedProcess => "flowchart: a rectangle with double sides — a subroutine",
            Document => "flowchart: a rectangle with a wavy bottom — a document",
            InternalStorage => "flowchart: a rectangle with a ruled corner — memory",
            Cube => "a box with a third face — a cube",
            Step => "a chevron block — one step in a sequence",
            Trapezoid => "a plain trapezoid, narrower at the top",
            Tape => "flowchart: wavy top and bottom — a tape",
            Note => "a rectangle with a folded corner — a note",
            Card => "a rectangle with a cut corner — a card",
            Callout => "a speech bubble — a callout",
            StickFigure => "a stick figure — a person",
            DataStorage => "flowchart: curved sides — a store of data",
            Delay => "flowchart: a rounded right side — a delay",
            Display => "flowchart: pointed left, rounded right — a display",
            ManualInput => "flowchart: a slanted top — input by hand",
            OffPage => "flowchart: a pentagon pointing down — continues elsewhere",
            BlockArrow => "a block arrow, pointing right",
            DoubleArrow => "a block arrow pointing both ways",
            And => "a half-ellipse, flat on the left — a logic AND",
            Or => "a leaf shape, flat on the left — a logic OR",
        }
    }

    /// The paragraph the manual and the ontology emit. What it is for, and the mistake people
    /// make with it.
    pub fn summary(self) -> &'static str {
        match self {
            Stakeholder => "Someone with a stake. Stakeholders have concerns, which become drivers; they do not have goals directly — the goal is what you write down once you know what they care about.",
            Driver => "Why change is wanted at all. Internal (cost, quality) or external (a regulation, a competitor). Assess a driver to find out what it actually means for you.",
            Assessment => "The result of looking at a driver: a strength, a weakness, an opportunity, a threat. An assessment influences goals; it does not become one.",
            Goal => "A high-level statement of intent. \"Halve time-to-market.\" Goals are refined into outcomes, and realized by the rest of the architecture.",
            Outcome => "A goal made measurable: \"releases ship weekly by Q3\". The thing you can point at afterwards and say whether it happened.",
            Principle => "A rule that guides design without prescribing it: \"buy before build\". Principles are realized by requirements that make them concrete.",
            Requirement => "What the architecture must do. Realized by any core element — a service, a component, a node — which is how you trace a box on a diagram back to the reason it exists.",
            Constraint => "A requirement about the how: a budget, a mandated platform, a deadline. The same shape as a requirement, deliberately, because it is one.",
            Resource => "What the enterprise has: staff, funding, an installed base, a brand. Resources are assigned to capabilities — they are what a capability is built from.",
            Capability => "What the enterprise can do, regardless of how: \"customer onboarding\", \"fraud detection\". Realized by the business and application behaviour that actually does it. Capabilities compose into capability maps.",
            ValueStream => "The stages by which value reaches a customer, in order. Each stage is served by capabilities. Not a process — coarser, and about value rather than activity.",
            CourseOfAction => "A way of using capabilities to reach a goal: a strategy, a tactic. Realizes goals and outcomes; influences other courses of action.",
            BusinessActor => "Who does the work: a person, a team, a partner organisation, a customer. An actor is assigned to the roles it plays.",
            BusinessRole => "A responsibility, separate from whoever holds it today: \"approver\", \"account manager\". Roles are assigned to processes and functions, so the process does not change when the person does.",
            BusinessInterface => "Where a business service is reached: a counter, a phone line, a web shop. Assigned to the roles that staff it; serves whoever comes through it.",
            BusinessProcess => "Activities in sequence, producing a defined result. Processes are triggered by events, realize services, and access business objects. Roles are assigned to them.",
            BusinessFunction => "Behaviour grouped by skill or resource rather than by sequence: \"claims handling\" as a department does it, not as a workflow. Composes processes.",
            BusinessEvent => "Something that happens: an order arrives, a deadline passes. Events trigger processes and are triggered by them.",
            BusinessService => "What the business offers, seen from outside: \"account opening\". Realized by a process; serves actors, roles and other processes. The unit of value the layer above sees.",
            BusinessObject => "A thing the business talks about: an order, a policy, an invoice. Realized by a data object in the application layer. Accessed by processes.",
            ApplicationComponent => "A unit of software that can be deployed and replaced as one: a system, a microservice, a library. Realizes application services, is assigned to functions, accesses data. This is the box most solution diagrams are made of.",
            ApplicationInterface => "How a component is reached: a REST API, a screen, a message queue. Assigned to the component that exposes it; serves the components and processes that use it.",
            ApplicationFunction => "What a component does, by kind: \"validate order\", \"render invoice\". Assigned to a component; realizes an application service.",
            ApplicationProcess => "What a component does, in sequence: a batch job, a saga. Triggered by events; realizes a service.",
            ApplicationEvent => "A state change in an application worth telling somebody about: \"order placed\". Triggers processes and functions. What an event bus carries.",
            ApplicationService => "Behaviour a component exposes for others to use — an API's promise, not its endpoint. Realized by a component or function; serves business processes and other components. The seam between application and business.",
            DataObject => "Data an application holds and other applications want: a table, a document, a message body. Realizes a business object; accessed by functions and processes. Draw a database as one of these.",
            Node => "Something that computes: a physical server, a VM, a cluster, a container host. Nodes host system software and artifacts, and compose devices and networks into a technology landscape.",
            Device => "Physical hardware: a laptop, a phone, a sensor, a rack. A device is a node you can touch. Assigned to the system software it runs.",
            SystemSoftware => "Software that other software runs on: an operating system, a JVM, a database engine, a message broker. Assigned to a node; realizes application components.",
            TechnologyInterface => "Where a node's services are reached: a port, a protocol endpoint. Assigned to the node; serves the nodes and components that use it.",
            CommunicationNetwork => "The links between nodes: a LAN, a VPC, a VPN, the public internet. Associated with the nodes it connects.",
            TechnologyFunction => "What a node does, by kind: \"store\", \"route\", \"encrypt\". Assigned to a node; realizes a technology service.",
            TechnologyProcess => "What a node does, in sequence: a backup job, a deployment pipeline. Triggered by events; realizes a service.",
            TechnologyService => "What infrastructure offers upward: storage, messaging, compute, identity. Realized by nodes and their functions; serves application components. The seam between technology and application.",
            Artifact => "A file: a binary, a container image, a configuration, a data dump. Assigned to the node it is deployed on; realizes an application component or a data object.",
            WorkPackage => "A unit of work with a start and an end: a project, an epic, a migration wave. Triggers other work packages; realizes deliverables. Roles and resources are assigned to it.",
            Deliverable => "A precisely defined thing a work package produces: a document, a deployed system. Realizes plateaus and other core elements.",
            ImplementationEvent => "A milestone: go-live, decommission, a decision point. Triggers work packages.",
            Plateau => "The architecture at a moment: as-is, transition, to-be. A plateau aggregates the elements that exist in that state. Gaps sit between plateaus.",
            Gap => "What differs between two plateaus — what has to be built, moved or retired. Associated with the plateaus on either side.",
            ObjectType => "The schema definition of a real-world entity or event; an object is one instance, an object set many. Carries its properties — one the primary key, one the title — its API name and status. Joined to other object types by link types, implements interfaces, is edited by action types, is backed by a datasource.",
            Interface => "An Ontology type that describes the shape of an object type and its capabilities. Abstract: never backed by data, never instantiated directly. Object types implement it, mapping their properties to its; a child interface extends it. An action may act on everything that implements it.",
            ActionType => "The schema definition of a set of changes a user makes at once: parameters in, rules out — create, modify or delete objects, create or delete links, on an object type or on everything implementing an interface — with submission criteria and side effects. Drawn as a node of its own, joined by its verbs to the types it touches.",
            Function => "Code-based logic with input parameters and an output. A function rule lets an action's edits be computed rather than declared: the action calls the function.",
            SharedProperty => "A property defined once and used on many object types, so a field means the same thing everywhere it appears. An object type's property row marks it; draw the shape when the sharing itself is the point.",
            ValueType => "A semantic wrapper around a field type — its base type, plus constraints (a pattern, a range, allowed values) and meaning. A property adopts it and inherits its rules.",
            Datasource => "The dataset or model that backs an object type — where its objects come from. Drawn when the picture is about lineage rather than shape.",
            ObjectTypeGroup => "A label that categorizes object types. A dashed box: the object types inside it belong to the group, and move with it.",
            Grouping => "A boundary drawn around related elements, of any kind: a system boundary, a domain, a bounded context. Elements inside it move with it. Aggregates whatever it contains.",
            Location => "A place: a data centre, a cloud region, an office. Aggregates the actors and nodes that are there.",
            Box => "A rectangle and nothing more. Use it for the parts of a picture the ontology has no word for — a legend, a system you have not classified yet, a thing from another notation. The rules leave plain shapes alone: a line between two of them is an association, an arrow is a flow, and nothing is ever refused.",
            RoundedBox => "A rounded rectangle and nothing more. Softer than a box; means whatever its label says.",
            Circle => "An ellipse and nothing more. A node in a sketch, a state, a bubble.",
            Diamond => "A diamond and nothing more. A decision in a flowchart, a junction, a gate.",
            Cylinder => "A cylinder and nothing more. The universal database glyph, without claiming it is a Data Object — use one of those when it is.",
            Cloud => "A cloud and nothing more. The internet, a third party, somewhere off the diagram.",
            Parallelogram => "A parallelogram and nothing more. A flowchart's input or output.",
            Hexagon => "A hexagon and nothing more.",
            Text => "A label standing on its own, no outline. A title for the diagram, a caption under a group, a note in the margin. It can be related like anything else, so a note can point at what it is about.",
            Square => "A square: a rectangle that is as tall as it is wide, in cells that are twice as tall as they are wide, so it looks square.",
            Ellipse => "An ellipse. Circle is the same shape at the proportions of a circle.",
            Triangle => "A triangle, point up. A warning, a delta, a direction.",
            PredefinedProcess => "The flowchart's predefined process: a rectangle with a second line down each side, meaning a step defined elsewhere. Not the architecture's Business Process, which has a kind and rules; this is a picture.",
            Document => "The flowchart's document: a page with a wavy foot.",
            InternalStorage => "The flowchart's internal storage: a rectangle with a line across the top and one down the left.",
            Cube => "A cube: a rectangle with its top and right faces showing. A server, a package, anything solid.",
            Step => "A step: a rectangle with a point on the right and a notch on the left, so a row of them reads as a sequence.",
            Trapezoid => "A trapezoid, narrower at the top. The flowchart's manual operation.",
            Tape => "The flowchart's tape: a rectangle with a wave along the top and the bottom.",
            Note => "A note: a rectangle with its top-right corner folded. A comment on the diagram, pointed at what it is about with a link.",
            Card => "A card: a rectangle with its top-left corner cut. The flowchart's punched card, a ticket, a record.",
            Callout => "A callout: a rectangle with a pointer at the bottom. What someone says about what it points at.",
            StickFigure => "A stick figure: a head, arms and legs. A person, a user, a role — as a picture. The architecture's Business Actor is the typed kind.",
            DataStorage => "The flowchart's data storage: a rectangle whose left side curves in and whose right side curves out.",
            Delay => "The flowchart's delay: a rectangle rounded on the right. A wait.",
            Display => "The flowchart's display: pointed on the left, rounded on the right. A screen, an output someone sees.",
            ManualInput => "The flowchart's manual input: a rectangle whose top slopes up to the right. Something typed in.",
            OffPage => "The flowchart's off-page connector: a pentagon pointing down. The diagram continues on another page — or, here, another tab.",
            BlockArrow => "A block arrow pointing right: a direction drawn as a shape rather than a line. Rotate it by making it a link instead.",
            DoubleArrow => "A block arrow pointing both ways: exchange, both directions, back and forth.",
            And => "The logic AND: flat on the left, rounded on the right.",
            Or => "The logic OR: flat on the left, curving to a point on the right.",
        }
    }
}

// ─── relations ──────────────────────────────────────────────────────────────

/// Every way two elements can be joined.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Composition,
    Aggregation,
    Assignment,
    Realization,
    Serving,
    Access,
    Influence,
    Triggering,
    Flow,
    Specialization,
    // ── the ontology layer's relations ──────────────────────────────────────────
    /// "The schema definition of a relationship between two object types"; its ends carry
    /// the cardinality — a bar for one, a crow's foot for many.
    LinkType,
    /// An object type implements an interface.
    Implements,
    /// A child interface extends its parent.
    Extends,
    /// An action type's rule creates objects of a type.
    Creates,
    /// An action type's rule modifies objects of a type.
    Modifies,
    /// An action type's rule deletes objects of a type.
    Deletes,
    /// An action type's rule creates links of a link type, on this end of it.
    Links,
    /// An action type's rule deletes links of a link type, on this end of it.
    Unlinks,
    /// A function-backed action calls its function.
    Calls,
    /// A type uses a shared property or a value type.
    Uses,
    /// An object type is backed by a datasource.
    BackedBy,
    Association,
    /// A plain link: a line whose look you choose, and which means nothing to the rules.
    /// What a freeform diagram joins its shapes with, and always allowed anywhere.
    Link,
}

use RelationKind::*;

/// How a relation is drawn: the line, the two ends, how thick, and in what colour.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Notation {
    pub line: LineStyle,
    pub tail: End,
    pub head: End,
    /// In braille dots: 1, 2 or 3. A kind's own notation is always 1.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub width: u8,
    /// A colour, or the relation's default grey.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Colour>,
    /// How the line gets from one end to the other.
    #[serde(default, skip_serializing_if = "Route::is_default")]
    pub route: Route,
    /// How big the ends are drawn.
    #[serde(default, skip_serializing_if = "EndSize::is_default")]
    pub end_size: EndSize,
    /// How solid the line is, 10 to 100 %.
    #[serde(default = "hundred", skip_serializing_if = "is_hundred")]
    pub opacity: u8,
}

fn one() -> u8 {
    1
}

fn hundred() -> u8 {
    100
}

fn is_hundred(n: &u8) -> bool {
    *n == 100
}

/// The path a relation's line takes: straight across, in right angles out of one port and
/// into the other, or a curve.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Route {
    #[default]
    Straight,
    Orthogonal,
    Curved,
}

impl Route {
    pub const ALL: [Route; 3] = [Route::Straight, Route::Orthogonal, Route::Curved];

    fn is_default(&self) -> bool {
        *self == Route::Straight
    }

    pub fn name(self) -> &'static str {
        match self {
            Route::Straight => "straight",
            Route::Orthogonal => "orthogonal",
            Route::Curved => "curved",
        }
    }

    pub fn parse(s: &str) -> Option<Route> {
        let want = s.trim().to_ascii_lowercase();
        Route::ALL.into_iter().find(|r| r.name().starts_with(&want) && !want.is_empty())
    }
}

/// How big a relation's ends are drawn: three quarters, the usual, or half again.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EndSize {
    Small,
    #[default]
    Normal,
    Large,
}

impl EndSize {
    pub const ALL: [EndSize; 3] = [EndSize::Small, EndSize::Normal, EndSize::Large];

    fn is_default(&self) -> bool {
        *self == EndSize::Normal
    }

    pub fn name(self) -> &'static str {
        match self {
            EndSize::Small => "small",
            EndSize::Normal => "normal",
            EndSize::Large => "large",
        }
    }

    pub fn parse(s: &str) -> Option<EndSize> {
        let want = s.trim().to_ascii_lowercase();
        EndSize::ALL.into_iter().find(|e| e.name().starts_with(&want) && !want.is_empty())
    }

    /// How much bigger or smaller than the usual end.
    pub fn scale(self) -> f64 {
        match self {
            EndSize::Small => 0.75,
            EndSize::Normal => 1.0,
            EndSize::Large => 1.5,
        }
    }
}

fn is_one(w: &u8) -> bool {
    *w == 1
}

/// The palette: the ten colours a shape or a link can be painted in, by name.
///
/// Named rather than hex, because a diagram is read on a dark terminal in one palette and the
/// point of a colour here is to *mean* something — a warning, a layer, a highlighted path —
/// not to match a brand. The values are the theme's own accents, so a painted shape sits in
/// the same picture as an unpainted one. `theme::paint` gives the pixels.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Paint {
    Red,
    Orange,
    Yellow,
    Green,
    Aqua,
    Blue,
    Purple,
    Sand,
    White,
    Grey,
}

impl Paint {
    pub const ALL: [Paint; 10] = [
        Paint::Red,
        Paint::Orange,
        Paint::Yellow,
        Paint::Green,
        Paint::Aqua,
        Paint::Blue,
        Paint::Purple,
        Paint::Sand,
        Paint::White,
        Paint::Grey,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Paint::Red => "red",
            Paint::Orange => "orange",
            Paint::Yellow => "yellow",
            Paint::Green => "green",
            Paint::Aqua => "aqua",
            Paint::Blue => "blue",
            Paint::Purple => "purple",
            Paint::Sand => "sand",
            Paint::White => "white",
            Paint::Grey => "grey",
        }
    }

    /// The colour as a web hex string on a dark ground — the value the sheet shows.
    pub fn hex(self) -> &'static str {
        match self {
            Paint::Red => "#cc241d",
            Paint::Orange => "#fe8019",
            Paint::Yellow => "#d79921",
            Paint::Green => "#8ec07c",
            Paint::Aqua => "#83a598",
            Paint::Blue => "#458588",
            Paint::Purple => "#d3869b",
            Paint::Sand => "#a89984",
            Paint::White => "#ebdbb2",
            Paint::Grey => "#928374",
        }
    }

    /// The colour on a dark or a light ground: a name means the same thing on either, and a
    /// pastel that glows on dark is invisible on paper, so each name has two values.
    pub fn on(self, light: bool) -> [u8; 3] {
        let i = Paint::ALL.iter().position(|p| *p == self).unwrap_or(0);
        if light { PAINTS_LIGHT[i] } else { PAINTS_DARK[i] }
    }

    pub fn parse(s: &str) -> Option<Paint> {
        let want = s.trim().to_ascii_lowercase();
        let want = if want == "gray" { "grey".to_string() } else { want };
        if want.is_empty() {
            return None;
        }
        if let Some(p) = Paint::ALL.into_iter().find(|p| p.name() == want) {
            return Some(p);
        }
        let mut hits = Paint::ALL.into_iter().filter(|p| p.name().starts_with(&want));
        let first = hits.next()?;
        hits.next().map_or(Some(first), |_| None)
    }

    pub fn rgb(self) -> [u8; 3] {
        self.on(false)
    }
}

/// The ten paints on a dark ground, in `Paint::ALL` order — gruvbox's accents.
pub const PAINTS_DARK: [[u8; 3]; 10] = [[204, 36, 29], [254, 128, 25], [215, 153, 33], [142, 192, 124], [131, 165, 152], [69, 133, 136], [211, 134, 155], [168, 153, 132], [235, 219, 178], [146, 131, 116]];
/// The same ten on paper — gruvbox's light variants, deep enough to read as ink.
pub const PAINTS_LIGHT: [[u8; 3]; 10] = [[157, 0, 6], [175, 58, 3], [181, 118, 20], [121, 116, 14], [66, 123, 88], [7, 102, 120], [143, 63, 113], [124, 111, 100], [60, 56, 54], [102, 92, 84]];

/// A colour, wherever one is set: one of the palette's names, or any colour by hex. Written
/// to the file as a string either way — `"purple"`, `"#e6e6e6"` — so a file reads the way
/// the sheet does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum Colour {
    Named(Paint),
    Hex([u8; 3]),
}

impl Colour {
    /// On a dark ground: what the sheet and the dark pictures use.
    pub fn rgb(self) -> [u8; 3] {
        self.on(false)
    }

    /// On a dark or a light ground: a name takes the ground's value, a hex is itself.
    pub fn on(self, light: bool) -> [u8; 3] {
        match self {
            Colour::Named(p) => p.on(light),
            Colour::Hex(c) => c,
        }
    }

    /// The hex on a given ground — the draw.io file is paper.
    pub fn hex_on(self, light: bool) -> String {
        let c = self.on(light);
        format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
    }

    pub fn hex(self) -> String {
        let c = self.rgb();
        format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
    }

    /// What the sheet shows: the name where there is one, else the hex.
    pub fn name(self) -> String {
        match self {
            Colour::Named(p) => p.name().to_string(),
            Colour::Hex(_) => self.hex(),
        }
    }

    /// A name (or a unique prefix of one), or a hex with or without its `#`, three or six
    /// digits.
    pub fn parse(s: &str) -> Option<Colour> {
        let t = s.trim();
        if let Some(p) = Paint::parse(t) {
            return Some(Colour::Named(p));
        }
        let h = t.trim_start_matches('#');
        let digits: Vec<u8> = h.chars().filter_map(|c| c.to_digit(16).map(|d| d as u8)).collect();
        if digits.len() != h.len() {
            return None;
        }
        match digits.len() {
            6 => Some(Colour::Hex([digits[0] * 16 + digits[1], digits[2] * 16 + digits[3], digits[4] * 16 + digits[5]])),
            3 => Some(Colour::Hex([digits[0] * 17, digits[1] * 17, digits[2] * 17])),
            _ => None,
        }
    }
}

impl From<Paint> for Colour {
    fn from(p: Paint) -> Colour {
        Colour::Named(p)
    }
}

impl From<Colour> for String {
    fn from(c: Colour) -> String {
        c.name()
    }
}

impl TryFrom<String> for Colour {
    type Error = String;
    fn try_from(s: String) -> Result<Colour, String> {
        Colour::parse(&s).ok_or_else(|| format!("not a colour: {s:?}"))
    }
}

/// `a` moved `t` of the way to `b`: what a colour looks like through a fill's opacity, or
/// tinted onto the terminal's ground.
pub fn mix(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

/// The eight looks a desktop tool offers as swatches: a fill and a line colour that go
/// together, by the name the eye gives the pair.
pub const LOOKS: [(&str, [u8; 3], [u8; 3]); 8] = [
    ("paper", [255, 255, 255], [0, 0, 0]),
    ("grey", [245, 245, 245], [102, 102, 102]),
    ("blue", [218, 232, 252], [108, 142, 191]),
    ("green", [213, 232, 212], [130, 179, 102]),
    ("orange", [255, 230, 204], [215, 155, 0]),
    ("yellow", [255, 242, 204], [214, 182, 86]),
    ("red", [248, 206, 204], [184, 84, 80]),
    ("purple", [225, 213, 231], [150, 115, 166]),
];

/// The standard picker's swatches: twelve hues by eight lightnesses, the way every colour
/// picker lays them out, plus a row of greys. Generated, not tabled.
pub fn swatches() -> Vec<Vec<[u8; 3]>> {
    let mut rows = Vec::new();
    // Greys, light to dark, white and black at the ends.
    rows.push((0..12).map(|i| { let v = 255 - (i as f64 * 255.0 / 11.0).round() as u8; [v, v, v] }).collect());
    // Lightness from pale to deep; hue round the wheel.
    for row in 0..8 {
        let l = 0.9 - row as f64 * 0.1;
        let s = if row < 2 { 0.6 } else { 0.85 };
        rows.push((0..12).map(|i| hsl(i as f64 * 30.0, s, l)).collect());
    }
    rows
}

fn hsl(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [((r + m) * 255.0).round() as u8, ((g + m) * 255.0).round() as u8, ((b + m) * 255.0).round() as u8]
}

#[cfg(test)]
mod colour_tests {
    use super::*;

    #[test]
    fn a_colour_is_a_name_or_a_hex_and_reads_back_from_either() {
        assert_eq!(Colour::parse("purple"), Some(Colour::Named(Paint::Purple)));
        assert_eq!(Colour::parse("#E6E6E6"), Some(Colour::Hex([230, 230, 230])));
        assert_eq!(Colour::parse("abc"), Some(Colour::Hex([170, 187, 204])));
        assert_eq!(Colour::parse("nope"), None);
        assert_eq!(Colour::Named(Paint::Red).hex(), "#cc241d");
        let s = serde_json::to_string(&Colour::Hex([1, 2, 3])).unwrap();
        assert_eq!(s, "\"#010203\"");
        assert_eq!(serde_json::from_str::<Colour>("\"aqua\"").unwrap(), Colour::Named(Paint::Aqua));
        assert!(serde_json::from_str::<Colour>("\"zz\"").is_err());
    }

    #[test]
    fn the_swatches_are_a_grid_with_greys_on_top() {
        let sw = swatches();
        assert_eq!(sw.len(), 9);
        assert!(sw.iter().all(|r| r.len() == 12));
        assert_eq!(sw[0][0], [255, 255, 255]);
        assert_eq!(sw[0][11], [0, 0, 0]);
        assert_eq!(hsl(0.0, 1.0, 0.5), [255, 0, 0]);
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl LineStyle {
    pub const ALL: [LineStyle; 3] = [LineStyle::Solid, LineStyle::Dashed, LineStyle::Dotted];

    pub fn name(self) -> &'static str {
        match self {
            LineStyle::Solid => "solid",
            LineStyle::Dashed => "dashed",
            LineStyle::Dotted => "dotted",
        }
    }

    pub fn parse(s: &str) -> Option<LineStyle> {
        let want = s.trim().to_ascii_lowercase();
        LineStyle::ALL.into_iter().find(|l| l.name() == want || (!want.is_empty() && l.name().starts_with(&want)))
    }
}

/// What sits at an end of a relation.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum End {
    #[default]
    None,
    /// A filled arrowhead — triggering, flow, assignment.
    Arrow,
    /// An open arrowhead (two strokes) — serving, access, influence.
    Open,
    /// A hollow triangle — realization, specialization.
    Triangle,
    /// A filled diamond — composition.
    Diamond,
    /// A hollow diamond — aggregation.
    HollowDiamond,
    /// A dot — the assigned-from end.
    Dot,
    /// A crow's foot — "many", in an entity-relationship diagram.
    Crow,
    /// A bar across the line — "one".
    Bar,
    /// A ring — "zero", optional.
    Circle,
}

impl End {
    pub const ALL: [End; 10] = [
        End::None,
        End::Arrow,
        End::Open,
        End::Triangle,
        End::Diamond,
        End::HollowDiamond,
        End::Dot,
        End::Crow,
        End::Bar,
        End::Circle,
    ];

    pub fn name(self) -> &'static str {
        match self {
            End::None => "none",
            End::Arrow => "arrow",
            End::Open => "open-arrow",
            End::Triangle => "triangle",
            End::Diamond => "diamond",
            End::HollowDiamond => "hollow-diamond",
            End::Dot => "dot",
            End::Crow => "crow",
            End::Bar => "bar",
            End::Circle => "circle",
        }
    }

    /// What it reads as, in the manual.
    pub fn describe(self) -> &'static str {
        match self {
            End::None => "nothing",
            End::Arrow => "a filled arrowhead",
            End::Open => "an open arrowhead",
            End::Triangle => "a hollow triangle",
            End::Diamond => "a filled diamond",
            End::HollowDiamond => "a hollow diamond",
            End::Dot => "a dot",
            End::Crow => "a crow's foot (many)",
            End::Bar => "a bar (one)",
            End::Circle => "a ring (zero, optional)",
        }
    }

    pub fn parse(s: &str) -> Option<End> {
        let want = s.trim().to_ascii_lowercase();
        if want.is_empty() {
            return None;
        }
        if let Some(e) = End::ALL.into_iter().find(|e| e.name() == want) {
            return Some(e);
        }
        let mut hits = End::ALL.into_iter().filter(|e| e.name().starts_with(&want));
        let first = hits.next()?;
        hits.next().map_or(Some(first), |_| None)
    }
}

impl RelationKind {
    /// Picker order: structural first, then dependency, then dynamic, then the catch-alls —
    /// association, which says the least, and the plain link, which says nothing at all.
    pub const ALL: [RelationKind; 23] = [
        Composition,
        Aggregation,
        Assignment,
        Realization,
        Serving,
        Access,
        Influence,
        Triggering,
        Flow,
        Specialization,
        LinkType,
        Implements,
        Extends,
        Creates,
        Modifies,
        Deletes,
        Links,
        Unlinks,
        Calls,
        Uses,
        BackedBy,
        Association,
        Link,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Composition => "composition",
            Aggregation => "aggregation",
            Assignment => "assignment",
            Realization => "realization",
            Serving => "serving",
            Access => "access",
            Influence => "influence",
            Triggering => "triggering",
            Flow => "flow",
            Specialization => "specialization",
            LinkType => "link-type",
            Implements => "implements",
            Extends => "extends",
            Creates => "creates",
            Modifies => "modifies",
            Deletes => "deletes",
            Links => "links",
            Unlinks => "unlinks",
            Calls => "calls",
            Uses => "uses",
            BackedBy => "backed-by",
            Association => "association",
            Link => "link",
        }
    }

    /// The relations of the ontology layer, which mean nothing to the architecture.
    pub fn is_ontology(self) -> bool {
        matches!(self, LinkType | Implements | Extends | Creates | Modifies | Deletes | Links | Unlinks | Calls | Uses | BackedBy)
    }

    /// The verb, for reading a relation aloud: "A *is composed of* B".
    pub fn verb(self) -> &'static str {
        match self {
            Composition => "is composed of",
            Aggregation => "aggregates",
            Assignment => "is assigned to",
            Realization => "realizes",
            Serving => "serves",
            Access => "accesses",
            Influence => "influences",
            Triggering => "triggers",
            Flow => "flows to",
            Specialization => "specializes",
            LinkType => "is linked to",
            Implements => "implements",
            Extends => "extends",
            Creates => "creates",
            Modifies => "modifies",
            Deletes => "deletes",
            Links => "links",
            Unlinks => "unlinks",
            Calls => "calls",
            Uses => "uses",
            BackedBy => "is backed by",
            Association => "is associated with",
            Link => "is linked to",
        }
    }

    pub fn parse(s: &str) -> Option<RelationKind> {
        let want = s.trim().to_ascii_lowercase();
        if want.is_empty() {
            return None;
        }
        if let Some(k) = RelationKind::ALL.iter().find(|k| k.name() == want) {
            return Some(*k);
        }
        let mut hits = RelationKind::ALL.iter().filter(|k| k.name().starts_with(&want));
        let first = hits.next()?;
        hits.next().map_or(Some(*first), |_| None)
    }

    pub fn tagline(self) -> &'static str {
        match self {
            Composition => "the whole owns the part — the part cannot exist without it",
            Aggregation => "the whole groups the part — the part can exist on its own",
            Assignment => "the structure that performs this behaviour, or hosts this thing",
            Realization => "the concrete thing that makes an abstract one real",
            Serving => "this provides something the other uses",
            Access => "behaviour reading or writing a passive object",
            Influence => "one motivation element affecting another, for better or worse",
            Triggering => "this happening causes that to start",
            Flow => "something — information, goods, money — passes from this to that",
            Specialization => "this is a particular kind of that",
            LinkType => "a link type between two object types — the ends say one or many",
            Implements => "this object type conforms to that interface",
            Extends => "a child interface inheriting its parent's properties",
            Creates => "this action's rule creates objects of that type",
            Modifies => "this action's rule modifies objects of that type",
            Deletes => "this action's rule deletes objects of that type",
            Links => "this action's rule creates links on that type",
            Unlinks => "this action's rule deletes links on that type",
            Calls => "this action is backed by that function",
            Uses => "this type uses that shared property or value type",
            BackedBy => "this object type's objects come from that datasource",
            Association => "related, in a way none of the others says",
            Link => "a plain line — its look is yours to set, and it means nothing to the rules",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Composition => "Part-of, strongly: a process is composed of its sub-processes; a component of its modules. Delete the whole and the parts go with it. Both ends must be the same sort of thing on the same layer — a component is never composed of a process.",
            Aggregation => "Part-of, loosely: a plateau aggregates the elements in that state; a grouping aggregates whatever is drawn inside it. The part outlives the whole. Same layer and category, except that a composite may aggregate anything.",
            Assignment => "Who does it, or where it runs. An actor is assigned to a role, a role to a process, a component to a function, an artifact to the node it is deployed on. Drawn from the structure to the behaviour it performs — read it as \"is responsible for\".",
            Realization => "How the abstract becomes concrete, and how a lower layer makes a higher one true. A process realizes a service; a component realizes an application service; system software realizes a component; a data object realizes a business object; any core element realizes a requirement. Points from the concrete to the abstract.",
            Serving => "Use without ownership. A service serves the processes that call it; an interface serves whoever reaches it; the application layer serves the business layer. Points from the provider to the user.",
            Access => "Behaviour touching data: a process accesses a business object, a function accesses a data object, a technology process accesses an artifact. Points from the behaviour to the object.",
            Influence => "Between reasons: an assessment influences a goal, a driver influences a stakeholder's concern, one requirement makes another harder. Motivation only.",
            Triggering => "Causation in time: an event triggers a process, one process triggers the next. Same layer, behaviour to behaviour. The arrow is the order things happen in.",
            Flow => "Something transferred: an order flows from one process to the next, data flows from one component to another. Like triggering, but the point is what moves rather than what starts.",
            Specialization => "Is-a: \"gold customer\" specializes \"customer\". Only between two elements of the same kind.",
            LinkType => "The schema definition of a relationship between two object types; a link is one instance. The ends are the cardinality, the way an entity-relationship diagram says it: a bar is one, a crow's foot is many — one-to-many by default, crow to crow for many-to-many (one-to-one is not offered). The centre label is the link type's name; the tail and head labels are the API name on each side. Between object types, or an interface as a link constraint.",
            Implements => "An object type conforms to an interface, mapping its properties to the interface's. Drawn from the object type to the interface, dashed, with a hollow head — realization's look, which is what it is.",
            Extends => "A child interface inherits its parent's properties and constraints. Interface to interface, solid, hollow head.",
            Creates => "An action type's create rule: the action makes objects of this type from its parameters. From the action to the type, dotted.",
            Modifies => "An action type's modify rule: the action changes property values on objects of this type. From the action to the type, dotted.",
            Deletes => "An action type's delete rule. From the action to the type, dotted.",
            Links => "An action type's create-link rule: the action links objects of this type. Draw one to each end of the link type it creates.",
            Unlinks => "An action type's delete-link rule. Draw one to each end of the link type it removes.",
            Calls => "A function-backed action: its edits are computed by the function rather than declared as rules. From the action to the function.",
            Uses => "A type uses a shared property or a value type. An object type's property row already marks the sharing; draw the line when the shared definition itself is on the diagram and the sharing is the point.",
            BackedBy => "An object type's objects come from a datasource — a dataset or a model. Drawn when the picture is about lineage.",
            Association => "The relation for when none of the others is right. Always allowed, and therefore says the least — reach for it last.",
            Link => "A line and nothing more, the way a general-purpose diagram tool draws one. What a freeform diagram joins its shapes with. Its look is configured on the link itself — the line solid, dashed or dotted, and either end nothing, an arrow, a triangle, a diamond, a dot, or an entity-relationship crow's foot, bar or ring. Always allowed, anywhere.",
        }
    }

    pub fn notation(self) -> Notation {
        use LineStyle::*;
        let n = |line, tail, head| Notation { line, tail, head, width: 1, color: None, route: crate::ontology::Route::Straight, end_size: crate::ontology::EndSize::Normal, opacity: 100 };
        match self {
            Composition => n(Solid, End::Diamond, End::None),
            Aggregation => n(Solid, End::HollowDiamond, End::None),
            Assignment => n(Solid, End::Dot, End::Arrow),
            Realization => n(Dashed, End::None, End::Triangle),
            Serving => n(Solid, End::None, End::Open),
            Access => n(Dotted, End::None, End::Open),
            Influence => n(Dashed, End::None, End::Open),
            Triggering => n(Solid, End::None, End::Arrow),
            Flow => n(Dashed, End::None, End::Arrow),
            Specialization => n(Solid, End::None, End::Triangle),
            LinkType => n(Solid, End::Bar, End::Crow),
            Implements => n(Dashed, End::None, End::Triangle),
            Extends => n(Solid, End::None, End::Triangle),
            Creates | Modifies | Deletes | Links | Unlinks => n(Dotted, End::None, End::Arrow),
            Calls => n(Dotted, End::None, End::Open),
            Uses => n(Dotted, End::None, End::None),
            BackedBy => n(Solid, End::None, End::Dot),
            Association => n(Solid, End::None, End::None),
            Link => n(Solid, End::None, End::Arrow),
        }
    }
}

// ─── the rules ──────────────────────────────────────────────────────────────

/// Whether `rel` may join `src` to `dst` — and if not, why not.
///
/// This is the whole grammar, in one function, and it is *advisory*: the app draws whatever
/// you ask it to, and marks a relation the rules refuse rather than refusing it. A diagram is
/// often a sketch before it is a model, and a tool that will not let you draw the wrong line
/// is a tool you stop using. But the mark is always there, `:lint` lists every one, and the
/// relation picker offers the allowed kinds first with the rest dimmed and the reason beside
/// them — which is how the rules teach rather than police.
///
/// The reasons are `&'static str` so that the picker, the status line and `:lint` all print
/// the same words for the same refusal.
pub fn allowed(rel: RelationKind, src: ShapeKind, dst: ShapeKind) -> Result<(), &'static str> {
    use Category as C;
    let (sl, dl) = (src.layer(), dst.layer());
    let (sc, dc) = (src.category(), dst.category());
    let core = |l: Layer| l.storey().is_some();
    // A plain shape means whatever its label says, so the rules have nothing to check: a line
    // is an association, an arrow is a flow, part-of is part-of, and everything else has no
    // meaning to attach to.
    if rel == Link {
        return Ok(());
    }
    // The ontology layer has a grammar of its own, and the architecture's relations mean
    // nothing there — nor its relations in the architecture.
    if rel.is_ontology() || sl == Layer::Ontology || dl == Layer::Ontology {
        return allowed_ontology(rel, src, dst);
    }
    if src.is_sketch() || dst.is_sketch() {
        return match rel {
            Association => Ok(()),
            Triggering | Flow => Ok(()),
            Specialization if src == dst => Ok(()),
            Specialization => Err("specialization joins two elements of the same kind"),
            Composition | Aggregation if src.is_sketch() && dst.is_sketch() => Ok(()),
            Composition | Aggregation if sc == C::Composite => Ok(()),
            _ => Err("a plain shape has no layer or category for this to mean anything — flow draws an arrow, association a line; give it a kind if it is one"),
        };
    }
    match rel {
        Association | Link => Ok(()),
        Specialization => match src == dst {
            true => Ok(()),
            false => Err("specialization joins two elements of the same kind"),
        },
        Composition | Aggregation => {
            // A composite may hold anything: that is what it is for.
            if sc == C::Composite {
                return Ok(());
            }
            if dc == C::Composite {
                return Err("a grouping or location is the whole, never the part");
            }
            // A plateau is the composite of the implementation layer.
            if src == Plateau && rel == Aggregation {
                return Ok(());
            }
            if sl != dl {
                return Err("a whole and its parts live on the same layer");
            }
            if sc != dc {
                return Err("a whole and its parts are the same sort of thing — structure of structure, behaviour of behaviour");
            }
            if sc == C::Motivation && rel == Composition {
                return Err("reasons aggregate; they are not composed — use aggregation");
            }
            Ok(())
        }
        Assignment => {
            // Structure performing behaviour, on the same layer — but a service is never
            // assigned: it is what the structure REALIZES, and the difference is the seam.
            if sc == C::Active && dc == C::Behaviour && sl == dl && !dst.is_service() {
                return Ok(());
            }
            if dst.is_service() {
                return Err("a service is realized, not assigned — the component or process behind it realizes it");
            }
            // Hosting: an actor takes a role, a node runs software and holds files, a device
            // is the hardware a node is.
            match (src, dst) {
                (BusinessActor, BusinessRole)
                | (BusinessRole, BusinessInterface)
                | (ApplicationComponent, ApplicationInterface)
                | (Node, SystemSoftware)
                | (Device, SystemSoftware)
                | (Device, Node)
                | (Node, Artifact)
                | (Device, Artifact)
                | (SystemSoftware, Artifact)
                | (Node, TechnologyInterface)
                | (Resource, Capability)
                | (Resource, CourseOfAction)
                | (BusinessActor, WorkPackage)
                | (BusinessRole, WorkPackage)
                | (Resource, WorkPackage) => return Ok(()),
                _ => {}
            }
            if sc != C::Active {
                return Err("assignment starts from structure — the actor, component or node that does it");
            }
            if sl != dl {
                return Err("structure is assigned to behaviour on its own layer; across layers, use realization or serving");
            }
            Err("assignment joins structure to the behaviour it performs, or to what it hosts")
        }
        Realization => {
            // Behaviour realizes a service on its own layer — and so does the structure that
            // performs it: "component realizes application service" is the most-drawn line
            // in any solution diagram, and it is the component's whole promise.
            if dst.is_service() && matches!(sc, C::Behaviour | C::Active) && sl == dl && !src.is_service() {
                return Ok(());
            }
            // A lower storey realizes the one above it.
            if let (Some(s), Some(d)) = (sl.storey(), dl.storey())
                && s == d + 1
                && dc != C::Motivation
            {
                return Ok(());
            }
            // Anything concrete realizes a reason, a capability or a course of action.
            if dc == C::Motivation && matches!(dst, Goal | Outcome | Principle | Requirement | Constraint) {
                return match sc {
                    C::Motivation => match (src, dst) {
                        (Requirement | Constraint, Principle | Goal | Outcome) => Ok(()),
                        (Outcome, Goal) => Ok(()),
                        (CourseOfAction, _) => Ok(()),
                        _ => Err("among reasons, a requirement realizes a principle or a goal, and an outcome realizes a goal"),
                    },
                    _ => Ok(()),
                };
            }
            if matches!(dst, Capability | ValueStream) && (core(sl) || sl == Layer::Strategy) {
                return Ok(());
            }
            // The implementation layer realizes the architecture.
            if src == Deliverable && (core(dl) || dl == Layer::Strategy || dst == Plateau) {
                return Ok(());
            }
            if src == WorkPackage && dst == Deliverable {
                return Ok(());
            }
            if dc == C::Motivation {
                return Err("only a goal, outcome, principle, requirement or constraint can be realized");
            }
            if sl == dl {
                return Err("on one layer, only a service is realized — by the process, function or component behind it");
            }
            Err("realization points up the stack: technology realizes application, application realizes business, and anything realizes a requirement")
        }
        Serving => {
            let provider = src.is_service() || src.is_interface();
            if !provider && sc != C::Behaviour && sc != C::Active {
                return Err("serving starts from something that provides — a service, an interface, or the behaviour and structure behind one");
            }
            if dc == C::Motivation || dc == C::Implementation || dc == C::Composite {
                return Err("a service serves the elements that use it — structure or behaviour, not reasons, plans or boxes");
            }
            match (sl.storey(), dl.storey()) {
                (Some(s), Some(d)) if d <= s => Ok(()),
                (Some(_), Some(_)) => Err("a lower layer serves the one above it, never the other way — technology serves application serves business"),
                (None, _) | (_, None) if sl == Layer::Strategy && dl == Layer::Strategy => Ok(()),
                _ => Err("serving runs within the core stack — business, application, technology"),
            }
        }
        Access => {
            if dc != C::Passive {
                return Err("access points at a passive object — a business object, a data object, an artifact");
            }
            if sc != C::Behaviour && sc != C::Active {
                return Err("access starts from behaviour or the structure that performs it");
            }
            if sl == dl {
                return Ok(());
            }
            Err("behaviour accesses the objects of its own layer; a data object realizes the business object it stands for")
        }
        Influence => {
            if dc != C::Motivation {
                return Err("influence lands on a reason — a driver, a goal, a requirement");
            }
            if sc == C::Motivation || sl == Layer::Strategy {
                return Ok(());
            }
            Err("influence is between reasons, or from a course of action to one")
        }
        Triggering | Flow => {
            let dynamic = |c: C| c == C::Behaviour || c == C::Active;
            if sc == C::Implementation && dc == C::Implementation {
                return match (src, dst) {
                    (WorkPackage | ImplementationEvent, WorkPackage | ImplementationEvent) => Ok(()),
                    _ => Err("in the plan, work packages and events trigger one another"),
                };
            }
            if !dynamic(sc) || !dynamic(dc) {
                return Err("triggering and flow join behaviour to behaviour — processes, functions, events, and the structure that performs them");
            }
            if sl != dl {
                return Err("triggering and flow stay on one layer; across layers a service serves");
            }
            Ok(())
        }
        LinkType | Implements | Extends | Creates | Modifies | Deletes | Links | Unlinks | Calls | Uses | BackedBy => {
            unreachable!("the ontology's relations were routed to their own rules above")
        }
    }
}

/// The ontology layer's grammar: which of Foundry's relations may join which of its types —
/// and where the ontology meets the rest of the architecture.
fn allowed_ontology(rel: RelationKind, src: ShapeKind, dst: ShapeKind) -> Result<(), &'static str> {
    use Category as C;
    let typed = |k: ShapeKind| matches!(k, ObjectType | Interface);
    let onto = |k: ShapeKind| k.layer() == Layer::Ontology;
    let both_ontology = onto(src) && onto(dst);
    // One side in the ontology, the other in the architecture: the bridges. An object type
    // is the digital twin of a business object, so it realizes it; a datasource realizes the
    // data object it holds; the application layer accesses object types the way it accesses
    // data; an action or a function serves the business that invokes it.
    if !both_ontology && !rel.is_ontology() && !src.is_sketch() && !dst.is_sketch() {
        return match rel {
            Association | Link => Ok(()),
            Composition | Aggregation if src.category() == C::Composite => Ok(()),
            Realization => match (src, dst) {
                (ObjectType, BusinessObject) | (Datasource, DataObject) | (Datasource, Artifact) => Ok(()),
                (ObjectType, _) => Err("an object type realizes the business object it is the twin of"),
                (_, ObjectType) => Err("an object type is realized by nothing but its datasource — draw backed-by"),
                _ => Err("across the seam, an object type realizes a business object and a datasource realizes a data object"),
            },
            Access => match (src.layer(), src.category(), dst) {
                (Layer::Application, C::Active | C::Behaviour, ObjectType | Interface | Datasource) => Ok(()),
                (_, _, ObjectType | Interface | Datasource) => Err("the application layer accesses object types — a component, a function, a process"),
                _ => Err("access from the ontology points at nothing: an action modifies, a function calls"),
            },
            Serving => match (src, dst.layer(), dst.category()) {
                (ActionType | Function, Layer::Business | Layer::Application, C::Active | C::Behaviour) => Ok(()),
                (ActionType | Function, ..) => Err("an action or a function serves the business or application that invokes it"),
                _ => Err("the ontology is served by nothing: the application accesses it"),
            },
            _ => Err("across the seam: an object type realizes a business object, the application accesses object types, an action serves a process"),
        };
    }
    match rel {
        Association | Link => Ok(()),
        // Foundry has no is-a between object types: an interface EXTENDS another, and an
        // object type IMPLEMENTS one.
        Specialization => Err("in an ontology, an interface extends another and an object type implements one — there is no specialization"),
        Composition | Aggregation => {
            if src.category() == C::Composite {
                return Ok(());
            }
            Err("in an ontology, an object type group is the only whole — the types inside it belong to it")
        }
        // An architecture relation means nothing to these types, and an ontology relation
        // means nothing to the architecture's.
        _ if !rel.is_ontology() => Err("that is an architecture relation; in an ontology the lines are link types, implements, extends, and an action's verbs"),
        _ if !both_ontology => Err("an ontology relation joins ontology types — object types, interfaces, action types; give the shape a kind from the ontology layer"),
        LinkType => match (typed(src), typed(dst)) {
            (true, true) => Ok(()),
            _ => Err("a link type joins two object types — or an interface, as a link constraint"),
        },
        Implements => match (src, dst) {
            (ObjectType, Interface) => Ok(()),
            (Interface, Interface) => Err("an interface extends another; an object type implements one"),
            _ => Err("implements runs from an object type to an interface"),
        },
        Extends => match (src, dst) {
            (Interface, Interface) => Ok(()),
            _ => Err("extends runs from a child interface to its parent"),
        },
        Creates | Modifies | Deletes | Links | Unlinks => match (src, typed(dst)) {
            (ActionType, true) => Ok(()),
            (ActionType, false) => Err("an action's rule acts on an object type, or on everything implementing an interface"),
            _ => Err("only an action type creates, modifies, deletes, links or unlinks"),
        },
        Calls => match (src, dst) {
            (ActionType, Function) => Ok(()),
            _ => Err("calls runs from an action type to the function that backs it"),
        },
        Uses => match (src, dst) {
            (ObjectType | Interface | ActionType, SharedProperty | ValueType) => Ok(()),
            (SharedProperty, ValueType) => Ok(()),
            _ => Err("uses runs from a type to a shared property or a value type"),
        },
        BackedBy => match (src, dst) {
            (ObjectType, Datasource) => Ok(()),
            _ => Err("backed-by runs from an object type to its datasource"),
        },
        _ => unreachable!("every architecture relation was refused above, and every ontology one matched"),
    }
}

/// The kinds a relation may take from `src` to `dst`, best first.
///
/// "Best" is the picker order in [`RelationKind::ALL`], which puts the specific relations
/// before the vague ones: association is always allowed and always last, so the default
/// offered is the most meaningful line the rules permit.
#[cfg(test)]
pub fn allowed_kinds(src: ShapeKind, dst: ShapeKind) -> Vec<RelationKind> {
    RelationKind::ALL
        .into_iter()
        .filter(|r| allowed(*r, src, dst).is_ok())
        .collect()
}

// ─── views ──────────────────────────────────────────────────────────────────

/// What kind of diagram this is — which decides which layers the palette offers.
///
/// A palette that lists all forty-odd kinds on a technology diagram is a menu of *choices*;
/// one that lists nodes, networks and the components they host is a menu of *possibilities*.
/// A view is the app's way of knowing the difference. `free` is the default and offers
/// everything.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum View {
    /// Plain shapes only — a whiteboard, the way a general-purpose diagram tool works. The
    /// other kind of diagram entirely: nothing here has a layer, so the rules have nothing to
    /// say and never do.
    Freeform,
    #[default]
    Free,
    Layered,
    Motivation,
    Business,
    Application,
    Technology,
    Implementation,
    /// The ontology layer, with the business it twins and the applications that read it.
    Ontology,
}

impl View {
    pub const ALL: [View; 9] = [
        View::Freeform,
        View::Free,
        View::Layered,
        View::Motivation,
        View::Business,
        View::Application,
        View::Technology,
        View::Implementation,
        View::Ontology,
    ];

    pub fn name(self) -> &'static str {
        match self {
            View::Freeform => "freeform",
            View::Free => "free",
            View::Layered => "layered",
            View::Motivation => "motivation",
            View::Business => "business",
            View::Application => "application",
            View::Technology => "technology",
            View::Implementation => "implementation",
            View::Ontology => "ontology",
        }
    }

    pub fn parse(s: &str) -> Option<View> {
        let want = s.trim().to_ascii_lowercase();
        View::ALL.into_iter().find(|v| v.name() == want).or_else(|| {
            let mut hits = View::ALL.into_iter().filter(|v| v.name().starts_with(&want));
            let first = hits.next()?;
            hits.next().map_or(Some(first), |_| None)
        })
    }

    pub fn tagline(self) -> &'static str {
        match self {
            View::Freeform => "plain shapes, like a whiteboard — no ontology, nothing refused",
            View::Free => "the architecture, every layer on offer",
            View::Layered => "the core stack: business over application over technology",
            View::Motivation => "stakeholders, drivers, goals and requirements — and the capabilities that answer them",
            View::Business => "actors, roles, processes and the services they offer",
            View::Application => "components, their services and data, and the business they serve",
            View::Technology => "nodes, networks and software, and the components they realize",
            View::Implementation => "work packages, plateaus and gaps against the architecture they change",
            View::Ontology => "object types, links, interfaces and actions, with the business they twin and the apps that read them",
        }
    }

    /// The layers this view is about. Composite and sketch are always in: a box around things
    /// is never out of place, and neither is a note.
    pub fn layers(self) -> &'static [Layer] {
        match self {
            View::Freeform => &[Layer::Sketch, Layer::Composite],
            View::Free => &Layer::ALL,
            View::Layered => &[Layer::Business, Layer::Application, Layer::Technology, Layer::Composite, Layer::Sketch],
            View::Motivation => &[Layer::Motivation, Layer::Strategy, Layer::Composite, Layer::Sketch],
            View::Business => &[Layer::Business, Layer::Composite, Layer::Sketch],
            View::Application => &[Layer::Application, Layer::Business, Layer::Composite, Layer::Sketch],
            View::Technology => &[Layer::Technology, Layer::Application, Layer::Composite, Layer::Sketch],
            View::Implementation => &[
                Layer::Implementation,
                Layer::Business,
                Layer::Application,
                Layer::Technology,
                Layer::Composite,
                Layer::Sketch,
            ],
            View::Ontology => &[Layer::Ontology, Layer::Business, Layer::Application, Layer::Composite, Layer::Sketch],
        }
    }

    pub fn shows(self, k: ShapeKind) -> bool {
        self.layers().contains(&k.layer())
    }

    /// The badge the header wears: which of the two kinds of diagram this is, and — for the
    /// architecture kind — which view narrows it.
    pub fn badge(self) -> String {
        match self {
            View::Freeform => "freeform".into(),
            View::Free => "architecture".into(),
            v => format!("architecture · {}", v.name()),
        }
    }
}

// ─── the rules, in words ────────────────────────────────────────────────────

/// What a model — or a person — has to know before drawing, that no table of kinds can say.
///
/// Each one is a lesson: the way a diagram goes wrong while looking right. They are emitted
/// with the ontology and printed in the manual, and phrased as the symptom you would see,
/// because the reader meets the symptom before the principle.
pub static RULES: &[&str] = &[
    "EVERY BOX IS A KIND. A rectangle labelled \"CRM\" is a picture; an application component \
     labelled \"CRM\" is a claim about what sort of thing it is, and that claim is what lets the \
     relations mean anything. Pick the kind before the label.",
    "A SERVICE IS THE SEAM. The layer above never touches a component, a node or a process \
     directly: it uses a SERVICE, which the component realizes. Draw business → application as \
     'process ← serves ← application service ← realizes ← component', and the component can be \
     replaced without the process noticing. A process served straight by a component is a \
     diagram that has hard-wired an implementation into a requirement.",
    "REALIZATION POINTS UP; SERVING POINTS UP. Technology realizes application realizes \
     business, and each serves the one above it. A serving arrow pointing down the stack is \
     almost always a realization drawn backwards.",
    "ASSIGNMENT IS WHO, NOT WHAT. It joins an actor to a role, a role to a process, a component \
     to a function, an artifact to the node it is deployed on. It never joins two behaviours \
     — that is triggering — and never crosses a layer — that is realization.",
    "ACCESS POINTS AT DATA. Only a business object, a data object or an artifact can be \
     accessed, and only by the behaviour of its own layer. A component 'accessing' a business \
     object is a data object missing from the diagram.",
    "TRIGGERING IS TIME. An arrow between two processes means the first ends and the second \
     starts. If what you mean is that data moves, it is FLOW; if what you mean is that one is \
     part of the other, it is COMPOSITION; if what you mean is that one calls the other, it is \
     SERVING. Four different facts are routinely drawn as one arrow.",
    "ASSOCIATION IS THE LAST RESORT. It is always allowed, which is exactly why it says nothing. \
     A diagram whose lines are all associations has been drawn, not modelled — every one of \
     them is a decision deferred.",
    "TRACE EVERY COMPONENT TO A REASON. A component that realizes no service, serves no \
     process and realizes no requirement is a box nobody asked for. It may be right; it is more \
     often the first thing to cut, and the diagram cannot tell you which until the line is \
     drawn.",
    "A GROUPING IS A BOUNDARY, NOT A LAYER. Put a system's components, data and interfaces \
     inside one grouping and it reads as a system; put every component of a layer in one and \
     it reads as a layer, which the layout already shows. Group what belongs together, not what \
     is the same kind.",
    "A PLAIN SHAPE IS A PICTURE, AND THAT IS ALLOWED. The sketch layer — box, circle, diamond, \
     cylinder, cloud, text — is for what the ontology has no word for: a legend, a note, a thing \
     from another notation, a system not yet classified. The rules leave it alone, which is the \
     point and the cost: a cylinder labelled 'customers' says nothing a Data Object would, so \
     reach for the typed kind once you know what the thing is.",
    "ONE DIAGRAM, ONE QUESTION. A layered view answers 'how does the stack fit together'; a \
     motivation view answers 'why'; an implementation view answers 'when'. A diagram that \
     tries to answer all three has fifty boxes and answers none. Set the view kind (:kind) \
     and let the palette narrow.",
    "AN OBJECT TYPE IS ITS PROPERTIES. In an ontology the box is the schema: a primary key, a \
     title, and typed properties. A link type's ends are its cardinality — a bar for one, a \
     crow's foot for many — and the label on each end is the API name from that side. A \
     picture of object types with no properties and unlabelled links is a picture, not an \
     ontology.",
    "AN ACTION IS A NODE, NOT AN ARROW. An action type touches several object types at once \
     — creates one, modifies another, links a third — and has parameters of its own. Draw it \
     as its own shape and join it by its verbs; an arrow labelled 'update' between two object \
     types hides both the parameters and the other types it edits.",
];

#[cfg(test)]
mod quick {
    use super::*;

    #[test]
    fn association_is_always_allowed_and_therefore_always_last() {
        for a in ShapeKind::ALL {
            for b in ShapeKind::ALL {
                assert_eq!(allowed(Association, a, b), Ok(()));
                assert_eq!(*allowed_kinds(a, b).last().unwrap(), Link, "the plain link is last of all");
            }
        }
    }
}
