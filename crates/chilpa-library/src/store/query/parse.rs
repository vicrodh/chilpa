//! The text a command line takes, read into a tree, and a tree written back
//! as that text.
//!
//! ```text
//! genre:jazz,soul -label:blue year:1955..1965 (composer:monk | artist:"bill evans") added:30d
//! ```
//!
//! Words side by side all have to hold; `|` or `OR` between them is enough
//! for either; `-` or `NOT` before one turns it around; parentheses group.
//! `field:a,b` holds either value, `field:none` holds nothing, a year or a
//! rating takes `from..to` with either end open, `added` takes a span back
//! from now (`30d`, `6w`, `3m`, `1y`). A word with no field is looked for in
//! the title, the artist and the record.

use super::{Field, Node, Values, DEEPEST, MOST_CONDITIONS};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Where in the text, in characters.
    pub at: usize,
    pub what: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at {})", self.what, self.at)
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Open,
    Close,
    Or,
    Not,
    /// A word as typed, with whether any of it was quoted.
    Word(String, bool),
}

fn tokens(text: &str) -> Result<Vec<(usize, Token)>, ParseError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c.is_whitespace() { at += 1; continue; }
        let start = at;
        match c {
            '(' => { out.push((start, Token::Open)); at += 1; continue; }
            ')' => { out.push((start, Token::Close)); at += 1; continue; }
            '|' => { out.push((start, Token::Or)); at += 1; continue; }
            // A minus that opens a word turns it around; one inside a word
            // ("hip-hop") is part of it.
            '-' if chars.get(at + 1).is_some_and(|n| !n.is_whitespace() && *n != ')') => {
                out.push((start, Token::Not));
                at += 1;
                continue;
            }
            _ => {}
        }
        let mut word = String::new();
        let mut quoted = false;
        while at < chars.len() {
            let c = chars[at];
            if c == '"' {
                quoted = true;
                at += 1;
                while at < chars.len() && chars[at] != '"' { word.push(chars[at]); at += 1; }
                if at == chars.len() { return Err(ParseError { at: start, what: chilpa_i18n::mark("a quote is not closed") }); }
                at += 1;
                continue;
            }
            if c.is_whitespace() || c == '(' || c == ')' || c == '|' { break; }
            word.push(c);
            at += 1;
        }
        let token = match word.as_str() {
            "OR" if !quoted => Token::Or,
            "AND" if !quoted => continue,
            "NOT" if !quoted => Token::Not,
            _ => Token::Word(word, quoted),
        };
        out.push((start, token));
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<(usize, Token)>,
    at: usize,
    end: usize,
    leaves: usize,
}

/// Reads a query. An empty text is the query that keeps everything.
pub fn parse(text: &str) -> Result<Node, ParseError> {
    let mut parser = Parser { tokens: tokens(text)?, at: 0, end: text.chars().count(), leaves: 0 };
    let node = parser.any(0)?;
    if let Some((at, _)) = parser.tokens.get(parser.at) {
        return Err(ParseError { at: *at, what: chilpa_i18n::mark("a closing parenthesis has nothing to close") });
    }
    Ok(simplify(node))
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|(_, t)| t)
    }

    fn here(&self) -> usize {
        self.tokens.get(self.at).map_or(self.end, |(at, _)| *at)
    }

    fn any(&mut self, depth: usize) -> Result<Node, ParseError> {
        let mut nodes = vec![self.all(depth)?];
        while self.peek() == Some(&Token::Or) {
            self.at += 1;
            nodes.push(self.all(depth)?);
        }
        Ok(if nodes.len() == 1 { nodes.pop().expect("one") } else { Node::Any(nodes) })
    }

    fn all(&mut self, depth: usize) -> Result<Node, ParseError> {
        let mut nodes = Vec::new();
        while let Some(token) = self.peek() {
            if matches!(token, Token::Or | Token::Close) { break; }
            nodes.push(self.unary(depth)?);
        }
        if nodes.is_empty() && self.peek() == Some(&Token::Or) {
            return Err(ParseError { at: self.here(), what: chilpa_i18n::mark("OR needs a condition on each side") });
        }
        Ok(if nodes.len() == 1 { nodes.pop().expect("one") } else { Node::All(nodes) })
    }

    fn unary(&mut self, depth: usize) -> Result<Node, ParseError> {
        if depth >= DEEPEST { return Err(ParseError { at: self.here(), what: chilpa_i18n::mark("the groups go too deep") }); }
        let at = self.here();
        match self.tokens.get(self.at).map(|(_, t)| t.clone()) {
            Some(Token::Not) => {
                self.at += 1;
                if matches!(self.peek(), None | Some(Token::Or | Token::Close)) {
                    return Err(ParseError { at, what: chilpa_i18n::mark("NOT needs a condition after it") });
                }
                Ok(Node::Not(Box::new(self.unary(depth + 1)?)))
            }
            Some(Token::Open) => {
                self.at += 1;
                let node = self.any(depth + 1)?;
                if self.peek() != Some(&Token::Close) { return Err(ParseError { at, what: chilpa_i18n::mark("a parenthesis is not closed") }); }
                self.at += 1;
                Ok(node)
            }
            Some(Token::Word(word, quoted)) => {
                self.at += 1;
                self.leaves += 1;
                if self.leaves > MOST_CONDITIONS { return Err(ParseError { at, what: chilpa_i18n::mark("the query has too many conditions") }); }
                leaf(&word, quoted, at)
            }
            _ => Err(ParseError { at, what: chilpa_i18n::mark("a condition is missing") }),
        }
    }
}

