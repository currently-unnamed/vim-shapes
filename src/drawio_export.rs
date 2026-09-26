//! `:export <file.drawio>` — the diagram as a draw.io / diagrams.net file, so what was drawn in
//! a terminal can be handed to someone who has never opened one.
//!
//! The mapping is deliberate rather than pixel-faithful: each kind keeps its shape, each layer
//! takes the fill colour architects already read it in, and each relation takes the line and
//! arrowheads of its notation. Cells are ten pixels wide and twenty tall, which is roughly the
//! aspect of a terminal cell, so the picture keeps its proportions.

use std::path::Path;

use crate::model::{Document, Element, Relation};
use crate::ontology::{End, Layer, LineStyle, Shape, ShapeKind};

const CELL_W: f64 = 10.0;
const CELL_H: f64 = 20.0;

pub fn export(doc: &Document, path: &Path) -> std::io::Result<()> {
    std::fs::write(path, to_xml(doc))
}

pub fn to_xml(doc: &Document) -> String {
    let mut s = String::new();
    s.push_str("<mxfile host=\"vim-shapes\">\n");
    s.push_str(&format!(
        "  <diagram name=\"{}\" id=\"page-1\">\n",
        escape(doc.metadata.title.as_deref().unwrap_or("Page-1"))
    ));
    let page = &doc.metadata.page;
    let (pw, ph) = page.paper.px();
    let (pw, ph) = if page.landscape { (ph, pw) } else { (pw, ph) };
    s.push_str(&format!(
        "    <mxGraphModel grid=\"{}\" gridSize=\"{}\" page=\"1\" pageVisible=\"{}\" pageWidth=\"{pw}\" pageHeight=\"{ph}\"{}{}>\n",
        page.grid as u8,
        // Our dots sit every `grid_step` cells; a desktop grid's lines are a quarter of that.
        (page.grid_step().0 as f64 * CELL_W / 4.0).round(),
        page.page_view as u8,
        page.background.map(|c| format!(" background=\"{}\"", c.hex_on(true))).unwrap_or_default(),
        page.grid_color.map(|c| format!(" gridColor=\"{}\"", c.hex_on(true))).unwrap_or_default(),
    ));
    s.push_str("      <root>\n        <mxCell id=\"0\"/>\n        <mxCell id=\"1\" parent=\"0\"/>\n");
    // The three looks are per cell in the file, so each cell carries the diagram's.
    let looks = format!(
        "{}{}{}",
        if page.rounded { "rounded=1;" } else { "" },
        if page.sketch { "sketch=1;curveFitting=1;jiggle=2;" } else { "" },
        if page.shadow { "shadow=1;" } else { "" }
    );
    for e in &doc.elements {
        s.push_str(&element_cell(e, &looks));
    }
    for r in &doc.relations {
        s.push_str(&relation_cell(doc, r, page.sketch));
    }
    s.push_str("      </root>\n    </mxGraphModel>\n  </diagram>\n</mxfile>\n");
    s
}

