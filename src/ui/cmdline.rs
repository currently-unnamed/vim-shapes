//! The `:` line's own editing state — the buffer, and where recall and completion currently
//! sit. What each typed word *means* is `excmd`'s job; what happens once one is submitted is
//! `App::run_excmd`'s. History is not kept here: a `State` is born fresh every time `:` is
//! pressed and thrown away when it closes, so anything that has to survive belongs to `App`.

pub const MAX_HISTORY: usize = 50;

pub struct State {
    pub buf: String,
    /// `:` for a command, `/` for a search. Decides what Enter does with the line.
    pub prompt: char,
    /// What was on the line when recall began — Up/Down keep searching against *this*.
    search: Option<String>,
    recall: Option<usize>,
    /// The word Tab is completing, and how far into its candidates it sits — set when a cycle
    /// begins, not read from `buf` every press: once a candidate has landed, `buf` no longer
    /// *is* the prefix.
    tab: Option<(String, usize)>,
}

impl State {
    pub fn new(prompt: char) -> State {
        State { buf: String::new(), prompt, search: None, recall: None, tab: None }
    }

    pub fn push(&mut self, c: char) {
        self.buf.push(c);
        self.reset_recall();
        self.tab = None;
    }

    /// Returns whether the line is now empty — backspacing past the last character is how
    /// vim's own command line closes.
    pub fn backspace(&mut self) -> bool {
        self.buf.pop();
        self.reset_recall();
        self.tab = None;
        self.buf.is_empty()
    }

    pub fn tab_prefix(&self) -> &str {
        self.tab.as_ref().map(|(p, _)| p.as_str()).unwrap_or(&self.buf)
    }

    /// Move to the next (`forward`) or previous entry of `list` and write it into the buffer.
    /// Wraps, so there is no dead end to cycle into.
    pub fn cycle_to(&mut self, list: &[String], forward: bool) {
        if list.is_empty() {
            return;
        }
        let prefix = self.tab_prefix().to_string();
        let i = self.tab.as_ref().map_or(if forward { list.len() - 1 } else { 0 }, |(_, i)| *i);
        let next = if forward { (i + 1) % list.len() } else { (i + list.len() - 1) % list.len() };
        self.tab = Some((prefix, next));
        self.buf = list[next].clone();
    }

    fn reset_recall(&mut self) {
        self.search = None;
        self.recall = None;
    }

    pub fn recall_prev(&mut self, history: &[String]) {
        let search = self.search.get_or_insert_with(|| self.buf.clone()).clone();
        let before = self.recall.unwrap_or(history.len());
        if let Some(i) = history[..before].iter().rposition(|h| h.starts_with(&search)) {
            self.recall = Some(i);
            self.buf = history[i].clone();
        }
    }

    pub fn recall_next(&mut self, history: &[String]) {
        let Some(search) = self.search.clone() else { return };
        let Some(at) = self.recall else { return };
        match history[at + 1..].iter().position(|h| h.starts_with(&search)) {
            Some(i) => {
                self.recall = Some(at + 1 + i);
                self.buf = history[at + 1 + i].clone();
            }
            None => {
                self.recall = None;
                self.buf = search;
            }
        }
    }

    pub fn submit(&mut self) -> String {
        let line = std::mem::take(&mut self.buf);
        self.reset_recall();
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn backspace_on_an_empty_line_says_so() {
        let mut s = State::new(':');
        assert!(s.backspace());
    }

    #[test]
    fn recall_walks_backward_then_forward_through_history() {
        let history = strs(&["a", "b", "c"]);
        let mut s = State::new(':');
        s.recall_prev(&history);
        assert_eq!(s.buf, "c");
        s.recall_prev(&history);
        assert_eq!(s.buf, "b");
        s.recall_next(&history);
        assert_eq!(s.buf, "c");
        s.recall_next(&history);
        assert_eq!(s.buf, "", "past the newest match: what was typed before recall began");
    }

    #[test]
    fn recall_only_offers_entries_that_share_what_was_typed() {
        let history = strs(&["write foo", "add node", "write bar"]);
        let mut s = State::new(':');
        s.push('w');
        s.recall_prev(&history);
        assert_eq!(s.buf, "write bar");
        s.recall_prev(&history);
        assert_eq!(s.buf, "write foo");
    }

    #[test]
    fn cycling_keeps_testing_the_original_prefix_not_whatever_landed_in_the_buffer() {
        let list = strs(&["write", "wq"]);
        let mut s = State::new(':');
        s.push('w');
        s.cycle_to(&list, true);
        assert_eq!(s.buf, "write");
        assert_eq!(s.tab_prefix(), "w");
        s.cycle_to(&list, true);
        assert_eq!(s.buf, "wq");
        s.cycle_to(&list, true);
        assert_eq!(s.buf, "write", "wraps");
        s.push('!');
        assert_eq!(s.tab_prefix(), "write!", "typing ends the cycle");
    }
}
