//! I add the running AppImage to the desktop's application menu.
//!
//! I copy the image's desktop entry and icon outside its temporary mount.
//! I point the entry directly at the installed icon so menus do not depend
//! on the current icon theme or its cache. On every start I write it under the
//! user's data folder with `Exec` on the image itself, so an image that moved
//! is followed on its next start. An entry I did not write stays as it is,
//! and so does an image another integrator already added.
use std::{fs, io, path::Path};

const ID: &str = "app.chilpa.Chilpa";
const ICON: &str = "icons/hicolor/512x512/apps/app.chilpa.Chilpa.png";
/// The key that tells my entry from one a package or the user installed.
const MARK: &str = "X-Chilpa-AppImage=true";
/// A desktop entry is a few hundred bytes; anything far larger is not one.
const ENTRY_LIMIT: u64 = 64 * 1024;

#[derive(Debug, PartialEq)]
enum Outcome { Written, Current, Foreign, Integrated, BadPath }

/// Before the window exists, so the first start already finds its entry.
pub fn integrate() {
    let Some(image) = chilpa_dirs::appimage() else { return };
    let bundled = chilpa_dirs::executable_helper(&format!("../share/applications/{ID}.desktop"));
    let (Some(bundled), Some(icon)) = (bundled, chilpa_dirs::executable_helper(&format!("../share/{ICON}"))) else { return };
    let result = fs::read_to_string(bundled).and_then(|entry| install(&image, &entry, &icon, &chilpa_dirs::data_base()));
    if let Err(error) = result {
        eprintln!("desktop entry: {error}");
    }
}

fn install(image: &Path, bundled: &str, icon: &Path, data: &Path) -> io::Result<Outcome> {
    let installed = data.join(ICON);
    let Some(entry) = image.to_str().zip(installed.to_str()).and_then(|(image, icon)| render(bundled, image, icon))
        else { return Ok(Outcome::BadPath) };
    let applications = data.join("applications");
    let target = applications.join(format!("{ID}.desktop"));
    let existing = fs::read_to_string(&target).ok();
    if existing.as_deref().is_some_and(|text| !text.lines().any(|line| line == MARK)) {
        return Ok(Outcome::Foreign);
    }
    if integrated_elsewhere(&applications, &target, image) {
        if existing.is_some() {
            fs::remove_file(&target)?;
        }
        return Ok(Outcome::Integrated);
    }
    let picture = fs::read(icon)?;
    if fs::read(&installed).ok().as_ref() != Some(&picture) {
        write(&installed, &picture)?;
    }
    if existing.as_deref() == Some(entry.as_str()) {
        return Ok(Outcome::Current);
    }
    write(&target, entry.as_bytes())?;
    Ok(Outcome::Written)
}

/// The bundled entry with `Exec` on the image, `TryExec` so the menu hides it
/// once the image is gone, my mark and the installed icon's absolute path.
fn render(bundled: &str, image: &str, icon: &str) -> Option<String> {
    if image.chars().chain(icon.chars()).any(char::is_control) {
        return None;
    }
    let mut group = "";
    let mut entry = String::new();
    for line in bundled.lines() {
        if line.starts_with('[') {
            group = line;
        }
        if group == "[Desktop Entry]" && (line.starts_with("TryExec=") || line == MARK) {
            continue;
        }
        if group == "[Desktop Entry]" && line.starts_with("Icon=") {
            entry.push_str(&format!("Icon={}\n", string(icon)));
            continue;
        }
        let Some(command) = line.strip_prefix("Exec=") else {
            entry.push_str(line);
            entry.push('\n');
            continue;
        };
        let arguments = command.split_once(' ').map(|(_, rest)| format!(" {rest}")).unwrap_or_default();
        entry.push_str(&format!("Exec={}{arguments}\n", string(&program(image))));
        if group == "[Desktop Entry]" {
            entry.push_str(&format!("TryExec={}\n{MARK}\n", string(image)));
        }
    }
    entry.lines().any(|line| line == MARK).then_some(entry)
}

