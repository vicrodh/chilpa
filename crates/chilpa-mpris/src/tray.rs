//! The tray icon: a StatusNotifierItem the panel shows, with a menu, a
//! tooltip that says what plays, and the clicks and the wheel going back
//! to the facade as the same events the media controls send.
//!
//! The panel picks the pixmap that fits its height from the four it is
//! given (22, 32, 44 and 64): the icon is never named, because a panel
//! that is handed a name looks it up in the icon theme and draws the
//! application's colour icon in place of the glyph it was given.

use crate::MediaEvent;
use ksni::{blocking::TrayMethods, menu::StandardItem, Icon, MenuItem, Orientation, ToolTip, Tray};
use std::sync::Arc;

/// The glyph the panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    /// A light glyph, for a dark panel.
    Light,
    /// A dark glyph, for a light panel.
    Dark,
    /// The symbol in its own colours.
    Color,
}

const LIGHT: [&[u8]; 4] = [
    include_bytes!("../icons/tray-light-22.png"),
    include_bytes!("../icons/tray-light-32.png"),
    include_bytes!("../icons/tray-light-44.png"),
    include_bytes!("../icons/tray-light-64.png"),
];
const DARK: [&[u8]; 4] = [
    include_bytes!("../icons/tray-dark-22.png"),
    include_bytes!("../icons/tray-dark-32.png"),
    include_bytes!("../icons/tray-dark-44.png"),
    include_bytes!("../icons/tray-dark-64.png"),
];
const COLOR: [&[u8]; 4] = [
    include_bytes!("../icons/tray-color-22.png"),
    include_bytes!("../icons/tray-color-32.png"),
    include_bytes!("../icons/tray-color-44.png"),
    include_bytes!("../icons/tray-color-64.png"),
];

/// A PNG as the panel takes it: ARGB, each pixel's bytes in network order.
fn pixmap(png: &[u8]) -> Option<Icon> {
    let image = image::load_from_memory(png).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    let data = image.pixels().flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
    Some(Icon { width: width as i32, height: height as i32, data })
}

fn pixmaps(variant: Variant) -> Vec<Icon> {
    let sources = match variant {
        Variant::Light => LIGHT,
        Variant::Dark => DARK,
        Variant::Color => COLOR,
    };
    sources.iter().filter_map(|png| pixmap(png)).collect()
}

/// What the tooltip says: its title and the lines under it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Said {
    pub title: String,
    pub lines: String,
}

/// The labels of the menu, in the language of the moment.
#[derive(Clone, Debug, Default)]
pub struct Labels {
    pub play_pause: String,
    pub next: String,
    pub previous: String,
    pub show_hide: String,
    pub quit: String,
}

struct Item {
    icons: Vec<Icon>,
    said: Said,
    labels: Labels,
    tell: Arc<dyn Fn(MediaEvent) + Send + Sync>,
}

impl Tray for Item {
    fn id(&self) -> String {
        crate::APP_ID.into()
    }

    fn title(&self) -> String {
        "Chilpa".into()
    }

    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icons.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip { title: self.said.title.clone(), description: self.said.lines.clone(), icon_name: String::new(), icon_pixmap: Vec::new() }
    }

    /// A click shows or hides the window.
    fn activate(&mut self, _x: i32, _y: i32) {
        (self.tell)(MediaEvent::ToggleWindow);
    }

    /// The middle button plays or pauses.
    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        (self.tell)(MediaEvent::Toggle);
    }

    /// The wheel moves the volume a step per notch. A panel reports 120 per
    /// notch; a touchpad reports less, and that counts as one.
    fn scroll(&mut self, delta: i32, orientation: Orientation) {
        if !matches!(orientation, Orientation::Vertical) || delta == 0 {
            return;
        }
        let notches = delta / 120;
        (self.tell)(MediaEvent::VolumeBy(if notches == 0 { delta.signum() } else { notches }));
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let entry = |label: &str, event: MediaEvent| -> MenuItem<Self> {
            StandardItem { label: label.to_owned(), activate: Box::new(move |item: &mut Self| (item.tell)(event.clone())), ..Default::default() }.into()
        };
        vec![
            entry(&self.labels.play_pause, MediaEvent::Toggle),
            entry(&self.labels.next, MediaEvent::Next),
            entry(&self.labels.previous, MediaEvent::Previous),
            MenuItem::Separator,
            entry(&self.labels.show_hide, MediaEvent::ToggleWindow),
            MenuItem::Separator,
            entry(&self.labels.quit, MediaEvent::QuitAsked),
        ]
    }
}

/// The tray icon while it is on the panel. Dropping it takes it down.
pub struct Handle {
    inner: ksni::blocking::Handle<Item>,
}

impl Handle {
    /// Tells the tooltip what plays.
    pub fn say(&self, said: Said) {
        self.inner.update(|item| item.said = said);
    }

    /// Changes the glyph.
    pub fn recolor(&self, variant: Variant) {
        let icons = pixmaps(variant);
        self.inner.update(|item| item.icons = icons);
    }

    /// Relabels the menu, after a change of language.
    pub fn relabel(&self, labels: Labels) {
        self.inner.update(|item| item.labels = labels);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let _ = self.inner.shutdown();
    }
}

/// Puts the icon on the panel. `None` where there is no panel to take it,
/// or no bus.
pub fn spawn(variant: Variant, labels: Labels, said: Said, tell: impl Fn(MediaEvent) + Send + Sync + 'static) -> Option<Handle> {
    let item = Item { icons: pixmaps(variant), said, labels, tell: Arc::new(tell) };
    let sandboxed = std::path::Path::new("/.flatpak-info").exists();
    item.disable_dbus_name(sandboxed).spawn().ok().map(|inner| Handle { inner })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_comes_in_the_four_sizes_the_panel_chooses_from() {
        for variant in [Variant::Light, Variant::Dark, Variant::Color] {
            let sizes: Vec<i32> = pixmaps(variant).iter().map(|icon| icon.width).collect();
            assert_eq!(sizes, [22, 32, 44, 64], "{variant:?}");
        }
    }

    /// The names say which panel each glyph is for, not what colour it is:
    /// the light glyph is the one that is light.
    #[test]
    fn the_light_glyph_is_light_and_the_dark_one_dark() {
        let shade = |variant: Variant| {
            let icon = pixmaps(variant).remove(0);
            let opaque: Vec<&[u8]> = icon.data.chunks(4).filter(|p| p[0] > 200).collect();
            opaque.iter().map(|p| u32::from(p[1]) + u32::from(p[2]) + u32::from(p[3])).sum::<u32>() / (3 * opaque.len() as u32)
        };
        assert!(shade(Variant::Light) > 200, "{}", shade(Variant::Light));
        assert!(shade(Variant::Dark) < 50, "{}", shade(Variant::Dark));
    }
}
