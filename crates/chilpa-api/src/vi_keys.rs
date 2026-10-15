//! A count or an incomplete key sequence expires after 600 ms.
use std::time::{Duration, Instant};
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Sequence {
    count: usize,
    g: bool,
    last: Option<Instant>,
}
#[derive(Debug, PartialEq)]
pub(crate) enum Input {
    Pending,
    Key(String, usize),
}
impl Sequence {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn press(&mut self, key: &str, now: Instant) -> Input {
        if self
            .last
            .is_some_and(|at| now.duration_since(at) >= Duration::from_millis(600))
        {
            self.clear();
        }
        if key == "Escape" && (self.g || self.count > 0) {
            self.clear();
            return Input::Pending;
        }
        if key.len() == 1 && key.as_bytes()[0].is_ascii_digit() {
            self.count = (self.count * 10 + (key.as_bytes()[0] - b'0') as usize).min(9999);
            self.last = Some(now);
            return Input::Pending;
        }
        if key == "g" && !self.g {
            self.g = true;
            self.last = Some(now);
            return Input::Pending;
        }
        let token = match (self.g, key) {
            (true, "g") => "gg",
            (true, "t") => "gt",
            (true, "Shift+T") => "gT",
            _ => key,
        };
        let count = self.count.max(1);
        let token = token.to_owned();
        self.clear();
        Input::Key(token, count)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn key(token: &str, count: usize) -> Input {
        Input::Key(token.into(), count)
    }
    #[test]
    fn counts_apply_to_any_action_and_sequences_expire() {
        let mut s = Sequence::default();
        let now = Instant::now();
        for token in ["j", "a", "f", "Space", "Shift+K"] {
            s.press("5", now);
            assert_eq!(s.press(token, now), key(token, 5));
        }
        s.press("g", now);
        assert_eq!(s.press("g", now), key("gg", 1));
        s.press("g", now);
        assert_eq!(s.press("t", now), key("gt", 1));
        s.press("g", now);
        assert_eq!(s.press("Shift+T", now), key("gT", 1));
        s.press("9", now);
        assert_eq!(s.press("k", now + Duration::from_millis(600)), key("k", 1));
        s.press("g", now);
        assert_eq!(s.press("Escape", now), Input::Pending);
        assert!(!s.g);
        s.press("g", now);
        assert_eq!(s.press("x", now), key("x", 1));
    }
}
