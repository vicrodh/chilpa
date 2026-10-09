//! The keyboard shortcuts: which action a key is, and the sheet and the
//! editor as the facade writes them.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, snapshot)]
        type Hotkeys = super::HotkeysRust;
        #[qinvokable]
        fn refresh(self: Pin<&mut Hotkeys>);
        /// The action of a key as Qt names it: empty when it is nobody's,
        /// "-" when it was taken and there is nothing to run.
        #[qinvokable]
        fn pressed(self: Pin<&mut Hotkeys>, key: i32, modifiers: i32, text: &QString, typing: bool) -> QString;
        /// The same, for a key pressed in a named scope: "window", "mini"
        /// or "immersive" (the overlay answers the window's keys too).
        #[qinvokable]
        fn pressed_in(self: Pin<&mut Hotkeys>, scope: &QString, key: i32, modifiers: i32, text: &QString, typing: bool) -> QString;
        #[qinvokable]
        fn command(self: Pin<&mut Hotkeys>, action: &QString, argument: &QString);
        /// Marks what plays as a favorite, or takes the mark off.
        #[qinvokable]
        fn like_playing(self: Pin<&mut Hotkeys>);
        /// The search box is asked to take the keyboard.
        #[qsignal]
        fn search_asked(self: Pin<&mut Hotkeys>);
    }
}
use chilpa_api::{Pressed, Scope};
use chilpa_qt_kit::app;
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct HotkeysRust {
    snapshot: QString,
}

impl qobject::Hotkeys {
    pub fn refresh(self: Pin<&mut Self>) {
        let snapshot = app::with(|api| api.keys_json());
        self.set_snapshot(QString::from(&snapshot));
    }

    pub fn pressed(self: Pin<&mut Self>, key: i32, modifiers: i32, text: &QString, typing: bool) -> QString {
        self.pressed_in(&QString::from("window"), key, modifiers, text, typing)
    }

    pub fn pressed_in(mut self: Pin<&mut Self>, scope: &QString, key: i32, modifiers: i32, text: &QString, typing: bool) -> QString {
        let scope = Scope::named(&scope.to_string());
        match app::with(|api| api.key_pressed_in(scope, key, modifiers, &text.to_string(), typing)) {
            Pressed::Pass => QString::default(),
            Pressed::Taken => {
                self.as_mut().refresh();
                QString::from("-")
            }
            Pressed::Run(action) => QString::from(action),
        }
    }

    pub fn command(mut self: Pin<&mut Self>, action: &QString, argument: &QString) {
        app::with(|api| api.keys_command(&action.to_string(), &argument.to_string()));
        self.as_mut().refresh();
    }

    pub fn like_playing(self: Pin<&mut Self>) {
        app::with(|api| api.like_playing());
    }
}
