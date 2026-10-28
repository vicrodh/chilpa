//! The window's own controls on a Mac: the three lights stay the system's,
//! and sit at the height of the bar they float over.
//!
//! The window is not frameless there. Frameless takes the lights away with
//! the frame, and a drawn set is a second set of buttons that behaves like
//! none a Mac has. The window asks Qt for its client area to reach under
//! the title bar and for the bar's background to go; what Qt does not do is
//! what is done here: hide the title's text and move the lights down.
//! AppKit parks them for a title bar of about 28 points, and the bar they
//! float over is taller, so left alone they sit above the middle of
//! everything beside them.
//!
//! The style mask is never touched from here. Qt computes it again from
//! its own flags on every change of state and assigns it over whatever was
//! set behind its back.
//!
//! The arithmetic is apart from AppKit, so it is tested wherever the
//! workspace is.

/// How far the lights may be from the target and count as there, in points.
const CLOSE_ENOUGH: f64 = 0.5;

/// What to add to the `y` of the lights' container so their middle lands
/// `target` points under the window's top, when it is `at` points under it
/// now. Nothing when they are already there. A flipped parent counts `y`
/// downward, any other upward.
pub fn shift(at: f64, target: f64, parent_flipped: bool) -> Option<f64> {
    let down = target - at;
    if down.abs() < CLOSE_ENOUGH {
        return None;
    }
    Some(if parent_flipped { down } else { -down })
}

/// Whether a move left the lights further from the target than they were.
/// Where AppKit parks them is acceptable; a wrong move is not, and is
/// undone.
pub fn went_wrong(before: f64, after: f64, target: f64) -> bool {
    (after - target).abs() > (before - target).abs()
}

#[cfg(target_os = "macos")]
mod lights;

pub mod glyph;
/// The menu-bar item.
#[cfg(target_os = "macos")]
pub mod tray;
/// The Dock's click on a hidden window.
#[cfg(target_os = "macos")]
pub mod reopen;

/// Hides the title's text and puts the middle of the lights `target` points
/// under the window's top. False while there is no window to do it to: the
/// caller asks again. It can be asked any number of times; lights already
/// there are not moved. On the thread the window lives on, and only there.
#[cfg(target_os = "macos")]
pub fn center_window_controls(target: f64) -> bool {
    lights::center(target)
}

/// Off a Mac the window draws its own controls and there is nothing to do.
#[cfg(not(target_os = "macos"))]
pub fn center_window_controls(_target: f64) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lights_parked_high_go_down_by_the_difference() {
        assert_eq!(shift(14.0, 20.0, true), Some(6.0));
    }

    #[test]
    fn a_parent_that_counts_upward_gets_the_opposite_sign() {
        assert_eq!(shift(14.0, 20.0, false), Some(-6.0));
    }

    #[test]
    fn lights_parked_low_go_up() {
        assert_eq!(shift(26.0, 20.0, true), Some(-6.0));
    }

    #[test]
    fn lights_already_there_are_not_moved() {
        assert_eq!(shift(20.3, 20.0, true), None);
        assert_eq!(shift(19.6, 20.0, false), None);
    }

    #[test]
    fn asking_again_after_the_move_moves_nothing() {
        let at = 14.0;
        let moved = at + shift(at, 20.0, true).unwrap();
        assert_eq!(shift(moved, 20.0, true), None);
    }

    #[test]
    fn a_move_that_ends_further_away_went_wrong() {
        assert!(went_wrong(14.0, 8.0, 20.0));
        assert!(!went_wrong(14.0, 20.0, 20.0));
        assert!(!went_wrong(14.0, 24.0, 20.0));
    }
}
