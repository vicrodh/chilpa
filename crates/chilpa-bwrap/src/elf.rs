//! What a process plugin's program needs from the system it runs on, read
//! from its file and never run: the loader or script interpreter it names,
//! the libraries it links and the symbol versions it asks of each. The
//! Flatpak backend checks them against the runtime its sandbox shares.
//!
//! Only the program itself is read. What it opens later by name, as a
//! bundled interpreter loads its own libraries, is not seen here.
//!
//! It runs in Chilpa's process before any sandbox bounds the plugin, so
//! what one file can make it hold is bounded: each table read, and all the
//! names copied out of one file together. A library is looked for only
//! where the plugin's sandbox sees it: the package, `/app` and `/usr`.

use crate::Incompatible;
use std::collections::HashMap;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

/// The most read for any one table. A program's tables are kilobytes; a
/// larger one means the file is not what its header says.
const MAX_TABLE: u64 = 4 << 20;
/// Names and version entries walked in one file, all tables together, for
/// the same reason.
const MAX_ENTRIES: usize = 4096;
/// Bytes of names copied out of one file, all of them together.
const MAX_NAMES: usize = 1 << 20;

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const DT_NEEDED: u64 = 1;
const DT_STRTAB: u64 = 5;
const DT_STRSZ: u64 = 10;
const DT_RPATH: u64 = 15;
const DT_RUNPATH: u64 = 29;
const DT_VERDEF: u64 = 0x6fff_fffc;
const DT_VERDEFNUM: u64 = 0x6fff_fffd;
const DT_VERNEED: u64 = 0x6fff_fffe;
const DT_VERNEEDNUM: u64 = 0x6fff_ffff;

/// The ELF machine of this processor, and its folder name in the runtime.
fn this_machine() -> Option<(u16, &'static str)> {
    match std::env::consts::ARCH {
        "x86_64" => Some((62, "x86_64-linux-gnu")),
        "aarch64" => Some((183, "aarch64-linux-gnu")),
        _ => None,
    }
}

/// A dynamically linked program, as its file describes it.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Program {
    pub(crate) interpreter: Option<String>,
    pub(crate) needed: Vec<String>,
    pub(crate) runpath: Vec<String>,
    /// Each library it asks versions of, with those versions.
    pub(crate) versions: Vec<(String, Vec<String>)>,
    /// The versions it defines itself, when it is a library.
    pub(crate) defines: Vec<String>,
}

impl Program {
    /// The newest glibc symbol version it asks for, such as `GLIBC_2.38`.
    pub(crate) fn newest_glibc(&self) -> Option<&str> {
        self.versions.iter().flat_map(|(_, versions)| versions).map(String::as_str)
            .filter(|version| family(version) == "GLIBC_").max_by(|a, b| number(a).cmp(&number(b)))
    }
}

/// What the file starts with: a program for this processor, or a script.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Program(Program),
    Script { interpreter: String, argument: Option<String> },
}

struct Reader {
    file: File,
    len: u64,
}

impl Reader {
    fn bytes(&self, offset: u64, size: u64) -> Option<Vec<u8>> {
        if size > MAX_TABLE || offset.checked_add(size)? > self.len { return None; }
        let mut buffer = vec![0; size as usize];
        self.file.read_exact_at(&mut buffer, offset).ok()?;
        Some(buffer)
    }
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at.checked_add(8)?)?.try_into().ok()?))
}

/// What is left to copy out of one file: entries walked and bytes of names.
struct Budget {
    entries: usize,
    bytes: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self { entries: MAX_ENTRIES, bytes: MAX_NAMES }
    }
}

impl Budget {
    /// One more entry walked; `None` past the budget.
    fn entry(&mut self) -> Option<()> {
        self.entries = self.entries.checked_sub(1)?;
        Some(())
    }

    /// The name at `at` in `table`, copied only while the budget allows it.
    fn string_at(&mut self, table: &[u8], at: u64) -> Option<String> {
        self.entry()?;
        let rest = table.get(usize::try_from(at).ok()?..)?;
        let end = rest.iter().take(self.bytes.saturating_add(1)).position(|byte| *byte == 0)?;
        self.bytes = self.bytes.checked_sub(end)?;
        String::from_utf8(rest[..end].to_vec()).ok()
    }
}

fn string_at(table: &[u8], at: u64) -> Option<String> {
    let rest = table.get(usize::try_from(at).ok()?..)?;
    let end = rest.iter().position(|byte| *byte == 0)?;
    String::from_utf8(rest[..end].to_vec()).ok()
}

