//! The app's own preferences — the first and only one being the theme.
//!
//! A diagram's file holds the diagram; a preference is the reader's, so it lives with the
//! reader: `$XDG_CONFIG_HOME/vim-shapes/config.json`, or `~/.config/vim-shapes/config.json`.
//! JSON, one object, so it can be read and written by hand as the diagram files can. Missing
//! or unreadable, it is as if it said nothing.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Config {
    /// `dark`, `light`, or `auto` (the terminal's own hint, then dark).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// `lines` or `braille`: how the diagram is drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ink: Option<String>,
}

/// Where the file lives, from the environment; `None` when there is no home to put it in.
pub fn path() -> Option<PathBuf> {
    let dir = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(dir.join("vim-shapes").join("config.json"))
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
}
