//! The Library's stack of conditions beyond its columns, as chips: what each
//! condition says, and the edits a click makes on them. The query is the
//! facade's (`chilpa_api::library_query`); this crate is the surface.

mod history;

pub mod bridge {
    pub mod chips;
    pub mod saved;
}