/// The image as the program of an `Exec` key: `%` doubled so it is not a
/// field code, and quoted when it holds a character the specification
/// reserves.
fn program(image: &str) -> String {
    const RESERVED: &str = " \t\n\"'\\><~|&;$*?#()`";
    let image = image.replace('%', "%%");
    if !image.contains(|c| RESERVED.contains(c)) {
        return image;
    }
    let mut quoted = String::from('"');
    for c in image.chars() {
        if "\"`$\\".contains(c) {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted
}

/// A value as a desktop entry string, where a backslash starts an escape.
fn string(value: &str) -> String {
    value.replace('\\', "\\\\")
}

/// Whether another entry already starts this image: AppImageLauncher, Gear
/// Lever and a hand-made entry all name its path in `Exec` or `TryExec`.
fn integrated_elsewhere(applications: &Path, mine: &Path, image: &Path) -> bool {
    let (Some(image), Ok(entries)) = (image.to_str(), fs::read_dir(applications)) else { return false };
    entries.flatten().map(|entry| entry.path()).any(|path| {
        path != mine
            && path.extension().is_some_and(|extension| extension == "desktop")
            && fs::metadata(&path).is_ok_and(|metadata| metadata.is_file() && metadata.len() <= ENTRY_LIMIT)
            && fs::read_to_string(&path).is_ok_and(|text| {
                text.lines().any(|line| (line.starts_with("Exec=") || line.starts_with("TryExec=")) && line.contains(image))
            })
    })
}

/// Beside the target and then over it, so the menu never reads half a file.
fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let Some(parent) = path.parent() else { return Err(io::ErrorKind::InvalidInput.into()) };
    fs::create_dir_all(parent)?;
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let partial = parent.join(format!(".{name}.partial"));
    fs::write(&partial, bytes)?;
    fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLED: &str = "[Desktop Entry]\nType=Application\nName=Chilpa\nExec=chilpa-qt\nIcon=app.chilpa.Chilpa\n";

    fn image_in(folder: &Path, name: &str) -> std::path::PathBuf {
        let image = folder.join(name);
        fs::write(&image, b"image").unwrap();
        image
    }

    #[test]
    fn a_first_start_writes_the_entry_and_its_icon() {
        let root = tempfile::tempdir().unwrap();
        let image = image_in(root.path(), "Chilpa.AppImage");
        let icon = image_in(root.path(), "icon.png");
        let data = root.path().join("share");
        assert_eq!(install(&image, BUNDLED, &icon, &data).unwrap(), Outcome::Written);
        let entry = fs::read_to_string(data.join("applications/app.chilpa.Chilpa.desktop")).unwrap();
        let path = image.to_str().unwrap();
        assert!(entry.contains(&format!("\nExec={}\nTryExec={}\n{MARK}\n", string(&program(path)), string(path))), "{entry}");
        assert!(entry.contains(&format!("\nIcon={}\n", string(data.join(ICON).to_str().unwrap()))), "{entry}");
        assert_eq!(fs::read(data.join(ICON)).unwrap(), b"image");
        assert_eq!(install(&image, BUNDLED, &icon, &data).unwrap(), Outcome::Current);
    }

    #[test]
    fn a_moved_image_is_followed_on_its_next_start() {
        let root = tempfile::tempdir().unwrap();
        let icon = image_in(root.path(), "icon.png");
        let data = root.path().join("share");
        install(&image_in(root.path(), "Old.AppImage"), BUNDLED, &icon, &data).unwrap();
        let moved = image_in(root.path(), "New.AppImage");
        assert_eq!(install(&moved, BUNDLED, &icon, &data).unwrap(), Outcome::Written);
        let entry = fs::read_to_string(data.join("applications/app.chilpa.Chilpa.desktop")).unwrap();
        assert!(entry.contains("New.AppImage") && !entry.contains("Old.AppImage"), "{entry}");
    }

    #[test]
    fn an_older_name_only_icon_is_repaired_without_a_second_entry() {
        let root = tempfile::tempdir().unwrap();
        let image = image_in(root.path(), "Chilpa.AppImage");
        let icon = image_in(root.path(), "icon.png");
        let data = root.path().join("share");
        let target = data.join("applications/app.chilpa.Chilpa.desktop");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, format!("{BUNDLED}{MARK}\n")).unwrap();
        assert_eq!(install(&image, BUNDLED, &icon, &data).unwrap(), Outcome::Written);
        let entry = fs::read_to_string(&target).unwrap();
        assert!(entry.contains(&format!("Icon={}\n", string(data.join(ICON).to_str().unwrap()))), "{entry}");
        assert_eq!(fs::read_dir(target.parent().unwrap()).unwrap().count(), 1);
        assert_eq!(install(&image, BUNDLED, &icon, &data).unwrap(), Outcome::Current);
    }

    #[test]
    fn an_entry_somebody_else_wrote_is_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("share");
        let target = data.join("applications/app.chilpa.Chilpa.desktop");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "[Desktop Entry]\nExec=/usr/bin/chilpa-qt\n").unwrap();
        let outcome = install(&image_in(root.path(), "Chilpa.AppImage"), BUNDLED, &image_in(root.path(), "icon.png"), &data);
        assert_eq!(outcome.unwrap(), Outcome::Foreign);
        assert_eq!(fs::read_to_string(&target).unwrap(), "[Desktop Entry]\nExec=/usr/bin/chilpa-qt\n");
        assert!(!data.join(ICON).exists());
    }

    #[test]
    fn an_image_another_integrator_added_gets_no_second_entry() {
        let root = tempfile::tempdir().unwrap();
        let image = image_in(root.path(), "Chilpa.AppImage");
        let icon = image_in(root.path(), "icon.png");
        let data = root.path().join("share");
        install(&image, BUNDLED, &icon, &data).unwrap();
        let theirs = data.join("applications/appimagekit_0123-Chilpa.desktop");
        fs::write(&theirs, format!("[Desktop Entry]\nExec={} %U\n", image.display())).unwrap();
        assert_eq!(install(&image, BUNDLED, &icon, &data).unwrap(), Outcome::Integrated);
        assert!(!data.join("applications/app.chilpa.Chilpa.desktop").exists());
        assert!(theirs.exists());
    }

    #[test]
    fn a_path_with_reserved_characters_is_quoted_and_escaped() {
        let entry = render(BUNDLED, "/home/me/My Apps/Chilpa$1 100%.AppImage", "/home/me/My Icons/logo\\color.png").unwrap();
        assert!(entry.contains("\nExec=\"/home/me/My Apps/Chilpa\\\\$1 100%%.AppImage\"\n"), "{entry}");
        assert!(entry.contains("\nTryExec=/home/me/My Apps/Chilpa$1 100%.AppImage\n"), "{entry}");
        assert!(entry.contains("\nIcon=/home/me/My Icons/logo\\\\color.png\n"), "{entry}");
    }

    #[test]
    fn a_path_no_entry_can_hold_is_not_written() {
        assert_eq!(render(BUNDLED, "/home/me/two\nlines.AppImage", "/home/me/icon.png"), None);
        assert_eq!(render(BUNDLED, "/home/me/Chilpa.AppImage", "/home/me/two\nlines.png"), None);
    }
}
