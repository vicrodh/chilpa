//! El shell de escritorio. Hoy solo levanta la fachada y se sale; el
//! frontend de Qt entra cuando cierre el spike de Widgets contra Quick.
//!
//! Este crate es el ÚNICO que puede tener tipos de Qt (ADR-001).

use chilpa_api::{Chilpa, Command, Event};
use std::path::PathBuf;

fn main() {
    let mut chilpa = Chilpa::new();
    let mut args = std::env::args().skip(1);
    if let Some(path) = args.next() {
        // Ruta mínima mientras no hay UI: chilpa-qt <archivo> [hw:0,0]
        let device = args.next().unwrap_or_else(|| "hw:0,0".to_string());
        let event = chilpa.dispatch(Command::PlayFile {
            path: PathBuf::from(path),
            device,
            bit_perfect: true,
        });
        match event {
            Event::Played { frames, quality, bit_perfect } => println!(
                "{frames} frames a {} Hz, {} bits, {} canales; bit-perfect: {}",
                quality.sample_rate_hz,
                quality.bit_depth.map_or("-".to_string(), |b| b.to_string()),
                quality.channels,
                if bit_perfect { "sí" } else { "no" }
            ),
            Event::Failed { message } => {
                eprintln!("{message}");
                std::process::exit(1);
            }
            other => eprintln!("respuesta inesperada: {other:?}"),
        }
        return;
    }
    match chilpa.dispatch(Command::ListSources) {
        Event::Sources(sources) => {
            for source in sources {
                println!("{source}");
            }
        }
        other => eprintln!("respuesta inesperada: {other:?}"),
    }
}
