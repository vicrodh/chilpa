//! The rows of Settings > Library folders: each folder, when it was last
//! scanned and whether it is there now. It rereads them from
//! `LibraryFolders` whenever its `rev` moves.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        /// Bound to `LibraryFolders.rev`.
        #[qproperty(i32, rev)]
        type FolderListModel = super::FolderListRust;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut FolderListModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut FolderListModel>);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &FolderListModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &FolderListModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &FolderListModel, parent: &QModelIndex) -> i32;
    }

    impl cxx_qt::Initialize for FolderListModel {}
}

use chilpa_api::LibraryFolder;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QDateTime, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QTimeZone, QVariant};
use std::pin::Pin;

const ROLES: &[&str] = &["path", "name", "lastScan", "reachable"];
const USER_ROLE: i32 = 256;

#[derive(Default)]
pub struct FolderListRust {
    rev: i32,
    folders: Vec<LibraryFolder>,
}

impl qobject::FolderListModel {
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(f) = self.rust().folders.get(index.row().max(0) as usize) else { return QVariant::default() };
        match role - USER_ROLE {
            0 => QVariant::from(&QString::from(&f.path)),
            1 => {
                let name = std::path::Path::new(&f.path).file_name().map(|n| n.to_string_lossy().into_owned());
                QVariant::from(&QString::from(&name.unwrap_or_else(|| f.path.clone())))
            }
            2 => match f.last_scan {
                Some(secs) => QVariant::from(&QDateTime::from_secs_since_epoch(secs, &QTimeZone::system_time_zone())),
                None => QVariant::default(),
            },
            3 => QVariant::from(&f.reachable),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        crate::format::roles(USER_ROLE, ROLES)
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().folders.len() as i32
    }

    fn reload(mut self: Pin<&mut Self>) {
        let folders = super::folders::read(|f| f.to_vec());
        // SAFETY: the reset is begun and ended around the swap, as Qt asks.
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().folders = folders;
        unsafe { self.as_mut().end_reset_model() };
    }
}

impl cxx_qt::Initialize for qobject::FolderListModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_rev_changed(|m| m.reload()).release();
        self.reload();
    }
}
