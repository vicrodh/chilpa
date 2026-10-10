//! "Open music link": the modal that takes a link and opens what it names,
//! in a source, in the library, or in the playlist importer. The work is
//! the facade's (`chilpa_api` `open_link`); this crate is the surface.

pub mod bridge {
    pub mod link;
}
