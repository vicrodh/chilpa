//! A view owns one article document and at most eight image demands.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qurl.h");
        type QString = cxx_qt_lib::QString;
        type QUrl = cxx_qt_lib::QUrl;
    }
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, document)]
        #[qproperty(QString, error)]
        #[qproperty(bool, busy)]
        #[qproperty(bool, active)]
        #[qproperty(i32, image_revision)]
        type NativeArticleModel = super::ArticleRust;
        #[qinvokable]
        fn open(self: Pin<&mut NativeArticleModel>, key: &QString) -> bool;
        #[qinvokable]
        fn close(self: Pin<&mut NativeArticleModel>);
        #[qinvokable]
        fn retire(self: Pin<&mut NativeArticleModel>);
        #[qinvokable]
        fn poll(self: Pin<&mut NativeArticleModel>);
        #[qinvokable]
        fn watch_image(self: Pin<&mut NativeArticleModel>, url: &QString) -> u64;
        #[qinvokable]
        fn release_image(self: Pin<&mut NativeArticleModel>, ticket: u64);
        #[qinvokable]
        fn image(self: &NativeArticleModel, url: &QString, revision: i32) -> QUrl;
    }
}

use chilpa_api::NativeArticleWindow;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QString, QUrl};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
};

#[derive(Default)]
pub struct ArticleRust {
    document: QString,
    error: QString,
    busy: bool,
    active: bool,
    image_revision: i32,
    handle: Option<NativeArticleWindow>,
    key: String,
    session: u64,
    serial: u64,
    generation: u64,
    pending_document: Option<u64>,
    pending_images: HashMap<u64, String>,
    allowed_images: HashSet<String>,
    leases: HashMap<u64, String>,
    images: HashMap<String, String>,
}

