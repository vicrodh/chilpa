//! The track formats of the status lines, as Fooyin writes them.
//!
//! Fooyin's script language (`src/core/scripting`), the part a status line
//! uses: `%field%`, `$function(argument,…)`, `[…]` that shows only when a
//! field inside it has a value, `"quoted"` text and `\` before a character
//! that would mean something else. Each piece answers a text and whether it
//! "holds": a field holds when it has a value, plain text always holds, and
//! `[…]`, `$if` and their kin decide on that, as in Fooyin's runtime
//! (`scriptruntime.cpp`, `evalConditional` and `evalFunctionArg`).
//!
//! A format that cannot be read -- a `%`, `)`, `]` or `"` left open, or a
//! function Fooyin does not have here -- renders as nothing, as an invalid
//! script does in Fooyin.

#[derive(Clone, Debug, PartialEq)]
enum Piece {
    Text(String),
    Field(String),
    Optional(Vec<Piece>),
    Call(&'static str, Vec<Vec<Piece>>),
}

/// A format, read once and rendered for each track.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Format(Vec<Piece>);

/// Why a format cannot be read, and at which character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Unreadable {
    pub at: usize,
}

/// The functions there are, by the name a format calls them.
const FUNCTIONS: &[&str] = &[
    "if", "if2", "if3", "and", "or", "not", "xor", "ifequal", "ifgreater", "iflonger", "upper", "lower", "caps", "trim",
    "left", "right", "num", "replace", "len", "abbr",
];

/// What stops a run of plain text where it is read.
#[derive(Clone, Copy, PartialEq)]
enum Inside {
    Top,
    Optional,
    Argument,
}

struct Reader {
    chars: Vec<char>,
    at: usize,
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn pieces(&mut self, inside: Inside) -> Result<Vec<Piece>, Unreadable> {
        let mut pieces = Vec::new();
        let mut text = String::new();
        while let Some(c) = self.peek() {
            let ends = match inside {
                Inside::Top => false,
                Inside::Optional => c == ']',
                Inside::Argument => c == ',' || c == ')',
            };
            if ends {
                break;
            }
            self.at += 1;
            let piece = match c {
                // Layout whitespace is not text (Fooyin's `IgnoreLayout`).
                '\n' | '\r' | '\t' => continue,
                '\\' => {
                    if let Some(next) = self.peek() {
                        self.at += 1;
                        text.push(next);
                    }
                    continue;
                }
                '"' => Piece::Text(self.quoted()?),
                '%' => Piece::Field(self.field()?),
                '[' => Piece::Optional(self.optional()?),
                '$' => self.call()?,
                other => {
                    text.push(other);
                    continue;
                }
            };
            if !text.is_empty() {
                pieces.push(Piece::Text(std::mem::take(&mut text)));
            }
            pieces.push(piece);
        }
        if !text.is_empty() {
            pieces.push(Piece::Text(text));
        }
        Ok(pieces)
    }

    fn quoted(&mut self) -> Result<String, Unreadable> {
        let start = self.at - 1;
        let mut text = String::new();
        loop {
            match self.peek() {
                None => return Err(Unreadable { at: start }),
                Some('"') => {
                    self.at += 1;
                    return Ok(text);
                }
                Some('\\') => {
                    self.at += 1;
                    match self.peek() {
                        Some(next @ ('"' | '\\')) => {
                            self.at += 1;
                            text.push(next);
                        }
                        _ => text.push('\\'),
                    }
                }
                Some(other) => {
                    self.at += 1;
                    text.push(other);
                }
            }
        }
    }

    fn field(&mut self) -> Result<String, Unreadable> {
        let start = self.at - 1;
        let mut name = String::new();
        loop {
            match self.peek() {
                None => return Err(Unreadable { at: start }),
                Some('%') => {
                    self.at += 1;
                    return Ok(name.to_uppercase());
                }
                Some(other) => {
                    self.at += 1;
                    name.push(other);
                }
            }
        }
    }

    fn optional(&mut self) -> Result<Vec<Piece>, Unreadable> {
        let start = self.at - 1;
        let pieces = self.pieces(Inside::Optional)?;
        if self.peek() != Some(']') {
            return Err(Unreadable { at: start });
        }
        self.at += 1;
        Ok(pieces)
    }

