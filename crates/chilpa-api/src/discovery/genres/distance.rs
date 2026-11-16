//! Reusable, banded edit-distance rows for a single genre resolution.
//! At most two small buffers live during recognition; nothing is retained.
#[derive(Default)]
pub(super) struct Rows {
    first: Vec<usize>,
    second: Vec<usize>,
}

impl Rows {
    pub fn between(&mut self, a: &[char], b: &[char], limit: usize) -> usize {
        let outside = limit + 1;
        if a.len().abs_diff(b.len()) > limit { return outside; }
        let capacity = a.len() + limit + 1;
        self.first.resize(capacity, outside);
        self.second.resize(capacity, outside);
        let (mut previous, mut current) = (&mut self.first, &mut self.second);
        previous[..=b.len()].fill(outside);
        for (at, cell) in previous.iter_mut().take(b.len().min(limit) + 1).enumerate() { *cell = at; }
        for (index, letter) in a.iter().enumerate() {
            let row = index + 1;
            current[..=b.len()].fill(outside);
            current[0] = row.min(outside);
            let mut least = current[0];
            for column in row.saturating_sub(limit).max(1)..=b.len().min(row + limit) {
                current[column] = (previous[column] + 1).min(current[column - 1] + 1)
                    .min(previous[column - 1] + usize::from(*letter != b[column - 1]));
                least = least.min(current[column]);
            }
            if least > limit { return outside; }
            std::mem::swap(&mut previous, &mut current);
        }
        previous[b.len()].min(outside)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banded_results_match_the_full_reference_including_reused_rows() {
        let mut words = vec![String::new()];
        for length in 1..=5 {
            for bits in 0..1usize << length {
                words.push((0..length).map(|at| if bits & (1 << at) == 0 { 'a' } else { 'b' }).collect());
            }
        }
        words.extend(["électro".into(), "音楽".into(), "Progressive Metal".into(), "Progresive Metal".into()]);
        let mut rows = Rows::default();
        for a in &words {
            for b in &words {
                let (a, b) = (a.chars().collect::<Vec<_>>(), b.chars().collect::<Vec<_>>());
                for limit in 0..=3 {
                    let expected = crate::library_query::distance(&a, &b, limit).min(limit + 1);
                    assert_eq!(rows.between(&a, &b, limit), expected, "{a:?} {b:?} limit={limit}");
                }
            }
        }
    }
}
