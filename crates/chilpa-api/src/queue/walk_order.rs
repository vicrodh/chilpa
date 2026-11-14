//! The order the queue in front of the window is walked in, and who holds
//! it.
//!
//! A play is launched with its order and keeps it as its own: what is set
//! for the next play is never read by the one on its way out. So an order
//! set while a play is in front of the window goes to that play, and to the
//! queue it plays, which is where the next launch reads it once no play is
//! in front.

use crate::Chilpa;

impl Chilpa {
    /// The order this machine walks the queue in: the play's in front of
    /// the window, or the queue's own when none is. Empty, or of another
    /// length than the queue, when there is none.
    pub(crate) fn local_order(&self) -> Vec<usize> {
        match self.current_launch() {
            Some(launch) => launch.order().into_iter().map(|index| index + self.base).collect(),
            None => self.listen.buffers.iter().find(|b| b.id == self.listen.active).map(|b| b.order.clone()).unwrap_or_default(),
        }
    }

    /// Sets the order the queue is walked in, from the end of the playing
    /// track on. Refused without a player, as the player refused one that
    /// is not a permutation of `0..len`.
    pub(crate) fn set_walk_order(&mut self, order: Vec<usize>, len: usize) -> bool {
        if self.player.is_none() || !chilpa_core::queue_playback::Order::new().set(order.clone(), len) {
            return false;
        }
        if let Some(launch) = self.current_launch() {
            launch.set_order(order.clone(), len);
        }
        if let Some(b) = self.listen.buffers.iter_mut().find(|b| b.id == self.listen.active) {
            b.order = order;
        }
        true
    }

    /// Goes back to the order things were queued in.
    pub(crate) fn clear_walk_order(&mut self) {
        if self.player.is_none() {
            return;
        }
        if let Some(launch) = self.current_launch() {
            launch.clear_order();
        }
        if let Some(b) = self.listen.buffers.iter_mut().find(|b| b.id == self.listen.active) {
            b.order.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Chilpa, Relaunch};

    fn rows(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| format!("/nowhere/{name}.flac")).collect()
    }

    /// A facade with a player and a play asked of it on `names`, paused.
    fn playing(names: &[&str], order: Vec<usize>) -> Chilpa {
        let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
        api.player = Some(chilpa_core::spawn_player());
        ask(&mut api, names, order);
        api
    }

    fn ask(api: &mut Chilpa, names: &[&str], order: Vec<usize>) {
        api.swap_rows(rows(names));
        api.removed = vec![false; names.len()];
        let spec = chilpa_core::launch::LaunchSpec {
            paths: api.queue.iter().map(std::path::PathBuf::from).collect(), device: "file:capture".into(), paused: true, order,
            ..Default::default()
        };
        api.launch(spec, 0, Relaunch::Track);
    }

    fn kept(api: &Chilpa) -> Vec<usize> {
        api.listen.buffers.iter().find(|b| b.id == api.listen.active).map(|b| b.order.clone()).unwrap_or_default()
    }

    #[test]
    fn an_order_goes_to_the_play_in_front_of_the_window_and_to_its_queue() {
        let mut api = playing(&["a", "b", "c"], vec![0, 1, 2]);
        assert_eq!(api.full_queue_order(), [0, 1, 2], "the order it was launched with");
        assert!(api.set_walk_order(vec![2, 1, 0], 3));
        assert_eq!(api.current_launch().unwrap().order(), [2, 1, 0]);
        assert_eq!(kept(&api), [2, 1, 0]);
        assert_eq!(api.full_queue_order(), [2, 1, 0]);
        assert!(!api.set_walk_order(vec![0, 0, 1], 3), "one that is no permutation is refused");
        assert_eq!(api.full_queue_order(), [2, 1, 0]);
        api.clear_walk_order();
        assert!(api.current_launch().unwrap().order().is_empty());
        assert_eq!(api.full_queue_order(), [0, 1, 2], "the queue's own");
    }

    /// The order was the player's, one for every play: what was set for the
    /// queue that replaced a play was read by that play while it unwound.
    #[test]
    fn an_order_set_after_a_play_left_the_window_does_not_reach_it() {
        let mut api = playing(&["a", "b", "c"], vec![2, 1, 0]);
        let old = api.current_launch().unwrap().clone();
        // Other rows take the queue: the play is on its way out.
        api.swap_rows(rows(&["d", "e", "f"]));
        assert!(api.set_walk_order(vec![1, 0, 2], 3));
        assert_eq!(old.order(), [2, 1, 0], "it keeps the order it had");
        assert_eq!(api.full_queue_order(), [1, 0, 2], "the queue keeps it for the play that follows");
        api.clear_walk_order();
        assert_eq!(old.order(), [2, 1, 0]);
        // And the play that follows is asked with its own.
        ask(&mut api, &["d", "e", "f"], vec![1, 2, 0]);
        assert_eq!(api.current_launch().unwrap().order(), [1, 2, 0]);
        assert_eq!(old.order(), [2, 1, 0]);
    }

    #[test]
    fn what_is_asked_with_no_play_in_front_reaches_no_play() {
        let mut api = playing(&["a", "b", "c"], Vec::new());
        let old = api.current_launch().unwrap().clone();
        api.stop();
        assert!(api.current_launch().is_none());
        api.seek(1.0);
        api.next_track();
        assert!(api.set_stop_after(Some(1)));
        // A row taken out of a queue nothing plays is marked, once.
        assert!(api.remove_from_queue(2));
        assert!(!api.remove_from_queue(2));
        assert!(api.removed[2]);
        assert_eq!(old.queue_len(), 3);
        assert!(old.remove(2), "its own queue was not asked");
    }

    #[test]
    fn an_order_is_refused_without_a_player_as_it_was() {
        let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
        api.swap_rows(rows(&["a", "b"]));
        assert!(!api.set_walk_order(vec![1, 0], 2));
        assert!(kept(&api).is_empty());
    }
}
