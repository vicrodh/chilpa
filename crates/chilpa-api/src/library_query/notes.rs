//! I report the same name resolution used when a query is applied.
use super::*;

#[derive(Clone, Debug, serde::Serialize)]
pub struct EngineNote {
    pub from: String,
    pub field: String,
    pub to: String,
    pub kind: &'static str,
    #[serde(skip)]
    pub(crate) literal: Literal,
}

#[derive(Clone, Debug)]
pub(crate) enum Literal { Words(Vec<Vec<usize>>), Genre(Vec<usize>, usize) }

fn child(path: &[usize], at: usize) -> Vec<usize> { let mut p = path.to_vec(); p.push(at); p }

fn name_notes(words: &[&Node], resolved: &Node, paths: Vec<Vec<usize>>, notes: &mut Vec<EngineNote>) {
    let from = words.iter().flat_map(|n| match n { Node::Is(_, Values::Any(v)) => v.clone(), _ => vec![] }).collect::<Vec<_>>().join(" ");
    let nodes = match resolved { Node::Any(nodes) => nodes.as_slice(), node => std::slice::from_ref(node) };
    for node in nodes {
        if let Node::Is(field, Values::Any(values)) = node {
            notes.push(EngineNote { from: from.clone(), field: field.name().into(), to: values.join(", "), kind: "name", literal: Literal::Words(paths.clone()) });
        }
    }
}

fn all_words<'a>(node: &'a Node, path: &[usize], out: &mut Vec<(&'a Node,Vec<usize>)>) {
    match node {
        Node::Is(Field::Text,Values::Any(_)) => out.push((node,path.to_vec())),
        Node::All(nodes) => for (i,n) in nodes.iter().enumerate() { all_words(n,&child(path,i),out); },
        Node::Group(inner) => all_words(inner,&child(path,0),out),
        _ => {}
    }
}

fn resolve_rest(node: &Node, known: &[(Field,Vec<String>)], notes: &mut Vec<EngineNote>, path: &[usize]) -> Option<Node> {
    match node {
        Node::Is(Field::Text,Values::Any(_)) => None,
        Node::All(nodes) => {
            let nodes: Vec<_>=nodes.iter().enumerate().filter_map(|(i,n)| resolve_rest(n,known,notes,&child(path,i))).collect();
            (!nodes.is_empty()).then_some(Node::All(nodes))
        }
        Node::Group(inner) => resolve_rest(inner,known,notes,&child(path,0)).map(|n| Node::Group(Box::new(n))),
        other => Some(resolve(other,known,notes,path)),
    }
}

pub(super) fn resolve(node: &Node, known: &[(Field, Vec<String>)], notes: &mut Vec<EngineNote>, path: &[usize]) -> Node {
    match node {
        Node::All(nodes) => {
            let mut gathered=Vec::new(); all_words(node,path,&mut gathered);
            let words: Vec<_>=gathered.iter().map(|(n,_)| *n).collect();
            match named(&words,known) {
                Some(name) => {
                    name_notes(&words,&name,gathered.iter().map(|(_,p)| p.clone()).collect(),notes);
                    Node::All(nodes.iter().enumerate().filter_map(|(i,n)| resolve_rest(n,known,notes,&child(path,i)))
                        .chain([name]).collect())
                }
                None => Node::All(nodes.iter().enumerate().map(|(i,n)| resolve(n,known,notes,&child(path,i))).collect()),
            }
        }
        Node::Any(nodes) => Node::Any(nodes.iter().enumerate().map(|(i,n)| resolve(n,known,notes,&child(path,i))).collect()),
        Node::Group(inner) => Node::Group(Box::new(resolve(inner,known,notes,&child(path,0)))),
        Node::Not(inner) => Node::Not(Box::new(resolve(inner,known,notes,&child(path,0)))),
        leaf @ Node::Is(Field::Text, Values::Any(_)) => {
            if let Some(name) = named(&[leaf],known) { name_notes(&[leaf],&name,vec![path.to_vec()],notes); name } else { leaf.clone() }
        }
        other => other.clone(),
    }
}

