//! I keep at most forty replaced texts for the lifetime of the bridge.
use std::collections::VecDeque;

#[derive(Default)]
pub(crate) struct History {
    current: Option<String>,
    replaced: VecDeque<String>,
}
impl History {
    pub fn observe(&mut self, text: &str) {
        if self.current.as_deref()==Some(text) { return; }
        if let Some(previous)=self.current.replace(text.into()) {
            if self.replaced.len()==40 { self.replaced.pop_front(); }
            self.replaced.push_back(previous);
        }
    }
    pub fn applied(&mut self, before: &str, after: &str) { self.observe(before); self.observe(after); }
    pub fn can_undo(&self) -> bool { !self.replaced.is_empty() }
    pub fn undo(&mut self) -> String {
        if let Some(text)=self.replaced.pop_back() { self.current=Some(text); }
        self.current.clone().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_and_external_texts_share_one_order_without_echoes() {
        let mut history=History::default(); history.observe("");
        history.applied("","genre:Jazz"); history.observe("genre:Jazz");
        history.observe("year:1960"); history.applied("year:1960","year:1960 rating:4..");
        assert_eq!(history.undo(),"year:1960"); history.observe("year:1960");
        assert_eq!(history.undo(),"genre:Jazz"); assert_eq!(history.undo(),"");
        assert!(!history.can_undo()); assert_eq!(history.undo(),"");
    }
    #[test]
    fn the_oldest_texts_expire_at_forty() {
        let mut history=History::default();
        for i in 0..=50 { history.observe(&i.to_string()); }
        for i in (10..50).rev() { assert_eq!(history.undo(),i.to_string()); }
        assert!(!history.can_undo());
    }
    #[test]
    fn a_noop_and_the_initial_text_do_not_create_undo_steps() {
        let mut history=History::default(); history.observe("genre:Jazz");
        history.applied("genre:Jazz","genre:Jazz"); assert!(!history.can_undo());
    }
}
