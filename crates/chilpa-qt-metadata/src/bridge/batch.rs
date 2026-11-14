//! A selection owns at most 400 rows and 4 MiB. Only an active, busy view polls.
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
        #[qproperty(QString, document)]
        #[qproperty(QString, kind)]
        #[qproperty(QString, group_key)]
        #[qproperty(i32, count)]
        #[qproperty(i32, revision)]
        #[qproperty(bool, busy)]
        #[qproperty(bool, active)]
        type MetadataBatch = super::BatchRust;
        #[qinvokable] fn open_paths(self: Pin<&mut MetadataBatch>, paths: &QString) -> bool;
        #[qinvokable] fn select(self: Pin<&mut MetadataBatch>, selection: &QString) -> bool;
        #[qinvokable] fn patch(self: Pin<&mut MetadataBatch>, patch: &QString) -> bool;
        #[qinvokable] fn save(self: Pin<&mut MetadataBatch>) -> bool;
        #[qinvokable] fn retry(self: Pin<&mut MetadataBatch>) -> bool;
        #[qinvokable] fn lookup_group(self: Pin<&mut MetadataBatch>, key: &QString) -> bool;
        #[qinvokable] fn candidate(self: Pin<&mut MetadataBatch>, proposal: &QString) -> bool;
        #[qinvokable] fn group_plan(self: Pin<&mut MetadataBatch>, proposal: &QString) -> bool;
        #[qinvokable] fn apply_plan(self: Pin<&mut MetadataBatch>, id: &QString) -> bool;
        #[qinvokable] fn browse(self: Pin<&mut MetadataBatch>, kind: &QString, group_key: &QString);
        #[qinvokable] fn cancel(self: Pin<&mut MetadataBatch>);
        #[qinvokable] fn close(self: Pin<&mut MetadataBatch>);
        #[qinvokable] fn poll(self: Pin<&mut MetadataBatch>);
        #[qinvokable] fn seek(self: Pin<&mut MetadataBatch>, first: i32, last: i32);
        #[qinvokable] fn entry(self: &MetadataBatch, index: i32, revision: i32) -> QString;
    }
    impl cxx_qt::Initialize for MetadataBatch {}
}
use chilpa_qt_kit::app;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QString;
use serde_json::json;
use std::pin::Pin;
use crate::batch_cache::BatchCache;

const PAGE: usize = 100;
const WINDOW: usize = 400;
const JSON_LIMIT: usize = 1024 * 1024;

