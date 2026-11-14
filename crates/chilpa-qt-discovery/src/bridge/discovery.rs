//! One running query and one replaceable request per mounted discovery view.
//! The shared worker owns the reader; destruction cancels queued publication.
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
        type DiscoveryModel = super::DiscoveryRust;
        #[qinvokable]
        fn request(self: Pin<&mut DiscoveryModel>, kind: &QString, key: &QString, text: &QString, page: i32, revision: i32);
        #[qinvokable]
        fn cancel(self: Pin<&mut DiscoveryModel>);
    }
    impl cxx_qt::Threading for DiscoveryModel {}
}
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QString, QUrl};
use std::pin::Pin;

struct Request { kind: String, key: String, text: String, page: usize, revision: i32 }
#[derive(Default)]
pub struct DiscoveryRust {
    document: QString,
    loading: bool,
    running: bool,
    pending: Option<Request>,
    displayed: Option<(String, String, String, usize)>,
    generation: chilpa_qt_kit::album_pages::Generation,
}
impl qobject::DiscoveryModel {
    pub fn cancel(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending = None;
        self.rust().generation.next();
        self.as_mut().set_loading(false);
    }
    pub fn request(mut self: Pin<&mut Self>, kind: &QString, key: &QString, text: &QString, page: i32, revision: i32) {
        if key.to_string().len() > 8192 || text.to_string().len() > 1024 { return; }
        let (kind, key, text, page) = (kind.to_string(), key.to_string(), text.to_string(), page.max(0) as usize);
        let identity = (kind.clone(), key.clone(), text.clone(), page);
        let changed = self.rust().displayed.as_ref() != Some(&identity);
        self.as_mut().rust_mut().displayed = Some(identity);
        self.rust().generation.next();
        self.as_mut().rust_mut().pending = Some(Request { kind, key, text, page, revision });
        if changed { self.as_mut().set_document(QString::from("{}")); }
        if changed || self.document().to_string() == "{}" { self.as_mut().set_loading(true); }
        self.start();
    }
    fn start(mut self: Pin<&mut Self>) {
        if self.rust().running { return; }
        let Some(request) = self.as_mut().rust_mut().pending.take() else { return; };
        self.as_mut().rust_mut().running = true;
        let ticket = self.rust().generation.next();
        let thread = self.qt_thread();
        chilpa_qt_kit::worker::submit_checked(move |reader| {
            let document = ticket.current().then(|| match reader {
                Ok(reader) => {
                    let mut value = reader.discovery_page(&request.kind, &request.key, &request.text, request.page);
                    if let Some(items) = value["items"].as_array_mut() { for item in items { image_urls(item); } }
                    image_urls(&mut value["summary"]);
                    value.to_string()
                }
                Err(error) => serde_json::json!({"error":error,"items":[],"total":0}).to_string(),
            });
            let _ = request.revision;
            let _ = thread.queue(move |mut model| {
                model.as_mut().rust_mut().running = false;
                if ticket.current() {
                    if let Some(document) = document {
                        let document = QString::from(document);
                        if model.document() != &document { model.as_mut().set_document(document); }
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
