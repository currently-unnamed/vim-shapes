//! The sheet — the properties of whatever the cursor is on, docked down the right.
//!
//! A pane, not a modal. It is about the *current selection*, and it follows the cursor while
//! it is open: `c` opens it and gives it the keyboard; `esc` hands the keyboard back to the
//! diagram and leaves the sheet up, so you walk the graph with hjkl and watch it retarget;
//! `c` again re-enters; `q` closes. Three tabs — style, text, arrange — with `tab` between
//! them, and the tab and field you were on remembered across retargets where the next item
//! has them, so tuning six links is l, l, l.
//!
//! Inside it the grammar is the one used everywhere else: `j`/`k` field, `i` step in and
//! type, `h`/`l` cycle a choice, `t` the label, `enter` on an action runs it. Every commit is
//! one undo step, and lands on the diagram at once. The fields come from `form`; this file
//! only shows them and takes keys.

use super::canvas::Target;
use super::form::{self, Field, Tab, Unit};
use super::{chrome, theme};
use crate::model::{Document, ElementId};
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

/// How wide the sheet is when docked.
pub const WIDTH: u16 = 40;
/// Narrower than this, the terminal cannot spare a dock: the sheet floats instead, and only
/// while it has the keyboard, so the diagram stays visible the rest of the time.
pub const DOCK_MIN: u16 = 100;

pub struct State {
    pub target: Option<Target>,
    /// Held on the diagram by `:diagram`: the sheet stops following the cursor until it
    /// loses the keyboard.
    pub pinned: bool,
    pub tab: Tab,
    pub sel: usize,
    pub editing: Option<String>,
    /// Whether the sheet has the keyboard.
    pub focused: bool,
    /// The field last stood on, by name — what a retarget tries to land on again.
    remembered: Option<&'static str>,
    /// Every field of the target, all tabs.
    all: Vec<Field>,
    /// The picked set, when the target is `Picked`.
    pub picked: Vec<ElementId>,
}

impl State {
    /// Open on a target: the style tab for a shape; for a relation node, the text tab on that
    /// node's label, which is what you most often came for.
    pub fn open(doc: &Document, target: Option<Target>, picked: &[ElementId]) -> State {
        let mut s = State { target: None, pinned: false, tab: Tab::Style, sel: 0, editing: None, focused: true, remembered: None, all: Vec::new(), picked: Vec::new() };
        if let Some(Target::Relation(_, node)) = target {
            s.tab = Tab::Text;
            s.remembered = Some(form::node_field(node));
        }
        s.retarget(doc, target, picked);
        s
    }

    /// Point the sheet at what the cursor is on now. Keeps the tab, and the field by name
    /// where the new item has it; a shape and a link share `colour` and `label`, so walking
    /// from one to the other keeps the same field under the cursor as often as not.
    pub fn retarget(&mut self, doc: &Document, target: Option<Target>, picked: &[ElementId]) {
        if self.target != target {
            self.editing = None;
        }
        self.target = target;
        self.picked = picked.to_vec();
        self.all = target.and_then(|t| form::fields_for(doc, t, picked)).unwrap_or_default();
        let want = self.remembered.or_else(|| self.field().map(|f| f.name));
        let in_tab = self.fields();
        self.sel = want.and_then(|n| in_tab.iter().position(|f| f.name == n)).unwrap_or(0);
    }

    /// Re-read every value after a commit.
    pub fn refresh(&mut self, doc: &Document) {
        let target = self.target;
        let keep = self.remembered;
        let picked = std::mem::take(&mut self.picked);
        self.retarget(doc, target, &picked);
        self.remembered = keep;
    }

    /// The fields of the current tab.
    pub fn fields(&self) -> Vec<&Field> {
        self.all.iter().filter(|f| f.tab == self.tab).collect()
    }

    pub fn field(&self) -> Option<&Field> {
        self.fields().get(self.sel).copied()
    }

    pub fn move_by(&mut self, delta: isize) {
        let n = self.fields().len() as isize;
        if n == 0 {
            return;
        }
        self.sel = (self.sel as isize + delta).rem_euclid(n) as usize;
        self.remembered = self.field().map(|f| f.name);
    }

