//! Where a sidecar lives, and how it gets there.

use super::document::{Override, Sidecar, SidecarError, VERSION};
use chilpa_models::RemoteEntry;
use std::path::{Path, PathBuf};

const SIDECAR: &str = ".chilpa.json";
const QBZ: &str = ".qbz.json";

/// What an overlay in the library goes under: a record on its server, or a
/// track that has no record there.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Scope {
    Album,
    Track,
}

impl Scope {
    /// As the library writes it down.
    pub fn name(self) -> &'static str {
        match self {
            Scope::Album => "album",
            Scope::Track => "track",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "album" => Some(Scope::Album),
            "track" => Some(Scope::Track),
            _ => None,
        }
    }
}

/// Where a track's sidecar lives.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Home {
    /// A folder of files, each a track of its own.
    Folder(PathBuf),
    /// A file that holds tracks.
    Container(PathBuf),
    /// A record, or a loose track, on a server of mine. What is laid over
    /// it lives in the library under its id on the server: no file of this
    /// machine stands for it, and no path is ever made from its entry.
    Hosted { source_id: i64, scope: Scope, key: String },
}

/// A path with something added to its last name: `a.iso` and `.chilpa.json`
/// make `a.iso.chilpa.json`.
fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// What a track goes by on its server: the id inside its entry, or the
/// entry itself when it is not one.
fn server_id(external_id: &str) -> String {
    RemoteEntry::parse(external_id).map(|entry| entry.external_id).unwrap_or_else(|| external_id.to_string())
}

impl Home {
    /// The home of a track: the file that holds it, when one does, and the
    /// folder it sits in when it is a file of its own.
    pub fn of(external_id: &str, container: Option<&str>) -> Self {
        match container {
            Some(file) => Home::Container(PathBuf::from(file)),
            None => Home::Folder(Path::new(external_id).parent().map(Path::to_path_buf).unwrap_or_default()),
        }
    }

    /// The home of a track on a server: its record's, by the record's id on
    /// the server, or its own when it has no record there.
    pub fn hosted(source_id: i64, album_key: Option<&str>, external_id: &str) -> Self {
        match album_key {
            Some(key) => Home::Hosted { source_id, scope: Scope::Album, key: key.to_string() },
            None => Home::Hosted { source_id, scope: Scope::Track, key: server_id(external_id) },
        }
    }

    pub fn is_hosted(&self) -> bool {
        matches!(self, Home::Hosted { .. })
    }

    /// The folder, or the file that holds the track. Empty for a home on a
    /// server, which has neither.
    pub fn path(&self) -> &Path {
        match self {
            Home::Folder(path) | Home::Container(path) => path,
            Home::Hosted { .. } => Path::new(""),
        }
    }

    /// The sidecar's file. Empty for a home on a server: its document is a
    /// row of the library.
    pub fn sidecar(&self) -> PathBuf {
        match self {
            Home::Folder(path) => path.join(SIDECAR),
            Home::Container(path) => suffixed(path, SIDECAR),
            Home::Hosted { .. } => PathBuf::new(),
        }
    }

    /// Where its cover goes. `extension` is "jpg" or "png". Empty for a
    /// home on a server: the library keeps its cover (`store::overlay`).
    pub fn cover(&self, extension: &str) -> PathBuf {
        let name = format!(".chilpa.cover.{extension}");
        match self {
            Home::Folder(path) => path.join(name),
            Home::Container(path) => suffixed(path, &name),
            Home::Hosted { .. } => PathBuf::new(),
        }
    }

    /// A file that sits beside the sidecar, by its name. Empty for a home
    /// on a server, which has nothing beside it on this machine.
    pub fn beside(&self, name: &str) -> PathBuf {
        match self {
            Home::Folder(path) => path.join(name),
            Home::Container(path) => path.with_file_name(name),
            Home::Hosted { .. } => PathBuf::new(),
        }
    }