pub(super) fn widen(node: &Node, known: &[String], aliases: &Aliases, notes: &mut Vec<EngineNote>, path: &[usize]) -> Node {
    match node {
        Node::All(nodes) => Node::All(nodes.iter().enumerate().map(|(i,n)| widen(n,known,aliases,notes,&child(path,i))).collect()),
        Node::Any(nodes) => Node::Any(nodes.iter().enumerate().map(|(i,n)| widen(n,known,aliases,notes,&child(path,i))).collect()),
        Node::Group(inner) => Node::Group(Box::new(widen(inner,known,aliases,notes,&child(path,0)))),
        Node::Not(inner) => Node::Not(Box::new(widen(inner,known,aliases,notes,&child(path,0)))),
        Node::Is(Field::Genre, Values::Any(names)) => {
            let mut widened = names.clone();
            for (i,name) in names.iter().enumerate().filter(|(_,name)| !known.iter().any(|k| k.eq_ignore_ascii_case(name))) {
                let typed = genre_key(name); let same = aliases.names_of(&typed);
                let key: Vec<char> = typed.chars().collect(); let most = if key.len()>6 {2} else {1};
                for candidate in known {
                    let theirs = genre_key(candidate); let other: Vec<char> = theirs.chars().collect();
                    let close = key.len()>=3 && other.len()>=3 && distance(&key,&other,most)<=most;
                    if (same.contains(&theirs.as_str()) || close) && !widened.contains(candidate) {
                        notes.push(EngineNote { from: name.clone(), field:"genre".into(), to:candidate.clone(),
                            kind:if theirs != typed && same.contains(&theirs.as_str()) {"alias"} else {"near"}, literal:Literal::Genre(path.to_vec(),i) });
                        widened.push(candidate.clone());
                    }
                }
            }
            Node::Is(Field::Genre,Values::Any(widened))
        }
        other => other.clone(),
    }
}

pub(crate) fn reported(node: &Node, known: &[(Field, Vec<String>)], genres: &[String]) -> Vec<EngineNote> {
    let mut notes = Vec::new();
    resolve(node,known,&mut notes,&[]);
    // Resolved names already exist verbatim; only typed genre leaves widen.
    widen(node,genres,bundled(),&mut notes,&[]);
    notes
}

fn at_mut<'a>(node: &'a mut Node, path: &[usize]) -> Option<&'a mut Node> {
    let Some((&first,rest)) = path.split_first() else { return Some(node) };
    match node {
        Node::All(nodes) | Node::Any(nodes) => at_mut(nodes.get_mut(first)?,rest),
        Node::Not(inner) | Node::Group(inner) if first==0 => at_mut(inner,rest), _ => None,
    }
}

pub(crate) fn literal(text: &str, at: usize, known: &[(Field,Vec<String>)], genres: &[String]) -> String {
    let Ok(Some(mut node)) = read(text) else { return text.into() };
    let Some(note) = reported(&node,known,genres).get(at).cloned() else { return text.into() };
    match note.literal {
        Literal::Words(paths) => for path in paths {
            if let Some(Node::Is(field,_)) = at_mut(&mut node,&path) { *field=Field::Words; }
        },
        Literal::Genre(path,i) => {
            let Some(leaf) = at_mut(&mut node,&path) else { return text.into() };
            let Node::Is(Field::Genre,Values::Any(values)) = leaf else { return text.into() };
            if i>=values.len() { return text.into(); }
            let value = values.remove(i);
            let exact = Node::Is(Field::ExactGenre,Values::Any(vec![value]));
            *leaf = if values.is_empty() { exact } else { Node::Group(Box::new(Node::Any(vec![Node::Is(Field::Genre,Values::Any(values.clone())),exact]))) };
        }
    }
    literal_words(node).to_string()
}

fn literal_words(node: Node) -> Node {
    match node {
        Node::All(nodes) => {
            let mut out: Vec<Node>=Vec::new(); let mut first=None;
            for node in nodes.into_iter().map(literal_words) {
                if let Node::Is(Field::Words,Values::Any(values))=&node {
                    if let Some(at)=first {
                        if let Node::Is(_,Values::Any(held))=&mut out[at] { held.extend(values.iter().cloned()); continue; }
                    } else { first=Some(out.len()); }
                }
                out.push(node);
            }
            if out.len()==1 { out.pop().expect("one") } else {Node::All(out)}
        }
        Node::Any(nodes) => Node::Any(nodes.into_iter().map(literal_words).collect()),
        Node::Group(inner) => Node::Group(Box::new(literal_words(*inner))),
        Node::Not(inner) => Node::Not(Box::new(literal_words(*inner))),
        other => other,
    }
}
