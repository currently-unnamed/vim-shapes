//! Which fonts this machine has — the catalogue a shape's `font` field cycles through.
//!
//! A font is a property of the *machine*: a diagram says "set this label in Menlo" and the
//! computer says with what file. So the list is scanned from the folders a terminal would find
//! its fonts in, by name, with the file kept out of sight. Read once, by directory listing
//! alone — a font folder holds hundreds of files and parsing each to ask its name is seconds of
//! I/O to open a menu, and the file's own stem is name enough to pick by.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// One family a label can be set in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Family {
    /// The name a diagram stores and the panel shows: the file's stem, its weight suffix
    /// dropped — `JetBrainsMono-Regular.ttf` and `JetBrainsMono-Bold.ttf` are one family.
    pub name: String,
    /// The regular face, or the first face found when there is none called that.
    pub path: PathBuf,
    /// The family's other faces, where the folder has them, by the file's own suffix.
    pub bold: Option<PathBuf>,
    pub italic: Option<PathBuf>,
    pub bold_italic: Option<PathBuf>,
}

/// Which face a file is, read from its stem's suffix: `-Bold`, `-Italic`, `-BoldItalic`,
/// `-Oblique` and their kin.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    Regular,
    Bold,
    Italic,
    BoldItalic,
    /// Some other weight or width — light, medium, condensed — which stands in for the
    /// regular face only until one turns up.
    Other,
}

impl Family {
    /// The file for a look: the face asked for, or the nearest the family has — a bold
    /// italic without such a face falls back to bold, then italic, then regular.
    pub fn face(&self, bold: bool, italic: bool) -> &Path {
        let pick = match (bold, italic) {
            (true, true) => self.bold_italic.as_ref().or(self.bold.as_ref()).or(self.italic.as_ref()),
            (true, false) => self.bold.as_ref(),
            (false, true) => self.italic.as_ref(),
            (false, false) => None,
        };
        pick.unwrap_or(&self.path).as_path()
    }
}

fn dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ["/System/Library/Fonts", "/Library/Fonts", "/usr/share/fonts", "/usr/local/share/fonts"]
        .iter()
        .map(PathBuf::from)
        .collect();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        out.extend([home.join("Library/Fonts"), home.join(".local/share/fonts"), home.join(".fonts")]);
    }
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.filter_map(Result::ok) {
        let p = e.path();
        if p.is_dir() {
            if depth < 4 {
                walk(&p, depth + 1, out);
            }
            continue;
        }
        let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
            out.push(p);
        }
    }
}

/// The family a file belongs to, and which face it is.
fn family_of(path: &Path) -> (String, Face) {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let (name, style) = match stem.split_once('-') {
        Some((n, s)) => (n.to_string(), s.to_ascii_lowercase()),
        None => (stem.clone(), String::new()),
    };
    let bold = style.contains("bold");
    let italic = style.contains("italic") || style.contains("oblique");
    let face = match (bold, italic) {
        (true, true) => Face::BoldItalic,
        (true, false) => Face::Bold,
        (false, true) => Face::Italic,
        (false, false) if style.is_empty() || style == "regular" => Face::Regular,
        (false, false) => Face::Other,
    };
    (name, face)
}

/// Every family found, once, sorted by name.
pub fn families() -> &'static [Family] {
    static ALL: OnceLock<Vec<Family>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut files = Vec::new();
        for d in dirs() {
            walk(&d, 0, &mut files);
        }
        let mut out: Vec<Family> = Vec::new();
        let mut regular: Vec<bool> = Vec::new();
        for p in files {
            let (name, face) = family_of(&p);
            if name.is_empty() {
                continue;
            }
            let i = match out.iter().position(|f| f.name == name) {
                Some(i) => i,
                None => {
                    out.push(Family { name, path: p.clone(), bold: None, italic: None, bold_italic: None });
                    regular.push(face == Face::Regular);
                    out.len() - 1
                }
            };
            let f = &mut out[i];
            match face {
                // The regular face wins the family's slot; any other face holds it only
                // until one turns up.
                Face::Regular => {
                    f.path = p;
                    regular[i] = true;
                }
                // The first file of a face is the face: a second `-Bold` is a duplicate.
                Face::Bold => {
                    f.bold.get_or_insert(p);
                }
                Face::Italic => {
                    f.italic.get_or_insert(p);
                }
                Face::BoldItalic => {
                    f.bold_italic.get_or_insert(p);
                }
                Face::Other => {}
            }
        }
        out.sort_by_key(|f| f.name.to_ascii_lowercase());
        out
    })
}

