use super::*;

#[derive(serde::Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum Edit {
    Value { field: String, value: Value, state: ValueState, target: Option<String> },
    RemoveValue { id: String, value: Value },
    Remove { id: String },
    Operator { id: String, to: String },
    Range { #[serde(default = "year")] field: String, from: Option<i64>, to: Option<i64>, target: Option<String> },
    Rating { min: i64, max: i64, target: Option<String> },
    Added { n: i64, unit: String, target: Option<String> },
    None { field: String, on: bool, target: Option<String> },
    Words { field: String, word: String, target: Option<String> },
    Mode { mode: String, target: Option<String> },
    Group { ids: Vec<String> },
    Ungroup { id: String },
}
fn year() -> String { "year".into() }
fn mode(s: &str) -> Option<bool> { match s { "all" => Some(false), "any" => Some(true), _ => None } }
fn value(v: Value) -> Option<String> {
    match v { Value::String(s) if !s.trim().is_empty() => Some(s), Value::Number(n) if n.is_i64() => Some(n.to_string()), _ => None }
}
fn index(id: &str) -> Option<usize> {
    let i: usize = id.parse().ok()?;
    (i.to_string() == id).then_some(i)
}
fn leaf(node: &mut Node) -> Option<(&mut Field, &mut Values)> {
    match node { Node::Is(f,v) => Some((f,v)), Node::Not(inner) => match inner.as_mut() { Node::Is(f,v) => Some((f,v)), _ => None }, _ => None }
}

fn in_target(nodes: &mut Vec<Node>, target: Option<&str>, edit: impl FnOnce(&mut bool, &mut Vec<Node>) -> Option<()>, any: &mut bool) -> Option<()> {
    match target {
        None => edit(any, nodes),
        Some("new") => {
            let mut children = Vec::new(); let mut mode = true;
            edit(&mut mode, &mut children)?;
            if let Some(node) = grouped(mode, children) { nodes.push(node); }
            Some(())
        }
        Some(id) => {
            let at = index(id)?;
            let (mut mode, mut children) = group(nodes.get(at)?)?;
            edit(&mut mode, &mut children)?;
            match grouped(mode, children) { Some(node) => nodes[at] = node, None => { nodes.remove(at); } }
            Some(())
        }
    }
}

fn at_row(nodes: &mut Vec<Node>, id: &str, edit: impl FnOnce(&mut Vec<Node>, usize) -> Option<()>) -> Option<()> {
    if let Some((parent, child)) = id.split_once('.') {
        let at = index(parent)?; let child = index(child)?;
        let (any, mut children) = group(nodes.get(at)?)?;
        children.get(child)?; edit(&mut children, child)?;
        match grouped(any, children) { Some(node) => nodes[at] = node, None => { nodes.remove(at); } }
    } else { let at = index(id)?; nodes.get(at)?; edit(nodes, at)?; }
    Some(())
}

fn replace(nodes: &mut Vec<Node>, f: Field, accepts: impl Fn(&Values) -> bool, new: Option<Values>) {
    let at = nodes.iter().position(|n| matches!(n, Node::Is(other,v) if *other == f && accepts(v))).unwrap_or(nodes.len());
    nodes.retain(|n| !matches!(n, Node::Is(other,v) if *other == f && accepts(v)));
    if let Some(v) = new { nodes.insert(at.min(nodes.len()), Node::Is(f,v)); }
}

fn remove_value(node: Node, f: Field, v: &str, state: ValueState) -> Option<Node> {
    match node {
        Node::All(nodes) => tree(false, nodes.into_iter().filter_map(|n| remove_value(n,f,v,state)).collect()),
        Node::Any(nodes) => tree(true, nodes.into_iter().filter_map(|n| remove_value(n,f,v,state)).collect()),
        Node::Group(inner) => remove_value(*inner,f,v,state).map(|n| Node::Group(Box::new(n))),
        Node::Not(inner) => remove_value(*inner,f,v,state).map(|n| Node::Not(Box::new(n))),
        Node::Is(other, values) if field_name(other) == field_name(f) => match values {
            Values::Any(mut list) => {
                list.retain(|s| s.to_lowercase() != v.to_lowercase());
                (!list.is_empty()).then_some(Node::Is(other,Values::Any(list)))
            }
            Values::Missing if state != ValueState::None => None,
            Values::Range(from,to) if v.parse::<i64>().is_ok_and(|n| from.is_none_or(|a| n>=a) && to.is_none_or(|b| n<=b)) => {
                let n=v.parse::<i64>().ok()?;
                let mut sides=Vec::new();
                if from.is_none_or(|a| a<n) {
                    if let Some(end)=n.checked_sub(1) { sides.push(Node::Is(other,Values::Range(from,Some(end)))); }
                }
                if to.is_none_or(|b| b>n) {
                    if let Some(start)=n.checked_add(1) { sides.push(Node::Is(other,Values::Range(Some(start),to))); }
                }
                if sides.len()>1 { grouped(true,sides) } else { tree(true,sides) }
            }
            values => Some(Node::Is(other,values)),
        },
        other => Some(other),
    }
}

