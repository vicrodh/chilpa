//! I publish one snapshot of stars and favorites to every surface, without resetting track lists.

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
        #[qproperty(i32, rev)]
        type Ratings = super::RatingsRust;
        #[qinvokable]
        fn stars(self: &Ratings, kind: &QString, id: i64, rev: i32) -> i32;
        #[qinvokable]
        fn rate(self: Pin<&mut Ratings>, kind: &QString, id: i64, stars: i32);
        #[qinvokable]
        fn favorite(self: &Ratings, kind: &QString, key: &QString, rev: i32) -> bool;
        #[qinvokable]
        fn set_favorite(self: Pin<&mut Ratings>, kind: &QString, key: &QString, on: bool);
        #[qinvokable]
        fn refresh(self: Pin<&mut Ratings>);
    }
    impl cxx_qt::Initialize for Ratings {}
}

use crate::app;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct RatingsRust {
    rev: i32,
    values: chilpa_api::Marks,
}

impl qobject::Ratings {
    pub fn stars(&self, kind: &QString, id: i64, _rev: i32) -> i32 {
        self.rust().values.stars(&kind.to_string(), id)
    }
    pub fn rate(self: Pin<&mut Self>, kind: &QString, id: i64, stars: i32) {
        if app::with(|api| api.rate_as(&kind.to_string(), id, stars)) { self.refresh(); }
    }
    pub fn favorite(&self, kind: &QString, key: &QString, _rev: i32) -> bool {
        self.rust().values.favorite(&kind.to_string(), &key.to_string())
    }
    pub fn set_favorite(self: Pin<&mut Self>, kind: &QString, key: &QString, on: bool) {
        if app::with(|api| api.set_favorite(&kind.to_string(), &key.to_string(), on)) { self.refresh(); }
    }
    pub fn refresh(mut self: Pin<&mut Self>) {
        if let Some(values) = app::with(|api| api.marks()) {
            self.as_mut().rust_mut().values = values;
            let next = self.rev().wrapping_add(1);
            self.set_rev(next);
        }
    }
}

impl cxx_qt::Initialize for qobject::Ratings {
    fn initialize(self: Pin<&mut Self>) { self.refresh(); }
}
