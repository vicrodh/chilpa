//! The menu-bar item on a Mac: an `NSStatusItem` with the same menu as the
//! panel icon elsewhere.
//!
//! The menu is not attached to the item: a left click toggles the window,
//! and a right or Control click flashes the menu on, performs the click and
//! takes it off again. Everything here lives on the thread AppKit runs on,
//! which in a Qt application is the one the window lives on.

use crate::glyph::{self, Click, Picture};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEventMask, NSEventModifierFlags, NSEventType, NSImage, NSMenu,
    NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSData, NSInteger, NSSize, NSString};
use std::cell::RefCell;

// 44 px pictures, 22 points at twice the density. `tray-dark` holds the
// white glyph and `tray-light` the black one, as on the other desktops.
const COLOR: &[u8] = include_bytes!("../icons/tray-color-44.png");
const WHITE: &[u8] = include_bytes!("../icons/tray-dark-44.png");
const BLACK: &[u8] = include_bytes!("../icons/tray-light-44.png");

/// What the item asks of Chilpa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    PlayPause,
    Next,
    Previous,
    /// Show the window if it is hidden, hide it if it is in sight.
    ToggleWindow,
    /// Quit, asking first when that is wanted.
    QuitAsked,
}

/// The menu's rows, in the language of the moment.
#[derive(Debug, Clone)]
pub struct Labels {
    pub play_pause: String,
    pub next: String,
    pub previous: String,
    pub show_hide: String,
    pub quit: String,
}

const TAG_PLAY_PAUSE: NSInteger = 1;
const TAG_NEXT: NSInteger = 2;
const TAG_PREVIOUS: NSInteger = 3;
const TAG_SHOW_HIDE: NSInteger = 4;
const TAG_QUIT: NSInteger = 5;

thread_local! {
    // Dropping the item takes it off the menu bar, so they are kept for as
    // long as the icon is wanted.
    static STATUS_ITEM: RefCell<Option<Retained<NSStatusItem>>> = const { RefCell::new(None) };
    static TARGET: RefCell<Option<Retained<TrayTarget>>> = const { RefCell::new(None) };
    static MENU: RefCell<Option<Retained<NSMenu>>> = const { RefCell::new(None) };
    static ON_EVENT: RefCell<Option<Box<dyn Fn(Event)>>> = const { RefCell::new(None) };
}

define_class!(
    // SAFETY: NSObject has no requirements on a subclass, and this one has no
    // instance variables and no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ChilpaTrayTarget"]
    struct TrayTarget;

    impl TrayTarget {
        #[unsafe(method(onMenuItem:))]
        fn on_menu_item(&self, sender: Option<&NSMenuItem>) {
            #[allow(unused_unsafe)]
            let tag = sender.map(|item| unsafe { item.tag() }).unwrap_or(0);
            tell(match tag {
                TAG_PLAY_PAUSE => Event::PlayPause,
                TAG_NEXT => Event::Next,
                TAG_PREVIOUS => Event::Previous,
                TAG_SHOW_HIDE => Event::ToggleWindow,
                TAG_QUIT => Event::QuitAsked,
                _ => return,
            });
        }

        // Fires on the release of the left and of the right button.
        #[unsafe(method(onStatusButton:))]
        fn on_status_button(&self, _sender: Option<&AnyObject>) {
            clicked();
        }
    }
);

impl TrayTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

fn tell(event: Event) {
    ON_EVENT.with(|slot| {
        if let Some(handler) = slot.borrow().as_ref() {
            handler(event);
        }
    });
}

fn clicked() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let (right, control) = match NSApplication::sharedApplication(mtm).currentEvent() {
        Some(event) => (
            event.r#type() == NSEventType::RightMouseUp,
            event.modifierFlags().contains(NSEventModifierFlags::Control),
        ),
        None => (false, false),
    };
    match glyph::click(right, control) {
        Click::Toggle => tell(Event::ToggleWindow),
        Click::Menu => pop_up_menu(mtm),
    }
}

/// The menu on for the length of one click.
#[allow(unused_unsafe)]
fn pop_up_menu(mtm: MainThreadMarker) {
    let Some(item) = STATUS_ITEM.with(|slot| slot.borrow().clone()) else { return };
    let Some(menu) = MENU.with(|slot| slot.borrow().clone()) else { return };
    unsafe { item.setMenu(Some(&menu)) };
    if let Some(button) = unsafe { item.button(mtm) } {
        unsafe { button.performClick(None) };
    }
    unsafe { item.setMenu(None) };
}

