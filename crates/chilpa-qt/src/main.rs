//! El shell de escritorio. Hoy solo levanta la fachada y se sale; el
//! frontend de Qt entra cuando cierre el spike de Widgets contra Quick.
//!
//! Este crate es el ÚNICO que puede tener tipos de Qt (ADR-001).

use chilpa_api::{Chilpa, Command, Event};

fn main() {
    let mut chilpa = Chilpa::new();
    match chilpa.dispatch(Command::ListSources) {
        Event::Sources(sources) => {
            for source in sources {
                println!("{source}");
            }
        }
        other => eprintln!("respuesta inesperada: {other:?}"),
    }
}
