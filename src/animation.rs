use std::time::{Duration, Instant};

/// Time-driven, application-owned sequence of frames.
///
/// Call [`Animation::update`] with the current instant and render [`Animation::current`] when it
/// reports a change; the type does not schedule redraws or own a runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Animation<T> {
    frames: Vec<T>,
    index: usize,
    frame_duration: Duration,
    looped: bool,
    completed: bool,
    last_update: Option<Instant>,
}

impl<T> Animation<T> {
    pub fn new(frames: impl IntoIterator<Item = T>) -> Self {
        let frames: Vec<T> = frames.into_iter().collect();
        let completed = frames.is_empty();
        Self {
            frames,
            index: 0,
            frame_duration: Duration::from_millis(100),
            looped: true,
            completed,
            last_update: None,
        }
    }

    /// A zero duration pauses the animation so updates remain safe no-ops.
    pub fn frame_duration(mut self, frame_duration: Duration) -> Self {
        self.frame_duration = frame_duration;
        self
    }

    pub fn looped(mut self, looped: bool) -> Self {
        self.looped = looped;
        self.completed = self.frames.is_empty() || (!looped && self.frames.len() == 1);
        self
    }

    pub fn current(&self) -> Option<&T> {
        self.frames.get(self.index)
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }

    pub fn update(&mut self, now: Instant) -> bool {
        if self.frames.is_empty() || self.completed || self.frame_duration.is_zero() {
            self.last_update = Some(now);
            return false;
        }

        let Some(last_update) = self.last_update else {
            self.last_update = Some(now);
            return false;
        };
        let elapsed = now.saturating_duration_since(last_update);
        let frame_nanos = self.frame_duration.as_nanos();
        let elapsed_nanos = elapsed.as_nanos();
        let available_steps = elapsed_nanos / frame_nanos;
        if available_steps == 0 {
            return false;
        }

        let remainder = duration_from_nanos(elapsed_nanos % frame_nanos);
        self.last_update = now.checked_sub(remainder).or(Some(now));
        self.advance_by(available_steps)
    }

    pub fn advance(&mut self) -> bool {
        self.advance_by(1)
    }

    fn advance_by(&mut self, available_steps: u128) -> bool {
        if self.frames.is_empty() || self.completed {
            return false;
        }

        if self.looped {
            let steps = (available_steps % self.frames.len() as u128) as usize;
            let next_index = (self.index + steps) % self.frames.len();
            let changed = next_index != self.index;
            self.index = next_index;
            return changed;
        }

        let remaining_steps = self
            .frames
            .len()
            .saturating_sub(1)
            .saturating_sub(self.index);
        let steps = available_steps.min(remaining_steps as u128) as usize;
        self.index += steps;
        self.completed = self.index + 1 == self.frames.len();
        steps > 0
    }
}

fn duration_from_nanos(nanos: u128) -> Duration {
    const NANOS_PER_SECOND: u128 = 1_000_000_000;
    Duration::new(
        (nanos / NANOS_PER_SECOND) as u64,
        (nanos % NANOS_PER_SECOND) as u32,
    )
}
