//! I bound each request to four facet queries and three word counts.
use super::*;
use crate::{Filter, Reader};
use crate::library_sources::{Prepared, SourceIndex, Stars};
use chilpa_library::store::query::fold;

fn weight(item: &crate::library_sources::SourceItem) -> usize { if item.track==0 {item.tracks} else {1} }

/// What a round answers when there is nothing to say: nothing typed, no
/// reader, no facade, or a count that failed.
pub(crate) fn nothing() -> Value { json!({"groups":[],"words":{"anywhere":0,"title":0,"album":0}}) }

fn constant(value: bool) -> Node { if value {Node::All(vec![])} else {Node::Any(vec![])} }

pub(crate) fn cofactor(node: &Node, f: Field, value: bool) -> Node {
    match node {
        Node::Is(other,_) if field_name(*other)==field_name(f) => constant(value),
        Node::Not(inner) if matches!(inner.as_ref(), Node::Is(other,_) if field_name(*other)==field_name(f)) => constant(value),
        Node::All(nodes) => Node::All(nodes.iter().map(|n| cofactor(n,f,value)).collect()),
        Node::Any(nodes) => Node::Any(nodes.iter().map(|n| cofactor(n,f,value)).collect()),
        Node::Not(inner) => Node::Not(Box::new(cofactor(inner,f,!value))),
        Node::Group(inner) => Node::Group(Box::new(cofactor(inner,f,value))),
        other => other.clone(),
    }
}
pub(crate) fn has_field(node: &Node,f: Field) -> bool {
    match node {
        Node::Is(other,_) => field_name(*other)==field_name(f),
        Node::All(nodes) | Node::Any(nodes) => nodes.iter().any(|n| has_field(n,f)),
        Node::Not(inner) | Node::Group(inner) => has_field(inner,f),
    }
}

fn effective(node: Option<Node>, known: &[(Field,Vec<String>)], genres: &[String]) -> Filter {
    Filter { query:node.map(|n| super::super::widen_genres(&super::super::resolve_names(&n.ungrouped(),known),genres)), ..Filter::default() }
}

impl Reader {
    fn refine_known(&self, index: &SourceIndex) -> Vec<(Field,Vec<String>)> {
        let mut known = self.lib.query_vocabulary().unwrap_or_else(|_| [Field::Artist,Field::Composer,Field::Genre,Field::Label].into_iter().map(|f| (f,vec![])).collect());
        for item in &index.items {
            known[0].1.extend(item.artists.iter().cloned()); known[1].1.extend(item.composers.iter().cloned());
            known[2].1.extend(item.genres.iter().cloned());
            if !item.label.is_empty() { known[3].1.push(item.label.clone()); }
        }
        for (_,names) in &mut known { names.sort(); names.dedup(); }
        known
    }

    fn refine_genres(&self, index: &SourceIndex) -> Vec<String> {
        let mut genres: Vec<_> = self.lib.genres().unwrap_or_default().into_iter().map(|(name,_)| name).collect();
        genres.extend(index.items.iter().flat_map(|item| item.genres.iter().cloned()));
        genres.sort(); genres.dedup(); genres
    }

    pub fn refine_notes(&self, text: &str) -> Vec<super::super::EngineNote> {
        let Ok(Some(node)) = read(text) else { return vec![] };
        let (_,index) = self.source_index();
        super::super::notes::reported(&node,&self.refine_known(&index),&self.refine_genres(&index))
    }

    /// Literal edits need the current vocabulary; other edits only need text.
    pub fn refine_edit(&self, text: &str, edit: &str) -> String {
        let Ok(edit_value) = serde_json::from_str::<Value>(edit) else { return text.into() };
        if edit_value["op"] != "literal" { return super::refine_edit(text,edit); }
        let Some(at) = edit_value["note"].as_u64().and_then(|n| usize::try_from(n).ok()) else { return text.into() };
        let (_,index) = self.source_index();
        super::super::notes::literal(text,at,&self.refine_known(&index),&self.refine_genres(&index))
    }

    fn count_refine(&self, filter: &Filter, index: &SourceIndex, stars: &Stars) -> Option<usize> {
        let local = self.lib.query_count(filter).ok()?;
        let prepared = Prepared::new(filter);
        Some(local + index.items.iter().filter(|item| prepared.passes(item,stars)).map(weight).sum::<usize>())
    }

    pub fn refine_suggest(&self, text: &str, typed: &str) -> Value {
        let (_,index) = self.source_index();
        self.suggest_using(text,typed,&index)
    }

