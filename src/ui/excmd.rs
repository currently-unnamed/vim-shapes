//! The `:` line's vocabulary — what a typed word names, and which one a typed *prefix* of it
//! means. What each one actually **does** lives in `App::run_excmd`: a table describes,
//! `mod.rs` acts.
//!
//! Resolution is vim's own rule: an exact alias wins outright, then a unique prefix of the
//! canonical name. `:h` is registered as help's alias rather than inferred, so it can never
//! collide with another `h…` command added later.

use super::manual;
use crate::ontology::{idiom, ShapeKind, View};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Write,
    Wq,
    Open,
    Import,
    New,
    Quit,
    Add,
    Idiom,
    Layout,
    Lint,
    Kind,
    Title,
    Export,
    Help,
    TabNew,
    TabRename,
    TabClose,
    Tab,
    Tabs,
    Grid,
    Theme,
    Ink,
    Render,
    Debug,
    Sheet,
    Layers,
    Tree,
    Props,
    Diagram,
}

/// What shape a command's argument takes — enough for Tab to complete helpfully.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arg {
    None,
    Path,
    /// A kind of shape, as `:add` takes.
    Kind,
    /// A view kind, as `:kind` takes.
    View,
    /// One of a small fixed set of words.
    Verb(&'static [&'static str]),
    Idiom,
    /// A manual topic.
    Topic,
    /// Free text.
    Text,
}

pub struct ExCmd {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub op: Op,
    pub arg: Arg,
    /// One line for the manual's command list.
    pub what: &'static str,
}

