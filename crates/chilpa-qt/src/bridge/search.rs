//! Search, on the window's side.
//!
//! What is local comes out at once; what the sources give arrives on the
//! tick and the list rebuilds itself.

use super::Chilpa;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

impl Chilpa {

    pub fn search(mut self: Pin<&mut Self>, text: &QString) {
        // A search takes over the results panel, so a list that was open
        // stops being what is on screen.
        self.as_mut().set_open_list(QString::from(""));
        let owned = text.to_string();
        self.as_mut().set_search_text(text.clone());
        // This only brings what is local, which comes out at once. Whatever
        // the sources contribute arrives later, and `tick` picks it up.
        self.as_mut().rust_mut().api.search_all(&owned, 200);
        self.as_mut().refresh_results();
    }

    /// Passes to the view what the search has joined so far.
    ///
    /// It is called on searching and on every tick while some source is
    /// still answering: the results grow on their own, without the user
    /// having to type again.
    pub fn refresh_results(mut self: Pin<&mut Self>) {
        // The flag is taken here so a direct caller does not leave a refresh
        // pending for the next tick.
        let _ = self.as_mut().rust_mut().api.search_changed();
        let hits = self.as_mut().rust_mut().api.found().to_vec();
        let mut list = QList::<QString>::default();
        // It is aligned line by line with the list, with an empty path where
        // there is nothing to play: if lines were skipped, "play from here"
        // would start on another track.
        let mut paths = Vec::with_capacity(hits.len());
        for h in &hits {
            let seconds = h.duration_ms / 1000;
            list.append(QString::from(&format!(
                "{}\t{}\t{}\t{}\t{}:{:02}\t{}\t{}\t{}\t{}",
                h.id,
                h.title,
                h.artist,
                h.album,
                seconds / 60,
                seconds % 60,
                h.analysis,
                h.source_name,
                h.also_in.join(", "),
                if h.playable { "1" } else { "" },
            )));
            paths.push(if h.playable { h.id.clone() } else { String::new() });
        }
        self.as_mut().set_results(QStringList::from(&list));
        let problems = self.as_mut().rust_mut().api.search_problems();
        self.as_mut().set_search_problems(QString::from(&problems.join(" · ")));
        self.as_mut().rust_mut().result_paths = paths;
    }

    pub fn play_results(mut self: Pin<&mut Self>, from: i32) {
        let (device, bit_perfect) = self.as_mut().chosen();
        let from = from.max(0) as usize;
        // The facade decides: what is local plays and queues what follows,
        // what is a source's is resolved first. Here which is which is not
        // known, and it is right that it is not.
        let ok = self.as_mut().rust_mut().api.play_hit(from, &device, bit_perfect);
        let titles = self.as_mut().rust_mut().api.queue_titles();
        self.as_mut().refresh_queue(&titles);
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(ok);
    }

    pub fn enqueue_results(mut self: Pin<&mut Self>, from: i32) {
        let (device, bit_perfect) = self.as_mut().chosen();
        let from = from.max(0) as usize;
        let paths: Vec<String> =
            self.as_mut().rust_mut().result_paths.iter().skip(from).cloned().collect();
        self.as_mut().rust_mut().api.enqueue(&paths, &device, bit_perfect);
        let queue = self.as_mut().rust_mut().api.queue_titles();
        self.as_mut().refresh_queue(&queue);
        self.as_mut().set_playing(true);
    }
}
