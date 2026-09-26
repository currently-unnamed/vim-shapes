//! Automatic arrangement — two pictures of the same diagram.
//!
//! **Layers** stacks the elements by the storey they live on: motivation on top, the core stack
//! beneath it, implementation under that. It is the picture an enterprise architect expects,
//! and it is right for any diagram whose point is *what sits on what*.
//!
//! **Flow** columns the elements by how far downstream they sit along their relations — sources
//! on the left, whatever everything feeds on the right. It is right for a process or a data
//! flow, where the point is *what leads to what*.
//!
//! Both return new positions and change nothing; the app applies them as one undo step. Both
//! work in world cells, and neither knows the screen exists.

use crate::model::{Document, ElementId};
use crate::ontology::Layer;

/// Gap between elements in a row, and between rows: room for a relation's three nodes and
/// their labels to be seen and stood on, not just a line's worth.
pub const GUT_X: f64 = 14.0;
pub const GUT_Y: f64 = 6.0;

/// Elements in rows by layer; within a row, ordered so that things sit near what they relate
/// to in the row above — one barycentre pass, because the row above is always placed first.
pub fn layers(doc: &Document) -> Vec<(ElementId, f64, f64)> {
    let mut out = Vec::new();
    let mut y = 0.0;
    let mut prev: Vec<(ElementId, f64)> = Vec::new();
    for layer in Layer::ALL {
        let mut row: Vec<&crate::model::Element> =
            doc.elements.iter().filter(|e| e.kind.layer() == layer).collect();
        if row.is_empty() {
            continue;
        }
        // Start from where things are now, so a layout is stable under repetition.
        row.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap().then(a.id.cmp(&b.id)));
        // Then pull each element toward the mean x of its neighbours in the row above.
        let key = |e: &crate::model::Element| -> f64 {
            let xs: Vec<f64> = doc
                .incident(e.id)
                .into_iter()
                .filter_map(|r| doc.other_end(r, e.id))
                .filter_map(|o| prev.iter().find(|(id, _)| *id == o).map(|(_, x)| *x))
                .collect();
            if xs.is_empty() { f64::MAX } else { xs.iter().sum::<f64>() / xs.len() as f64 }
        };
        let mut keyed: Vec<(f64, usize)> = row.iter().enumerate().map(|(i, e)| (key(e), i)).collect();
        keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));

        let mut x = 0.0;
        let mut placed = Vec::new();
        let row_h = row.iter().map(|e| e.h).fold(0.0, f64::max);
        for (_, i) in keyed {
            let e = row[i];
            out.push((e.id, x, y));
            placed.push((e.id, x + e.w / 2.0));
            x += e.w + GUT_X;
        }
        prev = placed;
        y += row_h + GUT_Y;
    }
    out
}

