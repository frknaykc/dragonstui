use std::time::Instant;

use crate::Animation;

/// Small [`Animation`] wrapper for a single text frame such as a Braille spinner.
pub struct Spinner {
    animation: Animation<String>,
}

impl Spinner {
    pub fn new(frames: &[&str]) -> Option<Self> {
        (!frames.is_empty()).then(|| Self {
            animation: Animation::new(frames.iter().map(|frame| (*frame).to_owned())),
        })
    }

    pub fn braille() -> Self {
        Self::new(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .expect("the built-in spinner has frames")
    }

    pub fn current(&self) -> &str {
        self.animation
            .current()
            .map(String::as_str)
            .expect("a Spinner is constructed with at least one frame")
    }

    pub fn update(&mut self, now: Instant) -> bool {
        self.animation.update(now)
    }

    pub fn advance(&mut self) {
        self.animation.advance();
    }
}