    fn call(&mut self) -> Result<Piece, Unreadable> {
        let start = self.at - 1;
        let mut name = String::new();
        while let Some(c) = self.peek().filter(|c| c.is_alphanumeric() || *c == '_') {
            self.at += 1;
            name.push(c);
        }
        let name = name.to_lowercase();
        let known = *FUNCTIONS.iter().find(|known| **known == name).ok_or(Unreadable { at: start })?;
        if self.peek() != Some('(') {
            return Err(Unreadable { at: start });
        }
        self.at += 1;
        let mut arguments = Vec::new();
        if self.peek() == Some(')') {
            self.at += 1;
            return Ok(Piece::Call(known, arguments));
        }
        loop {
            arguments.push(self.pieces(Inside::Argument)?);
            match self.peek() {
                Some(',') => self.at += 1,
                Some(')') => {
                    self.at += 1;
                    return Ok(Piece::Call(known, arguments));
                }
                _ => return Err(Unreadable { at: start }),
            }
        }
    }
}

/// What a piece comes to: its text, and whether it holds.
#[derive(Clone, Debug, Default, PartialEq)]
struct Value {
    text: String,
    holds: bool,
}

fn said(text: String) -> Value {
    let holds = !text.is_empty();
    Value { text, holds }
}

fn plain(piece: &Piece) -> bool {
    matches!(piece, Piece::Text(_))
}

impl Format {
    pub(crate) fn parse(text: &str) -> Result<Format, Unreadable> {
        let mut reader = Reader { chars: text.chars().collect(), at: 0 };
        reader.pieces(Inside::Top).map(Format)
    }

    /// The format for a track, its fields given by their upper-case names.
    /// An unknown field has no value.
    pub(crate) fn render(&self, fields: &dyn Fn(&str) -> String) -> String {
        self.0.iter().map(|piece| value(piece, fields).text).collect()
    }
}

fn value(piece: &Piece, fields: &dyn Fn(&str) -> String) -> Value {
    match piece {
        Piece::Text(text) => Value { text: text.clone(), holds: true },
        Piece::Field(name) => said(fields(name)),
        Piece::Optional(pieces) => {
            // One piece of plain text in brackets has nothing to decide on.
            if pieces.len() == 1 && plain(&pieces[0]) {
                return Value::default();
            }
            let joined = joined(pieces, fields);
            if joined.holds { joined } else { Value::default() }
        }
        Piece::Call(name, arguments) => {
            let arguments: Vec<Value> = arguments.iter().map(|argument| joined(argument, fields)).collect();
            call(name, &arguments)
        }
    }
}

/// Pieces side by side: their texts, holding when one that is not plain
/// text holds.
fn joined(pieces: &[Piece], fields: &dyn Fn(&str) -> String) -> Value {
    if let [only] = pieces {
        return value(only, fields);
    }
    let mut out = Value::default();
    for piece in pieces {
        let part = value(piece, fields);
        out.holds |= !plain(piece) && part.holds;
        out.text.push_str(&part.text);
    }
    out
}

fn number(text: &str) -> f64 {
    text.trim().parse().unwrap_or(0.0)
}

fn whole(text: &str) -> Option<i64> {
    text.trim().parse().ok()
}

fn decided(holds: bool) -> Value {
    Value { text: String::new(), holds }
}

