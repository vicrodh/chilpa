//! Where a dragged sidebar settles (design/02-layout.md 2, frontend
//! ADR-0011).
//!
//! Between 212 px and 40 % of the window the width is free. Dragged to the
//! left it snaps: under 150 px it collapses to its column of icons, under
//! 28 px it closes. The snap is live: while the pointer moves the sidebar
//! already shows where it would land, so it is never 100 px wide, and on
//! release it stays there. A snap slides as QBZ's does, and a line crossed
//! has to be passed by a margin to be crossed back, so a pointer resting
//! near one does not flip the sidebar on every move.

pub const OPEN_MIN: f64 = 212.0;
pub const MAX_SHARE: f64 = 0.40;
pub const COLLAPSE_BELOW: f64 = 150.0;
pub const CLOSE_BELOW: f64 = 28.0;
/// How far past a line a sidebar dragged back to the right has to go before
/// it opens again: collapsed it stays collapsed under 170 px, closed it
/// stays closed under 48 (QBZ's `REOPEN_MARGIN`, `panel_resize.rs`, where
/// without it "every pointer move [...] flipped the state").
pub const REOPEN_MARGIN: f64 = 20.0;
/// The right panel's own width, and the one point a drag of it snaps to.
pub const PANEL_DEFAULT: f64 = 300.0;
/// How near its own width a dragged right panel has to be to take it.
pub const PANEL_SNAP: f64 = 14.0;

/// The mode and, when open, the width a drag released at `width` leaves.
pub fn settle(width: f64, window_width: f64) -> (&'static str, i32) {
    if width < CLOSE_BELOW {
        ("closed", 0)
    } else if width < COLLAPSE_BELOW {
        ("collapsed", 0)
    } else {
        let max = (window_width * MAX_SHARE).max(OPEN_MIN);
        ("open", width.clamp(OPEN_MIN, max).round() as i32)
    }
}

/// Where a drag at `width` lands for a sidebar that shows `mode` now.
pub fn settle_from(mode: &str, width: f64, window_width: f64) -> (&'static str, i32) {
    let landed = settle(width, window_width);
    match (mode, landed.0) {
        ("closed", "collapsed" | "open") if width < CLOSE_BELOW + REOPEN_MARGIN => ("closed", 0),
        ("collapsed", "open") if width < COLLAPSE_BELOW + REOPEN_MARGIN => ("collapsed", 0),
        _ => landed,
    }
}

/// The width a right panel dragged to `width` has.
///
/// It is free between the sidebar's minimum and 40 % of the window, the
/// same room an open sidebar has, and next to its own width it takes that
/// width: the one point it snaps to. It has no column of icons to collapse
/// to and a drag does not close it; it is open at some width, or closed
/// from its buttons.
pub fn settle_panel(width: f64, window_width: f64) -> i32 {
    let max = (window_width * MAX_SHARE).max(OPEN_MIN);
    let width = if (width - PANEL_DEFAULT).abs() <= PANEL_SNAP { PANEL_DEFAULT } else { width };
    width.clamp(OPEN_MIN, max).round() as i32
}

#[cfg(test)]
mod tests {
    use super::{settle, settle_from, settle_panel};

    #[test]
    fn a_line_crossed_is_crossed_back_only_past_a_margin() {
        assert_eq!(settle_from("open", 160.0, 1280.0), ("open", 212));
        assert_eq!(settle_from("open", 149.0, 1280.0).0, "collapsed");
        assert_eq!(settle_from("collapsed", 160.0, 1280.0).0, "collapsed", "between the line and the margin it holds");
        assert_eq!(settle_from("collapsed", 170.0, 1280.0), ("open", 212));
        assert_eq!(settle_from("collapsed", 27.0, 1280.0).0, "closed");
        assert_eq!(settle_from("closed", 40.0, 1280.0).0, "closed");
        assert_eq!(settle_from("closed", 48.0, 1280.0).0, "collapsed");
        assert_eq!(settle_from("closed", 300.0, 1280.0), ("open", 300), "far past both it simply lands");
    }

    #[test]
    fn the_right_panel_is_free_between_the_sidebars_limits() {
        assert_eq!(settle_panel(250.0, 1280.0), 250);
        assert_eq!(settle_panel(420.0, 1280.0), 420);
        assert_eq!(settle_panel(900.0, 1280.0), 512, "40 % of 1280");
        assert_eq!(settle_panel(100.0, 1280.0), 212, "never narrower than an open sidebar, and never closed by a drag");
        assert_eq!(settle_panel(0.0, 1280.0), 212);
    }

    #[test]
    fn the_right_panel_snaps_only_to_its_own_width() {
        assert_eq!(settle_panel(290.0, 1280.0), 300);
        assert_eq!(settle_panel(314.0, 1280.0), 300);
        assert_eq!(settle_panel(315.0, 1280.0), 315, "past the snap it is free again");
        assert_eq!(settle_panel(285.0, 1280.0), 285);
        assert_eq!(settle_panel(300.0, 600.0), 240, "a small window gives it less than its own width");
    }

    #[test]
    fn a_drag_to_the_left_snaps_to_collapsed_then_closed() {
        assert_eq!(settle(149.0, 1280.0).0, "collapsed");
        assert_eq!(settle(28.0, 1280.0).0, "collapsed");
        assert_eq!(settle(27.0, 1280.0).0, "closed");
    }

    #[test]
    fn open_is_free_between_its_minimum_and_forty_percent() {
        assert_eq!(settle(150.0, 1280.0), ("open", 212), "too narrow to be open widens");
        assert_eq!(settle(300.0, 1280.0), ("open", 300));
        assert_eq!(settle(900.0, 1280.0), ("open", 512), "40 % of 1280");
    }

    #[test]
    fn a_small_window_still_allows_the_minimum() {
        assert_eq!(settle(400.0, 400.0), ("open", 212));
    }
}
