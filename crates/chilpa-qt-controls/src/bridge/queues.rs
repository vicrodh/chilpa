//! I combine listening queues through the facade, apart from the window bridge.
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
        type Queues = super::QueuesRust;
        #[qinvokable]
        fn merge(self: &Queues, ids: &QString, name: &QString, keep: bool) -> QString;
    }
}
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct QueuesRust;

impl qobject::Queues {
    pub fn merge(&self, ids: &QString, name: &QString, keep: bool) -> QString {
        let parsed = serde_json::from_str::<Vec<String>>(&ids.to_string()).ok()
            .and_then(|ids| ids.iter().map(|id| id.parse::<u64>().ok()).collect::<Option<Vec<_>>>());
        let Some(ids) = parsed else { return QString::from(chilpa_qt_kit::app::t("Choose at least two different queues.")); };
        let result = chilpa_qt_kit::app::with(|api| api.listen_merge(&ids, &name.to_string(), keep));
        QString::from(result.err().unwrap_or_default())
    }
}