pub static COMMANDS: &[ExCmd] = &[
    ExCmd { name: "write", aliases: &["w"], op: Op::Write, arg: Arg::Path, what: "save — to the file opened, or to the path given" },
    ExCmd { name: "wq", aliases: &["x"], op: Op::Wq, arg: Arg::Path, what: "save and quit" },
    ExCmd { name: "open", aliases: &["o", "e", "edit"], op: Op::Open, arg: Arg::Path, what: "open a diagram file" },
    ExCmd { name: "import", aliases: &[], op: Op::Import, arg: Arg::Path, what: "import a draw.io file, an ArchiMate exchange file, or a coArchi model folder — best-effort, never refused for an unrecognized shape" },
    ExCmd { name: "new", aliases: &["n"], op: Op::New, arg: Arg::None, what: "start an empty diagram" },
    ExCmd { name: "quit", aliases: &["q"], op: Op::Quit, arg: Arg::None, what: "quit — asks if there is unsaved work; :q! does not" },
    ExCmd { name: "add", aliases: &["a"], op: Op::Add, arg: Arg::Kind, what: "add an element — bare opens the palette, :add <kind> skips it" },
    ExCmd { name: "idiom", aliases: &["i"], op: Op::Idiom, arg: Arg::Idiom, what: "stamp a worked shape into the diagram — bare lists them" },
    ExCmd { name: "layout", aliases: &["l"], op: Op::Layout, arg: Arg::Verb(&["layers", "flow"]), what: "arrange everything: by layer (default), or by flow" },
    ExCmd { name: "lint", aliases: &[], op: Op::Lint, arg: Arg::None, what: "list every relation the rules refuse, and jump to the first" },
    ExCmd { name: "kind", aliases: &["k", "view"], op: Op::Kind, arg: Arg::View, what: "what sort of diagram this is — narrows what :add offers" },
    ExCmd { name: "title", aliases: &["t"], op: Op::Title, arg: Arg::Text, what: "name the diagram" },
    ExCmd { name: "export", aliases: &["E"], op: Op::Export, arg: Arg::Path, what: "export — bare opens the dialog (png, svg, pdf, xml, html); :export <file.ext> writes it outright" },
    ExCmd { name: "help", aliases: &["h"], op: Op::Help, arg: Arg::Topic, what: "the manual — :help <topic> jumps to a page" },
    ExCmd { name: "tabnew", aliases: &["tn"], op: Op::TabNew, arg: Arg::Verb(&["freeform", "architecture"]), what: "a new tab — freeform or architecture, then a name (^t asks)" },
    ExCmd { name: "tabrename", aliases: &["tr"], op: Op::TabRename, arg: Arg::Text, what: "rename this tab" },
    ExCmd { name: "tabclose", aliases: &["tc"], op: Op::TabClose, arg: Arg::None, what: "close this tab (asks if it is the only copy of work; :tabclose! does not)" },
    ExCmd { name: "tab", aliases: &[], op: Op::Tab, arg: Arg::Text, what: "go to tab N" },
    ExCmd { name: "tabs", aliases: &[], op: Op::Tabs, arg: Arg::None, what: "list the tabs" },
    ExCmd { name: "grid", aliases: &["g"], op: Op::Grid, arg: Arg::Verb(&["on", "off"]), what: "the dots that mark the canvas — toggle, or on / off" },
    ExCmd { name: "ink", aliases: &[], op: Op::Ink, arg: Arg::Verb(&["lines", "braille"]), what: "how the diagram is drawn: lines (box drawing — reads on a screenshare) or braille (dots) — kept in the config file" },
    ExCmd { name: "theme", aliases: &[], op: Op::Theme, arg: Arg::Verb(&["dark", "light", "auto"]), what: "the palette: dark, light, or auto (the terminal's hint) — kept in the config file; bare, says which and why" },
    ExCmd { name: "render", aliases: &["r"], op: Op::Render, arg: Arg::Path, what: "write this tab as a PNG, its cells drawn through a monospace font (:render <out.png> [px] [font])" },
    ExCmd { name: "debug", aliases: &["dbg"], op: Op::Debug, arg: Arg::None, what: "the debugging panel — the app's state, live, and what the last keys did" },
    ExCmd { name: "sheet", aliases: &[], op: Op::Sheet, arg: Arg::None, what: "the property sheet on the right — toggle it (c opens it and gives it the keys)" },
    ExCmd { name: "diagram", aliases: &["dia", "page"], op: Op::Diagram, arg: Arg::None, what: "the sheet on the diagram itself: rounded, sketch, shadow, background, grid, paper" },
    // Canonically `stack`, so `:lay…` still means layout; `:layers` reaches it as an alias.
    ExCmd { name: "props", aliases: &["properties", "params"], op: Op::Props, arg: Arg::None, what: "the property browser on the cursor's object type, interface or action type (P): its rows, typed" },
    ExCmd { name: "stack", aliases: &["layers", "ly"], op: Op::Layers, arg: Arg::None, what: "the layer browser (:layers): show, hide, lock, reorder, rename layers; move things between them" },
    ExCmd { name: "tree", aliases: &[], op: Op::Tree, arg: Arg::None, what: "the model tree: a coArchi import's own folders, fold/unfold and search, enter jumps to a view's tab" },
];

/// The command a typed name means, if any.
pub fn resolve(name: &str) -> Option<&'static ExCmd> {
    if let Some(c) = COMMANDS.iter().find(|c| c.name == name || c.aliases.contains(&name)) {
        return Some(c);
    }
    let mut hits = COMMANDS.iter().filter(|c| c.name.starts_with(name));
    let first = hits.next()?;
    hits.next().map_or(Some(first), |_| None)
}

/// Split a line into its command word, whether it carried a `!`, and its argument.
pub fn parse(line: &str) -> (String, bool, String) {
    let line = line.trim();
    let (word, rest) = match line.split_once(char::is_whitespace) {
        Some((w, r)) => (w, r.trim()),
        None => (line, ""),
    };
    let (word, bang) = match word.strip_suffix('!') {
        Some(w) => (w, true),
        None => (word, false),
    };
    (word.to_string(), bang, rest.to_string())
}