/// `field:values`, or words to look for.
fn leaf(word: &str, quoted: bool, at: usize) -> Result<Node, ParseError> {
    let field = word.split_once(':').and_then(|(name, rest)| {
        let name = name.to_lowercase();
        Field::NAMES.iter().find(|(known, _)| *known == name).map(|(_, field)| (*field, rest))
    });
    let Some((field, rest)) = field else {
        return Ok(Node::Is(Field::Text, Values::Any(vec![word.to_string()])));
    };
    if rest.is_empty() { return Err(ParseError { at, what: chilpa_i18n::mark("a field needs a value") }); }
    if !quoted && rest.eq_ignore_ascii_case("none") {
        return match field {
            Field::Year | Field::Genre | Field::Label | Field::Composer => Ok(Node::Is(field, Values::Missing)),
            _ => Err(ParseError { at, what: chilpa_i18n::mark("that field is never empty") }),
        };
    }
    match field {
        Field::Added => within(rest).map(|seconds| Node::Is(field, Values::Within(seconds)))
            .or_else(|| range(rest).map(|(from, to)| Node::Is(field, Values::Range(from, to))))
            .ok_or(ParseError { at, what: chilpa_i18n::mark("added takes a span like 30d, 6w, 3m or 1y") }),
        Field::Year | Field::Rating => {
            if let Some((from, to)) = range(rest) { return Ok(Node::Is(field, Values::Range(from, to))); }
            let numbers: Vec<String> = rest.split(',').map(str::trim).filter(|v| !v.is_empty()).map(String::from).collect();
            if numbers.is_empty() || numbers.iter().any(|n| n.parse::<i64>().is_err()) {
                return Err(ParseError { at, what: chilpa_i18n::mark("a year or a rating is a number") });
            }
            if field == Field::Rating && numbers.iter().any(|n| !(0..=5).contains(&n.parse::<i64>().unwrap_or(-1))) {
                return Err(ParseError { at, what: chilpa_i18n::mark("a rating goes from 0 to 5") });
            }
            Ok(Node::Is(field, Values::Any(numbers)))
        }
        Field::Album | Field::Title => Ok(Node::Is(field, Values::Any(rest.split_whitespace().map(String::from).collect()))),
        _ => {
            let names: Vec<String> = if quoted && !rest.contains(',') { vec![rest.to_string()] }
                else { rest.split(',').map(str::trim).filter(|v| !v.is_empty()).map(String::from).collect() };
            if names.is_empty() { return Err(ParseError { at, what: chilpa_i18n::mark("a field needs a value") }); }
            Ok(Node::Is(field, Values::Any(names)))
        }
    }
}

