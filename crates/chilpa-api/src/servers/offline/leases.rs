//! At most 64 active file gates. Closing a decoder releases memory only.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, Weak},
};

static FILES: LazyLock<Mutex<HashMap<String, Weak<Mutex<usize>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
const MAX_ACTIVE: usize = 64;

pub(super) fn gate(key: &str) -> Option<Arc<Mutex<usize>>> {
    let mut files = FILES.lock().unwrap_or_else(|error| error.into_inner());
    files.retain(|_, gate| gate.strong_count() > 0);
    if let Some(gate) = files.get(key).and_then(Weak::upgrade) {
        return Some(gate);
    }
    if files.len() >= MAX_ACTIVE {
        return None;
    }
    let gate = Arc::new(Mutex::new(0));
    files.insert(key.to_string(), Arc::downgrade(&gate));
    Some(gate)
}

struct Lease(Arc<Mutex<usize>>);
impl Drop for Lease {
    fn drop(&mut self) {
        let mut count = self.0.lock().unwrap_or_else(|error| error.into_inner());
        *count = count.saturating_sub(1);
    }
}

pub(super) fn acquire(key: &str) -> Option<Arc<dyn Send + Sync>> {
    let gate = gate(key)?;
    *gate.lock().unwrap_or_else(|error| error.into_inner()) += 1;
    Some(Arc::new(Lease(gate)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readers_pin_the_exact_file_until_their_last_shared_lease_closes() {
        let first = acquire("lease-fixture").unwrap();
        let shared = first.clone();
        let second = acquire("lease-fixture").unwrap();
        let gate = gate("lease-fixture").unwrap();
        assert_eq!(*gate.lock().unwrap(), 2);
        drop(first);
        assert_eq!(*gate.lock().unwrap(), 2);
        drop(shared);
        assert_eq!(*gate.lock().unwrap(), 1);
        drop(second);
        assert_eq!(*gate.lock().unwrap(), 0);
    }
}
