//! Where a dragged sidebar settles (design/02-layout.md 2, frontend
//! ADR-0011).
//!
//! Between 212 px and 40 % of the window the width is free. Dragged to the
//! left it snaps: under 150 px it collapses to its column of icons, under
//! 28 px it closes. The snap is live: while the pointer moves the sidebar
//! already shows where it would land, so it is never 100 px wide, and on
//! release it stays there. Nothing animates.

pub const OPEN_MIN: f64 = 212.0;
pub const MAX_SHARE: f64 = 0.40;
pub const COLLAPSE_BELOW: f64 = 150.0;
pub const CLOSE_BELOW: f64 = 28.0;

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

#[cfg(test)]
mod tests {
    use super::settle;

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
