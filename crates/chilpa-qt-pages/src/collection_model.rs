//! One view owns its broker handle and eight projected chunks. Hidden views close.
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
        #[qproperty(QString, action)]
        #[qproperty(i32, count)]
        #[qproperty(i32, revision)]
        #[qproperty(bool, busy)]
        #[qproperty(bool, active)]
        type NativeCollectionModel = super::CollectionRust;
        #[qinvokable] fn open(self: Pin<&mut NativeCollectionModel>, key: &QString) -> bool;
        #[qinvokable] fn close(self: Pin<&mut NativeCollectionModel>);
        #[qinvokable] fn retire(self: Pin<&mut NativeCollectionModel>);
        #[qinvokable] fn seek(self: Pin<&mut NativeCollectionModel>, first: i32, last: i32);
        #[qinvokable] fn refresh(self: Pin<&mut NativeCollectionModel>, manual: bool);
        #[qinvokable] fn prepare_action(self: Pin<&mut NativeCollectionModel>, token: &QString) -> bool;
        #[qinvokable] fn pin_action(self: Pin<&mut NativeCollectionModel>) -> bool;
        #[qinvokable] fn release_preview(self: Pin<&mut NativeCollectionModel>);
        #[qinvokable] fn release_action(self: Pin<&mut NativeCollectionModel>);
        #[qinvokable] fn poll(self: Pin<&mut NativeCollectionModel>);
        #[qinvokable] fn entry(self: &NativeCollectionModel, index: i32, revision: i32) -> QString;
    }
}
use crate::collection_cache::CollectionCache;
use chilpa_api::NativeCollectionWindow;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QString;
use serde_json::{Value, json};
use std::pin::Pin;

