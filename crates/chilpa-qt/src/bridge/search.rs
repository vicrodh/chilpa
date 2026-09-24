//! La búsqueda, del lado de la ventana.
//!
//! Lo local sale al instante; lo de los sources llega en el pulso y la lista
//! se vuelve a armar sola.

use super::Chilpa;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

impl Chilpa {

    pub fn search(mut self: Pin<&mut Self>, text: &QString) {
        let owned = text.to_string();
        self.as_mut().set_search_text(text.clone());
        // Esto solo trae lo local, que sale al instante. Lo que aporten los
        // sources llega después, y `tick` lo recoge.
        self.as_mut().rust_mut().api.search_all(&owned, 200);
        self.as_mut().refresh_results();
    }

    /// Pasa a la vista lo que la búsqueda lleva junto ahora.
    ///
    /// Se llama al buscar y en cada pulso mientras algún source siga
    /// contestando: los resultados crecen solos, sin que el usuario tenga
    /// que volver a escribir.
    pub fn refresh_results(mut self: Pin<&mut Self>) {
        // Se toma la marca aquí para que quien llame directo no deje un
        // refresco pendiente para el siguiente pulso.
        let _ = self.as_mut().rust_mut().api.search_changed();
        let hits = self.as_mut().rust_mut().api.found().to_vec();
        let mut list = QList::<QString>::default();
        // Va alineado renglón a renglón con la lista, con una ruta vacía
        // donde no hay nada que tocar: si se saltaran renglones, "reproducir
        // desde aquí" arrancaría en otra pista.
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
        // La fachada decide: lo local suena y encola lo que sigue, lo de un
        // source se resuelve primero. Aquí no se sabe cuál es cuál, y está
        // bien que no se sepa.
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
