//! What sounds from a source's listing: the card it was listed with, kept
//! from one run to the next, and the pictures that arrive for it.

use chilpa_plugin::messages::Entry;

use super::{Answer, Sounding};
use crate::Chilpa;

impl Chilpa {
    /// Starts an entry from the address it sounds from, and goes for its
    /// picture meanwhile.
    pub(super) fn sound(&mut self, url: String, source: String, entry: Entry) {
        let picture = entry.image_url.clone();
        self.sources.sounding = Some(Sounding { url: url.clone(), source, entry, picture: String::new() });
        self.write_sounding();
        let (device, bit_perfect) = self.chosen_output();
        self.play_url(&url, &device, bit_perfect);
        // The play that just went out, if one did: the picture is its card's.
        let play = self.plays.current().map(|play| play.id);
        if let Some(address) = picture {
            let tx = self.sources.tx.clone();
            crate::pictures::keep_later(address, move |path| {
                let _ = tx.send(Answer::Picture { play, url, song: String::new(), path });
            });
        }
    }

    /// Puts a picture that arrived on what is playing, if it is still that.
    /// One asked for a play goes on that play's own card, whether its track
    /// has started or not, and reaches the window only while that play is in
    /// front of it.
    pub(super) fn take_picture(&mut self, play: Option<u64>, url: String, song: String, path: String) {
        let stations = song.is_empty();
        if let (Some(id), true) = (play, stations) {
            if let Some(card) = self.plays.get_mut(id).and_then(|play| play.cards.entry.as_mut()).filter(|card| card.url == url) {
                card.picture = path.clone();
            }
        }
        let shown = play.is_none_or(|id| self.play_shown(id));
        let Some(sounding) = self.sources.sounding.as_mut().filter(|s| s.url == url) else { return };
        if stations {
            sounding.picture = path.clone();
        }
        // A song's cover that arrives after the song is over is not shown.
        let current = song.is_empty() && self.now.artwork_path.is_empty() || song == self.now.title;
        if shown && self.now.path == url && current {
            self.now.artwork_path = path;
        }
        if stations {
            self.write_sounding();
        }
    }

    /// Where the last thing that sounded from a source is written down, so
    /// the next run can still say what the queue's address is.
    pub(super) fn sounding_file() -> std::path::PathBuf {
        chilpa_dirs::data().join("sounding.json")
    }

    pub(super) fn write_sounding(&self) {
        if let Some(text) = self.sources.sounding.as_ref().and_then(|s| serde_json::to_string(s).ok()) {
            let _ = std::fs::write(Self::sounding_file(), text);
        }
    }

    /// Reads back what the last run left sounding, once.
    /// The card of what sounds from a source's listing, for the play that is
    /// being launched to keep as its own.
    pub(crate) fn sounding_card(&mut self) -> Option<Sounding> {
        self.look_back();
        self.sources.sounding.clone()
    }

    pub(super) fn look_back(&mut self) {
        if std::mem::replace(&mut self.sources.looked_back, true) || self.sources.sounding.is_some() {
            return;
        }
        self.sources.sounding = std::fs::read_to_string(Self::sounding_file()).ok().and_then(|text| serde_json::from_str(&text).ok());
    }
}