fn element_cell(e: &Element, looks: &str) -> String {
    let fill = match e.fill {
        crate::model::Fill::Auto => layer_fill(e.kind.layer()).to_string(),
        crate::model::Fill::None => "none".to_string(),
        crate::model::Fill::Colour(c) => c.hex_on(true),
    };
    let shape = match e.kind.shape() {
        // A leaned rectangle is the desktop tool's parallelogram, its slant in pixels; it
        // leans right of its own accord, so a left lean is the same shape flipped.
        Shape::Rectangle if e.skew != 0.0 && e.skew_y == 0.0 => format!(
            "shape=parallelogram;perimeter=parallelogramPerimeter;whiteSpace=wrap;html=1;size={};{}",
            e.skew.abs() * CELL_W,
            if e.skew < 0.0 { "flipH=1;" } else { "" }
        ),
        Shape::Rectangle => "rounded=0;whiteSpace=wrap;html=1;".to_string(),
        Shape::RoundedRectangle => "rounded=1;whiteSpace=wrap;html=1;".to_string(),
        Shape::Ellipse => "ellipse;whiteSpace=wrap;html=1;".to_string(),
        Shape::Diamond => "rhombus;whiteSpace=wrap;html=1;".to_string(),
        Shape::Cylinder => "shape=cylinder3;whiteSpace=wrap;html=1;boundedLbl=1;size=15;".to_string(),
        Shape::Cloud => "ellipse;shape=cloud;whiteSpace=wrap;html=1;".to_string(),
        Shape::Parallelogram => "shape=parallelogram;perimeter=parallelogramPerimeter;whiteSpace=wrap;html=1;".to_string(),
        Shape::Hexagon => "shape=hexagon;perimeter=hexagonPerimeter2;whiteSpace=wrap;html=1;".to_string(),
        Shape::Dashed => "rounded=0;whiteSpace=wrap;html=1;dashed=1;fillColor=none;verticalAlign=top;".to_string(),
        Shape::Text => "text;html=1;align=center;verticalAlign=middle;whiteSpace=wrap;".to_string(),
        Shape::Triangle => "triangle;whiteSpace=wrap;html=1;".to_string(),
        Shape::PredefinedProcess => "shape=process;whiteSpace=wrap;html=1;".to_string(),
        Shape::Document => "shape=document;whiteSpace=wrap;html=1;".to_string(),
        Shape::InternalStorage => "shape=internalStorage;whiteSpace=wrap;html=1;".to_string(),
        Shape::Cube => "shape=cube;whiteSpace=wrap;html=1;".to_string(),
        Shape::Step => "shape=step;perimeter=stepPerimeter;whiteSpace=wrap;html=1;".to_string(),
        Shape::Trapezoid => "shape=trapezoid;perimeter=trapezoidPerimeter;whiteSpace=wrap;html=1;".to_string(),
        Shape::Tape => "shape=tape;whiteSpace=wrap;html=1;".to_string(),
        Shape::Note => "shape=note;whiteSpace=wrap;html=1;".to_string(),
        Shape::Card => "shape=card;whiteSpace=wrap;html=1;".to_string(),
        Shape::Callout => "shape=callout;whiteSpace=wrap;html=1;".to_string(),
        Shape::StickFigure => "shape=umlActor;verticalLabelPosition=bottom;verticalAlign=top;html=1;".to_string(),
        Shape::DataStorage => "shape=dataStorage;whiteSpace=wrap;html=1;".to_string(),
        Shape::Delay => "shape=delay;whiteSpace=wrap;html=1;".to_string(),
        Shape::Display => "shape=display;whiteSpace=wrap;html=1;".to_string(),
        Shape::ManualInput => "shape=manualInput;whiteSpace=wrap;html=1;".to_string(),
        Shape::OffPage => "shape=offPageConnector;whiteSpace=wrap;html=1;".to_string(),
        Shape::BlockArrow => "shape=singleArrow;whiteSpace=wrap;html=1;".to_string(),
        Shape::DoubleArrow => "shape=doubleArrow;whiteSpace=wrap;html=1;".to_string(),
        Shape::And => "shape=and;whiteSpace=wrap;html=1;".to_string(),
        Shape::Or => "shape=or;whiteSpace=wrap;html=1;".to_string(),
    };
    let stroke = if e.outline { e.color.map(|p| p.hex_on(true)).unwrap_or_else(|| "#333333".into()) } else { "none".into() };
    let mut style = match e.kind.shape() {
        Shape::Dashed | Shape::Text => format!("{shape}strokeColor={};", e.color.map(|p| p.hex_on(true)).unwrap_or_else(|| "#666666".into())),
        _ => format!("{shape}fillColor={fill};strokeColor={stroke};"),
    };
    if e.stroke > 1 {
        style.push_str(&format!("strokeWidth={};", e.stroke));
    }
    match e.drawn_line() {
        LineStyle::Solid => {}
        LineStyle::Dashed => style.push_str("dashed=1;"),
        LineStyle::Dotted => style.push_str("dashed=1;dashPattern=1 2;"),
    }
    if e.drawn_opacity() < 100 {
        style.push_str(&format!("opacity={};", e.drawn_opacity()));
    }
    // A shadow falls from a body; rounded corners only mean something on a rectangle.
    style.push_str(&looks.replace("shadow=1;", if e.casts_shadow() { "shadow=1;" } else { "" }));
    style.push_str(&text_style(&e.text));
    if !e.text.wrap {
        style = style.replace("whiteSpace=wrap;", "");
    }
    if let Some(w) = e.text.width {
        style.push_str(&format!("labelWidth={};", w as f64 * CELL_W));
    }
    if e.text.padding > 0 {
        style.push_str(&format!("spacing={};", e.text.padding as f64 * CELL_W));
    }
    // A plain shape carries its label and nothing else: its kind is its outline.
    let value = match (e.label.trim().is_empty(), e.kind.is_sketch()) {
        (_, true) => escape(&e.label),
        // The ontology's stereotypes, the way UML writes them.
        _ if e.kind == ShapeKind::Interface => format!("«interface» {}", escape(&e.label)),
        _ if e.kind == ShapeKind::ActionType => format!("«action» {}", escape(&e.label)),
        (true, false) => format!("<i>{}</i>", escape(e.kind.short())),
        (false, false) => format!("{}<br><font style=\"font-size:9px\"><i>{}</i></font>", escape(&e.label), escape(e.kind.short())),
    };
    // A type with rows is the desktop tool's own UML class: a swimlane with a header and one
    // stacked child cell per property, which draw.io lays out and resizes itself.
    if e.has_rows() {
        let head = match e.kind {
            ShapeKind::Interface => format!("«interface» {}", escape(&e.label)),
            ShapeKind::ActionType => format!("«action» {}", escape(&e.label)),
            _ => escape(&e.label),
        };
        let (w, row_h) = (e.w * CELL_W, CELL_H);
        let shade = format!(
            "{}{}",
            if e.kind == ShapeKind::Interface || e.drawn_line() == LineStyle::Dashed { "dashed=1;" } else { "" },
            if e.drawn_opacity() < 100 { format!("opacity={};", e.drawn_opacity()) } else { String::new() }
        );
        let mut s = format!(
            "        <mxCell id=\"e{}\" value=\"{}\" style=\"swimlane;fontStyle=1;childLayout=stackLayout;horizontal=1;startSize={};horizontalStack=0;resizeParent=1;resizeParentMax=0;resizeLast=0;collapsible=1;marginBottom=0;whiteSpace=wrap;html=1;fillColor={};strokeColor={};{}{}\" vertex=\"1\" parent=\"1\">\n          <mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" as=\"geometry\"/>\n        </mxCell>\n",
            e.id,
            head,
            CELL_H * crate::model::Element::HEADER,
            fill,
            stroke,
            shade,
            looks,
            e.x * CELL_W,
            e.y * CELL_H,
            w,
            CELL_H * crate::model::Element::HEADER + row_h * e.properties.len() as f64
        );
        for (i, p) in e.properties.iter().enumerate() {
            let text = p.compact();
            s.push_str(&format!(
                "        <mxCell id=\"e{}.p{}\" value=\"{}\" style=\"text;strokeColor=none;fillColor=none;align=left;verticalAlign=top;spacingLeft=4;spacingRight=4;overflow=hidden;rotatable=0;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;whiteSpace=wrap;html=1;\" vertex=\"1\" parent=\"e{}\">\n          <mxGeometry y=\"{}\" width=\"{}\" height=\"{}\" as=\"geometry\"/>\n        </mxCell>\n",
                e.id,
                i,
                escape(&text),
                e.id,
                CELL_H * crate::model::Element::HEADER + row_h * i as f64,
                w,
                row_h
            ));
        }
        return s;
    }
    format!(
        "        <mxCell id=\"e{}\" value=\"{}\" style=\"{}\" vertex=\"1\" parent=\"1\">\n          <mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" as=\"geometry\"/>\n        </mxCell>\n",
        e.id,
        escape(&value),
        style,
        e.x * CELL_W,
        e.y * CELL_H,
        e.w * CELL_W,
        e.h * CELL_H
    )
}

