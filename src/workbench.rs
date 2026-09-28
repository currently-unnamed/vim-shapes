//! The architecture workbench's filesystem: a folder of folders and diagram files, scanned,
//! and written back to — new folder, new diagram, rename, move, delete. No `ratatui` here;
//! the panel (`ui::workbench`) only shows what this reads, the way `persistence` only reads
//! and writes and `ui::sheet` only shows.
//!
//! Deliberately unlike `ui::tree`'s one-shot import tree: every real folder is shown here,
//! even an empty one — a folder in a workbench is org structure under active management, not
//! a grouping derived from what happens to be inside it, so a folder just made to put
//! something in has to be visible before it holds anything. A `.json` file is a diagram leaf
//! by extension alone, never opened just to draw a row — the same leniency `:import` reads a
//! file with, "best-effort, never refused for an unrecognized shape" — its validity is only
//! checked when it is actually opened, through `persistence::load`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::model::{Document, Workspace};
use crate::persistence;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    Folder,
    Diagram,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Folder { path: PathBuf, name: String, children: Vec<Node> },
    Diagram { path: PathBuf, name: String },
}

impl Node {
    pub fn name(&self) -> &str {
        match self {
            Node::Folder { name, .. } | Node::Diagram { name, .. } => name,
        }
    }
}

/// A folder or a diagram's path, name and kind — enough to rename, move or delete it. Not a
/// `Node`: a `Node` is a subtree, and this is deliberately just the row itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub kind: NodeKind,
}

/// The folder tree rooted at `root`, recursively — every real subfolder, and every `.json`
/// file as a diagram leaf, folders first then files, both alphabetical. `root` missing or
/// unreadable is an empty tree, not an error — the panel says so itself.
pub fn scan(root: &Path) -> Vec<Node> {
    let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
    let mut folders: Vec<Node> = Vec::new();
    let mut files: Vec<Node> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(ty) = entry.file_type() else { continue };
        if ty.is_dir() {
            let children = scan(&path);
            folders.push(Node::Folder { path, name, children });
        } else if ty.is_file() && path.extension().is_some_and(|e| e == "json") {
            files.push(Node::Diagram { path, name });
        }
    }
    folders.sort_by(|a, b| a.name().cmp(b.name()));
    files.sort_by(|a, b| a.name().cmp(b.name()));
    folders.into_iter().chain(files).collect()
}

/// A name typed for a new folder, a new diagram, or a rename, made safe as a single path
/// segment — a `/` in a typed name would otherwise silently reach outside the folder it was
/// typed into. Also used on names read from imported files (`archimate_import`), where `.` or
/// `..` is not just an odd rename but a directory an attacker-authored import can walk out of —
/// so a cleaned name that is exactly `.` or `..` is defanged into dashes rather than passed
/// through, since neither can otherwise appear from splitting on the separators above.
pub(crate) fn sanitize(name: &str) -> String {
    let cleaned: String =
        name.trim().chars().map(|c| if matches!(c, '/' | '\\' | '\0') { '-' } else { c }).collect();
    match cleaned.as_str() {
        "." | ".." => "-".repeat(cleaned.len()),
        _ => cleaned,
    }
}

pub fn new_folder(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let path = parent.join(sanitize(name));
    fs::create_dir(&path)?;
    Ok(path)
}

/// A fresh, empty workspace — the same one `:new` starts — written as `{name}.json`, its one
/// tab named after the file rather than `Workspace::new()`'s own generic "diagram 1": opening
/// it from the workbench later finds it again by that name instead of piling up a duplicate.
pub fn new_diagram(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let stem = sanitize(name);
    let path = parent.join(format!("{stem}.json"));
    persistence::save(&Workspace::single(stem, Document::default()), &path)?;
    Ok(path)
}

/// Renamed in place — a folder keeps no extension, a diagram keeps `.json` regardless of
/// what was typed, since a workbench diagram is always this app's own file format.
pub fn rename(entry: &Entry, new_name: &str) -> io::Result<PathBuf> {
    let parent = entry.path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent folder"))?;
    let stem = sanitize(new_name);
    let to = match entry.kind {
        NodeKind::Folder => parent.join(stem),
        NodeKind::Diagram => parent.join(format!("{stem}.json")),
    };
    fs::rename(&entry.path, &to)?;
    Ok(to)
}

/// Moved into `new_parent`, keeping its own name — a real filesystem move, so everything
/// inside a moved folder moves with it for free.
pub fn move_to(entry: &Entry, new_parent: &Path) -> io::Result<PathBuf> {
    let to = new_parent.join(&entry.name);
    fs::rename(&entry.path, &to)?;
    Ok(to)
}