#[allow(unused_unsafe)]
fn image_for(preference: &str) -> Option<Retained<NSImage>> {
    let (picture, template) = glyph::picture_for(preference);
    let bytes = match picture {
        Picture::Color => COLOR,
        Picture::White => WHITE,
        Picture::Black => BLACK,
    };
    let data = NSData::with_bytes(bytes);
    let image = NSImage::initWithData(NSImage::alloc(), &data)?;
    unsafe { image.setTemplate(template) };
    // Without a size in points the bar draws the pixels as they are, and the
    // glyph comes out twice as large as the bar is tall.
    unsafe { image.setSize(NSSize::new(18.0, 18.0)) };
    Some(image)
}

#[allow(unused_unsafe)]
fn show_glyph(item: &NSStatusItem, preference: &str, mtm: MainThreadMarker) {
    let Some(image) = image_for(preference) else {
        eprintln!("tray: the menu-bar picture could not be read");
        return;
    };
    if let Some(button) = unsafe { item.button(mtm) } {
        unsafe { button.setImage(Some(&image)) };
    }
}

#[allow(unused_unsafe)]
fn row(mtm: MainThreadMarker, target: &AnyObject, title: &str, tag: NSInteger) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &NSString::from_str(title), Some(sel!(onMenuItem:)), &NSString::from_str(""))
    };
    unsafe {
        item.setTarget(Some(target));
        item.setTag(tag);
        item.setEnabled(true);
    }
    item
}

/// Puts the item on the menu bar. `on_event` is called on this thread. False
/// when it is not the thread AppKit runs on; the caller then has no tray,
/// and closing the window must not hide it into nothing.
#[allow(unused_unsafe)]
pub fn create(preference: &str, labels: &Labels, on_event: impl Fn(Event) + 'static) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("tray: not on the thread AppKit runs on");
        return false;
    };
    if STATUS_ITEM.with(|slot| slot.borrow().is_some()) {
        return true;
    }
    let target = TrayTarget::new(mtm);
    let target_object: &AnyObject = &target;
    let menu = NSMenu::new(mtm);
    menu.addItem(&row(mtm, target_object, &labels.play_pause, TAG_PLAY_PAUSE));
    menu.addItem(&row(mtm, target_object, &labels.next, TAG_NEXT));
    menu.addItem(&row(mtm, target_object, &labels.previous, TAG_PREVIOUS));
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&row(mtm, target_object, &labels.show_hide, TAG_SHOW_HIDE));
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&row(mtm, target_object, &labels.quit, TAG_QUIT));

    let item = unsafe { NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength) };
    show_glyph(&item, preference, mtm);
    // The menu is not attached: any click on an attached menu would open it.
    if let Some(button) = unsafe { item.button(mtm) } {
        unsafe {
            button.setTarget(Some(target_object));
            button.setAction(Some(sel!(onStatusButton:)));
            button.sendActionOn(NSEventMask::LeftMouseUp | NSEventMask::RightMouseUp);
        }
    }
    ON_EVENT.with(|slot| *slot.borrow_mut() = Some(Box::new(on_event)));
    STATUS_ITEM.with(|slot| *slot.borrow_mut() = Some(item));
    TARGET.with(|slot| *slot.borrow_mut() = Some(target));
    MENU.with(|slot| *slot.borrow_mut() = Some(menu));

    // A binary that is not run from a bundle is a background process to
    // macOS, which may not route the menu's actions. Regular and active
    // makes it an application.
    let app = NSApplication::sharedApplication(mtm);
    let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    true
}

/// Shows the glyph a preference asks for.
pub fn set_glyph(preference: &str) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    STATUS_ITEM.with(|slot| {
        if let Some(item) = slot.borrow().as_ref() {
            show_glyph(item, preference, mtm);
        }
    });
}

/// The menu's rows in another language.
#[allow(unused_unsafe)]
pub fn relabel(labels: &Labels) {
    MENU.with(|slot| {
        let Some(menu) = slot.borrow().clone() else { return };
        for (tag, title) in [
            (TAG_PLAY_PAUSE, &labels.play_pause),
            (TAG_NEXT, &labels.next),
            (TAG_PREVIOUS, &labels.previous),
            (TAG_SHOW_HIDE, &labels.show_hide),
            (TAG_QUIT, &labels.quit),
        ] {
            if let Some(item) = unsafe { menu.itemWithTag(tag) } {
                unsafe { item.setTitle(&NSString::from_str(title)) };
            }
        }
    });
}

/// Takes the item off the menu bar.
#[allow(unused_unsafe)]
pub fn remove() {
    if let Some(item) = STATUS_ITEM.with(|slot| slot.borrow_mut().take()) {
        unsafe { NSStatusBar::systemStatusBar().removeStatusItem(&item) };
    }
    TARGET.with(|slot| slot.borrow_mut().take());
    MENU.with(|slot| slot.borrow_mut().take());
    ON_EVENT.with(|slot| slot.borrow_mut().take());
}

/// Whether the item is on the menu bar.
pub fn present() -> bool {
    STATUS_ITEM.with(|slot| slot.borrow().is_some())
}
