pub use chilpa_api::format::{clock, span};

/// A list of strings as Qt takes it.
pub fn strings(items: impl Iterator<Item = String>) -> cxx_qt_lib::QStringList {
    let mut list = cxx_qt_lib::QList::<cxx_qt_lib::QString>::default();
    for item in items {
        list.append(cxx_qt_lib::QString::from(&item));
    }
    cxx_qt_lib::QStringList::from(&list)
}

/// The consecutive model roles shared by the list bridges.
pub fn roles(first: i32, names: &[&str]) -> cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray> {
    let mut roles = cxx_qt_lib::QHash::default();
    for (offset, name) in names.iter().enumerate() {
        roles.insert(first + offset as i32, cxx_qt_lib::QByteArray::from(*name));
    }
    roles
}
