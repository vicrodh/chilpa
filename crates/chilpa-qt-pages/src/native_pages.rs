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
        fn cover_override(self: &NativePages, id: i64, rev: i32) -> QString;
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
        #[qinvokable]
        fn web_allowed(
            self: &NativePages,
            plugin: &QString,
            initial: &QString,
            address: &QString,
            rev: i32,
        ) -> bool;
    }
}
use cxx_qt_lib::QString;
#[derive(Default)]
pub struct NativePagesRust;
impl qobject::NativePages {
    pub fn cover_override(&self, id: i64, _rev: i32) -> QString {
        QString::from(crate::app::with(|api| api.source_cover_override(id)))
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
    pub fn web_allowed(
        &self,
        plugin: &QString,
        initial: &QString,
        address: &QString,
        _rev: i32,
    ) -> bool {
        crate::app::with(|api| {
            api.source_web_allowed(
                &plugin.to_string(),
                &initial.to_string(),
                &address.to_string(),
            )
        })
    }
}
