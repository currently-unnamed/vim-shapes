use std::fmt;
use std::path::Path;

use crate::model::Document;

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Parse(serde_json::Error),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "could not read file: {e}"),
            LoadError::Parse(e) => write!(f, "malformed JSON: {e}"),
        }
    }
}

pub fn save(document: &Document, path: &Path) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(document).expect("Document always serializes");
    std::fs::write(path, json)
}

pub fn load(path: &Path) -> Result<Document, LoadError> {
    let text = std::fs::read_to_string(path).map_err(LoadError::Io)?;
    serde_json::from_str(&text).map_err(LoadError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, NodeStyle, ShapeKind};

    #[test]
    fn save_then_load_round_trips() {
        let mut doc = Document::default();
        let id = doc.alloc_node_id();
        doc.nodes.push(Node {
            id,
            kind: ShapeKind::Cloud,
            x: 3.0,
            y: 4.0,
            w: 14.0,
            h: 7.0,
            label: "Ingest".to_string(),
            style: NodeStyle::default(),
        });

        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-test-{}.json", std::process::id()));

        save(&doc, &path).expect("save should succeed");
        let restored = load(&path).expect("load should succeed");

        std::fs::remove_file(&path).ok();

        assert_eq!(restored.nodes.len(), 1);
        assert_eq!(restored.nodes[0].label, "Ingest");
    }

    #[test]
    fn load_missing_file_is_an_io_error() {
        let path = Path::new("/nonexistent/vim-shapes-does-not-exist.json");
        match load(path) {
            Err(LoadError::Io(_)) => {}
            other => panic!("expected Io error, got {other:?}"),
        }
    }

    #[test]
    fn load_malformed_json_is_a_parse_error() {
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-bad-{}.json", std::process::id()));
        std::fs::write(&path, "{ not valid json").unwrap();

        let result = load(&path);
        std::fs::remove_file(&path).ok();

        match result {
            Err(LoadError::Parse(_)) => {}
            other => panic!("expected Parse error, got {other:?}"),
        }
    }
}