/// Reads `path`. `Err` says, for the log, why it is no program this system runs.
pub(crate) fn read(path: &Path) -> Result<Kind, String> {
    let file = File::open(path).map_err(|error| format!("it cannot be opened: {error}"))?;
    let len = file.metadata().map_err(|error| error.to_string())?.len();
    let reader = Reader { file, len };
    let head = reader.bytes(0, len.min(256)).ok_or("it cannot be read")?;
    if let Some(line) = head.strip_prefix(b"#!") {
        let line = line.split(|byte| *byte == b'\n').next().unwrap_or_default();
        let line = std::str::from_utf8(line).map_err(|_| "its first line is not text")?.trim();
        // Linux passes whatever follows the interpreter as one argument.
        let (interpreter, argument) = match line.split_once(char::is_whitespace) {
            Some((interpreter, argument)) => (interpreter, Some(argument.trim().to_string())),
            None => (line, None),
        };
        if interpreter.is_empty() { return Err("its first line names no interpreter".into()); }
        return Ok(Kind::Script { interpreter: interpreter.into(), argument });
    }
    if !head.starts_with(b"\x7fELF") { return Err("it is neither a program nor a script".into()); }
    match head.get(4) { Some(2) => {}, Some(1) => return Err("it is a 32-bit program".into()), _ => return Err("its ELF class is unknown".into()) }
    if head.get(5) != Some(&1) { return Err("it is a big-endian program".into()); }
    let machine = u16_at(&head, 18).ok_or("its header is cut short")?;
    if let Some((expected, _)) = this_machine() {
        if machine != expected { return Err(format!("it is built for another processor (ELF machine {machine})")); }
    }
    program(&reader, &head).map(Kind::Program).ok_or_else(|| "its tables are malformed".into())
}

fn program(reader: &Reader, head: &[u8]) -> Option<Program> {
    let (table, size, count) = (u64_at(head, 32)?, u16_at(head, 54)? as u64, u16_at(head, 56)? as u64);
    if size < 56 { return None; }
    let headers = reader.bytes(table, size.checked_mul(count)?)?;
    let mut loads = Vec::new();
    let mut dynamic = None;
    let mut found = Program::default();
    for index in 0..count as usize {
        let header = &headers[index * size as usize..];
        let (kind, offset, address, length) = (u32_at(header, 0)?, u64_at(header, 8)?, u64_at(header, 16)?, u64_at(header, 32)?);
        match kind {
            PT_LOAD => loads.push((address, offset, length)),
            PT_DYNAMIC => dynamic = Some((offset, length)),
            PT_INTERP => found.interpreter = string_at(&reader.bytes(offset, length)?, 0),
            _ => {}
        }
    }
    let Some((offset, length)) = dynamic else { return Some(found) };
    let entries = reader.bytes(offset, length)?;
    let mut tags = Vec::new();
    for entry in entries.chunks_exact(16) {
        let (tag, value) = (u64_at(entry, 0)?, u64_at(entry, 8)?);
        if tag == 0 { break; }
        tags.push((tag, value));
    }
    let tag = |wanted: u64| tags.iter().find(|(tag, _)| *tag == wanted).map(|(_, value)| *value);
    // Addresses in the dynamic table are where the loader maps them.
    let at = |address: u64| loads.iter().find(|(start, _, length)| address >= *start && address - start < *length)
        .and_then(|(start, offset, _)| (address - start).checked_add(*offset));
    let strings = reader.bytes(at(tag(DT_STRTAB)?)?, tag(DT_STRSZ)?)?;
    let mut budget = Budget::default();
    for (kind, value) in &tags {
        match *kind {
            DT_NEEDED => found.needed.push(budget.string_at(&strings, *value)?),
            DT_RUNPATH | DT_RPATH => found.runpath.extend(budget.string_at(&strings, *value)?.split(':').filter(|dir| !dir.is_empty()).map(str::to_string)),
            _ => {}
        }
    }
    if let (Some(start), Some(count)) = (tag(DT_VERNEED), tag(DT_VERNEEDNUM)) {
        let mut entry = at(start)?;
        for _ in 0..(count as usize).min(MAX_ENTRIES) {
            let need = reader.bytes(entry, 16)?;
            let (versions, file, first, next) = (u16_at(&need, 2)?, u32_at(&need, 4)?, u32_at(&need, 8)?, u32_at(&need, 12)?);
            let mut names = Vec::new();
            let mut aux = entry.checked_add(first as u64)?;
            for _ in 0..(versions as usize).min(MAX_ENTRIES) {
                let version = reader.bytes(aux, 16)?;
                names.push(budget.string_at(&strings, u32_at(&version, 8)? as u64)?);
                let next = u32_at(&version, 12)?;
                if next == 0 { break; }
                aux = aux.checked_add(next as u64)?;
            }
            found.versions.push((budget.string_at(&strings, file as u64)?, names));
            if next == 0 { break; }
            entry = entry.checked_add(next as u64)?;
        }
    }
    if let (Some(start), Some(count)) = (tag(DT_VERDEF), tag(DT_VERDEFNUM)) {
        let mut entry = at(start)?;
        for _ in 0..(count as usize).min(MAX_ENTRIES) {
            let definition = reader.bytes(entry, 20)?;
            let (first, next) = (u32_at(&definition, 12)?, u32_at(&definition, 16)?);
            // The first name is the version; the ones after it are its parents.
            let name = reader.bytes(entry.checked_add(first as u64)?, 8)?;
            found.defines.push(budget.string_at(&strings, u32_at(&name, 0)? as u64)?);
            if next == 0 { break; }
            entry = entry.checked_add(next as u64)?;
        }
    }
    Some(found)
}

