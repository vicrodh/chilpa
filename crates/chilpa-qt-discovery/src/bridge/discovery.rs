//! One running query, two viewport requests and eight 60-entry pages per view.
//! Hidden views release their pages and cancel queued publication.
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
        #[qproperty(bool, loading)]
        #[qproperty(i32, count)]
        #[qproperty(i32, revision)]
        type DiscoveryModel = super::DiscoveryRust;
        #[qinvokable]
        fn request(self: Pin<&mut DiscoveryModel>, kind: &QString, key: &QString, text: &QString, options: &QString, revision: i32);
        #[qinvokable]
        fn seek(self: Pin<&mut DiscoveryModel>, first: i32, last: i32);
        #[qinvokable]
        fn entry(self: &DiscoveryModel, index: i32, revision: i32) -> QString;
        #[qinvokable]
        fn cancel(self: Pin<&mut DiscoveryModel>);
    }
    impl cxx_qt::Threading for DiscoveryModel {}
}
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QString, QUrl};
use std::pin::Pin;
use std::collections::VecDeque;

const PAGE: usize = 60;
#[derive(Clone, PartialEq)]
struct Request { kind: String, key: String, text: String, options: String, revision: i32 }
impl Request {
    fn same_view(&self, other: &Self) -> bool { (&self.kind,&self.key,&self.text,&self.options) == (&other.kind,&other.key,&other.text,&other.options) }
}
#[derive(Default)]
pub struct DiscoveryRust {
    document: QString,
    loading: bool,
    count: i32,
    revision: i32,
    running: bool,
    pending: VecDeque<usize>,
    active: Option<usize>,
    query: Option<Request>,
    pages: VecDeque<(usize, Vec<String>)>,
    window: (usize, usize),
    refreshing: bool,
    generation: chilpa_qt_kit::album_pages::Generation,
}
impl qobject::DiscoveryModel {
    pub fn cancel(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending.clear();
        self.as_mut().rust_mut().pages.clear();
        self.as_mut().rust_mut().query = None;
        self.as_mut().rust_mut().refreshing = false;
        self.rust().generation.next();
        self.as_mut().set_loading(false);
        self.as_mut().set_count(0);
        self.as_mut().set_document(QString::from("{}"));
    }
    pub fn request(mut self: Pin<&mut Self>, kind: &QString, key: &QString, text: &QString, options: &QString, revision: i32) {
        if key.to_string().len() > 8192 || text.to_string().len() > 1024 || options.to_string().len() > 512 { return; }
        let request = Request { kind:kind.to_string(), key:key.to_string(), text:text.to_string(), options:options.to_string(), revision };
        if self.rust().query.as_ref() == Some(&request) { return; }
        let same = self.rust().query.as_ref().is_some_and(|previous| previous.same_view(&request));
        self.rust().generation.next();
        self.as_mut().rust_mut().query = Some(request);
        self.as_mut().rust_mut().refreshing = same;
        self.as_mut().rust_mut().pending = VecDeque::from([0]);
        if !same {
            self.as_mut().rust_mut().pages.clear();
            self.as_mut().rust_mut().window = (0,0);
            self.as_mut().set_document(QString::from("{}"));
            self.as_mut().set_count(0);
            self.as_mut().set_loading(true);
        }
        self.start();
    }
    pub fn entry(&self, index: i32, _revision: i32) -> QString {
        if index < 0 { return QString::from("{}"); }
        let index = index as usize;
        self.rust().pages.iter().find(|(page, _)| *page == index / PAGE)
            .and_then(|(_, rows)| rows.get(index % PAGE)).map_or_else(|| QString::from("{}"), |row| QString::from(row.as_str()))
    }
    pub fn seek(mut self: Pin<&mut Self>, first: i32, last: i32) {
        if self.rust().query.is_none() || *self.count() == 0 { return; }
        let first = first.max(0) as usize / PAGE;
        let last = (last.max(0).min(*self.count() - 1) as usize / PAGE).min(first + 7);
        self.as_mut().rust_mut().window = (first,last);
        if self.rust().refreshing { self.start(); return; }
        self.refill();
    }
    fn refill(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending.clear();
        let (first,last) = self.rust().window;
        for page in first..=last {
            if self.rust().active != Some(page) && !self.rust().pages.iter().any(|(held, _)| *held == page) {
                self.as_mut().rust_mut().pending.push_back(page);
                if self.rust().pending.len() == 2 { break; }
            }
        }
        self.start();
    }
    fn start(mut self: Pin<&mut Self>) {
        if self.rust().running { return; }
        let Some(request) = self.rust().query.clone() else { return; };
        let Some(page) = self.as_mut().rust_mut().pending.pop_front() else { return; };
        self.as_mut().rust_mut().running = true;
        self.as_mut().rust_mut().active = Some(page);
        let ticket = self.rust().generation.next();
        let thread = self.qt_thread();
        chilpa_qt_kit::worker::submit_checked(move |reader| {
            let document = ticket.current().then(|| match reader {
                Ok(reader) => {
                    let mut value = reader.discovery_page_options(&request.kind, &request.key, &request.text, &request.options, page);
                    if let Some(items) = value["items"].as_array_mut() { for item in items { image_urls(item); } }
                    image_urls(&mut value["summary"]);
                    value
                }
                Err(error) => serde_json::json!({"error":error,"items":[],"total":0}),
            });
            let _ = thread.queue(move |mut model| {
                model.as_mut().rust_mut().running = false;
                model.as_mut().rust_mut().active = None;
                if ticket.current() {
                    if let Some(document) = document {
                        if model.rust().refreshing && page == 0 {
                            model.as_mut().rust_mut().refreshing = false;
                            model.as_mut().rust_mut().pages.clear();
                        }
                        let rows = document["items"].as_array().into_iter().flatten().map(|row| row.to_string()).collect();
                        if model.rust().pages.len() == 8 { model.as_mut().rust_mut().pages.pop_front(); }
                        model.as_mut().rust_mut().pages.push_back((page, rows));
                        if page == 0 {
                            if document.get("error").is_some() { model.as_mut().rust_mut().query = None; }
                            model.as_mut().set_count(document["total"].as_u64().unwrap_or(0).min(i32::MAX as u64) as i32);
                            model.as_mut().set_document(QString::from(document.to_string()));
                        }
                        let next = model.revision().wrapping_add(1);
                        model.as_mut().set_revision(next);
                        model.as_mut().refill();
                    }
                    model.as_mut().set_loading(false);
                }
                model.start();
            });
        });
    }
}

fn image_urls(value: &mut serde_json::Value) {
    fn url(image: &mut serde_json::Value) {
        if let Some(path) = image.as_str().filter(|path| !path.is_empty() && !path.contains("://")) {
            *image = QUrl::from_local_file(&QString::from(path)).to_string().to_string().into();
        }
    }
    if let Some(images) = value.get_mut("images").and_then(serde_json::Value::as_array_mut) { for image in images { url(image); } }
    if let Some(portrait) = value.get_mut("portrait") { url(portrait); }
}