fn value_condition(node: &Node, f: Field) -> bool {
    match node {
        Node::Is(other, Values::Any(_) | Values::Range(..)) => *other == f,
        Node::Group(inner) => value_condition(inner,f),
        Node::Any(nodes) => nodes.iter().all(|n| value_condition(n,f)),
        _ => false,
    }
}

fn value_terms(node: Node, terms: &mut Vec<Node>) {
    match node {
        Node::Group(inner) => value_terms(*inner,terms),
        Node::Any(nodes) => { for n in nodes { value_terms(n,terms); } }
        other => terms.push(other),
    }
}

fn set_value(nodes: &mut Vec<Node>, f: Field, v: String, state: ValueState) -> Option<()> {
    if !matches!(f, Field::Year | Field::Genre | Field::Label | Field::Composer | Field::Artist | Field::Album | Field::Rating | Field::Plays | Field::Type | Field::Quality) { return None; }
    let same = |s: &str| s.to_lowercase() == v.to_lowercase();
    let f = if f==Field::Genre && nodes.iter().any(|n| {
        let inner=if let Node::Not(inner)=n {inner.as_ref()} else {n};
        matches!(inner,Node::Is(Field::ExactGenre,Values::Any(values)) if values.iter().any(|s| same(s)))
    }) {Field::ExactGenre} else {f};
    let holds = |negated| nodes.iter().any(|n| {
        let inner=match (negated,n) { (true,Node::Not(inner))=>inner.as_ref(), (false,Node::Not(_))=>return false, (false,_)=>n, _=>return false };
        value_condition(inner,f) && super::state(inner,f,&v)==ValueState::Include
    });
    if state!=ValueState::None && holds(state==ValueState::Exclude) && !holds(state!=ValueState::Exclude) { return Some(()); }

    *nodes = nodes.drain(..).filter_map(|n| remove_value(n, f, &v, state)).collect();
    if state == ValueState::None { return Some(()); }
    let negated = state == ValueState::Exclude;
    for n in nodes.iter_mut() {
        if matches!(n, Node::Not(_)) != negated { continue; }
        let inner = match n { Node::Not(inner) => inner.as_mut(), n => n };
        if !value_condition(inner,f) { continue; }
        if let Node::Is(_,Values::Any(list)) = inner { list.push(v); }
        else {
            let extra = Node::Is(f,Values::Any(vec![v]));
            let mut terms = Vec::new();
            value_terms(std::mem::replace(inner,extra.clone()),&mut terms);
            terms.push(extra);
            *inner = Node::Group(Box::new(Node::Any(terms)));
        }
        return Some(());
    }
    let n = Node::Is(f, Values::Any(vec![v]));
    nodes.push(if negated { Node::Not(Box::new(n)) } else { n });
    Some(())
}

fn operator(nodes: &mut Vec<Node>, at: usize, to: &str) -> Option<()> {
    let (f, values) = leaf(&mut nodes[at])?;
    let f = *f; let values = values.clone();
    let new = match to {
        "include" | "exclude" => {
            if matches!(values, Values::Missing) { nodes.remove(at); return Some(()); }
            if !matches!(values, Values::Any(_)) || matches!(f, Field::Text | Field::Words | Field::Title) { return None; }
            let node = Node::Is(f, values);
            if to == "exclude" { Node::Not(Box::new(node)) } else { node }
        }
        "none" if matches!(f, Field::Year | Field::Genre | Field::ExactGenre | Field::Label | Field::Composer) => Node::Is(f, Values::Missing),
        "between" | "from" | "upTo" if matches!(f, Field::Year | Field::Plays) => {
            let (from, to_bound) = match values { Values::Range(a,b) => (a,b), _ => (None,None) };
            let (from, to_bound) = match to {
                "from" => (Some(from.unwrap_or(if f == Field::Year {1990} else {1})), None),
                "upTo" => (None, Some(to_bound.unwrap_or(if f == Field::Year {1969} else {10}))),
                _ => {
                    let from = from.unwrap_or(if f == Field::Year {1960} else {0});
                    let end = from.checked_add(9)?;
                    (Some(from), Some(to_bound.unwrap_or(if f==Field::Year {end.max(1969)} else {end})))
                }
            };
            Node::Is(f, Values::Range(from, to_bound))
        }
        "atLeast" | "exactly" | "ratingBetween" | "unrated" if f == Field::Rating => {
            let (min, _) = rating(&values)?;
            let (min,max) = match to {
                "atLeast" => (if min == 0 {4} else {min},5), "exactly" => { let n = if min == 0 {5} else {min}; (n,n) },
                "ratingBetween" => (2,4), _ => (0,0),
            };
            Node::Is(f, Values::Range(Some(min),Some(max)))
        }
        _ => return None,
    };
    nodes[at] = new; Some(())
}

