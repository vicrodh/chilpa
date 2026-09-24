//! The tick: collecting what happened and arranging it into the properties.
//!
//! QML calls it from a timer. Everything that runs in the background -- the
//! playback, the scan, the analysis, the search, connecting an account -- is
//! collected here and nowhere else.

use super::Chilpa;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

impl Chilpa {

    pub fn poll(mut self: Pin<&mut Self>) {
        // The analysis advances on its own; all that is needed is to report it.
        if self.analyzing {
            let state = self.as_mut().rust_mut().api.analysis_status().clone();
            let progress = if state.total > 0 {
                state.done as f64 / state.total as f64
            } else {
                0.0
            };
            self.as_mut().set_analysis_progress(progress);
            if state.running {
                self.as_mut().set_analysis_status(QString::from(&format!(
                    "{} de {} · {}",
                    state.done, state.total, state.last
                )));
            } else {
                self.as_mut().set_analyzing(false);
                self.as_mut().set_analysis_progress(1.0);
                let done = self.api.t_args(
                    "{} analysed, {} with no pulse, {} unread",
                    &[
                        &state.analyzed.to_string(),
                        &state.without_pulse.to_string(),
                        &state.failed.to_string(),
                    ],
                );
                self.as_mut().set_analysis_status(QString::from(&done));
                // The tempo lists now have something to fill themselves with.
                self.as_mut().refresh_playlists();
            }
        }

        // The scan first: on finishing it refreshes the albums.
        if self.scanning {
            let state = self.as_mut().rust_mut().api.scan_status().clone();
            let progress = if state.total > 0 {
                state.done as f64 / state.total as f64
            } else {
                0.0
            };
            self.as_mut().set_scan_progress(progress);
            if !state.running {
                let r = &state.summary;
                let text = if r.unreachable {
                    self.api.t("that folder could not be read; nothing was touched")
                } else {
                    self.api.t_args(
                        "{} files: {} read, {} unchanged, {} missing",
                        &[
                            &r.seen.to_string(),
                            &r.read.to_string(),
                            &r.unchanged.to_string(),
                            &r.missing.to_string(),
                        ],
                    )
                };
                self.as_mut().set_library_status(QString::from(&text));
                self.as_mut().set_scanning(false);
                self.as_mut().set_scan_progress(1.0);
                self.as_mut().refresh_albums();
                // The genres and the lists' counts change with what just came in.
                self.as_mut().refresh_playlists();
            }
        }

        // What goes out, for the visualizers. Only while it plays: reading
        // the ring with the player idle is wasted work.
        if self.playing {
            let envelope = self.api.scope_envelope(120);
            let mut e = QList::<QString>::default();
            for v in &envelope {
                e.append(QString::from(&format!("{v:.4}")));
            }
            self.as_mut().set_scope(QStringList::from(&e));

            let spectrum = self.api.scope_spectrum(32);
            let mut b = QList::<QString>::default();
            for v in &spectrum {
                b.append(QString::from(&format!("{v:.4}")));
            }
            self.as_mut().set_spectrum(QStringList::from(&b));

            let pares = self.api.scope_pairs(400);
            let mut pl = QList::<QString>::default();
            for (i, d) in &pares {
                pl.append(QString::from(&format!("{i:.4}\t{d:.4}")));
            }
            self.as_mut().set_pairs(QStringList::from(&pl));
            let correlation = self.api.scope_correlation();
            self.as_mut().set_correlation(f64::from(correlation.unwrap_or(999.0)));

            let levels = self.api.scope_levels();
            let mut l = QList::<QString>::default();
            for (pico, rms) in &levels {
                l.append(QString::from(&format!("{pico:.4}\t{rms:.4}")));
            }
            self.as_mut().set_levels(QStringList::from(&l));
        }

        let now = self.as_mut().rust_mut().api.poll().clone();
        self.as_mut().set_playing(now.playing);
        self.as_mut().set_paused(now.paused);
        self.as_mut().set_seconds(now.seconds);
        self.as_mut().set_total_seconds(now.total_seconds);
        self.as_mut().set_bit_perfect(now.bit_perfect);
        self.as_mut().set_format(QString::from(&now.format));
        self.as_mut().set_title(QString::from(&now.title));
        self.as_mut().set_artist(QString::from(&now.artist));
        self.as_mut().set_album(QString::from(&now.album));
        let cover = if now.artwork_path.is_empty() {
            String::new()
        } else {
            format!("file://{}", now.artwork_path)
        };
        self.as_mut().set_artwork(QString::from(&cover));
        self.as_mut().set_queue_index(now.index as i32);
        // The lyrics are only rebuilt when they change: remaking a list of
        // QStrings sixty times a minute is wasted work.
        if self.lyrics_of != now.path {
            let mut list = QList::<QString>::default();
            for l in &now.lyrics {
                list.append(QString::from(l));
            }
            self.as_mut().set_lyrics(QStringList::from(&list));
            self.as_mut().set_lyrics_synced(now.lyrics_synced);
            self.as_mut().set_lyrics_source(QString::from(&now.lyrics_source));
            self.as_mut().rust_mut().lyrics_of = now.path.clone();
        }
        self.as_mut().set_lyrics_line(now.lyrics_line);
        self.as_mut().set_analysis(QString::from(&now.analysis));
        self.as_mut().set_error(QString::from(&now.error));
        if !now.path.is_empty() {
            self.as_mut().set_track(QString::from(&now.path));
        }

        // An account that finishes connecting changes the plugin list, and
        // the user is looking at that tab waiting for it.
        if self.as_mut().rust_mut().api.connecting() != self.connecting_shown {
            let waiting = self.as_mut().rust_mut().api.connecting();
            self.as_mut().rust_mut().connecting_shown = waiting;
            self.as_mut().refresh_plugins();
            if !waiting {
                self.as_mut().set_library_status(QString::from(""));
            }
        }

        // When a source's results arrive, the list is rebuilt: that is what
        // makes results appear on their own. What is asked is whether they
        // changed and not whether the query is still alive, because it can
        // start and finish between two ticks.
        if self.as_mut().rust_mut().api.search_changed() {
            self.as_mut().refresh_results();
        }
    }
}