/// The family a typed name means: exact (case-insensitive) first, then a unique prefix, then
/// the only family containing the text — so `jet` finds JetBrainsMono and `mono` says which.
pub fn resolve(name: &str) -> Result<&'static Family, String> {
    let want = name.trim().to_ascii_lowercase();
    if want.is_empty() {
        return Err("no font named".into());
    }
    let all = families();
    if let Some(f) = all.iter().find(|f| f.name.to_ascii_lowercase() == want) {
        return Ok(f);
    }
    for test in [
        |f: &Family, w: &str| f.name.to_ascii_lowercase().starts_with(w),
        |f: &Family, w: &str| f.name.to_ascii_lowercase().contains(w),
    ] {
        let hits: Vec<&Family> = all.iter().filter(|f| test(f, &want)).collect();
        match hits.len() {
            1 => return Ok(hits[0]),
            0 => continue,
            n => {
                let names: Vec<&str> = hits.iter().take(6).map(|f| f.name.as_str()).collect();
                return Err(format!("{n} fonts match {name:?}: {}{}", names.join(", "), if n > 6 { "…" } else { "" }));
            }
        }
    }
    Err(format!("no font on this machine called {name:?}"))
}

/// The file for a family's face — bold, italic, both, or the regular one — if this machine
/// has the family; the nearest face it has, when it lacks the one asked for.
pub fn face_of(name: &str, bold: bool, italic: bool) -> Option<&'static Path> {
    families().iter().find(|f| f.name.eq_ignore_ascii_case(name.trim())).map(|f| f.face(bold, italic))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_family_is_named_by_its_stem_without_the_weight() {
        assert_eq!(family_of(Path::new("/x/JetBrainsMono-Bold.ttf")), ("JetBrainsMono".into(), Face::Bold));
        assert_eq!(family_of(Path::new("/x/JetBrainsMono-Regular.ttf")), ("JetBrainsMono".into(), Face::Regular));
        assert_eq!(family_of(Path::new("/x/Menlo.ttc")), ("Menlo".into(), Face::Regular));
        assert_eq!(family_of(Path::new("/x/Fira-BoldItalic.otf")).1, Face::BoldItalic);
        assert_eq!(family_of(Path::new("/x/Fira-LightOblique.otf")).1, Face::Italic);
        assert_eq!(family_of(Path::new("/x/Fira-Medium.otf")).1, Face::Other);
        let fam = Family { name: "F".into(), path: "/r".into(), bold: Some("/b".into()), italic: None, bold_italic: None };
        assert_eq!(fam.face(true, true), Path::new("/b"), "no bold italic: bold stands in");
        assert_eq!(fam.face(false, true), Path::new("/r"), "no italic: regular stands in");
    }

    #[test]
    fn the_catalogue_is_sorted_and_free_of_duplicates() {
        let all = families();
        for w in all.windows(2) {
            assert!(w[0].name.to_ascii_lowercase() <= w[1].name.to_ascii_lowercase());
            assert_ne!(w[0].name, w[1].name);
        }
    }

    #[test]
    fn resolving_finds_by_prefix_and_refuses_ambiguity_with_the_candidates_named() {
        let all = families();
        if let Some(first) = all.first() {
            assert_eq!(resolve(&first.name).unwrap().name, first.name);
            assert_eq!(resolve(&first.name.to_ascii_uppercase()).unwrap().name, first.name);
        }
        assert!(resolve("").is_err());
        assert!(resolve("no-such-font-zzz").unwrap_err().contains("no font"));
    }
}