#[derive(Default)]
pub struct BatchRust {
    document: QString, kind: QString, group_key: QString, count: i32, revision: i32, busy: bool, active: bool,
    session: u64, owned_session: u64, data_revision: u64, request_id: u64,
    pending: Option<(u64, usize, String, String, u64)>,
    pages: BatchCache,
    window: (usize, usize),
    needs_first: bool,
    configuring: bool,
    plan_id: String,
}
impl Drop for BatchRust {
    fn drop(&mut self) {
        if self.owned_session != 0 {
            app::with(|api| {
                if api.metadata_batch_session() == self.owned_session { api.metadata_batch_close(); }
            });
        }
    }
}
impl qobject::MetadataBatch {
    fn owns_current(&self) -> bool { self.rust().owned_session != 0 && app::with(|api| api.metadata_batch_session()) == self.rust().owned_session }
    fn section(&self) -> String {
        match self.kind().to_string().as_str() { "groups" => "groups".into(), "plan" => "plan".into(), _ => "items".into() }
    }
    fn change(mut self: Pin<&mut Self>, action: impl FnOnce(&mut chilpa_api::Chilpa) -> bool) -> bool {
        if !self.owns_current() { return false; }
        let accepted = app::with(action);
        if accepted { self.as_mut().rust_mut().pending = None; }
        self.sync();
        accepted
    }
    pub fn open_paths(mut self: Pin<&mut Self>, paths: &QString) -> bool {
        if paths.to_string().len() > JSON_LIMIT { return false; }
        let paths = paths.to_string();
        let accepted = app::with(|api| api.metadata_batch_open(&paths));
        if accepted {
            self.as_mut().rust_mut().owned_session = app::with(|api| api.metadata_batch_session());
            self.as_mut().rust_mut().pages.clear();
            self.as_mut().rust_mut().pending = None;
            self.as_mut().rust_mut().window = (0,0);
            self.as_mut().rust_mut().needs_first = true;
        }
        self.sync();
        accepted
    }
    pub fn select(self: Pin<&mut Self>, selection: &QString) -> bool {
        let selection = selection.to_string();
        if selection.len() > JSON_LIMIT { return false; }
        self.change(|api| api.metadata_batch_select(&selection))
    }
    pub fn patch(self: Pin<&mut Self>, patch: &QString) -> bool {
        let patch = patch.to_string();
        if patch.len() > JSON_LIMIT { return false; }
        self.change(|api| api.metadata_batch_patch(&patch))
    }
    pub fn save(self: Pin<&mut Self>) -> bool { self.change(|api| api.metadata_batch_save()) }
    pub fn retry(self: Pin<&mut Self>) -> bool { self.change(|api| api.metadata_batch_retry()) }
    pub fn lookup_group(self: Pin<&mut Self>, key: &QString) -> bool {
        let key = key.to_string();
        if key.len() > 512 { return false; }
        self.change(|api| api.metadata_batch_lookup_group(&key))
    }
    pub fn candidate(self: Pin<&mut Self>, proposal: &QString) -> bool {
        let proposal = proposal.to_string();
        if proposal.len() > JSON_LIMIT { return false; }
        self.change(|api| api.metadata_batch_candidate(&proposal))
    }
    pub fn group_plan(self: Pin<&mut Self>, proposal: &QString) -> bool {
        let proposal = proposal.to_string();
        if proposal.len() > JSON_LIMIT { return false; }
        self.change(|api| api.metadata_batch_group_plan(&proposal))
    }
    pub fn apply_plan(self: Pin<&mut Self>, id: &QString) -> bool {
        let id = id.to_string();
        if id.len() > 512 { return false; }
        self.change(|api| api.metadata_batch_apply_plan(&id))
    }
    pub fn browse(mut self: Pin<&mut Self>, kind: &QString, group_key: &QString) {
        if *self.busy() || !self.owns_current() || group_key.to_string().len() > 512 { return; }
        self.as_mut().rust_mut().configuring = true;
        self.as_mut().set_kind(kind.clone());
        self.as_mut().set_group_key(group_key.clone());
        self.as_mut().rust_mut().configuring = false;
        self.invalidate();
    }
    fn invalidate(mut self: Pin<&mut Self>) {
        if self.rust().configuring { return; }
        self.as_mut().rust_mut().pages.clear();
        self.as_mut().rust_mut().pending = None;
        self.as_mut().rust_mut().window = (0,0);
        self.as_mut().rust_mut().needs_first = true;
        self.as_mut().set_count(0);
        self.sync();
    }
    pub fn cancel(mut self: Pin<&mut Self>) { if self.owns_current() { app::with(|api| api.metadata_batch_cancel()); } self.as_mut().rust_mut().pending = None; self.sync(); }
    pub fn close(mut self: Pin<&mut Self>) {
        if self.owns_current() { app::with(|api| api.metadata_batch_close()); }
        self.as_mut().rust_mut().owned_session = 0;
        self.as_mut().rust_mut().session = 0;
        self.as_mut().rust_mut().pages.clear();
        self.as_mut().rust_mut().pending = None;
        self.sync();
    }
    pub fn poll(mut self: Pin<&mut Self>) {
        if !*self.active() || !self.owns_current() { return; }
        if app::with(|api| api.metadata_batch_poll()) { self.as_mut().sync(); }
        self.pump();
    }
    pub fn seek(mut self: Pin<&mut Self>, first: i32, last: i32) {
        let first = first.max(0) as usize;
        let last = last.max(first as i32).max(0) as usize;
        self.as_mut().rust_mut().window = (first, last.min(first.saturating_add(WINDOW - 1)));
        self.pump();
    }
    pub fn entry(&self, index: i32, _revision: i32) -> QString {
        if index < 0 { return QString::from("{}"); }
        let index = index as usize;
        self.rust().pages.get(index).map_or_else(|| QString::from("{}"), QString::from)
    }
    fn sync(mut self: Pin<&mut Self>) {
        let mut view = app::with(|api| api.metadata_batch_view());
        let session = view["session"].as_u64().unwrap_or(0);
        if session != 0 && session != self.rust().owned_session {
            self.as_mut().rust_mut().pages.clear();
            self.as_mut().rust_mut().pending = None;
            self.as_mut().set_count(0);
            self.as_mut().set_busy(false);
            view = json!({"state":"unavailable","busy":false,"dirty":false,"error":view["error"],"beets":view["beets"],
                "counts":{"files":0,"groups":0,"selected":0},"summary":{"changed":0},"fields":[]});
            self.as_mut().set_document(QString::from(view.to_string()));
            return;
        }
        let data = view["dataRevision"].as_u64().unwrap_or(0);
        if (session,data) != (self.rust().session,self.rust().data_revision) {
            self.as_mut().rust_mut().pages.clear();
            self.as_mut().rust_mut().pending = None;
            self.as_mut().rust_mut().session = session;
            self.as_mut().rust_mut().data_revision = data;
        }
        let kind = self.section();
        let plan = view["plan"]["id"].as_str().unwrap_or("").to_string();
        if kind == "plan" && self.rust().plan_id != plan {
            self.as_mut().rust_mut().pages.clear();
            self.as_mut().rust_mut().pending = None;
            self.as_mut().rust_mut().needs_first = !plan.is_empty();
        }
        self.as_mut().rust_mut().plan_id = plan;
        let group = if kind == "items" { self.group_key().to_string() } else { String::new() };
        let total = if kind == "plan" { view["plan"]["counts"]["previewRows"].as_u64() }
            else if !group.is_empty() {
                if view["pageIdentity"]["kind"] == "items" && view["pageIdentity"]["groupKey"].as_str() == Some(group.as_str()) { view["total"].as_u64() }
                else { Some((*self.count()).max(0) as u64) }
            }
            else { view["counts"][if kind == "groups" { "groups" } else { "files" }].as_u64() };
        let total = total.unwrap_or(0).min(i32::MAX as u64) as i32;
        self.as_mut().set_count(total);
        let response = view["requestId"].as_u64().unwrap_or(0);
        if let Some((request,page,section,requested_group,requested_data)) = self.rust().pending.clone() {
            if request == response && section == kind && requested_group == group && requested_data == data
                && view["pageIdentity"]["groupKey"].as_str().unwrap_or("") == group
                && view["pageIdentity"]["kind"] == kind && view["pageIdentity"]["offset"].as_u64() == Some(page as u64) {
                let rows = view["items"].as_array().map(|rows| rows.iter().take(PAGE).cloned().collect::<Vec<_>>()).unwrap_or_default();
                let window = self.rust().window;
                for (index,mut row) in rows.into_iter().enumerate() {
                    // The file preview paints changes, not a second full tag snapshot.
                    if let Some(object) = row.as_object_mut() { object.remove("values"); }
                    self.as_mut().rust_mut().pages.insert(page + index,row.to_string(),window);
                }
                self.as_mut().rust_mut().pending = None;
            }
        }
        if let Some(object) = view.as_object_mut() { object.remove("items"); }
        self.as_mut().set_document(QString::from(view.to_string()));
        self.as_mut().set_busy(view["busy"].as_bool().unwrap_or(false));
        let next = self.revision().wrapping_add(1);
        self.as_mut().set_revision(next);
        self.pump();
    }
    fn pump(mut self: Pin<&mut Self>) {
        if !*self.active() || *self.busy() || self.rust().pending.is_some() || !self.owns_current() { return; }
        let kind = self.section();
        let group = if kind == "items" { self.group_key().to_string() } else { String::new() };
        let (first,last) = self.rust().window;
        if *self.count() == 0 && !self.rust().needs_first { return; }
        // A filter's count arrives with its first page rather than the old document.
        let last = if *self.count() == 0 { first } else { last.min(*self.count() as usize - 1) };
        let page = if self.rust().needs_first { Some(0) } else { self.rust().pages.first_missing(first,last) };
        let Some(page) = page else { return; };
        let request = self.rust().request_id.wrapping_add(1).max(1);
        self.as_mut().rust_mut().request_id = request;
        let limit = if self.rust().needs_first { PAGE } else { PAGE.min(last - page + 1) };
        let accepted = app::with(|api| if group.is_empty() {
            api.metadata_batch_page(&kind,page,limit,request)
        } else { api.metadata_batch_group_page(&group,page,limit,request) });
        if accepted {
            let data = self.rust().data_revision;
            self.as_mut().rust_mut().pending = Some((request,page,kind,group,data));
            self.as_mut().rust_mut().needs_first = false;
            self.set_busy(true);
        }
    }
}
impl cxx_qt::Initialize for qobject::MetadataBatch {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_kind_changed(|model| model.invalidate()).release();
        self.as_mut().on_group_key_changed(|model| model.invalidate()).release();
        self.on_active_changed(|model| model.sync()).release();
    }
}
