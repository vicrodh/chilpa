//! How the lines of the lyrics are drawn around the one being sung.

/// How lit a line of lyrics is, from 0 to 1.
///
/// The sung line is whole. With strong dimming the others fade the further
/// they are from it, and what is to come fades harder than what was
/// already sung: the eye is led to the line, and to what it just heard
/// before what it has not. Soft dimming sets every other line alike, and
/// off leaves them all whole. Lyrics that do not follow the music have no
/// line to fade around: they are all a little short of whole. `position`
/// is the line's place and `current` the place of the sung one, or
/// negative while none is.
pub fn lyric_dimming(mode: &str, synced: bool, position: i32, current: i32) -> f64 {
    if !synced {
        return 0.85;
    }
    if mode == "off" || current < 0 || position == current {
        return 1.0;
    }
    if mode == "soft" {
        return 0.6;
    }
    let ladder = if position < current { [0.5, 0.35, 0.25, 0.18] } else { [0.4, 0.2, 0.08, 0.03] };
    let distance = position.abs_diff(current) as usize;
    ladder[distance.clamp(1, ladder.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::lyric_dimming;

    #[test]
    fn the_lines_fade_with_their_distance_and_harder_ahead() {
        let lit = |position| lyric_dimming("strong", true, position, 10);
        assert_eq!(lit(10), 1.0, "the sung line");
        assert_eq!([lit(9), lit(8), lit(7), lit(6), lit(0)], [0.5, 0.35, 0.25, 0.18, 0.18], "what was sung, going back");
        assert_eq!([lit(11), lit(12), lit(13), lit(14), lit(90)], [0.4, 0.2, 0.08, 0.03, 0.03], "what is to come");
        assert!(lit(11) < lit(9), "ahead fades harder than behind");
    }

    #[test]
    fn softer_dimming_and_none() {
        assert_eq!(lyric_dimming("soft", true, 3, 10), 0.6);
        assert_eq!(lyric_dimming("soft", true, 10, 10), 1.0);
        assert_eq!(lyric_dimming("off", true, 3, 10), 1.0);
    }

    #[test]
    fn with_no_line_to_fade_around_nothing_fades() {
        assert_eq!(lyric_dimming("strong", true, 3, -1), 1.0, "before the first line is sung");
        assert_eq!(lyric_dimming("strong", false, 3, 10), 0.85, "lyrics that do not follow the music");
    }
}
