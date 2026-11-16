//! An article transfers its body once. Only its eight image addresses remain
//! with the owning session; neither a shelf nor the disk window holds the text.
mod document;
#[cfg(test)]
mod tests;

use super::{
    Command, Job, NativeCollectionWindow, Shared, projection,
    worker::{Failure, PageSource},
};
use crate::source_catalog::Catalog;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

pub(super) struct Session {
    plugin: String,
    generation: u64,
    settings_epoch: u64,
    images: Vec<String>,
    pub(super) demands: BTreeMap<String, u64>,
}

/// A transient article shares the collection broker's bounded worker and mailbox.
/// Opening another view retires this session. Closing never waits for an RPC.
pub struct NativeArticleWindow {
    window: NativeCollectionWindow,
}

impl crate::Chilpa {
    pub fn native_article_window(&mut self) -> NativeArticleWindow {
        NativeArticleWindow {
            window: self.native_collection_window(),
        }
    }
}

impl NativeArticleWindow {
    pub fn open(&mut self, key: &str, request_id: u64) -> u64 {
        self.window.start(key, request_id, true)
    }
    pub fn close(&mut self, session: u64) {
        self.window.close(session);
    }
    pub fn busy(&self) -> bool {
        self.window.busy()
    }
    /// False means the URL is undeclared, the session is retired, or the two
    /// pending slots are full. A view may retry a full queue after consuming a reply.
    pub fn request_image(&self, session: u64, url: &str, request_id: u64) -> bool {
        if url.len() > document::URL_BYTES {
            return false;
        }
        let mut state = self.window.broker.shared.lock();
        if session == 0
            || state.session != session
            || self.window.session != session
            || state.shutdown
            || !state
                .article
                .as_ref()
                .is_some_and(|article| article.images.iter().any(|image| image == url))
        {
            return false;
        }
        if state
            .article
            .as_ref()
            .is_some_and(|article| article.demands.contains_key(url))
        {
            return true;
        }
        if state.pending.len() >= 2 {
            return false;
        }
        state.serial = state
            .serial
            .checked_add(1)
            .expect("article demand identifiers exhausted");
        let demand = state.serial;
        let epoch = state.epoch;
        state
            .article
            .as_mut()
            .unwrap()
            .demands
            .insert(url.into(), demand);
        state.pending.push_back(Job {
            session,
            epoch,
            request: request_id,
            command: Command::Image(url.into(), demand),
        });
        drop(state);
        self.window.broker.shared.wake.notify_all();
        true
    }
    /// Release one URL's demand. A finished fetch may stay in the shared disk
    /// artwork cache, but no queued or late result belongs to this view anymore.
    pub fn release_image(&self, session: u64, url: &str) {
        let mut state = self.window.broker.shared.lock();
        if session == 0 || state.session != session || self.window.session != session {
            return;
        }
        if let Some(article) = &mut state.article {
            article.demands.remove(url);
        }
        state
            .pending
            .retain(|job| !matches!(&job.command, Command::Image(image, _) if image == url));
        if state
            .mailbox
            .as_ref()
            .is_some_and(|value| value["kind"] == "image" && value["url"] == url)
        {
            state.mailbox = None;
        }
        drop(state);
        self.window.broker.shared.wake.notify_all();
    }
    pub fn poll(&mut self) -> Option<Value> {
        let mut state = self.window.broker.shared.lock();
        if self.window.session == 0 || state.session != self.window.session {
            return None;
        }
        if state.article.as_ref().is_some_and(|article| {
            self.window.broker.source.article_stamp(&article.plugin)
                != (article.generation, article.settings_epoch)
                || !self.window.broker.source.article_pages(&article.plugin)
        }) {
            state.article = None;
            state.pending.clear();
            state.mailbox = None;
            state.epoch += 1;
            let value = json!({"kind":"article","sessionId":state.session,"requestId":0,
                "generation":0,"busy":false,"error":"The source changed. Open the article again.","document":null});
            drop(state);
            self.window.broker.shared.wake.notify_all();
            return Some(value);
        }
        let value = state.mailbox.take();
        drop(state);
        self.window.broker.shared.wake.notify_all();
        value
    }
}

fn live(
    shared: &Shared,
    source: &dyn PageSource,
    job: &Job,
    plugin: &str,
    stamp: (u64, u64),
) -> Result<(), Failure> {
    if !shared.alive(job) {
        return Err(Failure::Cancelled);
    }
    if stamp.0 == 0 || source.article_stamp(plugin) != stamp || !source.article_pages(plugin) {
        return Err(Failure::Other(
            "The source changed. Open the article again.".into(),
        ));
    }
    Ok(())
}

