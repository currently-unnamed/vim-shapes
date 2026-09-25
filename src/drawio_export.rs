use std::path::Path;

use crate::model::{ArrowHead, Document, Edge, LineStyle, Node, ShapeKind};

/// 1 document grid unit = 40px = 4 draw.io grid squares (draw.io's default
/// `gridSize` is 10px). Independent of the terminal renderer's
/// GRID_UNIT_COLS/ROWS, which serve a different coordinate system.
const DRAWIO_UNIT_PX: f64 = 40.0;

/// Converts a Document into draw.io's uncompressed mxGraph XML format.
/// One-way only: there is no corresponding `.drawio` -> Document importer.
pub fn to_drawio_xml(document: &Document) -> String {
    let mut cells = String::new();
    for node in &document.nodes {
        cells.push_str(&node_cell_xml(node));
    }
    for edge in &document.edges {
        cells.push_str(&edge_cell_xml(edge));
    }

    format!(
        r#"<mxfile host="vim-shapes" version="1.0">
  <diagram name="Page-1" id="vim-shapes-export">
    <mxGraphModel dx="800" dy="600" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="850" pageHeight="1100" math="0" shadow="0">
      <root>
        <mxCell id="0" />
        <mxCell id="1" parent="0" />
{cells}      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
"#
    )
}

pub fn export(document: &Document, path: &Path) -> std::io::Result<()> {
    std::fs::write(path, to_drawio_xml(document))
}

fn node_cell_xml(node: &Node) -> String {
    let style = node_style_string(node);
    let x = node.x * DRAWIO_UNIT_PX;
    let y = node.y * DRAWIO_UNIT_PX;
    let w = node.w * DRAWIO_UNIT_PX;
    let h = node.h * DRAWIO_UNIT_PX;
    format!(
        "        <mxCell id=\"node-{id}\" value=\"{label}\" style=\"{style}\" vertex=\"1\" parent=\"1\">\n          <mxGeometry x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" as=\"geometry\" />\n        </mxCell>\n",
        id = node.id,
        label = xml_escape(&node.label),
    )
}

fn edge_cell_xml(edge: &Edge) -> String {
    let mut style = format!("endArrow={};html=1;", arrow_head_style(edge.style.arrow_head));
    if edge.style.line == LineStyle::Dashed {
        style.push_str("dashed=1;");
    }
    let label = edge.label.as_deref().unwrap_or("");
    format!(
        "        <mxCell id=\"edge-{id}\" value=\"{label}\" style=\"{style}\" edge=\"1\" parent=\"1\" source=\"node-{from}\" target=\"node-{to}\">\n          <mxGeometry relative=\"1\" as=\"geometry\" />\n        </mxCell>\n",
        id = edge.id,
        label = xml_escape(label),
        from = edge.from,
        to = edge.to,
    )
}

fn node_style_string(node: &Node) -> String {
    let mut style = match node.kind {
        ShapeKind::RoundedRectangle => "rounded=1;whiteSpace=wrap;html=1;arcSize=15;".to_string(),
        ShapeKind::Ellipse => "ellipse;whiteSpace=wrap;html=1;".to_string(),
        ShapeKind::Diamond => "rhombus;whiteSpace=wrap;html=1;".to_string(),
        ShapeKind::Cylinder => "shape=cylinder3;whiteSpace=wrap;html=1;".to_string(),
        ShapeKind::Cloud => "shape=cloud;whiteSpace=wrap;html=1;".to_string(),
        ShapeKind::Parallelogram => {
            "shape=parallelogram;perimeter=parallelogramPerimeter;whiteSpace=wrap;html=1;".to_string()
        }
    };
    if let Some(fill) = &node.style.fill {
        style.push_str(&format!("fillColor={fill};"));
    }
    if let Some(stroke) = &node.style.stroke {
        style.push_str(&format!("strokeColor={stroke};"));
    }
    style
}

fn arrow_head_style(arrow: ArrowHead) -> &'static str {
    match arrow {
        ArrowHead::Filled => "classic",
        ArrowHead::Open => "open",
        ArrowHead::None => "none",
        ArrowHead::Diamond => "diamond",
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeStyle, NodeStyle};

    fn sample_document() -> Document {
        let mut doc = Document::default();
        let a = doc.alloc_node_id();
        doc.nodes.push(Node {
            id: a,
            kind: ShapeKind::Cylinder,
            x: 1.0,
            y: 1.0,
            w: 10.0,
            h: 7.0,
            label: "DB & <cache>".to_string(),
            style: NodeStyle::default(),
        });
        let b = doc.alloc_node_id();
        doc.nodes.push(Node {
            id: b,
            kind: ShapeKind::RoundedRectangle,
            x: 20.0,
            y: 1.0,
            w: 12.0,
            h: 5.0,
            label: "Service".to_string(),
            style: NodeStyle::default(),
        });
        let e = doc.alloc_edge_id();
        doc.edges.push(Edge {
            id: e,
            from: b,
            to: a,
            label: Some("reads".to_string()),
            style: EdgeStyle::default(),
        });
        doc
    }

    #[test]
    fn every_shape_kind_maps_to_its_documented_style_string() {
        let cases = [
            (ShapeKind::RoundedRectangle, "rounded=1"),
            (ShapeKind::Ellipse, "ellipse;"),
            (ShapeKind::Diamond, "rhombus;"),
            (ShapeKind::Cylinder, "shape=cylinder3"),
            (ShapeKind::Cloud, "shape=cloud"),
            (ShapeKind::Parallelogram, "shape=parallelogram"),
        ];
        for (kind, expected_fragment) in cases {
            let node = Node {
                id: 0,
                kind,
                x: 0.0,
                y: 0.0,
                w: 4.0,
                h: 4.0,
                label: String::new(),
                style: NodeStyle::default(),
            };
            assert!(
                node_style_string(&node).contains(expected_fragment),
                "expected style for {kind:?} to contain {expected_fragment}"
            );
        }
    }

    #[test]
    fn edges_reference_the_correct_source_and_target_cells() {
        let doc = sample_document();
        let xml = to_drawio_xml(&doc);
        assert!(xml.contains(r#"source="node-1" target="node-0""#));
    }

    #[test]
    fn labels_are_xml_escaped() {
        let doc = sample_document();
        let xml = to_drawio_xml(&doc);
        assert!(xml.contains("DB &amp; &lt;cache&gt;"));
        assert!(!xml.contains("DB & <cache>"));
    }

    #[test]
    fn output_has_no_compression_and_is_a_well_formed_mxfile_shell() {
        let doc = sample_document();
        let xml = to_drawio_xml(&doc);
        assert!(xml.starts_with("<mxfile"));
        assert!(xml.contains("<mxGraphModel"));
        assert!(xml.contains(r#"<mxCell id="0" />"#));
        assert!(xml.contains(r#"<mxCell id="1" parent="0" />"#));
    }
}
