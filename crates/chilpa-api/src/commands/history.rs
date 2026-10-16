//! A profile keeps the last two hundred successful command lines.
const LIMIT: usize = 200;
pub fn read(saved: Option<String>) -> Vec<String> {
    let lines: Vec<String> = saved
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    lines
        .into_iter()
        .rev()
        .take(LIMIT)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
pub fn push(lines: &mut Vec<String>, line: &str) {
    let line = line.trim();
    if line.is_empty() || lines.last().is_some_and(|s| s == line) {
        return;
    }
    lines.push(line.into());
    if lines.len() > LIMIT {
        lines.drain(..lines.len() - LIMIT);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_survives_serialization_and_is_bounded() {
        let mut lines = vec![];
        for n in 0..205 {
            push(&mut lines, &format!("search {n}"));
        }
        push(&mut lines, "search 204");
        assert_eq!(lines.len(), 200);
        assert_eq!(lines[0], "search 5");
        assert_eq!(read(Some(serde_json::to_string(&lines).unwrap())), lines);
        assert!(read(Some("broken".into())).is_empty());
    }
}