fn envelope(job: &Job, kind: &str, generation: u64) -> Value {
    json!({"kind":kind,"sessionId":job.session,"requestId":job.request,"generation":generation,"busy":false,"error":""})
}

fn open(
    shared: &Shared,
    source: &dyn PageSource,
    catalog: &Arc<Catalog>,
    job: &Job,
    key: &str,
) -> Result<Value, Failure> {
    let (plugin, reference) = crate::source_catalog::split(key);
    if plugin.is_empty() || plugin.len() > 128 {
        return Err("The article source is invalid.".into());
    }
    let reference = reference
        .strip_prefix("native:")
        .ok_or("The article address is invalid.")?;
    let request = crate::native_page_request::decode(reference)?;
    if request.layout != "article" {
        return Err("The article address is invalid.".into());
    }
    let stamp = source.article_stamp(plugin);
    live(shared, source, job, plugin, stamp)?;
    let _permit = catalog
        .collection_permit(plugin, || shared.alive(job))
        .ok_or(Failure::Cancelled)?;
    let answer = source.page(plugin, request.params);
    live(shared, source, job, plugin, stamp)?;
    let answer = answer?;
    let images = document::validate(&answer)?;
    let mut state = shared.lock();
    if state.session != job.session || state.epoch != job.epoch || state.shutdown {
        return Err(Failure::Cancelled);
    }
    state.article = Some(Session {
        plugin: plugin.into(),
        generation: stamp.0,
        settings_epoch: stamp.1,
        images,
        demands: BTreeMap::new(),
    });
    drop(state);
    let mut response = envelope(job, "article", stamp.0);
    response["document"] = answer;
    Ok(response)
}

fn image(shared: &Shared, source: &dyn PageSource, job: &Job, url: &str) -> Result<Value, Failure> {
    let (plugin, stamp) = {
        let state = shared.lock();
        let article = state
            .article
            .as_ref()
            .filter(|_| state.session == job.session && state.epoch == job.epoch)
            .ok_or(Failure::Cancelled)?;
        if !article.images.iter().any(|image| image == url) {
            return Err("The image is not part of this article.".into());
        }
        (
            article.plugin.clone(),
            (article.generation, article.settings_epoch),
        )
    };
    live(shared, source, job, &plugin, stamp)?;
    let result = source.image(&plugin, url);
    live(shared, source, job, &plugin, stamp)?;
    let mut response = envelope(job, "image", stamp.0);
    response["url"] = url.into();
    response["path"] = result?.into();
    Ok(response)
}

pub(super) fn run(
    shared: &Arc<Shared>,
    source: &Arc<dyn PageSource>,
    catalog: &Arc<Catalog>,
    job: &Job,
) {
    let result = match &job.command {
        Command::Article(key) => open(shared, source.as_ref(), catalog, job, key),
        Command::Image(url, _) => image(shared, source.as_ref(), job, url),
        _ => unreachable!("only article commands reach this handler"),
    };
    match result {
        Ok(value) => shared.publish(job, value),
        Err(Failure::Cancelled) => {}
        Err(failure) => {
            let error = match failure {
                Failure::Other(error) | Failure::Offline(error) | Failure::Stale(error, _) => error,
                Failure::Cancelled => unreachable!(),
            };
            let generation = shared
                .lock()
                .article
                .as_ref()
                .map_or(0, |article| article.generation);
            let mut value = envelope(
                job,
                if matches!(&job.command, Command::Image(_, _)) {
                    "image"
                } else {
                    "article"
                },
                generation,
            );
            value["error"] = projection::label(&json!(error), 512).into();
            if let Command::Image(url, _) = &job.command {
                value["url"] = url.clone().into();
                value["path"] = "".into();
            } else {
                value["document"] = Value::Null;
            }
            shared.publish(job, value);
        }
    }
}

pub(super) fn navigation(plugin: &str, item: &Value) -> Result<Value, Failure> {
    let reference = item["ref"]
        .as_str()
        .ok_or("The article address is invalid.")?;
    let encoded = crate::native_page_request::encode(reference, json!({}), "article")?;
    Ok(
        json!({"lease":0,"key":"","trackRow":-1,"cardId":0,"kind":"article",
        "ref":reference,"navigationKey":format!("{plugin} native:{encoded}"),
        "title":projection::label(&item["title"],256),"artist":"","available":item["available"]!=false}),
    )
}
