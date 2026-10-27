use super::{Profile, Store, UiError, failure, transport};
use chilpa_sync::{Cancellation, RelativePath};
use std::io::Write;

/// I read one bounded playlist inside the paired root, without opening a journal writer.
pub(crate) fn read_playlist(
    _store: &Store,
    profile: &Profile,
    destination: &str,
    path: &RelativePath,
    cancel: &Cancellation,
) -> Result<String, UiError> {
    if !path.as_str().starts_with("Playlists/") {
        return Err(failure(
            chilpa_i18n::mark("This playlist is no longer available."),
            "",
        ));
    }
    let mut device = transport(&profile.root, destination, false, cancel)?;
    let check = |device: &mut dyn chilpa_sync::transport::Transport| -> Result<(), UiError> {
        let identity = chilpa_sync::inspect_target(device, cancel).map_err(UiError::from)?;
        if identity.as_ref().map(|t| t.id.as_str()) != Some(profile.target_id.as_str()) {
            return Err(failure(
                chilpa_i18n::mark("The selected storage changed or disconnected."),
                "",
            ));
        }
        Ok(())
    };
    check(device.as_mut())?;
    struct Text(Vec<u8>);
    impl Write for Text {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > 1024 * 1024 {
                return Err(std::io::Error::other("playlist exceeds 1 MiB"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bytes = Text(Vec::new());
    device
        .read(path, &mut bytes, cancel)
        .map_err(UiError::from)?;
    check(device.as_mut())?;
    String::from_utf8(bytes.0)
        .map_err(|e| failure(chilpa_i18n::mark("This playlist is not valid UTF-8."), e))
}
