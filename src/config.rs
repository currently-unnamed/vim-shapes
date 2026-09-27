//! The app's own preferences — the first and only one being the theme.
//!
//! A diagram's file holds the diagram; a preference is the reader's, so it lives with the
//! reader: `$XDG_CONFIG_HOME/vim-shapes/config.json`, or `~/.config/vim-shapes/config.json`, or
//! — on Windows, where neither of those is set by a stock PowerShell or cmd —
//! `%APPDATA%\vim-shapes\config.json`. JSON, one object, so it can be read and written by hand
//! as the diagram files can. Missing or unreadable, it is as if it said nothing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Config {
    /// `dark`, `light`, or `auto` (the terminal's own hint, then dark).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// `lines` or `braille`: how the diagram is drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ink: Option<String>,
    /// Every architecture workbench folder ever opened, most-recently-accessed first — a
    /// person may keep a personal one and a team one, so this is a list, not a single "the"
    /// workbench.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workbenches: Vec<String>,
    /// Which diagrams, under a workbench, were still open as tabs the last time the app
    /// quit — keyed by that workbench's own canonical path, so opening it again (by
    /// `:workbench`, or picking it off the startup dialog — always an explicit choice, never
    /// on its own) can offer to pick back up. One entry per workbench ever quit out of with
    /// something open; a workbench closed out with nothing open has none.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub sessions: HashMap<String, WorkbenchSession>,
}

/// What was left open in one workbench, at the last quit.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct WorkbenchSession {
    /// Every diagram file that was a tab, under this workbench, in the order first opened —
    /// a multi-tab file appears once, not once per tab inside it, since re-opening it (like
    /// picking its row in the workbench today) already brings every one of its own tabs
    /// back along with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<String>,
    /// Which of `tabs` was the one in front, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
}

/// How many recent workbenches the config remembers — enough to keep a personal one and a
/// few team ones in reach without the list becoming its own thing to manage.
const MAX_WORKBENCHES: usize = 10;

impl Config {
    /// Move `path` to the front of the remembered workbenches — de-duplicated, and capped so
    /// the list stays a "recent" one rather than growing forever.
    fn touch_workbench(&mut self, path: String) {
        self.workbenches.retain(|p| p != &path);
        self.workbenches.insert(0, path);
        self.workbenches.truncate(MAX_WORKBENCHES);
    }

    /// `tabs` empty removes `key` outright rather than storing an empty session — quitting
    /// with nothing open from this workbench is the state to remember, not a reason to keep
    /// whatever an earlier quit left behind.
    fn set_workbench_session(&mut self, key: String, tabs: Vec<String>, current: Option<String>) {
        if tabs.is_empty() {
            self.sessions.remove(&key);
        } else {
            self.sessions.insert(key, WorkbenchSession { tabs, current });
        }
    }
}

/// Record a workbench as just opened — read-modify-write, like every other config change.
pub fn touch_workbench(path: &std::path::Path) -> Result<(), String> {
    let path = path.to_string_lossy().to_string();
    save(move |c| c.touch_workbench(path))
}

/// Records which diagrams, under `root`, were left open at quit — read-modify-write, like
/// every other config change.
pub fn save_workbench_session(root: &std::path::Path, tabs: Vec<String>, current: Option<String>) -> Result<(), String> {
    let key = root.to_string_lossy().to_string();
    save(move |c| c.set_workbench_session(key, tabs, current))
}

/// What was left open in `root` at the last quit, if anything was.
pub fn workbench_session(root: &std::path::Path) -> Option<WorkbenchSession> {
    load().sessions.get(&root.to_string_lossy().to_string()).cloned()
}

/// Where the file lives, from the environment; `None` when there is no home to put it in.
pub fn path() -> Option<PathBuf> {
    resolve_path(std::env::var_os("XDG_CONFIG_HOME"), std::env::var_os("HOME"), std::env::var_os("APPDATA"))
}

/// [`path`]'s precedence, taken as arguments so it can be tested without touching the real
/// environment — the three variables it reads are process-global, and tests run in parallel.
fn resolve_path(xdg: Option<std::ffi::OsString>, home: Option<std::ffi::OsString>, appdata: Option<std::ffi::OsString>) -> Option<PathBuf> {
    if let Some(d) = xdg.filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d).join("vim-shapes").join("config.json"));
    }
    if let Some(h) = home.filter(|h| !h.is_empty()) {
        return Some(PathBuf::from(h).join(".config").join("vim-shapes").join("config.json"));
    }
    // Neither is set by a stock PowerShell or cmd; %APPDATA% is Windows' own equivalent, and
    // Windows apps don't nest under a ".config" folder the way XDG does.
    let appdata = appdata.filter(|a| !a.is_empty())?;
    Some(PathBuf::from(appdata).join("vim-shapes").join("config.json"))
}

