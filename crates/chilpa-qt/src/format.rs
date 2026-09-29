pub use chilpa_api::format::{clock, span};

/// A list of strings as Qt takes it.
pub fn strings(items: impl Iterator<Item = String>) -> cxx_qt_lib::QStringList {
    let mut list = cxx_qt_lib::QList::<cxx_qt_lib::QString>::default();
    for item in items {
        list.append(cxx_qt_lib::QString::from(&item));
    }
    cxx_qt_lib::QStringList::from(&list)
}
