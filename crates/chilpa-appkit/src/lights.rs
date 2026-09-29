//! The part that talks to AppKit.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSView, NSWindowButton, NSWindowTitleVisibility};

pub(crate) fn center(target: f64) -> bool {
    let Some(thread) = MainThreadMarker::new() else {
        eprintln!("window controls: not on the thread the window lives on");
        return false;
    };
    let app = NSApplication::sharedApplication(thread);
    // There is no main window until AppKit has shown the window and made it
    // main, which is later than its first frame; the key window covers the
    // moment in between. Anything else is asked again.
    let Some(window) = app.mainWindow().or_else(|| app.keyWindow()) else {
        return false;
    };

    // A property of the window, not a bit of its style mask, so Qt does not
    // undo it.
    window.setTitleVisibility(NSWindowTitleVisibility::Hidden);

    let Some(close) = window.standardWindowButton(NSWindowButton::CloseButton) else {
        // The title is hidden; without lights there is nothing to move, and
        // asking again would not bring them.
        return true;
    };
    let height = window.frame().size.height;
    // The middle of the close light, in points under the window's top. From
    // its bounds taken to the window: its frame is in its container's space
    // and counts the container's offset twice.
    let middle = |light: &NSView| -> f64 {
        let area = light.convertRect_toView(light.bounds(), None);
        height - (area.origin.y + area.size.height / 2.0)
    };

    let before = middle(&close);
    // The three lights are in one container: moving it moves them together
    // and keeps the space between them.
    // SAFETY: a view's tree is read on the thread it lives on.
    let Some(container) = (unsafe { close.superview() }) else {
        return true;
    };
    // SAFETY: as above.
    let flipped = unsafe { container.superview() }.map(|parent| parent.isFlipped()).unwrap_or(false);
    let Some(by) = crate::shift(before, target, flipped) else {
        return true;
    };
    let parked = container.frame();
    let mut moved = parked;
    moved.origin.y += by;
    container.setFrame(moved);

    let after = middle(&close);
    if crate::went_wrong(before, after, target) {
        container.setFrame(parked);
        eprintln!("window controls: left where AppKit parks them; moving them took them from {before:.1} to {after:.1} points, not to {target:.1}");
    }
    true
}
