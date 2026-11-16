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
        type NativePages = super::NativePagesRust;
        #[qinvokable]
        fn presentation(self: &NativePages, key: &QString, rev: i32) -> QString;
        #[qinvokable]
        fn cover_override(self: &NativePages, id: i64, rev: i32) -> QString;
        #[qinvokable]
        fn reference_cover_override(self: &NativePages, plugin: &QString, reference: &QString, rev: i32) -> QString;
        #[qinvokable]
        fn request_key(self: &NativePages, key: &QString, query: &QString) -> QString;
        #[qinvokable]
        fn same_navigation(
            self: &NativePages,
            key: &QString,
            navigation: &QString,
            rev: i32,
        ) -> bool;
        #[qinvokable]
        fn revalidate(self: &NativePages, key: &QString, manual: bool);
    }
}
use cxx_qt_lib::QString;
#[derive(Default)]
pub struct NativePagesRust;
impl qobject::NativePages {
    pub fn presentation(&self, key: &QString, _rev: i32) -> QString {
        QString::from(crate::app::with(|api| api.source_page_presentation(&key.to_string())))
    }
    pub fn cover_override(&self, id: i64, _rev: i32) -> QString {
        QString::from(crate::app::with(|api| api.source_cover_override(id)))
    }
    pub fn reference_cover_override(&self, plugin: &QString, reference: &QString, _rev: i32) -> QString {
        QString::from(crate::app::with(|api| api.source_reference_cover_override(&plugin.to_string(), &reference.to_string())))
    }
    pub fn request_key(&self, key: &QString, query: &QString) -> QString {
        QString::from(crate::app::with(|api| {
            api.source_page_request_key(&key.to_string(), &query.to_string())
        }))
    }
    pub fn same_navigation(&self, key: &QString, navigation: &QString, _rev: i32) -> bool {
        crate::app::with(|api| {
            api.source_page_same_navigation(&key.to_string(), &navigation.to_string())
        })
    }
    pub fn revalidate(&self, key: &QString, manual: bool) {
        crate::app::with(|api| api.source_page_revalidate(&key.to_string(), manual));
    }
}
