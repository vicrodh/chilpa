//! I edit the top-level conditions without keeping a second query state.
use super::{field_label, join, read};
use chilpa_library::store::query::{Field, Node, Values};

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct FocusRow {
    pub field: String,
    pub label: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub range: Option<(Option<i64>, Option<i64>)>,
    pub within: Option<i64>,
    pub missing: bool,
    pub minimum: Option<i64>,
}

#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct FocusRows {
    pub any: bool,
    pub rows: Vec<FocusRow>,
    pub advanced: bool,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueState { Include, Exclude, None }

fn field(name: &str) -> Option<Field> {
    Some(match name {
        "year" => Field::Year, "genre" => Field::Genre, "label" => Field::Label,
        "composer" => Field::Composer, "artist" => Field::Artist, "album" => Field::Album,
        "rating" => Field::Rating, "added" => Field::Added, _ => return None,
    })
}

// I keep the legacy Focus view's flattened groups until its callers migrate.
fn split(text: &str) -> (bool, Vec<Node>) {
    match read(text).ok().flatten().map(|node| node.ungrouped()) {
        Some(Node::All(nodes)) => (false,nodes), Some(Node::Any(nodes)) => (true,nodes),
        Some(node) => (false,vec![node]), None => (false,vec![]),
    }
}

/// Groups or repeated conditions whose meaning a row would change stay as text.
pub fn focus_rows(text: &str) -> FocusRows {
    let (any, nodes) = split(text);
    let mut result = FocusRows { any, text: read(text).ok().flatten().map(|n| n.to_string()).unwrap_or_else(|| text.into()), ..Default::default() };
    let mut seen = std::collections::HashSet::new();
    for node in &nodes {
        let (negated, inner) = match node { Node::Not(inner) => (true, inner.as_ref()), node => (false, node) };
        let Node::Is(f, values) = inner else { result.advanced = true; break };
        if field(f.name()).is_none() { result.advanced = true; break; }
        let kind = match (f, values, negated) {
            (_, Values::Any(_), _) => if negated { "exclude" } else { "include" },
            (Field::Year, Values::Range(..), false) => "range",
            (Field::Added, Values::Within(_), false) => "within",
            (Field::Genre | Field::Composer | Field::Label | Field::Year, Values::Missing, false) => "missing",
            (Field::Rating, Values::Range(Some(n), None), false) if (0..=5).contains(n) => "minimum",
            _ => { result.advanced = true; break; }
        };
        if !seen.insert((*f, kind)) { result.advanced = true; break; }
        let at = result.rows.iter().position(|r| r.field == f.name()).unwrap_or_else(|| {
            result.rows.push(FocusRow { field: f.name().into(), label: field_label(*f), ..Default::default() });
            result.rows.len() - 1
        });
        let row = &mut result.rows[at];
        match values {
            Values::Any(list) => if negated { row.exclude = list.clone() } else { row.include = list.clone() },
            Values::Range(from, to) if *f == Field::Year => row.range = Some((*from, *to)),
            Values::Range(from, _) => row.minimum = *from,
            Values::Within(seconds) => row.within = Some(seconds / 86_400),
            Values::Missing => row.missing = true,
        }
    }
    result.advanced |= read(text).is_err();
    if result.advanced { result.rows.clear(); }
    result
}

fn edited(text: &str, edit: impl FnOnce(&mut Vec<Node>)) -> String {
    if focus_rows(text).advanced { return text.into(); }
    let (any, mut nodes) = split(text);
    edit(&mut nodes);
    let written = join(any, nodes);
    // I validate the printed edit too, including the grammar's size limit.
    read(&written).map(|node| node.map(|n| n.to_string()).unwrap_or_default()).unwrap_or_else(|_| text.into())
}

