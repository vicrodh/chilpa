//! The order a queue is traversed in, and shuffling it.

/// The order the queue is traversed in, shared.
///
/// It can be changed while something is playing: when the track ends, where
/// it landed in the new order is looked up and it goes on from there,
/// without cutting the audio. Empty means the array's own order.
#[derive(Clone, Default)]
pub struct Order(std::sync::Arc<std::sync::Mutex<Vec<usize>>>);

impl Order {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an order. One that is not a permutation of `0..len` is ignored: a
    /// half order would leave tracks unplayed or playing twice.
    pub fn set(&self, order: Vec<usize>, len: usize) -> bool {
        if !is_permutation(&order, len) {
            return false;
        }
        if let Ok(mut g) = self.0.lock() {
            *g = order;
            return true;
        }
        false
    }

    /// Goes back to the array's own order.
    pub fn clear(&self) {
        if let Ok(mut g) = self.0.lock() {
            g.clear();
        }
    }

    pub fn get(&self) -> Vec<usize> {
        self.0.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Which track comes after `current`, or `None` if it ends there.
    pub(super) fn next(&self, actual: usize, len: usize, wrap: bool) -> Option<usize> {
        let order = self.get();
        if order.len() != len {
            // With no order of its own, the queue goes as it comes.
            let n = actual + 1;
            return if n < len {
                Some(n)
            } else if wrap {
                Some(0)
            } else {
                None
            };
        }
        // Where the current track landed is looked up: if the order changed
        // mid playback, it goes on from its new place.
        let pos = order.iter().position(|i| *i == actual)?;
        match order.get(pos + 1) {
            Some(n) => Some(*n),
            None if wrap => order.first().copied(),
            None => None,
        }
    }

    /// Where it starts.
    pub(super) fn first(&self, len: usize) -> usize {
        let order = self.get();
        if order.len() == len { order.first().copied().unwrap_or(0) } else { 0 }
    }
}

/// A random permutation of `0..len`.
///
/// The generator is a three line xorshift: shuffling a queue needs nothing
/// better, and one more dependency is one more dependency.
pub fn shuffled(len: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    let mut x = seed | 1;
    // Fisher and Yates, from the end backwards: every ordering is as likely.
    for i in (1..len).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        order.swap(i, (x % (i as u64 + 1)) as usize);
    }
    order
}

/// The same, seeded from the clock.
pub fn shuffled_now(len: usize) -> Vec<usize> {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545_F491_4F6C_DD1D);
    shuffled(len, seed)
}

fn is_permutation(order: &[usize], len: usize) -> bool {
    if order.len() != len {
        return false;
    }
    let mut seen = vec![false; len];
    for i in order {
        match seen.get_mut(*i) {
            Some(v) if !*v => *v = true,
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_an_order_of_its_own_the_queue_plays_as_it_came() {
        let o = Order::new();
        assert_eq!(o.first(3), 0);
        assert_eq!(o.next(0, 3, false), Some(1));
        assert_eq!(o.next(2, 3, false), None, "it ends there");
        assert_eq!(o.next(2, 3, true), Some(0), "with repeat, it goes back to the first");
    }

    #[test]
    fn an_explicit_order_wins() {
        let o = Order::new();
        assert!(o.set(vec![2, 0, 1], 3));
        assert_eq!(o.first(3), 2);
        assert_eq!(o.next(2, 3, false), Some(0));
        assert_eq!(o.next(1, 3, false), None, "1 is the last of the order");
        assert_eq!(o.next(1, 3, true), Some(2));
    }

    #[test]
    fn changing_the_order_mid_queue_continues_from_where_it_was() {
        let o = Order::new();
        assert!(o.set(vec![0, 1, 2, 3], 4));
        // Track 1 plays and the order changes: what comes next is whatever follows it now.
        assert!(o.set(vec![3, 1, 0, 2], 4));
        assert_eq!(o.next(1, 4, false), Some(0));
    }

    #[test]
    fn an_order_that_is_not_a_permutation_is_refused() {
        let o = Order::new();
        assert!(!o.set(vec![0, 0, 1], 3), "repeated");
        assert!(!o.set(vec![0, 1], 3), "incomplete");
        assert!(!o.set(vec![0, 1, 5], 3), "out of range");
        assert!(o.get().is_empty(), "none of that was kept");
    }

    #[test]
    fn shuffling_gives_a_permutation_and_not_always_the_same_one() {
        for seed in [1u64, 42, 99_999] {
            let o = shuffled(8, seed);
            assert!(is_permutation(&o, 8), "{o:?} is not a permutation");
        }
        assert_ne!(shuffled(16, 1), shuffled(16, 2), "different seeds, different shuffles");
        assert_eq!(shuffled(16, 7), shuffled(16, 7), "the same seed always gives the same thing");
        assert_eq!(shuffled(1, 3), vec![0], "a single track is not shuffled");
        assert!(shuffled(0, 3).is_empty());
    }

    #[test]
    fn clearing_restores_the_array_order() {
        let o = Order::new();
        assert!(o.set(vec![1, 0], 2));
        o.clear();
        assert_eq!(o.first(2), 0);
    }
}