/// `1990..1999`, `..1999`, `1990..`.
fn range(text: &str) -> Option<(Option<i64>, Option<i64>)> {
    let (from, to) = text.split_once("..")?;
    let bound = |s: &str| if s.trim().is_empty() { Some(None) } else { s.trim().parse().ok().map(Some) };
    let (from, to) = (bound(from)?, bound(to)?);
    (from.is_some() || to.is_some()).then_some((from, to))
}

/// `30d`, `6w`, `3m`, `1y`: seconds back from now.
fn within(text: &str) -> Option<i64> {
    let unit = text.chars().last()?;
    let count: i64 = text[..text.len() - unit.len_utf8()].parse().ok().filter(|n| *n > 0)?;
    let day = 86_400;
    Some(count * match unit { 'd' => day, 'w' => 7 * day, 'm' => 30 * day, 'y' => 365 * day, _ => return None })
}

/// One level of a group holds no group of the same kind, and a group of
/// one is that one.
fn simplify(node: Node) -> Node {
    match node {
        Node::All(nodes) => {
            let mut flat = Vec::new();
            for node in nodes.into_iter().map(simplify) {
                match node { Node::All(inner) => flat.extend(inner), other => flat.push(other) }
            }
            if flat.len() == 1 { flat.pop().expect("one") } else { Node::All(flat) }
        }
        Node::Any(nodes) => {
            let mut flat = Vec::new();
            for node in nodes.into_iter().map(simplify) {
                match node { Node::Any(inner) => flat.extend(inner), other => flat.push(other) }
            }
            if flat.len() == 1 { flat.pop().expect("one") } else { Node::Any(flat) }
        }
        Node::Not(inner) => match simplify(*inner) { Node::Not(again) => *again, other => Node::Not(Box::new(other)) },
        leaf => leaf,
    }
}

/// A value as the text would take it back: quoted when it holds a space, a
/// comma or a parenthesis.
fn written(value: &str) -> String {
    if value.is_empty() || value.chars().any(|c| c.is_whitespace() || matches!(c, ',' | '(' | ')' | '|' | '"' | ':')) {
        format!("\"{}\"", value.replace('"', ""))
    } else {
        value.to_string()
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn grouped(node: &Node, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match node {
                Node::All(nodes) | Node::Any(nodes) if nodes.len() > 1 => write!(f, "({node})"),
                _ => write!(f, "{node}"),
            }
        }
        match self {
            Node::All(nodes) => {
                for (at, node) in nodes.iter().enumerate() {
                    if at > 0 { write!(f, " ")?; }
                    match node { Node::Any(_) => grouped(node, f)?, _ => write!(f, "{node}")? }
                }
                Ok(())
            }
            Node::Any(nodes) => {
                for (at, node) in nodes.iter().enumerate() {
                    if at > 0 { write!(f, " | ")?; }
                    match node { Node::All(_) => grouped(node, f)?, _ => write!(f, "{node}")? }
                }
                Ok(())
            }
            Node::Not(node) => { write!(f, "-")?; grouped(node, f) }
            Node::Is(Field::Text, Values::Any(words)) => write!(f, "{}", words.iter().map(|w| written(w)).collect::<Vec<_>>().join(" ")),
            Node::Is(field, values) => {
                let name = field.name();
                match values {
                    Values::Any(list) if matches!(field, Field::Album | Field::Title) => write!(f, "{name}:{}", written(&list.join(" "))),
                    Values::Any(list) => write!(f, "{name}:{}", list.iter().map(|v| written(v)).collect::<Vec<_>>().join(",")),
                    Values::Range(from, to) => write!(f, "{name}:{}..{}", from.map(|n| n.to_string()).unwrap_or_default(), to.map(|n| n.to_string()).unwrap_or_default()),
                    Values::Within(seconds) => {
                        let day = 86_400;
                        let (count, unit) = if seconds % (365 * day) == 0 { (seconds / (365 * day), 'y') }
                            else if seconds % (30 * day) == 0 { (seconds / (30 * day), 'm') }
                            else if seconds % (7 * day) == 0 { (seconds / (7 * day), 'w') } else { (seconds / day, 'd') };
                        write!(f, "{name}:{count}{unit}")
                    }
                    Values::Missing => write!(f, "{name}:none"),
                }
            }
        }
    }
}
