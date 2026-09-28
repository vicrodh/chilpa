//! I keep personal ratings source-independent and report failed writes to the shell.

use crate::Chilpa;
pub use chilpa_library::store::ratings::{RatingTarget, Ratings};

impl Chilpa {
    pub fn ratings(&mut self) -> Option<Ratings> {
        let result = self.library()?.ratings();
        match result {
            Ok(values) => Some(values),
            Err(error) => {
                self.now.error =
                    chilpa_i18n::t_args("ratings could not be loaded: {}", &[&error.to_string()]);
                None
            }
        }
    }

    pub fn rate(&mut self, target: RatingTarget, stars: i32) -> bool {
        if !(0..=5).contains(&stars) {
            self.now.error = chilpa_i18n::t("a rating must be between 0 and 5 stars");
            return false;
        }
        let Some(lib) = self.library() else {
            return false;
        };
        match lib.rate(target, stars as u8) {
            Ok(()) => true,
            Err(error) => {
                self.now.error =
                    chilpa_i18n::t_args("the rating could not be saved: {}", &[&error.to_string()]);
                false
            }
        }
    }
}