fn call(name: &str, args: &[Value]) -> Value {
    let first = || args.first().map(|arg| arg.text.as_str()).unwrap_or_default();
    match (name, args.len()) {
        ("and", 1..) => decided(args.iter().all(|arg| arg.holds)),
        ("or", 1..) => decided(args.iter().any(|arg| arg.holds)),
        ("not", 1) => decided(!args[0].holds),
        ("xor", 1..) => decided(args.iter().filter(|arg| arg.holds).count() % 2 == 1),
        ("if", 2 | 3) => if args[0].holds { args[1].clone() } else { args.get(2).cloned().unwrap_or_default() },
        ("if2", 1 | 2) => if args[0].holds { args[0].clone() } else { args.get(1).cloned().unwrap_or_default() },
        ("if3", 2..) => args[..args.len() - 1].iter().find(|arg| arg.holds).unwrap_or(&args[args.len() - 1]).clone(),
        ("ifequal", 4) => if number(&args[0].text) == number(&args[1].text) { args[2].clone() } else { args[3].clone() },
        ("ifgreater", 3 | 4) => {
            if number(&args[0].text) > number(&args[1].text) { args[2].clone() } else { args.get(3).cloned().unwrap_or_default() }
        }
        ("iflonger", 4) => match whole(&args[1].text) {
            Some(length) if args[0].text.chars().count() as i64 > length => args[2].clone(),
            Some(_) => args[3].clone(),
            None => Value::default(),
        },
        ("upper", 1..) => said(first().to_uppercase()),
        ("lower", 1..) => said(first().to_lowercase()),
        ("caps", 1..) => said(title_cased(first())),
        ("trim", 1..) => said(first().trim().to_string()),
        ("len", 1..) => said(first().chars().count().to_string()),
        ("left", 2) => match whole(&args[1].text) {
            Some(count) if count >= 0 => said(first().chars().take(count as usize).collect()),
            Some(_) => said(first().to_string()),
            None => Value::default(),
        },
        ("right", 2) => {
            let length = first().chars().count();
            match whole(&args[1].text) {
                Some(count) if count >= 0 && count as usize <= length => said(first().chars().skip(length - count as usize).collect()),
                _ => Value::default(),
            }
        }
        ("num", 1 | 2) => match (whole(first()), args.get(1).and_then(|width| whole(&width.text))) {
            (Some(value), Some(width)) if args.len() == 2 => said(format!("{value:0width$}", width = width.max(0) as usize)),
            _ => said(first().to_string()),
        },
        ("replace", 1..) => said(replaced(first(), &args[1..])),
        ("abbr", 1 | 2) => said(abbreviated(first(), args.get(1).and_then(|limit| whole(&limit.text)), args.len())),
        _ => Value::default(),
    }
}

/// The first letter of each word up, the rest down.
fn title_cased(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut starts = true;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if starts { out.extend(c.to_uppercase()) } else { out.extend(c.to_lowercase()) }
            starts = false;
        } else {
            out.push(c);
            starts = c != '\'';
        }
    }
    out
}