    /// What a track goes by in this home: its file's name, what follows the
    /// holding file in its path, or its id on its server.
    pub fn key(&self, external_id: &str) -> String {
        match self {
            Home::Folder(_) => Path::new(external_id).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            Home::Container(path) => {
                let held_by = path.to_string_lossy();
                match external_id.rsplit_once(held_by.as_ref()) {
                    Some((_, after)) => after.to_string(),
                    None => external_id.to_string(),
                }
            }
            Home::Hosted { .. } => server_id(external_id),
        }
    }

    /// The `.qbz.json` files that stand in for a missing sidecar, in the
    /// order they are tried. QBZ kept one per record and looked for it in
    /// the record's folder and in the file's.
    pub fn qbz_candidates(&self, external_id: &str) -> Vec<PathBuf> {
        match self {
            Home::Container(path) => vec![suffixed(path, QBZ)],
            Home::Folder(path) => {
                let mut out = vec![path.join(QBZ)];
                let above = PathBuf::from(crate::album_folder::album_folder(Path::new(external_id)));
                if !above.as_os_str().is_empty() && above != *path {
                    out.push(above.join(QBZ));
                }
                out
            }
            Home::Hosted { .. } => Vec::new(),
        }
    }
}

/// The file whose word is in force for a home: its own sidecar, or the
/// first of QBZ's that is there. Nothing on disk for a home on a server.
pub fn in_force(home: &Home, external_id: &str) -> Option<PathBuf> {
    if home.is_hosted() {
        return None;
    }
    let own = home.sidecar();
    if own.is_file() {
        return Some(own);
    }
    home.qbz_candidates(external_id).into_iter().find(|path| path.is_file())
}

/// What is in force for a home. `None` when there is nothing.
pub fn load(home: &Home, external_id: &str) -> Result<Option<Sidecar>, SidecarError> {
    let Some(path) = in_force(home, external_id) else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(SidecarError::Io)?;
    if path == home.sidecar() {
        Sidecar::from_json(&bytes).map(Some)
    } else {
        super::qbz::import(&bytes).map(Some)
    }
}

/// A file written beside where it goes, not yet moved over.
struct Pending {
    from: PathBuf,
    to: PathBuf,
}

impl Pending {
    fn write(to: &Path, bytes: &[u8]) -> std::io::Result<Self> {
        let from = suffixed(to, ".tmp");
        let written = std::fs::write(&from, bytes)
            .and_then(|()| std::fs::OpenOptions::new().write(true).open(&from))
            .and_then(|file| file.sync_all());
        if let Err(e) = written {
            let _ = std::fs::remove_file(&from);
            return Err(e);
        }
        Ok(Self { from, to: to.to_path_buf() })
    }

    fn land(&self) -> std::io::Result<()> {
        std::fs::rename(&self.from, &self.to)
    }
}

impl Drop for Pending {
    /// What did not land goes away. What landed is no longer under this
    /// name, and there is nothing to remove.
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.from);
    }
}

fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// What a home is about to become: written beside, not yet moved over.
/// Dropping it leaves the home as it was.
pub struct StagedHome {
    sidecar: Option<Pending>,
    cover: Option<Pending>,
    /// What goes away when this lands.
    gone: Vec<PathBuf>,
    cover_at: Option<PathBuf>,
}

impl StagedHome {
    /// Where the record's cover is once this lands, when its home has one.
    pub fn cover(&self) -> Option<&Path> {
        self.cover_at.as_deref()
    }

    /// Moves what was written over what was there.
    pub fn land(self) -> Result<(), SidecarError> {
        if let Some(cover) = &self.cover {
            cover.land().map_err(SidecarError::Io)?;
        }
        if let Some(sidecar) = &self.sidecar {
            sidecar.land().map_err(SidecarError::Io)?;
        }
        for path in &self.gone {
            remove(path).map_err(SidecarError::Io)?;
        }
        Ok(())
    }
}