/// `GLIBC_` of `GLIBC_2.38`: what comes before the version number.
fn family(version: &str) -> &str {
    version.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.')
}

/// `[2, 38]` of `GLIBC_2.38`, compared as numbers: 2.5 is older than 2.38.
fn number(version: &str) -> Vec<u64> {
    version[family(version).len()..].split('.').filter_map(|part| part.parse().ok()).collect()
}

/// Where the loader in the plugin's sandbox looks for a library: the
/// program's own run paths, then the application's and the runtime's. What
/// is found counts only when, links resolved, it lies in the package, `/app`
/// or `/usr` of `system`: the rest of what Chilpa sees, its sandbox does not.
fn find(name: &str, program: &Path, runpath: &[String], system: &Path) -> Option<PathBuf> {
    let package = program.parent()?.canonicalize().ok()?;
    let seen: Vec<PathBuf> = ["app", "usr"].iter().filter_map(|dir| system.join(dir).canonicalize().ok())
        .chain([package.clone()]).collect();
    let confined = |path: PathBuf| path.canonicalize().ok()
        .filter(|real| real.is_file() && seen.iter().any(|root| real.starts_with(root)));
    let inside = |path: &str| system.join(path.trim_start_matches('/'));
    if name.contains('/') {
        // One relative to the sandbox's working folder is not looked for.
        return name.starts_with('/').then(|| confined(inside(name))).flatten();
    }
    // Inside the sandbox the package keeps the path it has here.
    let origin = package.to_string_lossy().into_owned();
    let own = runpath.iter().filter_map(|dir| {
        let expanded = dir.replace("${ORIGIN}", &origin).replace("$ORIGIN", &origin);
        if dir.starts_with("$ORIGIN") || dir.starts_with("${ORIGIN}") { Some(PathBuf::from(expanded)) }
        else if expanded.starts_with('/') { Some(inside(&expanded)) }
        else { None }
    });
    let triplet = this_machine().map(|(_, triplet)| triplet).unwrap_or("");
    let shared = ["app/lib".to_string(), format!("usr/lib/{triplet}"), "usr/lib64".into(), "usr/lib".into(),
        format!("lib/{triplet}"), "lib64".into(), "lib".into()].into_iter().map(|dir| system.join(dir));
    own.chain(shared).map(|dir| dir.join(name)).find_map(confined)
}

/// Whether the program in `program` can be started on the system whose
/// root is `system`, as its sandbox sees it: `/` inside Flatpak.
pub(crate) fn preflight(program: &Path, system: &Path) -> Result<(), Incompatible> {
    let inside = |path: &str| system.join(path.trim_start_matches('/'));
    match read(program).map_err(|why| { eprintln!("preflight: {}: {why}", program.display()); Incompatible::Format })? {
        Kind::Script { interpreter, argument } => {
            if !inside(&interpreter).is_file() { return Err(Incompatible::Interpreter(interpreter.into())); }
            // `env` finds its program on the plugin's PATH, which is /usr/bin.
            if interpreter.ends_with("/env") {
                let wanted = argument.unwrap_or_default();
                let named = wanted.split_whitespace().next().unwrap_or_default();
                if !named.is_empty() && !named.contains('/') && !inside("usr/bin").join(named).is_file() {
                    return Err(Incompatible::Interpreter(PathBuf::from("/usr/bin").join(named)));
                }
            }
            Ok(())
        }
        Kind::Program(found) => {
            if let Some(loader) = &found.interpreter {
                if !inside(loader).is_file() { return Err(Incompatible::Interpreter(loader.into())); }
            }
            for name in &found.needed {
                if find(name, program, &found.runpath, system).is_none() { return Err(Incompatible::Library(name.clone())); }
            }
            // Each library is read once, however many entries name it.
            let mut read_once: HashMap<PathBuf, Vec<String>> = HashMap::new();
            let mut check = |library: &String, versions: &[String]| -> Result<(), Incompatible> {
                let path = find(library, program, &found.runpath, system).ok_or_else(|| Incompatible::Library(library.clone()))?;
                let defines = read_once.entry(path).or_insert_with_key(|path| match read(path) {
                    Ok(Kind::Program(library)) => library.defines,
                    _ => Vec::new(),
                });
                match versions.iter().find(|version| !defines.contains(version)) {
                    Some(missing) => {
                        let newest = defines.iter().filter(|defined| family(defined) == family(missing))
                            .max_by(|a, b| number(a).cmp(&number(b))).cloned();
                        Err(Incompatible::Version { library: library.clone(), needed: missing.clone(), newest })
                    }
                    None => Ok(()),
                }
            };
            // The newest glibc it asks for is said first: it is what a
            // rebuild against an older one fixes.
            if let Some(glibc) = found.newest_glibc() {
                if let Some((library, _)) = found.versions.iter().find(|(_, versions)| versions.iter().any(|version| version == glibc)) {
                    check(library, &[glibc.to_string()][..])?;
                }
            }
            found.versions.iter().try_for_each(|(library, versions)| check(library, versions.as_slice()))
        }
    }
}

#[cfg(test)]
#[path = "elf_tests.rs"]
mod tests;
