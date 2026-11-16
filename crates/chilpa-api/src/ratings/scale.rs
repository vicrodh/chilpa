//! A source's rating scale, and how its values read as Chilpa's stars.
//!
//! Chilpa rates with whole stars, 1 to 5; 0 is unrated. A source rates on
//! its own scale, said as its lowest rated value, its highest and its step:
//! a server's comes with its kind, a plugin's with its `rating` capability.
//!
//! - A value reads as `value * 5 / max` stars, halves up: on 0 to 10 a 7
//!   (three and a half) is 4 and a 1 (half a star) is 1. Anything above
//!   zero is at least one star; zero is unrated.
//! - Stars are written as `stars * max / 5`, on the nearest step from `min`
//!   and within `min` and `max`. Zero stars takes the rating off.
//! - What a source holds and Chilpa only shows rounded is never written
//!   back: only a change the listener made goes out (`sync`).

use serde_json::Value;

/// The widest scale a source may declare, and the most values on it.
const LARGEST: f64 = 1000.0;
const MOST_STEPS: f64 = 10_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Scale {
    min: f64,
    max: f64,
    step: f64,
}

impl Scale {
    /// `None` for a scale that cannot be one: below zero, upside down, too
    /// wide or with too fine a step.
    pub(crate) fn new(min: f64, max: f64, step: f64) -> Option<Scale> {
        let finite = min.is_finite() && max.is_finite() && step.is_finite();
        let sound = finite && min >= 0.0 && max > min && max <= LARGEST && step > 0.0 && step <= max - min.max(0.0) + f64::EPSILON;
        (sound && (max - min) / step <= MOST_STEPS).then_some(Scale { min, max, step })
    }

    /// A server kind's own scale.
    pub(crate) fn of_server(scale: chilpa_servers::rating::Scale) -> Option<Scale> {
        Scale::new(scale.min, scale.max, scale.step)
    }

    /// A value of the source as whole stars; 0 for unrated.
    pub(crate) fn stars(&self, value: f64) -> u8 {
        if !value.is_finite() || value <= 0.0 {
            return 0;
        }
        // Times five first: 7 * 5 / 10 is 3.5 exactly, 7 / 10 * 5 is not.
        let stars = value.min(self.max) * 5.0 / self.max;
        ((stars + 0.5 + 1e-9).floor() as i64).clamp(1, 5) as u8
    }

    /// Stars as a value of the source; `None` takes the rating off.
    pub(crate) fn value(&self, stars: u8) -> Option<f64> {
        if stars == 0 {
            return None;
        }
        let wanted = f64::from(stars.min(5)) * self.max / 5.0;
        let snapped = self.min + ((wanted - self.min) / self.step).round() * self.step;
        let value = snapped.clamp(self.min.max(self.step.min(self.max)), self.max);
        Some((value * 1_000_000.0).round() / 1_000_000.0)
    }
}