/// Writes a home's sidecar, and its cover when one is given, beside where
/// they go. `cover` is the image's bytes as they are and "jpg" or "png".
///
/// A sidecar with nothing in it is not written: landing removes the one
/// that was there, and its cover. Unless it stood in for one of QBZ's: then
/// it is written empty, or QBZ's would be in force again and lay back what
/// was just written into the files.
pub fn stage(home: &Home, mut sidecar: Sidecar, cover: Option<(&[u8], &str)>, now: i64) -> Result<StagedHome, SidecarError> {
    if sidecar.version > VERSION {
        return Err(SidecarError::Newer(sidecar.version));
    }
    if home.is_hosted() {
        // Its document is a row of the library, written by whoever holds it.
        return Err(SidecarError::Io(std::io::Error::new(std::io::ErrorKind::Unsupported, "a home on a server has no file to write")));
    }
    let over_qbz = sidecar.imported;
    sidecar.version = VERSION;
    sidecar.updated_at = now;
    sidecar.imported = false;

    let mut gone = Vec::new();
    let mut pending_cover = None;
    match cover {
        Some((bytes, extension)) => {
            let at = home.cover(extension);
            let name = at.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            sidecar.album.cover = Override::Value(name);
            pending_cover = Some(Pending::write(&at, bytes).map_err(SidecarError::Io)?);
            gone.push(home.cover(if extension == "png" { "jpg" } else { "png" }));
        }
        None => {
            // A cover nothing names is a file nobody will find.
            if sidecar.album.cover.value().is_none() {
                sidecar.album.cover = Override::Absent;
                gone.push(home.cover("jpg"));
                gone.push(home.cover("png"));
            }
        }
    }
    let cover_at = sidecar.album.cover.value().map(|name| home.beside(name));
    let pending = if sidecar.is_empty() && !over_qbz {
        gone.push(home.sidecar());
        None
    } else {
        Some(Pending::write(&home.sidecar(), &sidecar.to_json()).map_err(SidecarError::Io)?)
    };
    Ok(StagedHome { sidecar: pending, cover: pending_cover, gone, cover_at })
}

