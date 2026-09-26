//! Save and load. The file is the document, as JSON, pretty-printed so that it diffs and can
//! be edited by hand.
//!
//! Loading treats the file as untrusted: every relation must name two elements that exist, and
//! a file that does not check out is refused whole rather than opened half-broken.

use std::fmt;
use std::path::Path;

use crate::model::{Document, Workspace, CURRENT_VERSION, WORKSPACE_VERSION};

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Parse(serde_json::Error),
    Invalid(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "could not read file: {e}"),
            LoadError::Parse(e) => write!(f, "malformed JSON: {e}"),
            LoadError::Invalid(why) => write!(f, "invalid diagram: {why}"),
        }
    }
}

pub fn save(ws: &Workspace, path: &Path) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(ws).expect("a workspace always serializes");
    std::fs::write(path, json)
}

/// Open a file as a workspace. A version-2 file — one bare diagram, before there were tabs —
/// opens as a workspace of one tab named after the file, so nothing saved earlier is lost.
pub fn load(path: &Path) -> Result<Workspace, LoadError> {
    let text = std::fs::read_to_string(path).map_err(LoadError::Io)?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(LoadError::Parse)?;
    let ws: Workspace = if value.get("tabs").is_some() {
        serde_json::from_value(value).map_err(LoadError::Parse)?
    } else {
        let doc: Document = serde_json::from_value(value).map_err(LoadError::Parse)?;
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "diagram 1".into());
        Workspace::single(name, doc)
    };
    validate_workspace(&ws).map_err(LoadError::Invalid)?;
    Ok(ws)
}

pub fn validate_workspace(ws: &Workspace) -> Result<(), String> {
    if ws.version > WORKSPACE_VERSION {
        return Err(format!("format {} is newer than this build reads ({WORKSPACE_VERSION})", ws.version));
    }
    if ws.tabs.is_empty() {
        return Err("a workspace with no tabs".into());
    }
    for t in &ws.tabs {
        validate(&t.diagram).map_err(|e| format!("tab {:?}: {e}", t.name))?;
    }
    Ok(())
}

/// What a diagram has to get right before it is opened.
pub fn validate(doc: &Document) -> Result<(), String> {
    if doc.version > CURRENT_VERSION {
        return Err(format!("format {} is newer than this build reads ({CURRENT_VERSION})", doc.version));
    }
    for (i, e) in doc.elements.iter().enumerate() {
        if doc.elements[..i].iter().any(|o| o.id == e.id) {
            return Err(format!("element id {} is used twice", e.id));
        }
        if !(e.w > 0.0 && e.h > 0.0) || !e.x.is_finite() || !e.y.is_finite() {
            return Err(format!("element {} has an impossible box", e.id));
        }
    }
    for r in &doc.relations {
        if doc.element(r.from).is_none() || doc.element(r.to).is_none() {
            return Err(format!("relation {} names an element that is not there", r.id));
        }
        if r.from == r.to {
            return Err(format!("relation {} joins an element to itself", r.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::ShapeKind;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("vim-shapes-{tag}-{}.json", std::process::id()));
        p
    }

    #[test]
    fn save_then_load_round_trips_every_tab() {
        let mut ws = Workspace::new();
        ws.tabs[0].diagram.add(ShapeKind::CommunicationNetwork, "Ingest", 3.0, 4.0);
        ws.tabs.push(crate::model::Tab { name: "second".into(), diagram: Document::default() });
        ws.current = 1;
        ws.grid = false;
        let path = tmp("rt");
        save(&ws, &path).expect("save");
        let restored = load(&path).expect("load");
        std::fs::remove_file(&path).ok();
        assert_eq!(restored, ws);
    }

    #[test]
    fn a_single_diagram_file_from_before_tabs_opens_as_one_tab() {
        let path = tmp("v2");
        std::fs::write(
            &path,
            r#"{ "version": 2, "elements": [ { "id": 0, "kind": "node", "label": "db", "x": 0, "y": 0, "w": 4, "h": 4 } ] }"#,
        )
        .unwrap();
        let ws = load(&path).expect("load");
        std::fs::remove_file(&path).ok();
        assert_eq!(ws.tabs.len(), 1);
        assert_eq!(ws.tabs[0].diagram.elements[0].label, "db");
        assert!(ws.tabs[0].name.starts_with("vim-shapes-v2"), "named after the file: {}", ws.tabs[0].name);
        assert!(ws.grid);
    }

    #[test]
    fn load_missing_file_is_an_io_error() {
        match load(Path::new("/nonexistent/vim-shapes-does-not-exist.json")) {
            Err(LoadError::Io(_)) => {}
            other => panic!("expected Io error, got {other:?}"),
        }
    }

    #[test]
    fn load_malformed_json_is_a_parse_error() {
        let path = tmp("bad");
        std::fs::write(&path, "{ not valid json").unwrap();
        let result = load(&path);
        std::fs::remove_file(&path).ok();
        match result {
            Err(LoadError::Parse(_)) => {}
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn a_relation_to_a_missing_element_is_refused_whole() {
        let path = tmp("dangling");
        std::fs::write(
            &path,
            r#"{ "version": 2, "elements": [ { "id": 0, "kind": "node", "x": 0, "y": 0, "w": 4, "h": 4 } ],
                 "relations": [ { "id": 0, "kind": "flow", "from": 0, "to": 9 } ] }"#,
        )
        .unwrap();
        let result = load(&path);
        std::fs::remove_file(&path).ok();
        match result {
            Err(LoadError::Invalid(why)) => assert!(why.contains("not there"), "{why}"),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