/// I move a value between the field's include and exclude lists, or remove it.
pub fn with_value(text: &str, name: &str, value: &str, state: ValueState) -> String {
    let Some(f) = field(name).filter(|f| *f != Field::Added) else { return text.into() };
    if value.trim().is_empty() { return text.into(); }
    edited(text, |nodes| {
        let holds = |wanted| nodes.iter().any(|node| {
            let inner = match (wanted, node) {
                (ValueState::Include, Node::Is(..)) => node,
                (ValueState::Exclude, Node::Not(inner)) => inner.as_ref(), _ => return false,
            };
            matches!(inner, Node::Is(other, Values::Any(list)) if *other == f && list.iter().any(|v| v.eq_ignore_ascii_case(value)))
        });
        let opposite = if state == ValueState::Include { ValueState::Exclude } else { ValueState::Include };
        if holds(state) && !holds(opposite) { return; }
        nodes.retain_mut(|node| {
            let inner = match node { Node::Not(inner) => inner.as_mut(), node => node };
            if let Node::Is(other, Values::Any(list)) = inner {
                if *other == f {
                    list.retain(|v| !v.eq_ignore_ascii_case(value));
                    return !list.is_empty();
                }
            }
            true
        });
        if state == ValueState::None { return; }
        let negated = state == ValueState::Exclude;
        for node in nodes.iter_mut() {
            let inner = match (negated, node) {
                (true, Node::Not(inner)) => inner.as_mut(), (false, node) => node, _ => continue,
            };
            if let Node::Is(other, Values::Any(list)) = inner {
                if *other == f { list.push(value.into()); return; }
            }
        }
        let node = Node::Is(f, Values::Any(vec![value.into()]));
        nodes.push(if negated { Node::Not(Box::new(node)) } else { node });
    })
}

fn replace(text: &str, f: Field, accepts: impl Fn(&Values) -> bool, value: Option<Values>) -> String {
    edited(text, |nodes| {
        let at = nodes.iter().position(|n| matches!(n, Node::Is(other, values) if *other == f && accepts(values))).unwrap_or(nodes.len());
        nodes.retain(|n| !matches!(n, Node::Is(other, values) if *other == f && accepts(values)));
        if let Some(value) = value { nodes.insert(at.min(nodes.len()), Node::Is(f, value)); }
    })
}

/// Two absent bounds remove the range.
pub fn with_range(text: &str, name: &str, from: Option<i64>, to: Option<i64>) -> String {
    let Some(f) = field(name).filter(|f| *f == Field::Year) else { return text.into() };
    if from.zip(to).is_some_and(|(a, b)| a > b) { return text.into(); }
    replace(text, f, |v| matches!(v, Values::Range(..)), (from.is_some() || to.is_some()).then_some(Values::Range(from, to)))
}

/// Zero days removes the span; a positive span counts back from now.
pub fn with_within(text: &str, name: &str, days: i64) -> String {
    if name != "added" || days < 0 { return text.into(); }
    let Some(seconds) = days.checked_mul(86_400) else { return text.into() };
    replace(text, Field::Added, |v| matches!(v, Values::Within(_)), (days > 0).then_some(Values::Within(seconds)))
}

pub fn with_missing(text: &str, name: &str, on: bool) -> String {
    let Some(f) = field(name).filter(|f| matches!(f, Field::Genre | Field::Composer | Field::Label | Field::Year)) else { return text.into() };
    replace(text, f, |v| matches!(v, Values::Missing), on.then_some(Values::Missing))
}

/// No minimum removes the condition; zero through five are valid ratings.
pub fn with_minimum(text: &str, name: &str, stars: Option<i64>) -> String {
    if name != "rating" || stars.is_some_and(|n| !(0..=5).contains(&n)) { return text.into(); }
    replace(text, Field::Rating, |v| matches!(v, Values::Range(..)), stars.map(|n| Values::Range(Some(n), None)))
}

#[cfg(test)]
mod tests;

/// I keep the other conditions when offering alternatives for this field.
pub(crate) fn facet_filter(filter: &crate::Filter, name: &str) -> crate::Filter {
    let mut out = filter.clone();
    out.query = out.query.map(|node| node.ungrouped());
    let Some(f) = field(name) else { return out };
    let keeps = |node: &Node| match node {
        Node::Is(other, _) => super::refine::field_name(*other) != super::refine::field_name(f),
        Node::Not(inner) => !matches!(inner.as_ref(), Node::Is(other, _) if super::refine::field_name(*other) == super::refine::field_name(f)),
        _ => true,
    };
    out.query = match out.query {
        Some(Node::All(mut nodes)) => {
            nodes.retain(keeps);
            if nodes.is_empty() { None } else { Some(Node::All(nodes)) }
        }
        Some(Node::Any(mut nodes)) => {
            // An alternative in this field can match outside every other branch.
            if nodes.iter().any(|node| !keeps(node)) { None } else { Some(Node::Any(std::mem::take(&mut nodes))) }
        }
        Some(node) if !keeps(&node) => None,
        other => other,
    };
    out
}
