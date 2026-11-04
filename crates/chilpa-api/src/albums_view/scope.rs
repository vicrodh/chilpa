//! Optional per-view filters, without changing the shared library selection.
use crate::{Filter, Reader};
use super::AlbumsFound;
#[derive(Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Scope { excluded_sources:Vec<String>, favorites_only:bool }
impl Reader {
    pub fn albums_view_scoped(&self, order:&str, group:&str, text:&str, extras:&Filter, scope:&str)->AlbumsFound {
        if scope.is_empty(){return self.albums_view(order,group,text,extras)}
        if scope.len()>65536{return AlbumsFound::default()}
        let Some(scope)=serde_json::from_str::<Scope>(scope).ok().filter(|s|scope.len()<=65536 && s.excluded_sources.len()<=256 && s.excluded_sources.iter().all(|id|id.len()<=512)) else {return AlbumsFound::default()};
        let mut filter=extras.clone();filter.excluded_sources=scope.excluded_sources;
        self.albums_view_selected(order,group,text,&filter,&self.source_index().1,scope.favorites_only)
    }
}
