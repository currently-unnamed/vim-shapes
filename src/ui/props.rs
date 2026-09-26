//! The property browser — `P` on an object type, an interface or an action type; `:props`.
//!
//! An object type is its properties, and a list wants the grammar the layer browser already
//! has: a docked panel with the keyboard, one row per property, a key per thing a row can
//! be. Every change is one undo step and redraws the box at once, grown to hold its rows.

use super::{chrome, theme};
use crate::model::{BaseType, Document, Element, ElementId, Property, Status};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub const WIDTH: u16 = 40;

/// What is being typed into the selected row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Typing {
    /// A new property's name, to go below the selection.
    New,
    Name,
    /// A base type, by name or prefix.
    Type,
    ValueType,
    ApiName,
}

pub struct State {
    /// The shape whose rows these are.
    pub target: ElementId,
    pub sel: usize,
    pub editing: Option<(Typing, String)>,
}

impl State {
    pub fn new(target: ElementId) -> State {
        State { target, sel: 0, editing: None }
    }

    pub fn move_by(&mut self, delta: isize, n: usize) {
        if n > 0 {
            self.sel = (self.sel as isize + delta).rem_euclid(n as isize) as usize;
        }
    }
}

/// How tall the panel wants to be for `n` rows.
pub fn height(n: usize, avail: u16) -> u16 {
    ((n.max(1) + 6) as u16).min(avail)
}

pub struct Browser<'a> {
    pub state: &'a State,
    pub doc: &'a Document,
}

impl Widget for Browser<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let Some(e) = self.doc.element(s.target) else { return };
        let title = format!("{} — {}", e.display(), e.rows_word());
        let inner = chrome::panel(buf, area, &title, theme::layer_color(e.kind.layer()));
        let hint = match &s.editing {
            Some((Typing::New, _)) => " name the new one — enter, or esc",
            Some((Typing::Name, _)) => " rename — enter, or esc",
            Some((Typing::Type, _)) => " a base type: string, integer, date… — enter, or esc",
            Some((Typing::ValueType, _)) => " the value type it adopts, blank for none — enter, or esc",
            Some((Typing::ApiName, _)) => " the API name, blank for none — enter, or esc",
            None => " j/k  n new  r rename  t type  T by name  p key  l title  s shared  [ array  * required  v value type  a api  J/K move  d delete  esc",
        };
        let body = chrome::hint(buf, inner, hint);
        let dim = Style::new().fg(theme::t().dim);
        let gone = Status::Deprecated.mark(false).unwrap_or_default();
        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled(" ⚿ primary key  ✎ title  ✱ shared  * required  ", dim),
                Span::styled(gone, Style::new().fg(theme::t().red).bold()),
                Span::styled(" deprecated", dim),
            ]),
            Line::raw(""),
        ];
        if e.properties.is_empty() && s.editing.is_none() {
            lines.push(Line::styled(" no rows yet — n adds one", Style::new().fg(theme::t().dim)));
        }
        for (i, p) in e.properties.iter().enumerate() {
            let on = i == s.sel;
            let text = match &s.editing {
                // The mark, not the row's first two bytes: every symbol mark is three.
                Some((Typing::Name, t)) if on => format!(" {}{t}█", p.mark(false)),
                Some((Typing::Type, t)) if on => format!(" {}  → {t}█", p.row(body.width as usize - 4)),
                Some((Typing::ValueType, t)) if on => format!(" {}  ‹{t}█›", p.row(body.width as usize - 4)),
                Some((Typing::ApiName, t)) if on => format!(" {}  api {t}█", p.row(body.width as usize - 4)),
                _ => format!(" {}", p.row(body.width as usize - 2)),
            };
            let style = if on { Style::new().fg(theme::t().inverse).bg(theme::layer_color(e.kind.layer())).bold() } else { Style::new().fg(theme::t().ink) };
            let text = fit(&text, body.width as usize);
            // A deprecated row's ✗ in red; the row's own colours for the rest of it.
            lines.push(match text.strip_prefix(' ').and_then(|t| t.strip_prefix(gone)) {
                Some(rest) if p.mark(false).starts_with(gone) => Line::from(vec![
                    Span::styled(" ", style),
                    Span::styled(gone, style.fg(theme::t().red).bold()),
                    Span::styled(rest.to_string(), style),
                ]),
                _ => Line::styled(text, style),
            });
        }
        if let Some((Typing::New, t)) = &s.editing {
            lines.push(Line::styled(format!(" new: {t}█"), Style::new().fg(theme::t().green).bold()));
        }
        Paragraph::new(lines).render(body, buf);
    }
}

fn fit(s: &str, room: usize) -> String {
    if s.chars().count() <= room { s.to_string() } else { s.chars().take(room.saturating_sub(1)).chain(['…']).collect() }
}

/// One key on one row, applied to the element. Returns what to say, or why nothing changed.
pub fn apply(e: &mut Element, sel: usize, key: char) -> Result<Option<String>, String> {
    let n = e.properties.len();
    if n == 0 {
        return Err("no rows yet — n adds one".into());
    }
    let i = sel.min(n - 1);
    match key {
        // One primary key per object type: the mark moves.
        'p' => {
            let was = e.properties[i].primary_key;
            for p in &mut e.properties {
                p.primary_key = false;
            }
            e.properties[i].primary_key = !was;
        }
        'l' => {
            let was = e.properties[i].title;
            for p in &mut e.properties {
                p.title = false;
            }
            e.properties[i].title = !was;
        }
        's' => e.properties[i].shared = !e.properties[i].shared,
        '[' => e.properties[i].array = !e.properties[i].array,
        '*' => e.properties[i].required = !e.properties[i].required,
        't' => e.properties[i].base_type = e.properties[i].base_type.next(),
        'd' => {
            let gone = e.properties.remove(i);
            return Ok(Some(format!("{} removed", gone.name)));
        }
        _ => return Err(format!("no key {key:?} here")),
    }
    Ok(None)
}