pub fn load() -> Config {
    path().map(|p| load_from(&p)).unwrap_or_default()
}

pub fn load_from(p: &std::path::Path) -> Config {
    std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Write the whole file — read, changed, written — so a field added later survives a save.
pub fn save(change: impl FnOnce(&mut Config)) -> Result<(), String> {
    let p = path().ok_or("no home directory to keep a config file in")?;
    save_to(&p, change)
}

pub fn save_to(p: &std::path::Path, change: impl FnOnce(&mut Config)) -> Result<(), String> {
    let mut c = load_from(p);
    change(&mut c);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(&c).map_err(|e| e.to_string())?;
    std::fs::write(p, text + "\n").map_err(|e| format!("{}: {e}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_round_trips_through_its_file_and_a_missing_one_says_nothing() {
        let dir = std::env::temp_dir().join(format!("vim-shapes-config-{}", std::process::id()));
        let p = dir.join("vim-shapes").join("config.json");
        assert_eq!(load_from(&p), Config::default());
        save_to(&p, |c| c.theme = Some("light".into())).unwrap();
        assert_eq!(load_from(&p).theme.as_deref(), Some("light"));
        assert!(std::fs::read_to_string(&p).unwrap().contains("\"theme\": \"light\""));
        save_to(&p, |c| c.theme = None).unwrap();
        assert_eq!(load_from(&p), Config::default(), "cleared, and the file still parses");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_path_falls_back_from_xdg_to_home_to_appdata_for_a_home_less_windows_shell() {
        let some = |s: &str| Some(std::ffi::OsString::from(s));
        assert_eq!(resolve_path(some("/x"), some("/h"), some("/a")), Some(PathBuf::from("/x/vim-shapes/config.json")), "XDG_CONFIG_HOME wins when set");
        assert_eq!(resolve_path(None, some("/h"), some("/a")), Some(PathBuf::from("/h/.config/vim-shapes/config.json")), "HOME next");
        assert_eq!(
            resolve_path(None, None, some(r"C:\Users\a\AppData\Roaming")),
            Some(PathBuf::from(r"C:\Users\a\AppData\Roaming").join("vim-shapes").join("config.json")),
            "a stock PowerShell or cmd sets neither XDG_CONFIG_HOME nor HOME, so %APPDATA% is what's left"
        );
        assert_eq!(resolve_path(None, None, None), None, "nowhere to put it");
        assert_eq!(resolve_path(some(""), some(""), some("")), None, "set-but-empty is the same as unset");
    }

    #[test]
    fn touching_a_workbench_moves_it_to_the_front_deduplicated_and_capped() {
        let mut c = Config::default();
        c.touch_workbench("/a".into());
        c.touch_workbench("/b".into());
        c.touch_workbench("/c".into());
        assert_eq!(c.workbenches, vec!["/c", "/b", "/a"]);
        c.touch_workbench("/a".into());
        assert_eq!(c.workbenches, vec!["/a", "/c", "/b"], "re-touching moves it to the front instead of duplicating");
        for i in 0..MAX_WORKBENCHES + 5 {
            c.touch_workbench(format!("/many/{i}"));
        }
        assert_eq!(c.workbenches.len(), MAX_WORKBENCHES, "the list stays a \"recent\" one");
    }

    #[test]
    fn a_workbench_session_round_trips_through_its_file_and_an_empty_one_removes_it() {
        let dir = std::env::temp_dir().join(format!("vim-shapes-config-session-{}", std::process::id()));
        let p = dir.join("vim-shapes").join("config.json");
        save_to(&p, |c| c.set_workbench_session("/wb".into(), vec!["/wb/a.json".into(), "/wb/b.json".into()], Some("/wb/b.json".into()))).unwrap();
        let session = load_from(&p).sessions.get("/wb").cloned().expect("saved");
        assert_eq!(session.tabs, vec!["/wb/a.json", "/wb/b.json"]);
        assert_eq!(session.current.as_deref(), Some("/wb/b.json"));
        save_to(&p, |c| c.set_workbench_session("/wb".into(), Vec::new(), None)).unwrap();
        assert!(!load_from(&p).sessions.contains_key("/wb"), "quitting with nothing open removes the entry rather than storing an empty one");
        std::fs::remove_dir_all(&dir).ok();
    }
}
