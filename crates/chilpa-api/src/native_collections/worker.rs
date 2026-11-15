use super::{
    Command, Job, Shared, projection,
    store::{Page, Query, Store},
};
use crate::{
    natives::Natives,
    source_catalog::{Catalog, collection_window},
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

pub(super) enum Failure {
    Other(String),
    Offline(String),
    Stale(String, Option<String>),
    Cancelled,
}
impl From<String> for Failure {
    fn from(error: String) -> Self {
        Self::Other(error)
    }
}
impl From<&str> for Failure {
    fn from(error: &str) -> Self {
        Self::Other(error.into())
    }
}
type Result<T> = std::result::Result<T, Failure>;

pub(super) trait PageSource: Send + Sync {
    fn generation(&self, plugin: &str) -> u64;
    fn page(&self, plugin: &str, params: Value) -> Result<Value>;
}
pub(super) struct NativeSource(pub Arc<Natives>);
impl PageSource for NativeSource {
    fn generation(&self, plugin: &str) -> u64 {
        self.0.collection_generation(plugin)
    }
    fn page(&self, plugin: &str, params: Value) -> Result<Value> {
        match self
            .0
            .call_raw(plugin, "page.get", params, Duration::from_secs(90))
        {
            Ok(value) => Ok(value),
            Err(chilpa_plugin::native::NativeError::Rpc {
                data: Some(data),
                message,
                ..
            }) if data["stale_cursor"] == true => Err(Failure::Stale(message, restart(&data))),
            Err(error) => Err(Failure::Offline(error.to_string())),
        }
    }
}
fn restart(value: &Value) -> Option<String> {
    value["restart_ref"]
        .as_str()
        .filter(|reference| crate::native_page_request::valid_reference(reference))
        .map(str::to_string)
}

struct Engine {
    store: Store,
    query: Option<Query>,
    stale: bool,
    boot: String,
    source: Arc<dyn PageSource>,
    catalog: Arc<Catalog>,
    shared: Arc<Shared>,
}

pub(super) fn run(
    shared: Arc<Shared>,
    source: Arc<dyn PageSource>,
    catalog: Arc<Catalog>,
    path: PathBuf,
) {
    let boot = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let mut engine: Option<Engine> = None;
    loop {
        let job = {
            let mut state = shared.lock();
            while !state.shutdown
                && !state.cleanup
                && (state.pending.is_empty() || state.mailbox.is_some())
            {
                if state.session == 0 {
                    if let Some(engine) = &mut engine {
                        engine.query = None;
                        engine.stale = false;
                    }
                }
                state = shared
                    .wake
                    .wait(state)
                    .unwrap_or_else(|error| error.into_inner());
            }
            if state.cleanup {
                state.cleanup = false;
                drop(state);
                catalog.reap_collection_actions();
                continue;
            }
            if state.shutdown {
                break;
            }
            state.working = true;
            state
                .pending
                .pop_front()
                .expect("queued collection request")
        };
        if engine.is_none() {
            match Store::open(&path) {
                Ok(store) => engine=Some(Engine { store,query:None,stale:false,boot:boot.clone(),source:source.clone(),catalog:catalog.clone(),shared:shared.clone() }),
                Err(error) => shared.publish(&job,json!({"sessionId":job.session,"requestId":job.request,"knownCount":0,"busy":false,"error":projection::label(&json!(error),512)})),
            }
        }
        if let Some(engine) = &mut engine {
            let result = engine.handle(&job);
            match result {
                Ok(value) => shared.publish(&job, value),
                Err(Failure::Cancelled) => {}
                Err(failure) => {
                    let value = engine.failure(&job, failure);
                    shared.publish(&job, value);
                }
            }
        }
        catalog.reap_collection_actions();
        shared.lock().working = false;
        shared.wake.notify_all();
    }
}

impl Engine {
    fn stamp(&self, plugin: &str, personal: bool) -> String {
        let (all, history) = self.catalog.collection_epochs(plugin);
        format!(
            "{}:{}:{}:{}",
            self.boot,
            self.source.generation(plugin),
            all,
            if personal { history } else { 0 }
        )
    }
    fn live(&self, job: &Job) -> Result<()> {
        if self.shared.alive(job) {
            Ok(())
        } else {
            Err(Failure::Cancelled)
        }
    }
    fn check(&self, query: &Query) -> Result<()> {
        if self.stale || query.host != self.stamp(&query.plugin, query.personal) {
            Err(Failure::Stale(
                "The collection changed. Reload it to continue.".into(),
                None,
            ))
        } else {
            Ok(())
        }
    }
    fn rpc(&self, query: &Query, reference: &str, manual: bool, job: &Job) -> Result<Value> {
        self.live(job)?;
        let stamp = self.stamp(&query.plugin, query.personal);
        let personal_stamp = self.stamp(&query.plugin, true);
        let _permit = self
            .catalog
            .collection_permit(&query.plugin, || self.shared.alive(job))
            .ok_or(Failure::Cancelled)?;
        let mut params = if query.params["ref"] == reference {
            query.params.clone()
        } else {
            json!({})
        };
        params["ref"] = json!(reference);
        params["refresh"] = json!(manual);
        let answer = self.source.page(&query.plugin, params);
        self.live(job)?;
        if stamp != self.stamp(&query.plugin, query.personal) {
            return Err(Failure::Stale(
                "The source changed while reading this collection.".into(),
                None,
            ));
        }
        let value = answer?;
        if value["personal"] == true && personal_stamp != self.stamp(&query.plugin, true) {
            return Err(Failure::Stale(
                "The source changed while reading this collection.".into(),
                None,
            ));
        }
        collection_window::validate(&value)?;
        if value["stale_cursor"] == true {
            return Err(Failure::Stale(
                "The collection changed. Reload it to continue.".into(),
                restart(&value),
            ));
        }
        if let Some(message) = value["error"]
            .as_str()
            .filter(|message| !message.is_empty())
        {
            return Err(Failure::Offline(message.to_string()));
        }
        Ok(value)
    }
    fn handle(&mut self, job: &Job) -> Result<Value> {
        self.live(job)?;
        match &job.command {
            Command::Open(key) => self.open(key, job),
            Command::Refresh(manual) => self.refresh(*manual, job),
            Command::Seek(first, limit) => self.seek(*first, *limit, job),
            Command::Action(token) => self.action(token, job),
        }
    }
    fn open(&mut self, key: &str, job: &Job) -> Result<Value> {
        self.query = None;
        self.stale = false;
        let (plugin, reference) = crate::source_catalog::split(key);
        if plugin.is_empty() || plugin.len() > 128 {
            return Err("The collection source is invalid.".into());
        }
        let reference = reference
            .strip_prefix("native:")
            .ok_or("The collection address is invalid.")?;
        let request = crate::native_page_request::decode(reference)?;
        let mut params = request.params;
        if params.get("limit").is_none() {
            params["limit"] = json!(100);
        }
        let key = projection::hash(format!("{plugin}\0{params}").as_bytes());
        let query = Query {
            id: 0,
            generation: 1,
            plugin: plugin.into(),
            params,
            header: json!({}),
            revision: None,
            host: self.stamp(plugin, false),
            personal: false,
            known: 0,
            pages: 0,
            next: None,
            total: None,
            anchor: 0,
            restart: None,
        };
        self.query = Some(self.store.open_query(&key, query)?);
        self.refresh(false, job)
    }
    fn current(&self) -> Result<Query> {
        self.query
            .clone()
            .ok_or_else(|| Failure::Other("Open a collection first.".into()))
    }

    fn refresh(&mut self, manual: bool, job: &Job) -> Result<Value> {
        let mut query = self.current()?;
        let reference = query
            .restart
            .as_deref()
            .or_else(|| query.params["ref"].as_str())
            .ok_or("The collection address is invalid.")?
            .to_string();
        query.params["ref"] = json!(reference);
        let wire = self.rpc(&query, &reference, manual, job)?;
        let header = projection::header(&wire, &query)?;
        let hash = projection::content(&wire);
        let next = projection::next(&wire)?;
        let revision = projection::revision(&wire);
        let old = self.store.at(&query, 0)?;
        let same = old.as_ref().is_some_and(|page| {
            page.hash == hash
                && page.next_hash == next.as_ref().map(|next| projection::hash(next.as_bytes()))
        }) && query.revision == revision
            && query.personal == (wire["personal"] == true)
            && query.host == self.stamp(&query.plugin, query.personal)
            && !self.stale;
        query.header = header;
        query.restart = None;
        query.total = wire["total"].as_u64();
        if same {
            self.store.save(&query)?;
            self.query = Some(query.clone());
            return self.seek(query.anchor.min(query.known.saturating_sub(1)), 100, job);
        }
        if query.pages > 0 {
            query.generation = query
                .generation
                .checked_add(1)
                .ok_or("Collection generations exhausted.")?;
        }
        query.revision = revision;
        query.personal = wire["personal"] == true;
        query.host = self.stamp(&query.plugin, query.personal);
        query.known = wire["items"].as_array().map_or(0, Vec::len);
        query.pages = 1;
        query.next = next;
        query.anchor = 0;
        let page = Page {
            ordinal: 0,
            first: 0,
            count: query.known,
            reference: Some(reference.clone()),
            hash,
            next_hash: query
                .next
                .as_ref()
                .map(|next| projection::hash(next.as_bytes())),
        };
        self.store.append(&query, &reference, &page)?;
        self.stale = false;
        self.query = Some(query.clone());
        self.project(&query, &page, &wire, 0, 100, job)
    }

    fn load(&self, query: &Query, target: usize, job: &Job) -> Result<Value> {
        self.check(query)?;
        let checkpoint = self.store.checkpoint(query, target)?;
        let mut reference = checkpoint
            .reference
            .ok_or("The collection checkpoint is missing.")?;
        for ordinal in checkpoint.ordinal..=target {
            let wire = self.rpc(query, &reference, false, job)?;
            let page = self
                .store
                .at(query, ordinal)?
                .ok_or("The collection page is missing.")?;
            let next = projection::next(&wire)?;
            if projection::revision(&wire) != query.revision
                || projection::content(&wire) != page.hash
                || next.as_ref().map(|next| projection::hash(next.as_bytes())) != page.next_hash
            {
                return Err(Failure::Stale(
                    "The collection changed. Reload it to continue.".into(),
                    None,
                ));
            }
            if ordinal == target {
                return Ok(wire);
            }
            reference = next.ok_or_else(|| {
                Failure::Stale("The collection ended before this item.".into(), None)
            })?;
        }
        Err("The collection position is invalid.".into())
    }

    fn project(
        &self,
        query: &Query,
        page: &Page,
        wire: &Value,
        first: usize,
        limit: usize,
        job: &Job,
    ) -> Result<Value> {
        let document = projection::chunk(query, job, page, wire, first, limit)?;
        self.store.cache(query, &document["page"])?;
        Ok(document)
    }

    fn seek(&mut self, first: usize, limit: usize, job: &Job) -> Result<Value> {
        let mut query = self.current()?;
        self.check(&query)?;
        if first > query.known {
            return Err("This collection position has not been loaded yet.".into());
        }
        query.anchor = first.min(query.known.saturating_sub(1));
        self.store.save(&query)?;
        self.query = Some(query.clone());
        if first < query.known {
            if let Some(chunk) = self.store.cached(&query, first)? {
                return Ok(projection::cached(&query, job, &chunk, first, limit)?);
            }
            let page = self
                .store
                .containing(&query, first)?
                .ok_or("The collection position is unavailable.")?;
            let wire = self.load(&query, page.ordinal, job)?;
            return self.project(&query, &page, &wire, first, limit, job);
        }
        let Some(reference) = query.next.clone() else {
            return Ok(projection::document(&query, job));
        };
        let wire = self.rpc(&query, &reference, false, job)?;
        if projection::revision(&wire) != query.revision {
            return Err(Failure::Stale(
                "The collection changed. Reload it to continue.".into(),
                None,
            ));
        }
        let next = projection::next(&wire)?;
        let count = wire["items"].as_array().map_or(0, Vec::len);
        let page = Page {
            ordinal: query.pages,
            first: query.known,
            count,
            reference: Some(reference.clone()),
            hash: projection::content(&wire),
            next_hash: next.as_ref().map(|next| projection::hash(next.as_bytes())),
        };
        query.known = query
            .known
            .checked_add(count)
            .filter(|count| *count <= i32::MAX as usize)
            .ok_or("The collection exceeds the supported position range.")?;
        if count > 0 {
            query.anchor = first;
        }
        query.pages += 1;
        query.next = next;
        self.store.append(&query, &reference, &page)?;
        self.query = Some(query.clone());
        self.project(&query, &page, &wire, first, limit, job)
    }

    fn action(&mut self, token: &str, job: &Job) -> Result<Value> {
        let query = self.current()?;
        self.check(&query)?;
        let (id, generation, ordinal, local, hash) =
            projection::parse_token(token).ok_or("The selected item is invalid.")?;
        if id != query.id || generation != query.generation {
            return Err("The selected item belongs to an earlier collection.".into());
        }
        let page = self
            .store
            .at(&query, ordinal)?
            .ok_or("The selected item is no longer available.")?;
        if hash != page.hash || local >= page.count {
            return Err("The selected item is no longer available.".into());
        }
        let old = self.shared.lock().action.take();
        drop(old);
        let wire = self.load(&query, ordinal, job)?;
        let id = {
            let mut state = self.shared.lock();
            state.serial += 1;
            state.serial
        };
        let (lease, mut action) =
            self.catalog
                .collection_action(&query.plugin, &wire, local, id)?;
        action["title"] = json!(projection::label(&wire["items"][local]["title"], 256));
        action["artist"] = json!(projection::label(&wire["items"][local]["artist"], 256));
        action["available"] = json!(wire["items"][local]["available"] != false);
        self.live(job)?;
        let mut state = self.shared.lock();
        if state.session != job.session || state.epoch != job.epoch {
            drop(state);
            drop(lease);
            return Err(Failure::Cancelled);
        }
        state.action = Some(lease);
        drop(state);
        let mut document = projection::document(&query, job);
        document["action"] = action;
        Ok(document)
    }

    fn failure(&mut self, job: &Job, failure: Failure) -> Value {
        let (message, offline, stale) = match failure {
            Failure::Offline(message) => (message, true, false),
            Failure::Stale(message, restart) => {
                self.stale = true;
                if let Some(query) = self.query.as_mut().filter(|_| restart.is_some()) {
                    query.restart = restart;
                    let _ = self.store.save(query);
                }
                (message, false, true)
            }
            Failure::Other(message) => (message, false, false),
            Failure::Cancelled => unreachable!(),
        };
        let mut document=self.query.as_ref().map(|query|projection::document(query,job))
            .unwrap_or_else(||json!({"sessionId":job.session,"requestId":job.request,"knownCount":0,"hasMore":false,"busy":false}));
        document["error"] = json!(projection::label(&json!(message), 512));
        document["offline"] = json!(offline);
        document["stale"] = json!(
            stale
                || self
                    .query
                    .as_ref()
                    .is_some_and(|query| query.host != self.stamp(&query.plugin, query.personal))
        );
        if let Some(query) = &self.query {
            if let Ok(Some(chunk)) = self.store.cached(query, query.anchor) {
                if let Ok(projected) =
                    projection::cached_document(document.clone(), &chunk, query.anchor, 100)
                {
                    return projected;
                }
            }
        }
        document
    }
}