    pub(crate) fn suggest_using(&self, text: &str, typed: &str, index: &SourceIndex) -> Value {
        let typed = typed.trim();
        let empty = nothing;
        if typed.is_empty() { return empty(); }
        let Ok(node) = read(text) else { return empty() };
        let needs_names=node.as_ref().is_some_and(|n| super::super::names_words(n) || super::super::names_genres(n));
        let known = if needs_names {self.refine_known(index)} else {Vec::new()};
        let genres = if needs_names {self.refine_genres(index)} else {Vec::new()};
        let stars = Stars::read(&self.lib,index);
        let applied = effective(node.clone(),&known,&genres);
        let mut groups = Vec::new();
        let key = fold(typed);
        for f in [Field::Artist,Field::Composer,Field::Genre,Field::Label] {
            let present = applied.query.as_ref().is_some_and(|n| has_field(n,f));
            let high = if present { Filter {query:applied.query.as_ref().map(|n| cofactor(n,f,true)),..Filter::default()} } else {applied.clone()};
            let low = Filter {query:Some(if present {cofactor(applied.query.as_ref().expect("query"),f,false)} else {constant(false)}),..Filter::default()};
            let Ok((mut baseline,local)) = self.lib.query_facet_counts(&high,present.then_some(&low),f,typed) else { return empty() };
            let mut counts: std::collections::BTreeMap<String,(String,usize)> = local.into_iter().map(|(name,n)| (name.to_lowercase(),(name,n))).collect();
            let high = Prepared::new(&high); let low = Prepared::new(&low);
            for item in &index.items {
                let lo = low.passes(item,&stars); if lo { baseline+=weight(item); }
                let increment = if !lo && high.passes(item,&stars) {weight(item)} else {0};
                let values = match f {Field::Artist=>item.artists.as_slice(),Field::Composer=>item.composers.as_slice(),Field::Genre=>item.genres.as_slice(),_=>std::slice::from_ref(&item.label)};
                let mut seen = std::collections::HashSet::new();
                for value in values.iter().filter(|v| !v.is_empty() && fold(v).contains(&key)) {
                    if seen.insert(value.to_lowercase()) { counts.entry(value.to_lowercase()).or_insert_with(|| (value.clone(),0)).1+=increment; }
                }
            }
            let mut counts: Vec<_> = counts.into_values().collect();
            counts.sort_by_cached_key(|(name,_)| (fold(name),name.clone())); counts.truncate(3);
            let items: Vec<_> = counts.into_iter().map(|(value,count)| {
                let state = applied.query.as_ref().map(|n| state(n,f,&value)).unwrap_or(ValueState::None);
                json!({"value":value,"count":count+baseline,"state":match state {ValueState::Include=>"include",ValueState::Exclude=>"exclude",ValueState::None=>"none"}})
            }).collect();
            if !items.is_empty() { groups.push(json!({"field":f.name(),"items":items})); }
        }
        let kinds = [("anywhere","words"),("title","title"),("album","album")];
        let queries: Vec<Filter> = kinds.iter().map(|(_,f)| effective(read(&super::edits::preview_words(text,f,typed)).ok().flatten(),&known,&genres)).collect();
        // One pass over the library answers the three.
        let Ok(local) = self.lib.query_counts(&queries.iter().collect::<Vec<_>>()) else { return empty() };
        let mut words = json!({});
        for (((name,_),query),local) in kinds.iter().zip(&queries).zip(local) {
            let prepared = Prepared::new(query);
            words[name]=json!(local + index.items.iter().filter(|item| prepared.passes(item,&stars)).map(weight).sum::<usize>());
        }
        json!({"groups":groups,"words":words})
    }

    pub fn refine_without_each(&self, text: &str) -> Value {
        let (_,index) = self.source_index();
        self.without_each_using(text,&index)
    }

    pub(crate) fn without_each_using(&self, text: &str, index: &SourceIndex) -> Value {
        let Ok(node) = read(text) else { return json!([]) };
        let needs_names=node.as_ref().is_some_and(|n| super::super::names_words(n) || super::super::names_genres(n));
        let known = if needs_names {self.refine_known(index)} else {Vec::new()};
        let genres = if needs_names {self.refine_genres(index)} else {Vec::new()};
        let stars = Stars::read(&self.lib,index);
        if self.count_refine(&effective(node.clone(),&known,&genres),index,&stars)!=Some(0) { return json!([]); }
        let (any,nodes) = parts(node); let mut found = Vec::new();
        for at in 0..nodes.len() {
            let mut rest = nodes.clone(); rest.remove(at);
            if let Some(count) = self.count_refine(&effective(tree(any,rest),&known,&genres),index,&stars) {
                if count>0 { found.push((at,count)); }
            }
        }
        found.sort_by_key(|&(id,count)| (std::cmp::Reverse(count),id)); found.truncate(3);
        json!(found.into_iter().map(|(id,count)| json!({"id":id.to_string(),"count":count})).collect::<Vec<_>>())
    }
}
