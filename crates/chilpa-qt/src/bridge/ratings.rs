//! I publish one rating snapshot to every surface, without resetting track lists.

#[cxx_qt::bridge]
pub mod qobject {
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, rev)]
        type Ratings = super::RatingsRust;
        #[qinvokable]
        fn track(self: &Ratings, id: i64, rev: i32) -> i32;
        #[qinvokable]
        fn album(self: &Ratings, id: i64, rev: i32) -> i32;
        #[qinvokable]
        fn rate_track(self: Pin<&mut Ratings>, id: i64, stars: i32);
        #[qinvokable]
        fn rate_album(self: Pin<&mut Ratings>, id: i64, stars: i32);
        #[qinvokable]
        fn refresh(self: Pin<&mut Ratings>);
    }
    impl cxx_qt::Initialize for Ratings {}
}

use crate::app;
use chilpa_api::RatingTarget;
use cxx_qt::CxxQtType as _;
use std::pin::Pin;

#[derive(Default)]
pub struct RatingsRust {
    rev: i32,
    values: chilpa_api::Ratings,
}

impl qobject::Ratings {
    pub fn track(&self, id: i64, _rev: i32) -> i32 {
        self.rust().values.get(RatingTarget::Track(id)).into()
    }
    pub fn album(&self, id: i64, _rev: i32) -> i32 {
        self.rust().values.get(RatingTarget::Album(id)).into()
    }
    pub fn rate_track(self: Pin<&mut Self>, id: i64, stars: i32) {
        if app::with(|api| api.rate(RatingTarget::Track(id), stars)) { self.refresh(); }
    }
    pub fn rate_album(self: Pin<&mut Self>, id: i64, stars: i32) {
        if app::with(|api| api.rate(RatingTarget::Album(id), stars)) { self.refresh(); }
    }
    pub fn refresh(mut self: Pin<&mut Self>) {
        if let Some(values) = app::with(|api| api.ratings()) {
            self.as_mut().rust_mut().values = values;
            let next = self.rev().wrapping_add(1);
            self.set_rev(next);
        }
    }
}

impl cxx_qt::Initialize for qobject::Ratings {
    fn initialize(self: Pin<&mut Self>) { self.refresh(); }
}