    pub fn next_tab(&mut self, forward: bool) {
        let i = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0) as isize;
        let n = Tab::ALL.len() as isize;
        self.tab = Tab::ALL[(i + if forward { 1 } else { -1 }).rem_euclid(n) as usize];
        self.editing = None;
        self.sel = 0;
        self.remembered = self.field().map(|f| f.name);
    }

    pub fn step_in(&mut self) {
        if let Some(f) = self.field()
            && f.unit != Unit::Action
        {
            self.editing = Some(f.value.clone());
        }
    }

    /// `t`: the label, wherever the sheet is — the text tab, on the label.
    pub fn step_into_text(&mut self) {
        self.tab = Tab::Text;
        let found = {
            let fs = self.fields();
            fs.iter()
                .position(|f| f.name == "label")
                .or_else(|| fs.iter().position(|f| f.unit == Unit::Text))
                .map(|i| (i, fs[i].name))
        };
        if let Some((i, name)) = found {
            self.sel = i;
            self.remembered = Some(name);
            self.step_in();
        }
    }

    /// The value `h`/`l` move to on a choice field.
    pub fn cycled(&self, delta: isize, doc: &Document) -> Option<String> {
        let f = self.field()?;
        let names = match f.unit {
            Unit::Layer => form::layer_choices(doc),
            u => u.choices()?,
        };
        let n = names.len() as isize;
        // A value that is none of the choices — a `custom` look — steps onto the first one
        // going forward and the last going back, rather than skipping one.
        let i = match names.iter().position(|x| *x == f.value) {
            Some(i) => (i as isize + delta).rem_euclid(n),
            None if delta > 0 => 0,
            None => n - 1,
        };
        Some(names[i as usize].clone())
    }
}

pub struct Sheet<'a> {
    pub state: &'a State,
    pub doc: &'a Document,
}