/// What a plugin declares in its `rating` capability: its scale and the
/// kinds of item it rates. Anything else, or a scale that cannot be one,
/// is no capability at all.
pub(crate) fn declared(capability: &Value) -> Option<(Scale, Vec<String>)> {
    let number = |key: &str| capability.get(key).and_then(Value::as_f64);
    let scale = Scale::new(number("min")?, number("max")?, number("step").unwrap_or(1.0))?;
    let kinds: Vec<String> = match capability.get("kinds") {
        None => vec!["track".into(), "album".into()],
        Some(Value::Array(kinds)) => kinds.iter().filter_map(Value::as_str).filter(|k| matches!(*k, "track" | "album")).map(str::to_string).collect(),
        Some(_) => return None,
    };
    (!kinds.is_empty()).then_some((scale, kinds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ten() -> Scale {
        Scale::new(1.0, 10.0, 1.0).unwrap()
    }

    #[test]
    fn ten_points_read_as_stars_with_halves_up() {
        let scale = ten();
        let read: Vec<u8> = (0..=10).map(|v| scale.stars(f64::from(v))).collect();
        assert_eq!(read, [0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5], "1 is half a star and shows as one");
        assert_eq!(scale.stars(7.3), 4);
        assert_eq!(scale.stars(0.2), 1, "anything rated is at least one star");
        assert_eq!(scale.stars(12.0), 5, "above the top is the top");
        assert_eq!(scale.stars(-1.0), 0);
        assert_eq!(scale.stars(f64::NAN), 0);
    }

    #[test]
    fn stars_are_written_on_the_source_s_steps_and_zero_takes_the_rating_off() {
        let scale = ten();
        let written: Vec<Option<f64>> = (0..=5).map(|s| scale.value(s)).collect();
        assert_eq!(written, [None, Some(2.0), Some(4.0), Some(6.0), Some(8.0), Some(10.0)]);
        let five = Scale::new(1.0, 5.0, 1.0).unwrap();
        assert_eq!((1..=5).map(|s| five.value(s).unwrap()).collect::<Vec<_>>(), [1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn half_star_and_hundred_point_scales_read_and_write_whole_stars() {
        let halves = Scale::new(0.5, 5.0, 0.5).unwrap();
        let read: Vec<u8> = [0.5, 1.0, 1.5, 2.5, 3.5, 4.5, 5.0].iter().map(|v| halves.stars(*v)).collect();
        assert_eq!(read, [1, 1, 2, 3, 4, 5, 5], "half stars go up");
        assert_eq!(halves.value(3), Some(3.0));
        let hundred = Scale::new(1.0, 100.0, 1.0).unwrap();
        assert_eq!((hundred.stars(69.0), hundred.stars(70.0), hundred.stars(9.0)), (3, 4, 1));
        assert_eq!(hundred.value(4), Some(80.0));
        let zero_to_ten = Scale::new(0.0, 10.0, 1.0).unwrap();
        assert_eq!(zero_to_ten.stars(0.0), 0, "a scale that starts at zero still reads zero as unrated");
        assert_eq!(zero_to_ten.value(1), Some(2.0));
    }

    #[test]
    fn whole_stars_written_read_back_as_the_same_stars() {
        for scale in [ten(), Scale::new(1.0, 5.0, 1.0).unwrap(), Scale::new(0.5, 5.0, 0.5).unwrap(), Scale::new(1.0, 100.0, 1.0).unwrap(), Scale::new(0.0, 10.0, 0.5).unwrap()] {
            for stars in 1..=5 {
                assert_eq!(scale.stars(scale.value(stars).unwrap()), stars, "{scale:?} {stars}");
            }
        }
        let coarse = Scale::new(1.0, 3.0, 1.0).unwrap();
        assert!((1..=5).all(|s| (1.0..=3.0).contains(&coarse.value(s).unwrap())), "never off the source's own scale");
    }

    #[test]
    fn a_scale_that_cannot_be_one_is_refused() {
        for (min, max, step) in [(-1.0, 5.0, 1.0), (5.0, 5.0, 1.0), (5.0, 1.0, 1.0), (0.0, 10.0, 0.0), (0.0, 10.0, 20.0),
            (0.0, 1e9, 1.0), (0.0, 100.0, 0.001), (f64::NAN, 5.0, 1.0)] {
            assert_eq!(Scale::new(min, max, step), None, "{min} {max} {step}");
        }
    }

    #[test]
    fn a_plugin_declares_its_scale_and_kinds() {
        assert_eq!(declared(&json!({"min": 1, "max": 10, "step": 1, "kinds": ["track"]})), Some((ten(), vec!["track".into()])));
        assert_eq!(declared(&json!({"min": 0.5, "max": 5, "step": 0.5})).map(|d| d.1), Some(vec!["track".to_string(), "album".to_string()]));
        assert_eq!(declared(&json!({"min": 1, "max": 5})).map(|d| d.0), Scale::new(1.0, 5.0, 1.0), "step defaults to 1");
        for refused in [json!(true), json!({"max": 5}), json!({"min": 1, "max": 5, "kinds": ["playlist"]}), json!({"min": 1, "max": 5, "kinds": "track"}), Value::Null] {
            assert_eq!(declared(&refused), None, "{refused}");
        }
    }
}