/// Every pair of what to find and what to put, left to right. Two that
/// start at the same place: the later pair wins, as in Fooyin.
fn replaced(text: &str, pairs: &[Value]) -> String {
    if pairs.len() < 2 {
        return text.to_string();
    }
    let mut found: std::collections::BTreeMap<usize, (usize, &str)> = std::collections::BTreeMap::new();
    for pair in pairs.chunks_exact(2) {
        let (find, put) = (pair[0].text.as_str(), pair[1].text.as_str());
        if find.is_empty() {
            continue;
        }
        for (at, _) in text.match_indices(find) {
            found.insert(at, (find.len(), put));
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (at, (length, put)) in found {
        if at < cursor {
            continue;
        }
        out.push_str(&text[cursor..at]);
        out.push_str(put);
        cursor = at + length;
    }
    out.push_str(&text[cursor..]);
    out
}

/// The first character of each word, unless the text already fits.
fn abbreviated(text: &str, limit: Option<i64>, count: usize) -> String {
    if count == 2 && limit.is_some_and(|limit| text.chars().count() as i64 <= limit) {
        return text.to_string();
    }
    text.replace(['(', ')'], " ").split_whitespace().filter_map(|word| word.chars().next()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(name: &str) -> String {
        match name {
            "TITLE" => "Time".into(),
            "ARTIST" => "Pink Floyd".into(),
            "ALBUM" => "The Dark Side of the Moon".into(),
            "YEAR" => "1973".into(),
            "TRACK" => "4".into(),
            _ => String::new(),
        }
    }

    fn shown(format: &str) -> String {
        Format::parse(format).expect(format).render(&track)
    }

    #[test]
    fn the_default_formats_say_the_field() {
        assert_eq!(shown("%title%"), "Time");
        assert_eq!(shown("%artist%"), "Pink Floyd");
        assert_eq!(shown("%album%"), "The Dark Side of the Moon");
        assert_eq!(shown("%Album%"), "The Dark Side of the Moon", "field names are not case sensitive");
    }

    #[test]
    fn brackets_show_only_when_a_field_inside_has_a_value() {
        assert_eq!(shown("%album%[ (%year%)]"), "The Dark Side of the Moon (1973)");
        assert_eq!(shown("%album%[ (%date%)]"), "The Dark Side of the Moon");
        assert_eq!(shown("[plain text]"), "", "nothing to decide on");
        assert_eq!(shown("[%genre% - %year%]"), " - 1973", "one field with a value is enough");
        assert_eq!(shown("[%genre%[ / %composer%]]"), "");
    }

    #[test]
    fn quotes_and_backslashes_keep_what_would_mean_something_else() {
        assert_eq!(shown("\"%title%\" %title%"), "%title% Time");
        assert_eq!(shown("\\%title\\%"), "%title%");
        assert_eq!(shown("\"say \\\"hi\\\" \\\\ \\x\""), "say \"hi\" \\ \\x");
        assert_eq!(shown("a, (b) ]"), "a, (b) ]", "outside a call these are text");
        assert_eq!(shown("line\none\ttab"), "lineonetab", "layout whitespace is not text");
    }

    #[test]
    fn conditions_decide_on_whether_a_field_has_a_value() {
        assert_eq!(shown("$if(%year%,%year%,unknown)"), "1973");
        assert_eq!(shown("$if(%date%,%date%,unknown)"), "unknown");
        assert_eq!(shown("$if(%date%,%date%)"), "");
        assert_eq!(shown("$if2(%date%,%year%)"), "1973");
        assert_eq!(shown("$if3(%date%,%genre%,%year%,none)"), "1973");
        assert_eq!(shown("$if3(%date%,%genre%,none)"), "none");
        assert_eq!(shown("$if($and(%title%,%year%),both)"), "both");
        assert_eq!(shown("$if($or(%genre%,%date%),one,neither)"), "neither");
        assert_eq!(shown("$if($not(%genre%),no genre)"), "no genre");
        assert_eq!(shown("$if($xor(%title%,%year%),odd,even)"), "even");
        assert_eq!(shown("$ifequal(%track%,4,fourth,other)"), "fourth");
        assert_eq!(shown("$ifgreater(%year%,1980,new,old)"), "old");
        assert_eq!(shown("$iflonger(%title%,3,long,short)"), "long");
    }

    #[test]
    fn text_functions_cut_pad_and_replace() {
        assert_eq!(shown("$upper(%title%)"), "TIME");
        assert_eq!(shown("$lower(%artist%)"), "pink floyd");
        assert_eq!(shown("$caps(tHE dARK side)"), "The Dark Side");
        assert_eq!(shown("$trim(  x  )"), "x");
        assert_eq!(shown("$len(%title%)"), "4");
        assert_eq!(shown("$left(%album%,3)"), "The");
        assert_eq!(shown("$right(%album%,4)"), "Moon");
        assert_eq!(shown("$right(%title%,9)"), "", "more than there is gives nothing in Fooyin");
        assert_eq!(shown("$num(%track%,2)"), "04");
        assert_eq!(shown("$num(x,2)"), "x");
        assert_eq!(shown("$replace(%album%,The ,)"), "Dark Side of the Moon", "case sensitive");
        assert_eq!(shown("$replace(abcabc,a,1,b,2)"), "12c12c");
        assert_eq!(shown("$abbr(%album%)"), "TDSotM");
        assert_eq!(shown("$abbr(%title%,10)"), "Time");
    }

    #[test]
    fn a_function_returns_whether_its_text_holds() {
        assert_eq!(shown("[$upper(%genre%) ]%title%"), "Time");
        assert_eq!(shown("[$upper(%artist%) - ]%title%"), "PINK FLOYD - Time");
    }

    #[test]
    fn a_format_left_open_or_calling_what_is_not_there_cannot_be_read() {
        for broken in ["%title", "$if(%title%,x", "[%title%", "\"open", "$nothing(x)", "$if", "$"] {
            assert!(Format::parse(broken).is_err(), "{broken}");
        }
        assert_eq!(Format::parse("ab%title").unwrap_err(), Unreadable { at: 2 });
        assert_eq!(shown(""), "");
        assert_eq!(shown("$if()"), "", "a call without what it needs is nothing");
    }
}
