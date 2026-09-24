//! Poner algo a sonar y manejar la cola.
//!
//! La salida y la política de bit-perfect salen de lo que el usuario eligió
//! en la ventana; aquí solo se le pasan a la fachada.

use super::Chilpa;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

impl Chilpa {
    pub fn refresh_outputs(mut self: Pin<&mut Self>) {
        let outputs = self.api.outputs();
        let mut tags = QList::<QString>::default();
        let mut ids = QList::<QString>::default();
        for output in &outputs {
            tags.append(QString::from(&output.label));
            ids.append(QString::from(&output.id));
        }
        self.as_mut().set_outputs(QStringList::from(&tags));
        self.as_mut().set_output_ids(QStringList::from(&ids));
    }

    pub fn play(mut self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        // QML entrega la ruta de un diálogo de archivo como URL.
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().set_error(QString::from(""));
        self.as_mut().set_track(QString::from(&path));
        self.as_mut().set_playing(true);
        self.as_mut()
            .rust_mut()
            .api
            .play_file(std::path::Path::new(&path), &device, bit_perfect);
        // Un archivo suelto también es una cola de una pista: la vista tiene
        // que poder guardarla y vaciarla como cualquier otra.
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

    /// Encola una carpeta entera y la echa a andar.
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
            self.as_mut().set_library_status(QString::from(&format!("lista guardada en {path}")));
        }
    }

    /// Deja la cola que reporta la fachada en las propiedades de la vista.
    pub(super) fn refresh_queue(mut self: Pin<&mut Self>, titles: &[String]) {
        let mut list = QList::<QString>::default();
        for t in titles {
            list.append(QString::from(t));
        }
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().refresh_removed();
    }

    /// Qué posiciones de la cola están apagadas.
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

    /// La salida y la política elegidas, que es lo que usa todo lo que suena.
    pub(super) fn chosen(self: Pin<&mut Self>) -> (String, bool) {
        (self.device.to_string(), self.want_bit_perfect)
    }

    pub fn choose(mut self: Pin<&mut Self>, device: &QString, bit_perfect: bool) {
        let id = device.to_string();
        self.as_mut().set_device(device.clone());
        self.as_mut().set_want_bit_perfect(bit_perfect);
        self.as_mut().rust_mut().api.remember(&id, bit_perfect);
    }

    pub fn automix(mut self: Pin<&mut Self>) {
        let mixed = self.as_mut().rust_mut().api.automix();
        if mixed > 0 {
            self.as_mut().set_library_status(QString::from(&format!(
                "mezcla por tempo: {mixed} pistas"
            )));
            // La mezcla apaga el aleatorio: son dos formas de lo mismo.
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

    pub fn run_wizard(mut self: Pin<&mut Self>) {
        let (device, _) = self.as_mut().chosen();
        let report = self.as_mut().rust_mut().api.wizard(&device);
        let mut l = QList::<QString>::default();
        // Primero lo que el dispositivo dijo de sí mismo.
        if report.probed {
            l.append(QString::from(&format!(
                "Frecuencias\t{}\t",
                report
                    .rates
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
            l.append(QString::from(&format!("Formatos\t{}\t", report.formats.join(", "))));
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