impl qobject::NativeArticleModel {
    fn next_id(mut self: Pin<&mut Self>) -> u64 {
        let id = self
            .rust()
            .serial
            .checked_add(1)
            .expect("article identifiers exhausted");
        self.as_mut().rust_mut().serial = id;
        id
    }
    pub fn open(mut self: Pin<&mut Self>, key: &QString) -> bool {
        let key = key.to_string();
        if !*self.active() || key.is_empty() || key.len() > 32768 {
            return false;
        }
        if self.rust().key == key && self.rust().session != 0 && self.error().is_empty() {
            return true;
        }
        self.as_mut().close();
        let request = self.as_mut().next_id();
        let mut handle = crate::app::with(|api| api.native_article_window());
        let session = handle.open(&key, request);
        if session == 0 {
            return false;
        }
        self.as_mut().rust_mut().handle = Some(handle);
        self.as_mut().rust_mut().session = session;
        self.as_mut().rust_mut().key = key;
        self.as_mut().rust_mut().pending_document = Some(request);
        self.set_busy(true);
        true
    }
    pub fn retire(mut self: Pin<&mut Self>) {
        let session = self.rust().session;
        if let Some(mut handle) = self.as_mut().rust_mut().handle.take() {
            handle.close(session);
        }
        self.as_mut().rust_mut().session = 0;
        self.as_mut().rust_mut().pending_document = None;
        self.as_mut().rust_mut().pending_images.clear();
        self.as_mut().rust_mut().leases.clear();
        self.set_busy(false);
    }
    pub fn close(mut self: Pin<&mut Self>) {
        self.as_mut().retire();
        self.as_mut().rust_mut().key.clear();
        self.as_mut().rust_mut().generation = 0;
        self.as_mut().rust_mut().allowed_images.clear();
        self.as_mut().rust_mut().images.clear();
        self.as_mut().set_document(QString::from("{}"));
        self.as_mut().set_error(QString::default());
        self.bump_images();
    }
    pub fn watch_image(mut self: Pin<&mut Self>, url: &QString) -> u64 {
        let url = url.to_string();
        if !*self.active()
            || self.rust().session == 0
            || self.rust().leases.len() >= 8
            || !self.rust().allowed_images.contains(&url)
        {
            return 0;
        }
        let ticket = self.as_mut().next_id();
        self.as_mut().rust_mut().leases.insert(ticket, url);
        self.pump_images();
        ticket
    }
    pub fn release_image(mut self: Pin<&mut Self>, ticket: u64) {
        let Some(url) = self.as_mut().rust_mut().leases.remove(&ticket) else {
            return;
        };
        if !self.rust().leases.values().any(|value| value == &url) {
            if let Some(handle) = self.rust().handle.as_ref() {
                handle.release_image(self.rust().session, &url);
            }
            self.as_mut()
                .rust_mut()
                .pending_images
                .retain(|_, value| value != &url);
        }
        self.update_busy();
    }
    pub fn image(&self, url: &QString, _revision: i32) -> QUrl {
        self.rust()
            .images
            .get(&url.to_string())
            .filter(|path| !path.is_empty())
            .map(|path| QUrl::from_local_file(&QString::from(path.as_str())))
            .unwrap_or_default()
    }
    pub fn poll(mut self: Pin<&mut Self>) {
        if !*self.active() {
            return;
        }
        let value = self
            .as_mut()
            .rust_mut()
            .handle
            .as_mut()
            .and_then(NativeArticleWindow::poll);
        if let Some(mut value) = value {
            if value["sessionId"].as_u64() != Some(self.rust().session) {
                return;
            }
            let request = value["requestId"].as_u64().unwrap_or(0);
            if value["kind"] == "article" && request == 0 && value["generation"] == 0 {
                let error = value["error"].as_str().unwrap_or_default().to_owned();
                self.as_mut().close();
                self.set_error(QString::from(error));
                return;
            }
            if value["kind"] == "article" && self.rust().pending_document == Some(request) {
                self.as_mut().rust_mut().pending_document = None;
                self.as_mut().rust_mut().generation = value["generation"].as_u64().unwrap_or(0);
                self.as_mut()
                    .set_error(QString::from(value["error"].as_str().unwrap_or_default()));
                let document = value
                    .get_mut("document")
                    .map(Value::take)
                    .unwrap_or(Value::Null);
                if document.is_object() {
                    let article = &document["article"];
                    let mut images = HashSet::new();
                    if let Some(url) = article["image"].as_str() {
                        images.insert(url.to_owned());
                    }
                    for block in article["blocks"].as_array().into_iter().flatten() {
                        if block["type"] == "image" {
                            if let Some(url) = block["url"].as_str() {
                                images.insert(url.to_owned());
                            }
                        }
                    }
                    self.as_mut().rust_mut().allowed_images = images;
                    self.as_mut()
                        .set_document(QString::from(document.to_string()));
                }
            } else if value["kind"] == "image" {
                if let Some(url) = self.as_mut().rust_mut().pending_images.remove(&request) {
                    if value["generation"].as_u64() == Some(self.rust().generation)
                        && value["url"] == url
                    {
                        self.as_mut()
                            .rust_mut()
                            .images
                            .insert(url, value["path"].as_str().unwrap_or_default().to_owned());
                        self.as_mut().bump_images();
                    }
                }
            }
        }
        self.as_mut().pump_images();
        self.update_busy();
    }
    fn pump_images(mut self: Pin<&mut Self>) {
        if !*self.active() || self.rust().session == 0 {
            return;
        }
        let mut wanted: Vec<_> = self
            .rust()
            .leases
            .iter()
            .map(|(ticket, url)| (*ticket, url.clone()))
            .collect();
        wanted.sort_by_key(|(ticket, _)| *ticket);
        for (_, url) in wanted {
            if self.rust().images.contains_key(&url)
                || self
                    .rust()
                    .pending_images
                    .values()
                    .any(|value| value == &url)
            {
                continue;
            }
            let request = self.as_mut().next_id();
            let accepted = self
                .rust()
                .handle
                .as_ref()
                .is_some_and(|handle| handle.request_image(self.rust().session, &url, request));
            if !accepted {
                break;
            }
            self.as_mut().rust_mut().pending_images.insert(request, url);
        }
        self.update_busy();
    }
    fn update_busy(mut self: Pin<&mut Self>) {
        let busy = self.rust().pending_document.is_some()
            || !self.rust().pending_images.is_empty()
            || self
                .rust()
                .handle
                .as_ref()
                .is_some_and(NativeArticleWindow::busy);
        if *self.busy() != busy {
            self.set_busy(busy);
        }
    }
    fn bump_images(mut self: Pin<&mut Self>) {
        let revision = self.image_revision().wrapping_add(1);
        self.as_mut().set_image_revision(revision);
    }
}
