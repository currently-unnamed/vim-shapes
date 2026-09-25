use serde::{Deserialize, Serialize};

pub type NodeId = u32;
pub type EdgeId = u32;

pub const CURRENT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Document {
    pub version: u32,
    #[serde(default)]
    pub metadata: DocumentMetadata,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub next_node_id: NodeId,
    #[serde(default)]
    pub next_edge_id: EdgeId,
}

impl Default for Document {
    fn default() -> Self {
        Document {
            version: CURRENT_VERSION,
            metadata: DocumentMetadata::default(),
            nodes: Vec::new(),
            edges: Vec::new(),
            next_node_id: 0,
            next_edge_id: 0,
        }
    }
}

impl Document {
    pub fn alloc_node_id(&mut self) -> NodeId {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }

    pub fn alloc_edge_id(&mut self) -> EdgeId {
        let id = self.next_edge_id;
        self.next_edge_id += 1;
        id
    }

    pub fn remove_node(&mut self, id: NodeId) {
        self.nodes.retain(|n| n.id != id);
        self.edges.retain(|e| e.from != id && e.to != id);
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DocumentMetadata {
    #[serde(default)]
    pub title: Option<String>,
    /// Free-form hint: "ea" | "sa" | "ontology" | user-defined. Not enforced
    /// against the shape palette - purely descriptive.
    #[serde(default)]
    pub diagram_kind: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    pub kind: ShapeKind,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub style: NodeStyle,
}

impl Node {
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    RoundedRectangle,
    Ellipse,
    Diamond,
    Cylinder,
    Cloud,
    Parallelogram,
}

impl ShapeKind {
    pub const ALL: [ShapeKind; 6] = [
        ShapeKind::RoundedRectangle,
        ShapeKind::Ellipse,
        ShapeKind::Diamond,
        ShapeKind::Cylinder,
        ShapeKind::Cloud,
        ShapeKind::Parallelogram,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            ShapeKind::RoundedRectangle => "Rounded Rectangle",
            ShapeKind::Ellipse => "Ellipse",
            ShapeKind::Diamond => "Diamond",
            ShapeKind::Cylinder => "Cylinder",
            ShapeKind::Cloud => "Cloud",
            ShapeKind::Parallelogram => "Parallelogram",
        }
    }

    pub fn default_size(&self) -> (f64, f64) {
        match self {
            ShapeKind::RoundedRectangle => (12.0, 5.0),
            ShapeKind::Ellipse => (12.0, 6.0),
            ShapeKind::Diamond => (12.0, 6.0),
            ShapeKind::Cylinder => (10.0, 7.0),
            ShapeKind::Cloud => (14.0, 7.0),
            ShapeKind::Parallelogram => (12.0, 5.0),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NodeStyle {
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(default)]
    pub stroke: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Edge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub style: EdgeStyle,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct EdgeStyle {
    #[serde(default)]
    pub arrow_head: ArrowHead,
    #[serde(default)]
    pub line: LineStyle,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArrowHead {
    #[default]
    Filled,
    Open,
    None,
    Diamond,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_round_trips_through_json() {
        let mut doc = Document::default();
        let n1 = doc.alloc_node_id();
        doc.nodes.push(Node {
            id: n1,
            kind: ShapeKind::Ellipse,
            x: 1.0,
            y: 2.0,
            w: 12.0,
            h: 6.0,
            label: "Service A".to_string(),
            style: NodeStyle::default(),
        });
        let n2 = doc.alloc_node_id();
        doc.nodes.push(Node {
            id: n2,
            kind: ShapeKind::Cylinder,
            x: 20.0,
            y: 2.0,
            w: 10.0,
            h: 7.0,
            label: "DB".to_string(),
            style: NodeStyle::default(),
        });
        let e1 = doc.alloc_edge_id();
        doc.edges.push(Edge {
            id: e1,
            from: n1,
            to: n2,
            label: Some("reads".to_string()),
            style: EdgeStyle::default(),
        });

        let json = serde_json::to_string(&doc).expect("serialize");
        let restored: Document = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.version, doc.version);
        assert_eq!(restored.nodes.len(), 2);
        assert_eq!(restored.edges.len(), 1);
        assert_eq!(restored.nodes[0].label, "Service A");
        assert_eq!(restored.edges[0].from, n1);
        assert_eq!(restored.edges[0].to, n2);
    }

    #[test]
    fn old_partial_json_still_loads_via_serde_default() {
        // Simulates a file saved before optional fields existed.
        let old_json = r#"{
            "version": 1,
            "nodes": [
                { "id": 0, "kind": "diamond", "x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0 }
            ],
            "edges": []
        }"#;
        let doc: Document = serde_json::from_str(old_json).expect("deserialize old file");
        assert_eq!(doc.nodes[0].label, "");
        assert!(doc.metadata.title.is_none());
    }

    #[test]
    fn remove_node_also_removes_incident_edges() {
        let mut doc = Document::default();
        let n1 = doc.alloc_node_id();
        let n2 = doc.alloc_node_id();
        doc.nodes.push(Node {
            id: n1,
            kind: ShapeKind::RoundedRectangle,
            x: 0.0,
            y: 0.0,
            w: 4.0,
            h: 4.0,
            label: String::new(),
            style: NodeStyle::default(),
        });
        doc.nodes.push(Node {
            id: n2,
            kind: ShapeKind::RoundedRectangle,
            x: 10.0,
            y: 0.0,
            w: 4.0,
            h: 4.0,
            label: String::new(),
            style: NodeStyle::default(),
        });
        let e1 = doc.alloc_edge_id();
        doc.edges.push(Edge {
            id: e1,
            from: n1,
            to: n2,
            label: None,
            style: EdgeStyle::default(),
        });

        doc.remove_node(n1);

        assert_eq!(doc.nodes.len(), 1);
        assert!(doc.edges.is_empty());
    }
}