/// A property typed in: a new one below `sel`, or a field of the selected one.
pub fn typed(e: &mut Element, sel: usize, what: Typing, text: &str) -> Result<usize, String> {
    let text = text.trim();
    match what {
        Typing::New => {
            if text.is_empty() {
                return Err("a property needs a name".into());
            }
            let at = if e.properties.is_empty() { 0 } else { sel.min(e.properties.len() - 1) + 1 };
            e.properties.insert(at, Property::new(text));
            Ok(at)
        }
        _ => {
            let n = e.properties.len();
            if n == 0 {
                return Err("no rows yet — n adds one".into());
            }
            let i = sel.min(n - 1);
            let p = &mut e.properties[i];
            match what {
                Typing::Name => {
                    if text.is_empty() {
                        return Err("a property needs a name".into());
                    }
                    p.name = text.to_string();
                }
                Typing::Type => p.base_type = BaseType::parse(text).ok_or_else(|| format!("no base type called {text:?} — {}", BaseType::ALL.iter().map(|b| b.name()).collect::<Vec<_>>().join(", ")))?,
                Typing::ValueType => p.value_type = if text.is_empty() { None } else { Some(text.to_string()) },
                Typing::ApiName => p.api_name = if text.is_empty() { None } else { Some(text.to_string()) },
                Typing::New => unreachable!(),
            }
            Ok(i)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::ShapeKind;

    #[test]
    fn renaming_a_marked_row_keeps_its_mark_and_a_deprecated_one_is_red() {
        use ratatui::{backend::TestBackend, Terminal};
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::ObjectType, "Airport", 0.0, 0.0);
        let mut code = Property::new("code");
        code.primary_key = true;
        let mut iata = Property::new("iata");
        iata.status = Status::Deprecated;
        doc.element_mut(id).unwrap().properties = vec![code, iata];
        // Renaming the key row used to cut its three-byte mark at byte two, and panic.
        let st = State { target: id, sel: 0, editing: Some((Typing::Name, "icao".into())) };
        let mut term = Terminal::new(TestBackend::new(70, 12)).unwrap();
        term.draw(|f| f.render_widget(Browser { state: &st, doc: &doc }, f.area())).unwrap();
        let out: String = term.backend().buffer().content.iter().map(|c| c.symbol()).collect();
        assert!(out.contains("⚿ icao█"), "{out}");
        let reds = term.backend().buffer().content.iter().filter(|c| c.symbol() == "✗" && c.fg == theme::t().red).count();
        assert_eq!(reds, 2, "the legend's and the deprecated row's");
    }

    #[test]
    fn rows_are_added_marked_typed_and_removed_and_the_box_grows_to_hold_them() {
        let mut doc = Document::default();
        let id = doc.add(ShapeKind::ObjectType, "Airport", 0.0, 0.0);
        let e = doc.element_mut(id).unwrap();
        let h0 = e.h;
        assert!(e.takes_rows() && !e.has_rows());
        assert_eq!(typed(e, 0, Typing::New, "code").unwrap(), 0);
        assert_eq!(typed(e, 0, Typing::New, "name").unwrap(), 1);
        assert_eq!(typed(e, 0, Typing::New, "country").unwrap(), 1, "below the selection");
        assert_eq!(e.properties.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["code", "country", "name"]);
        apply(e, 0, 'p').unwrap();
        apply(e, 2, 'l').unwrap();
        apply(e, 1, 'p').unwrap();
        assert!(!e.properties[0].primary_key && e.properties[1].primary_key, "one primary key: the mark moved");
        typed(e, 1, Typing::Type, "geo").unwrap_err();
        typed(e, 1, Typing::Type, "geop").unwrap();
        assert_eq!(e.properties[1].base_type, BaseType::Geopoint);
        apply(e, 1, '[').unwrap();
        typed(e, 2, Typing::ValueType, "Name").unwrap();
        let r = e.properties[1].row(30);
        assert!(r.starts_with("⚿ country") && r.ends_with(" geopoint[]") && r.chars().count() == 30, "{r:?}");
        let r = e.properties[2].row(30);
        assert!(r.starts_with("✎ name") && r.ends_with(" string ‹Name›") && r.chars().count() == 30, "{r:?}");
        assert!(e.properties[2].row(12).contains('…'), "a long row is cut");
        assert_eq!(e.properties[1].compact(), "⚿ country: geopoint[]");
        assert_eq!(e.properties[2].compact(), "✎ name: string ‹Name›");
        assert_eq!(Property::new("plain").compact(), "plain: string");
        e.fit_rows();
        assert_eq!(e.h, 2.0 + Element::HEADER + 3.0);
        assert!(e.h > h0);
        assert_eq!(e.row_lines().len(), 3);
        assert_eq!(e.header_rule(), Some(Element::HEADER));
        assert_eq!(apply(e, 0, 'd').unwrap().as_deref(), Some("code removed"));
        assert_eq!(e.properties.len(), 2);
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("\"properties\":[") && json.contains("\"type\":\"geopoint\"") && json.contains("\"primary_key\":true"), "{json}");
        assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), doc);
        assert!(!serde_json::to_string(&Document::default()).unwrap().contains("properties"));
    }
}
