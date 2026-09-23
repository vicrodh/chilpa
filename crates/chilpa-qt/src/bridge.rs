//! El puente entre QML y la fachada.
//!
//! Aquí no vive lógica: el objeto expone propiedades que QML lee y órdenes
//! que QML dispara, y cada una es una línea que llama a `chilpa-api`
//! (ADR-001). Si una vista necesita algo que no está en la fachada, se amplía
//! la fachada, no este archivo.

use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod chilpa_bridge {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Las salidas que se pueden elegir, ya con etiqueta para mostrar.
        #[qproperty(QStringList, outputs)]
        /// Los identificadores, en el mismo orden que las etiquetas.
        #[qproperty(QStringList, output_ids)]
        /// Lo que suena y su estado.
        #[qproperty(QString, track)]
        #[qproperty(QString, format)]
        #[qproperty(QString, error)]
        #[qproperty(bool, playing)]
        #[qproperty(bool, bit_perfect)]
        #[qproperty(f64, seconds)]
        type Chilpa = super::ChilpaRust;

        /// Vuelve a leer las salidas de la máquina.
        #[qinvokable]
        fn refresh_outputs(self: Pin<&mut Chilpa>);

        /// Pone a sonar un archivo en la salida elegida.
        #[qinvokable]
        fn play(self: Pin<&mut Chilpa>, path: &QString, device: &QString, bit_perfect: bool);

        /// Para lo que esté sonando.
        #[qinvokable]
        fn stop(self: Pin<&mut Chilpa>);

        /// Recoge los eventos del reproductor y actualiza las propiedades.
        /// QML lo llama desde un temporizador.
        #[qinvokable]
        fn poll(self: Pin<&mut Chilpa>);
    }
}

use chilpa_bridge::Chilpa;

/// El estado del lado de Rust.
pub struct ChilpaRust {
    outputs: QStringList,
    output_ids: QStringList,
    track: QString,
    format: QString,
    error: QString,
    playing: bool,
    bit_perfect: bool,
    seconds: f64,
    api: chilpa_api::Chilpa,
}

impl Default for ChilpaRust {
    fn default() -> Self {
        Self {
            outputs: QStringList::default(),
            output_ids: QStringList::default(),
            track: QString::from(""),
            format: QString::from(""),
            error: QString::from(""),
            playing: false,
            bit_perfect: false,
            seconds: 0.0,
            api: chilpa_api::Chilpa::new(),
        }
    }
}

impl Chilpa {
    pub fn refresh_outputs(mut self: Pin<&mut Self>) {
        let salidas = self.api.outputs();
        let mut etiquetas = QList::<QString>::default();
        let mut ids = QList::<QString>::default();
        for salida in &salidas {
            etiquetas.append(QString::from(&salida.label));
            ids.append(QString::from(&salida.id));
        }
        self.as_mut().set_outputs(QStringList::from(&etiquetas));
        self.as_mut().set_output_ids(QStringList::from(&ids));
    }

    pub fn play(mut self: Pin<&mut Self>, path: &QString, device: &QString, bit_perfect: bool) {
        let ruta = path.to_string();
        // QML entrega la ruta de un diálogo de archivo como URL.
        let ruta = ruta.strip_prefix("file://").unwrap_or(&ruta).to_string();
        let dispositivo = device.to_string();
        self.as_mut().set_error(QString::from(""));
        self.as_mut().set_track(QString::from(&ruta));
        self.as_mut().set_playing(true);
        self.as_mut()
            .rust_mut()
            .api
            .play_file(std::path::Path::new(&ruta), &dispositivo, bit_perfect);
    }

    pub fn stop(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.stop();
    }

    pub fn poll(mut self: Pin<&mut Self>) {
        let ahora = self.as_mut().rust_mut().api.poll().clone();
        self.as_mut().set_playing(ahora.playing);
        self.as_mut().set_seconds(ahora.seconds);
        self.as_mut().set_bit_perfect(ahora.bit_perfect);
        self.as_mut().set_format(QString::from(&ahora.format));
        self.as_mut().set_error(QString::from(&ahora.error));
        if !ahora.path.is_empty() {
            self.as_mut().set_track(QString::from(&ahora.path));
        }
    }
}
