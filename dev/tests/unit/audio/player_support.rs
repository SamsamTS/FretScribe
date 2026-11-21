use super::*;

impl Player {
    pub(crate) fn headless(rate: u32) -> (Self, Engine) {
        Self::build(rate)
    }
}
