//! The QObjects QML sees.
//!
//! One file per object, all in this directory: cxx-qt reads every file that
//! carries a `#[cxx_qt::bridge]` and only accepts one directory for them.
//! No file here decides anything; each property and each invokable is a
//! call into the facade through [`crate::app`].

pub mod album;
pub mod albums;
pub mod dev;
pub mod facets;
pub mod i18n;
pub mod library;
pub mod navigation;
pub mod player;
pub mod settings;
pub mod theme;
pub mod tracks;
