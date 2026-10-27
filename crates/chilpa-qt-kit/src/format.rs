use cxx_qt_lib::{QString, QUrl};

/// I share empty and local artwork URL handling across album and track models.
pub fn art_url(path: &str) -> QUrl {
    if path.is_empty() { QUrl::default() } else { QUrl::from_local_file(&QString::from(path)) }
}

pub use chilpa_api::format::{clock, span};

/// I return a generated artwork path through the shared C++ image-provider buffer.
///
/// # Safety
/// `out` must point to writable storage of at least `capacity` bytes.
pub unsafe fn write_path(made: Option<std::path::PathBuf>, out: *mut std::ffi::c_char, capacity: std::ffi::c_ulong) -> bool {
    let Some(made) = made else { return false };
    let bytes = made.to_string_lossy().into_owned().into_bytes();
    if bytes.len() + 1 > capacity as usize {
        return false;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast::<u8>(), bytes.len());
        *out.add(bytes.len()) = 0;
    }
    true
}

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