#[derive(Default)]
pub struct CollectionRust {
    document: QString, action: QString, count: i32, revision: i32, busy: bool, active: bool,
    handle: Option<NativeCollectionWindow>, key: String, session: u64,
    generation: u64, data_revision: u64, request_id: u64,
    pending: Option<u64>, pending_action: Option<u64>, lease: u64, preview_lease: u64,
    pages: CollectionCache, window: (usize, usize), has_more: bool, failed: bool,
}
impl qobject::NativeCollectionModel {
    fn next_request(mut self: Pin<&mut Self>) -> u64 {
        let next = self.rust().request_id.wrapping_add(1).max(1);
        self.as_mut().rust_mut().request_id = next;
        next
    }
    pub fn open(mut self: Pin<&mut Self>, key: &QString) -> bool {
        let key = key.to_string();
        if !*self.active() || key.is_empty() || key.len() > 65536 { return false; }
        if self.rust().key == key && self.rust().session != 0 { return true; }
        self.as_mut().close();
        let request = self.as_mut().next_request();
        let mut handle = crate::app::with(|api| api.native_collection_window());
        let session = handle.open(&key, request);
        if session == 0 { return false; }
        self.as_mut().rust_mut().handle = Some(handle);
        self.as_mut().rust_mut().key = key;
        self.as_mut().rust_mut().session = session;
        self.as_mut().rust_mut().pending = Some(request);
        self.set_busy(true);
        true
    }
    pub fn close(mut self: Pin<&mut Self>) {
        self.as_mut().retire();
        self.as_mut().rust_mut().generation = 0;
        self.as_mut().rust_mut().data_revision = 0;
        self.as_mut().rust_mut().pages.clear();
        self.as_mut().rust_mut().window = (0, 0);
        self.as_mut().rust_mut().has_more = false;
        self.as_mut().rust_mut().failed = false;
        self.as_mut().set_count(0);
        self.as_mut().set_document(QString::from("{}"));
        self.as_mut().bump();
    }
    pub fn retire(mut self: Pin<&mut Self>) {
        let session = self.rust().session;
        if let Some(mut handle) = self.as_mut().rust_mut().handle.take() { handle.close(session); }
        self.as_mut().rust_mut().session = 0;
        self.as_mut().rust_mut().pending = None;
        self.as_mut().rust_mut().pending_action = None;
        self.as_mut().rust_mut().lease = 0;
        self.as_mut().rust_mut().preview_lease = 0;
        self.as_mut().set_action(QString::from("{}"));
        self.as_mut().set_busy(false);
    }
    pub fn entry(&self, index: i32, _revision: i32) -> QString {
        if index < 0 { return QString::from("{}"); }
        QString::from(self.rust().pages.get(index as usize).unwrap_or("{}"))
    }
    pub fn seek(mut self: Pin<&mut Self>, first: i32, last: i32) {
        let first = first.max(0) as usize;
        let last = (last.max(first as i32) as usize).min(first.saturating_add(799));
        self.as_mut().rust_mut().window = (first, last);
        self.pump();
    }
    pub fn refresh(mut self: Pin<&mut Self>, manual: bool) {
        if !*self.active() || self.rust().session == 0 || self.rust().pending.is_some() || self.rust().pending_action.is_some() { return; }
        let request = self.as_mut().next_request();
        let accepted = self.rust().handle.as_ref().is_some_and(|handle| handle.refresh(self.rust().session, manual, request));
        if accepted {
            self.as_mut().rust_mut().pending = Some(request);
            self.as_mut().rust_mut().failed = false;
            self.set_busy(true);
        }
    }
    pub fn prepare_action(mut self: Pin<&mut Self>, token: &QString) -> bool {
        let token = token.to_string();
        if !*self.active() || self.rust().session == 0 || token.is_empty() || token.len() > 512 || self.rust().pending_action.is_some() { return false; }
        self.as_mut().release_action();
        let request = self.as_mut().next_request();
        let accepted = self.rust().handle.as_ref().is_some_and(|handle| handle.prepare_action(self.rust().session, &token, request));
        if accepted {
            self.as_mut().rust_mut().pending_action = Some(request);
            self.as_mut().set_busy(true);
        }
        accepted
    }
    pub fn pin_action(mut self: Pin<&mut Self>) -> bool {
        let lease = self.rust().lease;
        if lease == 0 || !self.rust().handle.as_ref().is_some_and(|handle| handle.pin_action(self.rust().session, lease)) { return false; }
        self.as_mut().rust_mut().preview_lease = lease;
        self.as_mut().rust_mut().lease = 0;
        true
    }
    pub fn release_preview(mut self: Pin<&mut Self>) {
        let lease = self.rust().preview_lease;
        if lease != 0 { if let Some(handle) = self.rust().handle.as_ref() { handle.release_action(lease); } }
        self.as_mut().rust_mut().preview_lease = 0;
    }
    pub fn release_action(mut self: Pin<&mut Self>) {
        let lease = self.rust().lease;
        if lease != 0 { if let Some(handle) = self.rust().handle.as_ref() { handle.release_action(lease); } }
        self.as_mut().rust_mut().lease = 0;
        self.set_action(QString::from("{}"));
    }
    pub fn poll(mut self: Pin<&mut Self>) {
        if !*self.active() { return; }
        let value = self.as_mut().rust_mut().handle.as_mut().and_then(|handle| handle.poll());
        if let Some(mut value) = value {
            if value["sessionId"].as_u64() != Some(self.rust().session) { return; }
            let request = value["requestId"].as_u64().unwrap_or(0);
            if self.rust().pending_action == Some(request) {
                self.as_mut().rust_mut().pending_action = None;
                if let Some(action) = value.get("action") {
                    self.as_mut().rust_mut().lease = action["lease"].as_u64().unwrap_or(0);
                    self.as_mut().set_action(QString::from(action.to_string()));
                } else {
                    self.as_mut().set_action(QString::from(json!({"error":value["error"].as_str().unwrap_or(""),"failed":true}).to_string()));
                }
            } else if self.rust().pending == Some(request) {
                self.as_mut().rust_mut().pending = None;
                let generation = value["generation"].as_u64().unwrap_or(0);
                let data = value["dataRevision"].as_u64().unwrap_or(0);
                if (generation, data) != (self.rust().generation, self.rust().data_revision) {
                    self.as_mut().rust_mut().pages.clear();
                    self.as_mut().rust_mut().generation = generation;
                    self.as_mut().rust_mut().data_revision = data;
                }
                if let Some(page) = value.get_mut("page") {
                    let first = page["first"].as_u64().unwrap_or(0) as usize;
                    let rows = page.get_mut("rows").and_then(Value::as_array_mut).map(std::mem::take).unwrap_or_default().into_iter().map(|row| row.to_string()).collect();
                    let window = self.rust().window;
                    self.as_mut().rust_mut().pages.insert(first, rows, window);
                }
                if let Some(object) = value.as_object_mut() { object.remove("page"); }
                self.as_mut().rust_mut().has_more = value["hasMore"].as_bool().unwrap_or(false);
                self.as_mut().rust_mut().failed = value["error"].as_str().is_some_and(|error| !error.is_empty());
                self.as_mut().set_count(value["knownCount"].as_u64().unwrap_or(0).min(i32::MAX as u64) as i32);
                self.as_mut().set_document(QString::from(value.to_string()));
                self.as_mut().bump();
            }
        }
        let busy = self.rust().handle.as_ref().is_some_and(NativeCollectionWindow::busy);
        self.as_mut().set_busy(busy);
        self.pump();
    }
    fn bump(mut self: Pin<&mut Self>) { let next = self.revision().wrapping_add(1); self.as_mut().set_revision(next); }
    fn pump(mut self: Pin<&mut Self>) {
        if !*self.active() || self.rust().session == 0 || self.rust().pending.is_some() || self.rust().pending_action.is_some() || self.rust().failed { return; }
        let count = (*self.count()).max(0) as usize;
        let (first, last) = self.rust().window;
        let missing = (first < count).then(|| self.rust().pages.first_missing(first, last.min(count - 1))).flatten();
        let next = missing.or_else(|| (self.rust().has_more && last.saturating_add(12) >= count).then_some(count));
        let Some(first) = next else { return; };
        let request = self.as_mut().next_request();
        let accepted = self.rust().handle.as_ref().is_some_and(|handle| handle.seek(self.rust().session, first, 100, request));
        if accepted { self.as_mut().rust_mut().pending = Some(request); self.set_busy(true); }
    }
}
