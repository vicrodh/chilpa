//! One immutable integrated index per catalog, built on a query worker.
//! Readers borrow an Arc for one query and retain only its revision afterward.
//! The builder lock may precede inner; catalog writers never take the builder.

mod inputs;
mod hint;
#[cfg(test)]
mod tests;

use super::{Catalog, Inner};
use crate::library_sources::{SourceIndex, Stamp};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Source {
    pub id: String,
    pub albums: bool,
    pub tracks: bool,
    pub name: String,
    pub icon: String,
}

pub(super) struct Publication {
    fingerprint: [u8; 32],
    serial: u64,
    index: Arc<SourceIndex>,
    hint: hint::Hint,
}

impl Inner {
    pub(super) fn retire_integrated_index(&mut self) -> Option<Publication> {
        self.index_epoch = self.index_epoch.wrapping_add(1);
        self.integrated_index.take()
    }
}

impl Catalog {
    pub(crate) fn integration_epoch(&self) -> u64 { self.lock().index_epoch }

    pub(crate) fn clear_integrated_index(&self) {
        let retired = {
            let mut inner = self.lock();
            inner.retire_integrated_index()
        };
        drop(retired);
    }

    /// Blocking construction belongs exclusively to the existing query workers.
    pub(crate) fn integrated_index(&self, sources: &[Source], epoch: u64) -> (Stamp, Arc<SourceIndex>) {
        self.integrated_index_with_clock(sources, epoch, super::now)
    }

    fn integrated_index_with_clock(&self, sources: &[Source], epoch: u64, clock: impl Fn() -> u64) -> (Stamp, Arc<SourceIndex>) {
        if sources.is_empty() {
            let retired = {
                let mut inner = self.lock();
                if inner.index_epoch == epoch { inner.retire_integrated_index() } else { None }
            };
            drop(retired);
            return Default::default();
        }
        let chosen = sources.iter().map(|source| (source.id.clone(), source.albums, source.tracks)).collect();
        if let Some((serial, index)) = self.cached_integrated_index(sources, epoch, clock()) {
            return ((serial, chosen), index);
        }
        let _builder = self.index_builder.lock().unwrap_or_else(|error| error.into_inner());
        if let Some((serial, index)) = self.cached_integrated_index(sources, epoch, clock()) {
            return ((serial, chosen), index);
        }
        let inputs = inputs::Inputs::capture(self, sources, clock());
        let fingerprint = inputs.fingerprint();
        let same_music = {
            let inner = self.lock();
            inner.integrated_index.as_ref().filter(|held| inner.index_epoch == epoch && held.fingerprint == fingerprint)
                .map(|held| (held.serial, held.index.clone()))
        };
        if let Some((serial, index)) = same_music {
            let hint = inputs.into_hint();
            let retired = {
                let mut inner = self.lock();
                if inner.index_epoch == epoch && hint.matches(&inner, sources, clock()) {
                    inner.integrated_index.as_mut().filter(|held| held.serial == serial)
                        .map(|held| std::mem::replace(&mut held.hint, hint))
                } else { None }
            };
            drop(retired);
            return ((serial, chosen), index);
        }
        let index = Arc::new(inputs.build());
        let hint = inputs.into_hint();
        let (serial, retired) = {
            let mut inner = self.lock();
            inner.index_serial = inner.index_serial.wrapping_add(1);
            let serial = inner.index_serial;
            let retired = if epoch == inner.index_epoch && hint.matches(&inner, sources, clock()) {
                inner.integrated_index.replace(Publication { fingerprint, serial, index: index.clone(), hint })
            } else { None };
            (serial, retired)
        };
        drop(retired);
        ((serial, chosen), index)
    }

    fn cached_integrated_index(&self, sources: &[Source], epoch: u64, now: u64) -> Option<(u64, Arc<SourceIndex>)> {
        let inner = self.lock();
        inner.integrated_index.as_ref().filter(|held| inner.index_epoch == epoch && held.hint.matches(&inner, sources, now))
            .map(|held| (held.serial, held.index.clone()))
    }
}