fn apply(any: &mut bool, nodes: &mut Vec<Node>, edit: Edit) -> Option<()> {
    match edit {
        Edit::Remove {id} => at_row(nodes, &id, |ns,i| { ns.remove(i); Some(()) }),
        Edit::RemoveValue {id,value: v} => {
            let v = value(v)?;
            at_row(nodes, &id, |ns,i| {
                let (_, Values::Any(values)) = leaf(&mut ns[i])? else { return None };
                values.retain(|s| s.to_lowercase() != v.to_lowercase());
                if values.is_empty() { ns.remove(i); }
                Some(())
            })
        }
        Edit::Operator {id,to} => at_row(nodes, &id, |ns,i| operator(ns,i,&to)),
        Edit::Value {field:f,value:v,state,target} => {
            let f = field(&f)?; let v = value(v)?;
            in_target(nodes,target.as_deref(),|_,ns| set_value(ns,f,v,state),any)
        }
        Edit::Range {field:f,from,to,target} => {
            let f = field(&f).filter(|f| matches!(f,Field::Year | Field::Plays))?;
            if from.zip(to).is_some_and(|(a,b)| a>b) || (f == Field::Plays && [from,to].into_iter().flatten().any(|n| n<0)) { return None; }
            in_target(nodes,target.as_deref(),|_,ns| {
                replace(ns,f,|v| matches!(v,Values::Range(..) | Values::Missing), (from.is_some() || to.is_some()).then_some(Values::Range(from,to)));
                Some(())
            },any)
        }
        Edit::Rating {min,max,target} => {
            if !(0..=5).contains(&min) || !(min..=5).contains(&max) { return None; }
            in_target(nodes,target.as_deref(),|_,ns| { replace(ns,Field::Rating, |_| true,Some(Values::Range(Some(min),Some(max)))); Some(()) },any)
        }
        Edit::Added {n,unit,target} => {
            if n<0 { return None; }
            let days = match unit.as_str() { "d"=>1,"w"=>7,"m"=>30,"y"=>365,_=>return None };
            let seconds = n.checked_mul(days)?.checked_mul(86_400)?;
            in_target(nodes,target.as_deref(),|_,ns| { replace(ns,Field::Added, |_| true,(n>0).then_some(Values::Within(seconds))); Some(()) },any)
        }
        Edit::None {field:f,on,target} => {
            let f = field(&f).filter(|f| matches!(f,Field::Year | Field::Genre | Field::Label | Field::Composer))?;
            in_target(nodes,target.as_deref(),|_,ns| {
                if on { ns.retain_mut(|n| leaf(n).is_none_or(|(other,_)| field_name(*other) != field_name(f))); }
                replace(ns,f,|v| matches!(v,Values::Missing),on.then_some(Values::Missing)); Some(())
            },any)
        }
        Edit::Words {field:f,word,target} => {
            let f = field(&f).filter(|f| matches!(f,Field::Text | Field::Album | Field::Title))?;
            let f = if f==Field::Text {Field::Words} else {f};
            if word.trim().is_empty() { return None; }
            in_target(nodes,target.as_deref(),|_,ns| {
                for n in ns.iter_mut() {
                    if let Node::Is(other,Values::Any(values)) = n {
                        if *other == f || (f==Field::Words && *other==Field::Text) { *other=f; if !values.contains(&word) { values.push(word); } return Some(()); }
                    }
                }
                ns.push(Node::Is(f,Values::Any(vec![word]))); Some(())
            },any)
        }
        Edit::Mode {mode:m,target} => { let m = mode(&m)?; in_target(nodes,target.as_deref(),|any,_| { *any=m; Some(()) },any) }
        Edit::Group {ids} => {
            let mut ids = ids.iter().map(|s| index(s)).collect::<Option<Vec<_>>>()?;
            ids.sort_unstable(); ids.dedup(); let first = *ids.first()?;
            if ids.iter().any(|&i| nodes.get(i).is_none_or(|n| group(n).is_some())) { return None; }
            let children = ids.iter().map(|&i| nodes[i].clone()).collect();
            for i in ids.into_iter().rev() { nodes.remove(i); }
            nodes.insert(first,grouped(true,children)?); Some(())
        }
        Edit::Ungroup {id} => {
            let at = index(&id)?; let (_,children) = group(nodes.get(at)?)?;
            nodes.splice(at..=at,children); Some(())
        }
    }
}

/// Invalid edits and unsupported shapes leave the original query untouched.
pub fn refine_edit(text: &str, edit: &str) -> String {
    let Ok(edit) = serde_json::from_str::<Edit>(edit) else { return text.into() };
    let value_edit = matches!(edit, Edit::Value { .. });
    if read(text).is_err() || (!value_edit && !refine(text)["complex"].is_null()) { return text.into(); }
    let (mut any, mut nodes) = parts(read(text).ok().flatten());
    if apply(&mut any,&mut nodes,edit).is_none() { return text.into(); }
    let written = printed(any,nodes);
    if read(&written).is_err() || (!value_edit && !refine(&written)["complex"].is_null()) { return text.into(); }
    written
}

/// I preview words on valid queries even when their rows are too deep to edit.
pub(super) fn preview_words(text: &str, f: &str, word: &str) -> String {
    let (mut any,mut nodes)=parts(read(text).ok().flatten());
    let _=apply(&mut any,&mut nodes,Edit::Words {field:f.into(),word:word.into(),target:None});
    printed(any,nodes)
}
