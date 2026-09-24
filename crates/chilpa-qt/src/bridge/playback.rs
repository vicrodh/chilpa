//! Starting something playing and driving the queue.
//!
//! The output and the bit-perfect policy come from what the user chose in
//! the window; here they are only handed to the facade.

use super::Chilpa;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

impl Chilpa {
    /// Reads the stacks, then shows the outputs of the one in use.
    ///
    /// Which one is in use comes from the chosen output, not from a second
    /// thing to keep in step: the identifier already names its stack, so
    /// there is nothing that can disagree with it.
    pub fn refresh_outputs(mut self: Pin<&mut Self>) {
        let stacks = self.api.stacks();
        let mut rows = QList::<QString>::default();
        for stack in &stacks {
            rows.append(QString::from(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                stack.key,
                stack.label,
                if stack.present { "1" } else { "" },
                stack.missing,
                stack.fidelity,
                stack.caveat
            )));
        }
        self.as_mut().set_stacks(QStringList::from(&rows));

        let chosen = self.device().to_string();
        let from_device = self.api.stack_of(&chosen);
        // What to show when nothing is chosen yet: the first stack that is
        // here and has something to offer.
        let key = if stacks.iter().any(|s| s.key == from_device && s.present) {
            from_device
        } else {
            stacks
                .iter()
                .find(|s| s.present && !self.api.stack_outputs(&s.key).is_empty())
                .map(|s| s.key.clone())
                .unwrap_or_default()
        };
        self.as_mut().choose_stack(&QString::from(&key));
    }

    /// Shows one stack's outputs.
    pub fn choose_stack(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        self.as_mut().set_stack(QString::from(&key));
        let caveat = self
            .api
            .stacks()
            .into_iter()
            .find(|s| s.key == key)
            .map(|s| s.caveat)
            .unwrap_or_default();
        self.as_mut().set_stack_caveat(QString::from(&caveat));
        let outputs = self.api.stack_outputs(&key);
        let mut tags = QList::<QString>::default();
        let mut ids = QList::<QString>::default();
        for output in &outputs {
            // Three fields, tab separated, as everything else that crosses
            // the bridge as a list: the name, the line under it, and whether
            // it carries samples untouched.
            tags.append(QString::from(&format!(
                "{}\t{}\t{}",
                output.label,
                output.detail,
                if output.bit_perfect { "1" } else { "" }
            )));
            ids.append(QString::from(&output.id));
        }
        self.as_mut().set_outputs(QStringList::from(&tags));
        self.as_mut().set_output_ids(QStringList::from(&ids));
    }

    pub fn play(mut self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        // QML hands a file dialog's path over as a URL.
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().set_error(QString::from(""));
        self.as_mut().set_track(QString::from(&path));
        self.as_mut().set_playing(true);
        self.as_mut()
            .rust_mut()
            .api
            .play_file(std::path::Path::new(&path), &device, bit_perfect);
        // A lone file is a one track queue too: the view has to be able to
        // save it and empty it like any other.
        let queue = self.as_mut().rust_mut().api.queue_titles();
        self.as_mut().refresh_queue(&queue);
        self.as_mut().set_queue_index(0);
        self.as_mut().remember_folder(&path);
    }

    pub fn play_url(mut self: Pin<&mut Self>, url: &QString) {
        let address = url.to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().rust_mut().api.play_url(&address, &device, bit_perfect);
        let mut list = QList::<QString>::default();
        list.append(QString::from(&address));
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(true);
        self.as_mut().set_track(QString::from(&address));
    }

    /// Queues a whole folder and sets it going.
    pub fn play_folder(mut self: Pin<&mut Self>, folder: &QString) {
        let path = folder.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().remember_folder(&format!("{path}/."));
        let tracks = self
            .as_mut()
            .rust_mut()
            .api
            .play_folder(std::path::Path::new(&path), &device, bit_perfect);
        let mut list = QList::<QString>::default();
        for track in &tracks {
            list.append(QString::from(track));
        }
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!tracks.is_empty());
    }

    pub fn play_cd(mut self: Pin<&mut Self>, drive: &QString) {
        let unit = drive.to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().set_error(QString::from(""));
        if self.as_mut().rust_mut().api.play_cd(&unit, &device, bit_perfect) {
            self.as_mut().set_playing(true);
        }
    }

    pub fn refresh_cd_drives(mut self: Pin<&mut Self>) {
        let units = self.api.cd_drives();
        let mut l = QList::<QString>::default();
        for u in &units {
            l.append(QString::from(u));
        }
        self.as_mut().set_cd_drives(QStringList::from(&l));
    }

    pub fn save_queue(mut self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        let ok = self.as_mut().rust_mut().api.save_queue(std::path::Path::new(&path));
        if ok {
            let saved = self.api.t_args("playlist saved to {}", &[&path]);
            self.as_mut().set_library_status(QString::from(&saved));
        }
    }

    /// Puts the queue the facade reports into the view's properties.
    pub(super) fn refresh_queue(mut self: Pin<&mut Self>, titles: &[String]) {
        let mut list = QList::<QString>::default();
        for t in titles {
            list.append(QString::from(t));
        }
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().refresh_removed();
    }

    /// Which positions of the queue are greyed out.
    pub(super) fn refresh_removed(mut self: Pin<&mut Self>) {
        let removed: Vec<bool> = self.api.queue_removed().to_vec();
        let mut l = QList::<QString>::default();
        for q in removed {
            l.append(QString::from(if q { "1" } else { "0" }));
        }
        self.as_mut().set_queue_removed(QStringList::from(&l));
    }

    pub fn remove_from_queue(mut self: Pin<&mut Self>, index: i32) {
        if index < 0 {
            return;
        }
        if self.as_mut().rust_mut().api.remove_from_queue(index as usize) {
            self.as_mut().refresh_removed();
        }
    }

    pub fn clear_queue(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.clear_queue();
        self.as_mut().set_queue(QStringList::default());
        self.as_mut().set_queue_removed(QStringList::default());
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(false);
    }

    /// The chosen output and policy, which is what everything that plays uses.
    pub(super) fn chosen(self: Pin<&mut Self>) -> (String, bool) {
        (self.device.to_string(), self.want_bit_perfect)
    }

    /// Chooses the output, and moves the music to it if something is playing.
    ///
    /// Moving is the point. Somebody who plugs in a DAC and picks it wants to
    /// hear the track they are listening to come out of it, not the next one:
    /// a picker that only takes effect on the next track is a setting, not a
    /// control. What it costs is a short gap, because bit-perfect means one
    /// device at a time and the old one is released before the new one opens.
    pub fn choose(mut self: Pin<&mut Self>, device: &QString, bit_perfect: bool) {
        let id = device.to_string();
        let moved = id != self.device().to_string() || bit_perfect != *self.want_bit_perfect();
        self.as_mut().set_device(device.clone());
        self.as_mut().set_want_bit_perfect(bit_perfect);
        self.as_mut().rust_mut().api.remember(&id, bit_perfect);
        if moved {
            self.as_mut().rust_mut().api.move_output(&id, bit_perfect);
        }
        // Another output means another control, or none at all.
        self.as_mut().refresh_volume();
    }

    pub fn automix(mut self: Pin<&mut Self>) {
        let mixed = self.as_mut().rust_mut().api.automix();
        if mixed > 0 {
            let mix = self.api.tf(
                "mixed by tempo: {} track",
                "mixed by tempo: {} tracks",
                i64::try_from(mixed).unwrap_or_default(),
                &[&mixed.to_string()],
            );
            self.as_mut().set_library_status(QString::from(&mix));
            // The mix switches shuffle off: they are two forms of the same thing.
            self.as_mut().rust_mut().shuffle = false;
            self.as_mut().shuffle_changed();
        }
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn shuffle_queue(mut self: Pin<&mut Self>, shuffle: bool) {
        if self.shuffle == shuffle {
            return;
        }
        self.as_mut().rust_mut().shuffle = shuffle;
        self.as_mut().rust_mut().api.set_shuffle(shuffle);
        self.as_mut().shuffle_changed();
    }

    pub fn cycle_repeat(mut self: Pin<&mut Self>) {
        let next = (self.repeat + 1) % 3;
        let mode = match next {
            1 => chilpa_api::Repeat::All,
            2 => chilpa_api::Repeat::One,
            _ => chilpa_api::Repeat::Off,
        };
        self.as_mut().set_repeat(next);
        self.as_mut().rust_mut().api.set_repeat(mode);
    }

    pub fn next_track(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.next_track();
    }

    pub fn previous_track(mut self: Pin<&mut Self>) {
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().rust_mut().api.previous_track(&device, bit_perfect);
    }

    pub fn pause(mut self: Pin<&mut Self>, paused: bool) {
        self.as_mut().rust_mut().api.set_paused(paused);
        self.as_mut().set_paused(paused);
    }

    pub fn seek(mut self: Pin<&mut Self>, seconds: f64) {
        self.as_mut().rust_mut().api.seek(seconds);
    }

    pub fn stop(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.stop();
    }

    pub fn choose_visualizer(mut self: Pin<&mut Self>, name: &QString) {
        let owned = name.to_string();
        self.as_mut().set_visualizer(name.clone());
        self.as_mut().rust_mut().api.remember_visualizer(&owned);
    }

    pub fn choose_layout(mut self: Pin<&mut Self>, name: &QString) {
        let owned = name.to_string();
        self.as_mut().set_layout(name.clone());
        self.as_mut().rust_mut().api.remember_layout(&owned);
    }

    /// Reads the chosen output's own volume control into the properties.
    ///
    /// Called when the output changes and when the window asks, not on the
    /// tick: it is a mixer syscall, and nothing moves the knob but the user
    /// and whatever else is on the machine.
    pub fn refresh_volume(mut self: Pin<&mut Self>) {
        let (device, _) = self.as_mut().chosen();
        let v = self.as_mut().rust_mut().api.volume(&device);
        self.as_mut().set_has_volume(v.available);
        self.as_mut().set_volume(i32::from(v.percent));
        self.as_mut().set_muted(v.muted);
        self.as_mut().set_can_mute(v.can_mute);
        self.as_mut().set_volume_control(QString::from(&v.element));
    }

    /// Moves the output's own volume control.
    pub fn choose_volume(mut self: Pin<&mut Self>, percent: i32) {
        let (device, _) = self.as_mut().chosen();
        let percent = percent.clamp(0, 100) as u8;
        if self.as_mut().rust_mut().api.set_volume(&device, percent) {
            self.as_mut().set_volume(i32::from(percent));
        }
    }

    /// Mutes or unmutes the output.
    pub fn choose_muted(mut self: Pin<&mut Self>, muted: bool) {
        let (device, _) = self.as_mut().chosen();
        if self.as_mut().rust_mut().api.set_muted(&device, muted) {
            self.as_mut().set_muted(muted);
        }
    }

    pub fn run_wizard(mut self: Pin<&mut Self>) {
        let (device, _) = self.as_mut().chosen();
        let report = self.as_mut().rust_mut().api.wizard(&device);
        let mut l = QList::<QString>::default();
        // First what the device said about itself.
        if report.probed {
            let rates =
                report.rates.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
            l.append(QString::from(&format!("{}\t{}\t", self.api.t("Rates"), rates)));
            l.append(QString::from(&format!(
                "{}\t{}\t",
                self.api.t("Formats"),
                report.formats.join(", ")
            )));
        }
        for f in &report.rows {
            l.append(QString::from(&format!("{}\t{}\t{}", f.name, f.format, f.verdict)));
        }
        self.as_mut().set_wizard_rows(QStringList::from(&l));
        self.as_mut().set_wizard_summary(QString::from(&format!(
            "{}: {}",
            report.device, report.summary
        )));
    }
}