/// A diagram `remove_file`s; a folder `remove_dir_all`s — the caller (the panel's confirm
/// step) has already asked first.
pub fn delete(entry: &Entry) -> io::Result<()> {
    match entry.kind {
        NodeKind::Folder => fs::remove_dir_all(&entry.path),
        NodeKind::Diagram => fs::remove_file(&entry.path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("vim-shapes-workbench-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn scanning_lists_folders_before_files_both_alphabetical_and_recurses() {
        let root = tmp("scan");
        fs::create_dir(root.join("Billing")).unwrap();
        fs::create_dir(root.join("Application")).unwrap();
        fs::write(root.join("Application").join("Overview.json"), "{}").unwrap();
        fs::write(root.join("zzz.json"), "{}").unwrap();
        fs::write(root.join("aaa.json"), "{}").unwrap();

        let nodes = scan(&root);
        let names: Vec<&str> = nodes.iter().map(Node::name).collect();
        assert_eq!(names, vec!["Application", "Billing", "aaa.json", "zzz.json"], "folders first, then files, both alphabetical");

        let Node::Folder { children, .. } = &nodes[0] else { panic!("Application should be a folder") };
        assert_eq!(children.len(), 1);
        assert!(matches!(&children[0], Node::Diagram { name, .. } if name == "Overview.json"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_empty_folder_still_appears_unlike_the_import_tree() {
        let root = tmp("empty");
        fs::create_dir(root.join("Nothing Yet")).unwrap();
        let nodes = scan(&root);
        assert_eq!(nodes.len(), 1, "org structure under active management is shown even before it holds anything");
        assert!(matches!(&nodes[0], Node::Folder { children, .. } if children.is_empty()));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_non_workspace_json_is_still_listed_as_a_leaf() {
        let root = tmp("garbage");
        fs::write(root.join("not-a-diagram.json"), "not json at all").unwrap();
        let nodes = scan(&root);
        let Node::Diagram { path, name } = &nodes[0] else { panic!("scanning never parses — only opening does") };
        assert_eq!(name, "not-a-diagram.json");
        assert!(persistence::load(path).is_err(), "and opening it is what actually catches it");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_dot_dot_name_is_defanged_not_a_traversal() {
        let root = tmp("traversal");
        let escaped = root.parent().unwrap().join("vim-shapes-workbench-traversal-escaped");
        fs::remove_dir_all(&escaped).ok();

        let made = new_folder(&root, "..").unwrap();
        assert!(made.starts_with(&root), "a folder named .. must land inside the parent it was made in, not above it: {made:?}");
        assert!(!escaped.exists());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn new_folder_and_new_diagram_write_real_things_that_reopen() {
        let root = tmp("new");
        new_folder(&root, "Payments").unwrap();
        assert!(root.join("Payments").is_dir());
        let path = new_diagram(&root, "Checkout Flow").unwrap();
        assert_eq!(path, root.join("Checkout Flow.json"));
        let ws = persistence::load(&path).unwrap();
        assert_eq!(ws.tabs.len(), 1, "the same fresh workspace :new starts");
        assert_eq!(ws.tabs[0].name, "Checkout Flow", "named after the file, not the generic \"diagram 1\" — so reopening it finds it by name");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn renaming_a_diagram_keeps_the_json_extension_and_a_folder_keeps_none() {
        let root = tmp("rename");
        let diagram_path = new_diagram(&root, "old-name").unwrap();
        let renamed = rename(&Entry { path: diagram_path.clone(), name: "old-name.json".into(), kind: NodeKind::Diagram }, "new-name").unwrap();
        assert_eq!(renamed, root.join("new-name.json"));
        assert!(!diagram_path.exists() && renamed.exists());

        let folder_path = new_folder(&root, "old-folder").unwrap();
        let renamed = rename(&Entry { path: folder_path.clone(), name: "old-folder".into(), kind: NodeKind::Folder }, "new-folder").unwrap();
        assert_eq!(renamed, root.join("new-folder"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn moving_a_folder_takes_everything_inside_it_along() {
        let root = tmp("move");
        fs::create_dir(root.join("dest")).unwrap();
        let src = root.join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("inside.json"), "{}").unwrap();
        let entry = Entry { path: src.clone(), name: "src".into(), kind: NodeKind::Folder };
        let moved = move_to(&entry, &root.join("dest")).unwrap();
        assert_eq!(moved, root.join("dest").join("src"));
        assert!(moved.join("inside.json").exists());
        assert!(!src.exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn deleting_removes_a_file_outright_and_a_folder_recursively() {
        let root = tmp("delete");
        let file = new_diagram(&root, "gone").unwrap();
        delete(&Entry { path: file.clone(), name: "gone.json".into(), kind: NodeKind::Diagram }).unwrap();
        assert!(!file.exists());

        let dir = root.join("gone-folder");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("inside.json"), "{}").unwrap();
        delete(&Entry { path: dir.clone(), name: "gone-folder".into(), kind: NodeKind::Folder }).unwrap();
        assert!(!dir.exists());
        fs::remove_dir_all(&root).ok();
    }
}