/// Every full line the buffer could become, for the word under the cursor.
pub fn candidates(buf: &str) -> Vec<String> {
    let (word, _, rest) = parse(buf);
    if !buf.contains(char::is_whitespace) {
        // Completing the command name.
        let mut v: Vec<String> = COMMANDS
            .iter()
            .filter(|c| c.name.starts_with(&word))
            .map(|c| c.name.to_string())
            .collect();
        v.sort();
        return v;
    }
    let Some(cmd) = resolve(&word) else { return Vec::new() };
    let mut args: Vec<String> = match cmd.arg {
        Arg::None | Arg::Text => Vec::new(),
        Arg::Path => paths(&rest),
        Arg::Kind => ShapeKind::ALL.iter().map(|k| k.slug().to_string()).filter(|s| s.starts_with(&rest)).collect(),
        Arg::View => View::ALL.iter().map(|v| v.name().to_string()).filter(|s| s.starts_with(&rest)).collect(),
        Arg::Verb(list) => list.iter().map(|s| s.to_string()).filter(|s| s.starts_with(&rest)).collect(),
        Arg::Idiom => idiom::IDIOMS.iter().map(|i| i.name.to_string()).filter(|s| s.starts_with(&rest)).collect(),
        Arg::Topic => manual::tags().into_iter().filter(|t| t.to_ascii_lowercase().starts_with(&rest.to_ascii_lowercase())).collect(),
    };
    if matches!(cmd.arg, Arg::Kind | Arg::View | Arg::Verb(_) | Arg::Idiom) {
        args.sort();
    }
    args.into_iter().map(|a| format!("{word} {a}")).collect()
}

/// Filesystem completion, the way a shell does it: split on the last `/`, list what matches,
/// and give a directory its own trailing `/` so a second Tab walks deeper.
fn paths(partial: &str) -> Vec<String> {
    let (dir, stem) = match partial.rsplit_once('/') {
        Some((d, s)) => (format!("{d}/"), s.to_string()),
        None => (String::new(), partial.to_string()),
    };
    let read_dir = if dir.is_empty() { "." } else { dir.as_str() };
    let Ok(entries) = std::fs::read_dir(read_dir) else { return Vec::new() };
    let mut v: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with(&stem) || (stem.is_empty() && name.starts_with('.')) {
                return None;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            Some(format!("{dir}{name}{}", if is_dir { "/" } else { "" }))
        })
        .collect();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_alias_wins_and_a_unique_prefix_resolves() {
        assert_eq!(resolve("w").map(|c| c.op), Some(Op::Write));
        assert_eq!(resolve("wr").map(|c| c.op), Some(Op::Write));
        assert_eq!(resolve("lay").map(|c| c.op), Some(Op::Layout));
        assert_eq!(resolve("l").map(|c| c.op), Some(Op::Layout), "l is layout's own alias");
        assert!(resolve("zz").is_none());
    }

    #[test]
    fn a_bang_is_split_off_the_word() {
        assert_eq!(parse("q!"), ("q".into(), true, String::new()));
        assert_eq!(parse("w  out.json"), ("w".into(), false, "out.json".into()));
    }

    #[test]
    fn completion_knows_the_shape_of_each_argument() {
        assert_eq!(candidates("la"), vec!["layout"]);
        assert_eq!(candidates("layout "), vec!["layout flow", "layout layers"]);
        assert!(candidates("add comp").contains(&"add component".to_string()));
        assert!(candidates("kind tech").contains(&"kind technology".to_string()));
        assert!(candidates("idiom s").contains(&"idiom service".to_string()));
        assert!(candidates("new ").is_empty());
        assert_eq!(candidates("tabnew "), vec!["tabnew architecture", "tabnew freeform"]);
        assert_eq!(resolve("tr").map(|c| c.op), Some(Op::TabRename));
    }

    #[test]
    fn every_ex_command_has_a_description() {
        for c in COMMANDS {
            assert!(!c.what.is_empty(), "{} has no description", c.name);
        }
    }
}