/// Elements in columns by rank — the longest path to each from anything that nothing feeds.
/// Longest, not shortest: an element must sit to the right of *everything* that leads to it.
pub fn flow(doc: &Document) -> Vec<(ElementId, f64, f64)> {
    let ids: Vec<ElementId> = doc.elements.iter().map(|e| e.id).collect();
    let idx = |id: ElementId| ids.iter().position(|&i| i == id);
    let edges: Vec<(usize, usize)> = doc
        .relations
        .iter()
        .filter_map(|r| Some((idx(r.from)?, idx(r.to)?)))
        .filter(|(a, b)| a != b)
        .collect();
    let n = ids.len();
    let mut rank = vec![0usize; n];
    // Bounded: relations may form cycles (a flow both ways is legal), and a bounded relaxation
    // gives a merely odd layout instead of a hang.
    for _ in 0..n {
        let mut moved = false;
        for &(s, d) in &edges {
            if rank[d] < rank[s] + 1 {
                rank[d] = rank[s] + 1;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }

    let cols = rank.iter().copied().max().map_or(0, |m| m + 1);
    let mut out = Vec::new();
    let mut x = 0.0;
    let mut prev_mid: Vec<(usize, f64)> = Vec::new();
    for c in 0..cols {
        let mut members: Vec<usize> = (0..n).filter(|&i| rank[i] == c).collect();
        let key = |i: usize| -> f64 {
            let ys: Vec<f64> = edges
                .iter()
                .filter(|(_, d)| *d == i)
                .filter_map(|(s, _)| prev_mid.iter().find(|(j, _)| j == s).map(|(_, y)| *y))
                .collect();
            if ys.is_empty() { f64::MAX } else { ys.iter().sum::<f64>() / ys.len() as f64 }
        };
        members.sort_by(|&a, &b| key(a).partial_cmp(&key(b)).unwrap().then(a.cmp(&b)));
        let col_w = members.iter().map(|&i| doc.elements[i].w).fold(0.0, f64::max);
        let mut y = 0.0;
        let mut placed = Vec::new();
        for i in members {
            let e = &doc.elements[i];
            out.push((e.id, x, y));
            placed.push((i, y + e.h / 2.0));
            y += e.h + GUT_Y;
        }
        prev_mid = placed;
        x += col_w + GUT_X;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::ShapeKind::*;
    use crate::model::Document;
    use crate::ontology::RelationKind;

    fn at(pos: &[(ElementId, f64, f64)], id: ElementId) -> (f64, f64) {
        let p = pos.iter().find(|p| p.0 == id).unwrap();
        (p.1, p.2)
    }

    #[test]
    fn layers_stack_business_over_application_over_technology() {
        let mut doc = Document::default();
        let n = doc.add(Node, "", 0.0, 0.0);
        let p = doc.add(BusinessProcess, "", 0.0, 0.0);
        let c = doc.add(ApplicationComponent, "", 0.0, 0.0);
        let pos = layers(&doc);
        assert!(at(&pos, p).1 < at(&pos, c).1, "business above application");
        assert!(at(&pos, c).1 < at(&pos, n).1, "application above technology");
    }

    #[test]
    fn layers_put_an_element_under_what_it_relates_to() {
        let mut doc = Document::default();
        let p1 = doc.add(BusinessProcess, "left", 0.0, 0.0);
        let p2 = doc.add(BusinessProcess, "right", 50.0, 0.0);
        // Added first, so it would sit on the left if only x were used — but it serves the
        // process on the right, and should land under it.
        let s = doc.add(ApplicationService, "", 0.0, 0.0);
        let s2 = doc.add(ApplicationService, "", 1.0, 0.0);
        doc.connect(RelationKind::Serving, s, p2).unwrap();
        doc.connect(RelationKind::Serving, s2, p1).unwrap();
        let pos = layers(&doc);
        assert!(at(&pos, s2).0 < at(&pos, s).0, "s2 serves the left process, so it sits left of s");
    }

    #[test]
    fn flow_columns_by_the_longest_path_not_the_shortest() {
        let mut doc = Document::default();
        let a = doc.add(BusinessEvent, "", 0.0, 0.0);
        let b = doc.add(BusinessProcess, "", 0.0, 0.0);
        let c = doc.add(BusinessProcess, "", 0.0, 0.0);
        doc.connect(RelationKind::Triggering, a, b).unwrap();
        doc.connect(RelationKind::Triggering, b, c).unwrap();
        // Also directly a → c: the short path would place c level with b.
        doc.connect(RelationKind::Flow, a, c).unwrap();
        let pos = flow(&doc);
        assert!(at(&pos, a).0 < at(&pos, b).0);
        assert!(at(&pos, b).0 < at(&pos, c).0, "c is fed by b, so it must be right of b");
    }

    #[test]
    fn a_cycle_does_not_hang_the_flow_layout() {
        let mut doc = Document::default();
        let a = doc.add(BusinessProcess, "", 0.0, 0.0);
        let b = doc.add(BusinessProcess, "", 0.0, 0.0);
        doc.connect(RelationKind::Flow, a, b).unwrap();
        doc.connect(RelationKind::Flow, b, a).unwrap();
        assert_eq!(flow(&doc).len(), 2);
    }
}
