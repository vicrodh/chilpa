//! I hand the same completed album snapshot to its header and its list.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

thread_local! {
    static PAGES: RefCell<(i32, VecDeque<(i32, Weak<chilpa_api::AlbumPage>)>)> = const { RefCell::new((0, VecDeque::new())) };
}

pub fn keep(page: chilpa_api::AlbumPage) -> (i32, Rc<chilpa_api::AlbumPage>) {
    PAGES.with(|slot| {
        let mut slot = slot.borrow_mut();
        slot.0 = slot.0.wrapping_add(1).max(1);
        let token = slot.0;
        let page = Rc::new(page);
        slot.1.retain(|(_, page)| page.strong_count() > 0);
        slot.1.push_back((token, Rc::downgrade(&page)));
        (token, page)
    })
}

pub fn get(token: i32, id: i64) -> Option<Rc<chilpa_api::AlbumPage>> {
    PAGES.with(|slot| slot.borrow().1.iter().find(|(key, _)| *key == token)
        .and_then(|(_, page)| page.upgrade()).filter(|page| page.detail.album.id == id))
}

/// I cancel obsolete work before SQL starts and before Qt applies its result.
#[derive(Default)]
pub struct Generation(Arc<AtomicU64>);

#[derive(Clone)]
pub struct Ticket { generation: Arc<AtomicU64>, asked: u64 }

impl Generation {
    pub fn next(&self) -> Ticket {
        Ticket { generation: self.0.clone(), asked: self.0.fetch_add(1, Ordering::Relaxed).wrapping_add(1) }
    }
}

impl Drop for Generation {
    fn drop(&mut self) { self.0.fetch_add(1, Ordering::Relaxed); }
}

impl Ticket {
    pub fn current(&self) -> bool { self.generation.load(Ordering::Relaxed) == self.asked }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_last_navigation_can_query_or_publish() {
        let generation = Generation::default();
        let first = generation.next();
        let second = generation.next();
        assert!(!first.current());
        assert!(second.current());
        drop(generation);
        assert!(!second.current(), "destroying the page cancels its queued query");
    }
}