/// The desktop tool's text keys: `fontStyle` is a bit set — 1 bold, 2 italic, 4 underline.
fn text_style(t: &crate::model::TextStyle) -> String {
    let mut s = String::new();
    let bits = (t.bold as u8) | ((t.italic as u8) << 1) | ((t.underline as u8) << 2);
    if bits != 0 {
        s.push_str(&format!("fontStyle={bits};"));
    }
    if let Some(c) = t.color {
        s.push_str(&format!("fontColor={};", c.hex_on(true)));
    }
    if t.band {
        s.push_str("labelBackgroundColor=#ffffff;");
    }
    if let Some(f) = &t.font {
        s.push_str(&format!("fontFamily={};", escape(f)));
    }
    if let Some(px) = t.size {
        s.push_str(&format!("fontSize={px};"));
    }
    s
}

fn relation_cell(doc: &Document, r: &Relation, sketch: bool) -> String {
    let n = r.notation();
    use crate::ontology::Route;
    let end = |e: End, start: bool| -> String {
        let key = if start { "startArrow" } else { "endArrow" };
        let fill = if start { "startFill" } else { "endFill" };
        match e {
            End::None => format!("{key}=none;"),
            End::Arrow => format!("{key}=block;{fill}=1;"),
            End::Open => format!("{key}=open;{fill}=0;"),
            End::Triangle => format!("{key}=block;{fill}=0;"),
            End::Diamond => format!("{key}=diamondThin;{fill}=1;"),
            End::HollowDiamond => format!("{key}=diamondThin;{fill}=0;"),
            End::Dot => format!("{key}=oval;{fill}=1;"),
            End::Circle => format!("{key}=oval;{fill}=0;"),
            End::Crow => format!("{key}=ERmany;{fill}=0;"),
            End::Bar => format!("{key}=ERone;{fill}=0;"),
        }
    };
    // The route in the desktop tool's words: no edge style is a straight line.
    let mut style = String::from(match n.route {
        Route::Straight => "rounded=0;html=1;",
        Route::Orthogonal => "edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;",
        Route::Curved => "curved=1;rounded=0;html=1;",
    });
    if sketch {
        style.push_str("sketch=1;curveFitting=1;jiggle=2;");
    }
    style.push_str(&text_style(&r.text));
    style.push_str(&end(n.tail, true));
    style.push_str(&end(n.head, false));
    match n.line {
        LineStyle::Solid => {}
        LineStyle::Dashed => style.push_str("dashed=1;"),
        LineStyle::Dotted => style.push_str("dashed=1;dashPattern=1 3;"),
    }
    if n.width > 1 {
        style.push_str(&format!("strokeWidth={};", n.width));
    }
    if let Some(p) = n.color {
        style.push_str(&format!("strokeColor={};", p.hex_on(true)));
    }
    if n.end_size != crate::ontology::EndSize::Normal {
        let px = 6.0 * n.end_size.scale();
        style.push_str(&format!("startSize={px};endSize={px};"));
    }
    if n.opacity < 100 {
        style.push_str(&format!("opacity={};", n.opacity));
    }
    // An elbow set by hand is a waypoint: the route's first turn, in pixels.
    let points = match (n.route, r.elbow, doc.route(r)) {
        (Route::Orthogonal, Some(_), Some(pts)) if pts.len() == 4 => {
            format!("\n            <Array as=\"points\">\n              <mxPoint x=\"{}\" y=\"{}\"/>\n            </Array>", pts[1].0 * CELL_W, pts[1].1 * CELL_H)
        }
        _ => String::new(),
    };
    format!(
        "        <mxCell id=\"r{}\" value=\"{}\" style=\"{}\" edge=\"1\" parent=\"1\" source=\"e{}\" target=\"e{}\">\n          <mxGeometry relative=\"1\" as=\"geometry\">{}</mxGeometry>\n        </mxCell>\n",
        r.id,
        escape(r.label.as_deref().unwrap_or("")),
        style,
        r.from,
        r.to,
        points
    )
}