impl Widget for Sheet<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let s = self.state;
        let (title, accent) = match s.target {
            Some(Target::Element(id)) => match self.doc.element(id) {
                Some(e) => (format!("{} · {}", e.display(), e.kind.name().to_ascii_lowercase()), theme::layer_color(e.kind.layer())),
                None => ("shape".to_string(), theme::t().aqua),
            },
            Some(Target::Relation(id, _)) => match self.doc.relation(id) {
                Some(r) => {
                    let name = |i| self.doc.element(i).map(|e| e.display()).unwrap_or_default();
                    (format!("{} {} {}", name(r.from), r.kind.verb(), name(r.to)), theme::t().structure)
                }
                None => ("relation".to_string(), theme::t().aqua),
            },
            Some(Target::Diagram) => {
                let what = self.doc.metadata.title.clone().unwrap_or_else(|| "diagram".into());
                (format!("{what} · {}", self.doc.metadata.view.name()), theme::t().sand)
            }
            Some(Target::Picked) => (format!("{} shapes picked · what they share", s.picked.len()), theme::t().green),
            None => ("nothing selected".to_string(), theme::t().dim),
        };
        let accent = if s.focused { accent } else { theme::t().dim };
        let inner = chrome::panel(buf, area, &title, accent);
        let hint = match (s.focused, s.editing.is_some()) {
            (_, true) => " type, then esc or enter to leave the field",
            (true, false) => " j/k field  i type  h/l cycle  t label  tab next  esc → diagram  q close",
            (false, false) => " c into the sheet   q closes",
        };
        let body = chrome::hint(buf, inner, hint);
        if body.height < 3 {
            return;
        }
        // The tabs: the current one lit, the others dim; a marker, not an indent.
        let mut tabs: Vec<Span> = vec![Span::raw(" ")];
        for t in Tab::ALL {
            let on = t == s.tab;
            let style = match (on, s.focused) {
                (true, true) => Style::new().fg(theme::t().inverse).bg(theme::t().aqua).bold(),
                (true, false) => Style::new().fg(theme::t().aqua).bold(),
                _ => Style::new().fg(theme::t().dim),
            };
            tabs.push(Span::styled(format!(" {} ", t.name()), style));
            tabs.push(Span::raw(" "));
        }
        Paragraph::new(Line::from(tabs)).render(Rect { height: 1, ..body }, buf);
        let list = Rect { y: body.y + 2, height: body.height.saturating_sub(2), ..body };
        let mut lines: Vec<Line> = Vec::new();
        if s.target.is_none() {
            lines.push(Line::styled(" put the cursor on a shape or a relation, or :diagram", Style::new().fg(theme::t().dim)));
        }
        for (i, f) in s.fields().iter().enumerate() {
            let on = i == s.sel && s.focused;
            let name = format!("{}{:<11}", chrome::marker(i == s.sel), f.name);
            let (value, vs) = match (&s.editing, on, f.mixed) {
                (Some(t), true, _) => (format!("{t}█"), Style::new().fg(theme::t().green).bold()),
                (_, _, true) => ("mixed".into(), Style::new().fg(theme::t().dim).italic()),
                _ => (f.value.clone(), if on { Style::new().fg(theme::t().ink).bold() } else { Style::new().fg(theme::t().muted) }),
            };
            let ns = if on { Style::new().fg(theme::t().inverse).bg(accent).bold() } else { Style::new().fg(accent) };
            let room = (list.width as usize).saturating_sub(12 + 1);
            let value: String = if value.chars().count() > room { value.chars().take(room.saturating_sub(1)).chain(['…']).collect() } else { value };
            lines.push(Line::from(vec![Span::styled(name, ns), Span::raw(" "), Span::styled(value, vs)]));
            // The unit, dim, on its own line under the value: the sheet is narrow.
            lines.push(Line::styled(format!("            {}", f.unit.name()), Style::new().fg(theme::t().dim)));
        }
        Paragraph::new(lines).render(list, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Node;
    use crate::ontology::{RelationKind, ShapeKind};

    fn doc() -> (Document, u32, u32, u32) {
        let mut d = Document::default();
        let a = d.add(ShapeKind::ApplicationComponent, "CRM", 0.0, 0.0);
        let b = d.add(ShapeKind::ApplicationService, "API", 40.0, 0.0);
        let r = d.connect(RelationKind::Realization, a, b).unwrap();
        (d, a, b, r)
    }

    #[test]
    fn the_sheet_opens_on_style_for_a_shape_and_on_the_node_label_for_a_relation() {
        let (d, a, _, r) = doc();
        let s = State::open(&d, Some(Target::Element(a)), &[]);
        assert_eq!((s.tab, s.field().map(|f| f.name)), (Tab::Style, Some("look")));
        let s = State::open(&d, Some(Target::Relation(r, Node::Head)), &[]);
        assert_eq!((s.tab, s.field().map(|f| f.name)), (Tab::Text, Some("head label")));
    }

    #[test]
    fn retargeting_keeps_the_tab_and_the_field_by_name_where_the_next_item_has_it() {
        let (d, a, _, r) = doc();
        let mut s = State::open(&d, Some(Target::Element(a)), &[]);
        s.move_by(5);
        assert_eq!(s.field().unwrap().name, "stroke");
        s.retarget(&d, Some(Target::Relation(r, Node::Centre)), &[]);
        assert_eq!((s.tab, s.field().unwrap().name), (Tab::Style, "kind"), "no stroke on a relation: back to the top of the same tab");
        s.move_by(8);
        assert_eq!(s.field().unwrap().name, "colour");
        s.retarget(&d, Some(Target::Element(a)), &[]);
        assert_eq!(s.field().unwrap().name, "colour", "a field both have stays under the cursor");
        s.move_by(1);
        assert_eq!(s.field().unwrap().name, "line");
        s.retarget(&d, Some(Target::Relation(r, Node::Centre)), &[]);
        assert_eq!(s.field().unwrap().name, "line", "a shape's line pattern and a link's are the same field");
        s.retarget(&d, None, &[]);
        assert!(s.field().is_none());
    }

    #[test]
    fn tabs_cycle_and_t_goes_to_the_label_on_the_text_tab() {
        let (d, a, _, _) = doc();
        let mut s = State::open(&d, Some(Target::Element(a)), &[]);
        s.next_tab(true);
        assert_eq!(s.tab, Tab::Text);
        s.next_tab(true);
        assert_eq!(s.tab, Tab::Arrange);
        s.next_tab(true);
        assert_eq!(s.tab, Tab::Style, "wraps");
        s.step_into_text();
        assert_eq!((s.tab, s.field().unwrap().name), (Tab::Text, "label"));
        assert_eq!(s.editing.as_deref(), Some("CRM"));
    }
}
