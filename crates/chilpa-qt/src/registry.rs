//! How one QObject asks another for something.
//!
//! The singletons register the handle to their thread when they start. A
//! model that needs one of them -- the settings page changing the theme, a
//! finished scan telling the library to read itself again -- queues a
//! closure on that handle, and the closure runs on the window's thread once
//! the current call is over. No object holds a pointer to another, and
//! nothing runs inside another object's method.

use crate::bridge::{album_actions, library, navigation, player, playlists, settings};
use cxx_qt::CxxQtThread;
use std::cell::RefCell;

thread_local! {
    static SETTINGS: RefCell<Option<CxxQtThread<settings::qobject::SettingsModel>>> = const { RefCell::new(None) };
    static LIBRARY: RefCell<Option<CxxQtThread<library::qobject::Library>>> = const { RefCell::new(None) };
    static NAVIGATION: RefCell<Option<CxxQtThread<navigation::qobject::Navigation>>> = const { RefCell::new(None) };
    static PLAYLISTS: RefCell<Option<CxxQtThread<playlists::qobject::Playlists>>> = const { RefCell::new(None) };
    static ALBUM_ACTIONS: RefCell<Option<CxxQtThread<album_actions::qobject::AlbumActions>>> = const { RefCell::new(None) };
    static PLAYER: RefCell<Option<CxxQtThread<player::qobject::Player>>> = const { RefCell::new(None) };
}

macro_rules! handle {
    ($register:ident, $ask:ident, $slot:ident, $ty:ty) => {
        pub fn $register(thread: CxxQtThread<$ty>) {
            $slot.with(|s| *s.borrow_mut() = Some(thread));
        }

        /// Queues `f` on that object. Dropped if it has not started, which
        /// only happens before the window is up.
        pub fn $ask(f: impl FnOnce(std::pin::Pin<&mut $ty>) + Send + 'static) {
            $slot.with(|s| {
                if let Some(thread) = s.borrow().as_ref() {
                    let _ = thread.queue(f);
                }
            });
        }
    };
}

handle!(register_settings, settings, SETTINGS, settings::qobject::SettingsModel);
handle!(register_library, library, LIBRARY, library::qobject::Library);
handle!(register_navigation, navigation, NAVIGATION, navigation::qobject::Navigation);
handle!(register_playlists, playlists, PLAYLISTS, playlists::qobject::Playlists);
handle!(register_player, player, PLAYER, player::qobject::Player);
handle!(register_album_actions, album_actions, ALBUM_ACTIONS, album_actions::qobject::AlbumActions);