/// The fills architects already read the layers in.
fn layer_fill(l: Layer) -> &'static str {
    match l {
        Layer::Motivation => "#CCCCFF",
        Layer::Strategy => "#F5DEAA",
        Layer::Business => "#FFFFB5",
        Layer::Application => "#B5FFFF",
        Layer::Technology => "#C9E7B7",
        Layer::Implementation => "#FFE0E0",
        Layer::Ontology => "#E2EEFA",
        Layer::Composite => "none",
        Layer::Sketch => "#FFFFFF",
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::{ShapeKind, RelationKind};

    #[test]
    fn the_export_is_a_draw_io_file_with_every_element_and_relation() {
        let mut doc = Document::default();
        let a = doc.add(ShapeKind::ApplicationComponent, "CRM <v2>", 1.0, 1.0);
        let b = doc.add(ShapeKind::ApplicationService, "Contacts", 20.0, 1.0);
        doc.connect(RelationKind::Realization, a, b).unwrap();
        let xml = to_xml(&doc);
        assert!(xml.starts_with("<mxfile"));
        // Escaped twice on purpose: once for HTML (the value is html=1) and once for the XML
        // attribute it sits in. draw.io unwraps both and shows the angle brackets.
        assert!(xml.contains("CRM &amp;lt;v2&amp;gt;"), "labels are escaped: {xml}");
        assert!(xml.contains("source=\"e0\" target=\"e1\""));
        assert!(xml.contains("dashed=1;") && xml.contains("endFill=0;"), "realization is dashed with a hollow head");
        assert!(xml.contains("fillColor=#B5FFFF"), "application layer fill");
        doc.element_mut(a).unwrap().color = Some(crate::ontology::Paint::Purple.into());
        let r = doc.relations[0].id;
        let mut n = doc.relation(r).unwrap().notation();
        n.width = 2;
        n.color = Some(crate::ontology::Paint::Red.into());
        doc.relation_mut(r).unwrap().style = Some(n);
        let xml = to_xml(&doc);
        assert!(xml.contains("strokeColor=#8f3f71"), "a painted shape keeps its colour in draw.io — the paper value of purple: {xml}");
        assert!(xml.contains("strokeWidth=2;strokeColor=#9d0006;"), "and so does a painted, widened link");
        assert!(xml.contains("grid=\"1\" gridSize=\"10\" page=\"1\" pageVisible=\"0\" pageWidth=\"850\" pageHeight=\"1100\">"), "{xml}");
        let page = &mut doc.metadata.page;
        page.rounded = true;
        page.sketch = true;
        page.shadow = true;
        page.page_view = true;
        page.landscape = true;
        page.paper = crate::model::Paper::A4;
        page.grid = false;
        page.background = Some(crate::ontology::Colour::Hex([255, 255, 240]));
        let xml = to_xml(&doc);
        assert!(xml.contains("grid=\"0\" gridSize=\"10\" page=\"1\" pageVisible=\"1\" pageWidth=\"1160\" pageHeight=\"830\" background=\"#fffff0\">"), "{xml}");
        assert!(xml.contains("rounded=1;sketch=1;curveFitting=1;jiggle=2;shadow=1;\" vertex"), "the three looks on every shape: {xml}");
        assert!(xml.contains("html=1;sketch=1;curveFitting=1;jiggle=2;"), "a sketched edge too");
        let e = doc.element_mut(b).unwrap();
        e.fill = crate::model::Fill::None;
        e.outline = false;
        e.line = LineStyle::Dotted;
        e.opacity = 30;
        let xml = to_xml(&doc);
        assert!(xml.contains("fillColor=none;strokeColor=none;dashed=1;dashPattern=1 2;opacity=30;"), "{xml}");
        doc.element_mut(a).unwrap().fill = crate::model::Fill::Colour(crate::ontology::Colour::Hex([1, 2, 3]));
        assert!(to_xml(&doc).contains("fillColor=#010203;"));
        let t = &mut doc.element_mut(b).unwrap().text;
        t.bold = true;
        t.underline = true;
        t.color = Some(crate::ontology::Colour::Hex([1, 2, 3]));
        t.band = true;
        t.wrap = false;
        t.padding = 2;
        let xml = to_xml(&doc);
        assert!(xml.contains("fontStyle=5;fontColor=#010203;labelBackgroundColor=#ffffff;") && xml.contains("spacing=20;"), "{xml}");
        assert!(!xml.contains("html=1;\" vertex=\"1\"") || xml.matches("whiteSpace=wrap").count() == 1, "one shape stopped wrapping");
        doc.relations[0].text.italic = true;
        assert!(to_xml(&doc).contains("jiggle=2;fontStyle=2;"), "a link's labels have a look too");
        {
            let rel = doc.relation_mut(r).unwrap();
            let mut look = rel.notation();
            look.route = crate::ontology::Route::Orthogonal;
            look.end_size = crate::ontology::EndSize::Small;
            look.opacity = 60;
            rel.style = Some(look);
            rel.elbow = Some(4);
        }
        let xml = to_xml(&doc);
        assert!(xml.contains("edgeStyle=orthogonalEdgeStyle;") && xml.contains("startSize=4.5;endSize=4.5;opacity=60;"), "{xml}");
        assert!(xml.contains("<Array as=\"points\">") && xml.contains("<mxPoint x="), "an elbow is a waypoint");
        let mut look = doc.relation(r).unwrap().notation();
        look.route = crate::ontology::Route::Curved;
        doc.relation_mut(r).unwrap().style = Some(look);
        let xml = to_xml(&doc);
        assert!(xml.contains("style=\"curved=1;rounded=0;html=1;") && !xml.contains("<Array"), "{xml}");
        doc.element_mut(a).unwrap().skew = -3.0;
        assert!(to_xml(&doc).contains("shape=parallelogram;perimeter=parallelogramPerimeter;whiteSpace=wrap;html=1;size=30;flipH=1;"), "a leaned rectangle");
    }
}