/// Writes a home's sidecar. Beside and moved over: a sidecar half written
/// is never read.
pub fn store(home: &Home, sidecar: &Sidecar, now: i64) -> Result<(), SidecarError> {
    stage(home, sidecar.clone(), None, now)?.land()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("chilpa-sidecar-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn titled(title: &str) -> Sidecar {
        let mut s = Sidecar::new(0);
        s.album.title = Override::Value(title.to_string());
        s
    }

    #[test]
    fn a_file_s_home_is_its_folder_and_a_held_track_s_is_what_holds_it() {
        let home = Home::of("/music/Record/01 - One.flac", None);
        assert_eq!(home.path(), Path::new("/music/Record"));
        assert_eq!(home.sidecar(), Path::new("/music/Record/.chilpa.json"));
        assert_eq!(home.cover("jpg"), Path::new("/music/Record/.chilpa.cover.jpg"));
        assert_eq!(home.key("/music/Record/01 - One.flac"), "01 - One.flac");

        let held = Home::of("sacd:/music/Record/Disc #1.iso#3", Some("/music/Record/Disc #1.iso"));
        assert_eq!(held.sidecar(), Path::new("/music/Record/Disc #1.iso.chilpa.json"));
        assert_eq!(held.cover("png"), Path::new("/music/Record/Disc #1.iso.chilpa.cover.png"));
        assert_eq!(held.key("sacd:/music/Record/Disc #1.iso#3"), "#3");
        assert_eq!(held.beside("Disc #1.iso.chilpa.cover.png"), Path::new("/music/Record/Disc #1.iso.chilpa.cover.png"));
    }

    #[test]
    fn a_track_on_a_server_has_no_file_and_goes_by_its_id_there() {
        let entry = "remote:example-server/aaaa/song%2F7";
        let record = Home::hosted(3, Some("al-9"), entry);
        assert_eq!(record, Home::Hosted { source_id: 3, scope: Scope::Album, key: "al-9".into() });
        assert_eq!(record.key(entry), "song/7", "the id as the server wrote it");
        assert_eq!(record.path(), Path::new(""));
        assert_eq!(record.sidecar(), PathBuf::new(), "no file stands for it");
        assert!(record.qbz_candidates(entry).is_empty());
        assert_eq!(in_force(&record, entry), None);
        assert!(load(&record, entry).unwrap().is_none());
        assert!(stage(&record, titled("A"), None, 1).is_err(), "nothing of it is written to disk here");

        assert_eq!(record.cover("jpg"), PathBuf::new(), "the library keeps its cover");
        assert_eq!(record.beside("al-9.jpg"), PathBuf::new());

        let loose = Home::hosted(3, None, entry);
        assert_eq!(loose, Home::Hosted { source_id: 3, scope: Scope::Track, key: "song/7".into() });
        for scope in [Scope::Album, Scope::Track] {
            assert_eq!(Scope::from_name(scope.name()), Some(scope), "a scope reads back as it is written");
        }
        assert_eq!(Scope::from_name("folder"), None);
    }

    #[test]
    fn a_track_is_found_after_its_folder_moved() {
        let first = folder("moved-from");
        let file = first.join("01.flac");
        let home = Home::of(&file.to_string_lossy(), None);
        let mut s = Sidecar::new(0);
        s.track_mut(&home.key(&file.to_string_lossy())).title = Override::Value("Kept".to_string());
        store(&home, &s, 5).unwrap();

        let second = std::env::temp_dir().join("chilpa-sidecar-moved-to");
        let _ = std::fs::remove_dir_all(&second);
        std::fs::rename(&first, &second).unwrap();

        let moved = second.join("01.flac");
        let home = Home::of(&moved.to_string_lossy(), None);
        let read = load(&home, &moved.to_string_lossy()).unwrap().unwrap();
        assert_eq!(read.tracks[&home.key(&moved.to_string_lossy())].title, Override::Value("Kept".to_string()));
        assert_eq!(read.updated_at, 5);
        let _ = std::fs::remove_dir_all(second);
    }

    #[test]
    fn a_half_written_sidecar_is_never_read() {
        let dir = folder("half");
        let file = dir.join("01.flac").to_string_lossy().into_owned();
        let home = Home::of(&file, None);
        let staged = stage(&home, titled("New"), None, 1).unwrap();
        assert!(load(&home, &file).unwrap().is_none(), "what is beside is not what is read");
        assert!(dir.join(".chilpa.json.tmp").is_file());
        drop(staged);
        assert!(!dir.join(".chilpa.json.tmp").exists(), "and what did not land goes away");
        assert!(!home.sidecar().exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_empty_sidecar_is_removed_with_its_cover() {
        let dir = folder("emptied");
        let file = dir.join("01.flac").to_string_lossy().into_owned();
        let home = Home::of(&file, None);
        stage(&home, titled("A"), Some((b"a cover", "jpg")), 1).unwrap().land().unwrap();
        assert!(home.sidecar().is_file() && home.cover("jpg").is_file());

        store(&home, &Sidecar::new(0), 2).unwrap();
        assert!(!home.sidecar().exists());
        assert!(!home.cover("jpg").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_cover_of_the_other_kind_is_removed() {
        let dir = folder("cover-kind");
        let file = dir.join("01.flac").to_string_lossy().into_owned();
        let home = Home::of(&file, None);
        stage(&home, titled("A"), Some((b"a png", "png")), 1).unwrap().land().unwrap();
        let staged = stage(&home, titled("A"), Some((b"a jpeg", "jpg")), 2).unwrap();
        assert_eq!(staged.cover(), Some(home.cover("jpg").as_path()));
        staged.land().unwrap();
        assert!(!home.cover("png").exists());
        assert_eq!(std::fs::read(home.cover("jpg")).unwrap(), b"a jpeg");
        let read = load(&home, &file).unwrap().unwrap();
        assert_eq!(read.album.cover, Override::Value(".chilpa.cover.jpg".to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_cover_is_kept_when_a_save_brings_none() {
        let dir = folder("cover-kept");
        let file = dir.join("01.flac").to_string_lossy().into_owned();
        let home = Home::of(&file, None);
        stage(&home, titled("A"), Some((b"a cover", "jpg")), 1).unwrap().land().unwrap();
        let mut again = load(&home, &file).unwrap().unwrap();
        again.album.title = Override::Value("B".to_string());
        store(&home, &again, 2).unwrap();
        assert!(home.cover("jpg").is_file());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_newer_sidecar_is_never_rewritten() {
        let dir = folder("newer");
        let file = dir.join("01.flac").to_string_lossy().into_owned();
        let home = Home::of(&file, None);
        let newer = br#"{"format":"chilpa-sidecar","version":2,"album":{"title":"A"},"tracks":{}}"#;
        std::fs::write(home.sidecar(), newer).unwrap();
        let read = load(&home, &file).unwrap().unwrap();
        assert!(matches!(store(&home, &read, 9), Err(SidecarError::Newer(2))));
        assert_eq!(std::fs::read(home.sidecar()).unwrap(), newer);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn our_sidecar_wins_over_qbz_and_qbz_is_never_written() {
        let dir = folder("both");
        let file = dir.join("01.flac").to_string_lossy().into_owned();
        let home = Home::of(&file, None);
        let theirs = br#"{"version":1,"updatedAt":1,"album":{"albumTitle":"From QBZ","albumArtist":null,"year":null,"genre":null,"catalogNumber":null},"tracks":[]}"#;
        std::fs::write(dir.join(".qbz.json"), theirs).unwrap();

        let read = load(&home, &file).unwrap().unwrap();
        assert!(read.imported);
        assert_eq!(read.album.title, Override::Value("From QBZ".to_string()));
        assert_eq!(in_force(&home, &file), Some(dir.join(".qbz.json")));

        store(&home, &titled("From Chilpa"), 2).unwrap();
        assert_eq!(in_force(&home, &file), Some(home.sidecar()));
        let read = load(&home, &file).unwrap().unwrap();
        assert!(!read.imported);
        assert_eq!(read.album.title, Override::Value("From Chilpa".to_string()));
        assert_eq!(std::fs::read(dir.join(".qbz.json")).unwrap(), theirs, "theirs is as it was");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn emptying_what_stood_in_for_qbz_keeps_qbz_out() {
        let dir = folder("masked");
        let file = dir.join("01.flac").to_string_lossy().into_owned();
        let home = Home::of(&file, None);
        let theirs = br#"{"version":1,"updatedAt":1,"album":{"albumTitle":"From QBZ","albumArtist":null,"year":null,"genre":null,"catalogNumber":null},"tracks":[]}"#;
        std::fs::write(dir.join(".qbz.json"), theirs).unwrap();

        // What a save into the files does: everything QBZ laid over the
        // files is now in them, and the sidecar says nothing.
        let mut read = load(&home, &file).unwrap().unwrap();
        read.album.title = Override::Absent;
        assert!(read.is_empty());
        store(&home, &read, 2).unwrap();
        assert_eq!(in_force(&home, &file), Some(home.sidecar()), "an empty one of ours, not theirs");
        let again = load(&home, &file).unwrap().unwrap();
        assert!(again.is_empty());
        assert_eq!(std::fs::read(dir.join(".qbz.json")).unwrap(), theirs);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_disc_folder_looks_for_qbz_s_sidecar_in_the_record_s_too() {
        let home = Home::of("/music/Box/CD1/01.flac", None);
        assert_eq!(
            home.qbz_candidates("/music/Box/CD1/01.flac"),
            [PathBuf::from("/music/Box/CD1/.qbz.json"), PathBuf::from("/music/Box/.qbz.json")]
        );
        let plain = Home::of("/music/Record/01.flac", None);
        assert_eq!(plain.qbz_candidates("/music/Record/01.flac"), [PathBuf::from("/music/Record/.qbz.json")]);
    }
}
