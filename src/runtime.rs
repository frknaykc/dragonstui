use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crate::{
    Buffer, Event, Frame, Position, diff, render_changed_cells, set_cursor, terminal::poll_event,
};

/// Immediate-mode redraw runtime with one previous [`Buffer`] for diffing.
///
/// Runtime does not retain a UI/component tree. Applications own event dispatch and call
/// [`Runtime::render_with_cursor`] after constructing each frame.
pub struct Runtime {
    previous: Option<Buffer>,
    tick_interval: Option<Duration>,
    last_tick: Instant,
    needs_redraw: bool,
}

impl Runtime {
    pub fn new(tick_interval: Option<Duration>) -> Self {
        Self {
            previous: None,
            tick_interval,
            last_tick: Instant::now(),
            needs_redraw: true,
        }
    }

    pub fn next_event(&mut self) -> io::Result<Event> {
        loop {
            let timeout = match self.tick_interval {
                Some(interval) => {
                    let now = Instant::now();
                    if tick_due(now, self.last_tick, interval) {
                        self.last_tick = now;
                        return Ok(Event::Tick(now));
                    }
                    interval.saturating_sub(now.saturating_duration_since(self.last_tick))
                }
                None => Duration::from_secs(24 * 60 * 60),
            };

            if let Some(event) = poll_event(timeout)? {
                return Ok(event);
            }

            if self.tick_interval.is_some() {
                let now = Instant::now();
                self.last_tick = now;
                return Ok(Event::Tick(now));
            }
        }
    }

    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    pub fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    pub fn render(&mut self, output: &mut impl Write, frame: Frame) -> io::Result<()> {
        self.render_with_cursor(output, frame, None)
    }

    pub fn render_with_cursor(
        &mut self,
        output: &mut impl Write,
        frame: Frame,
        cursor: Option<Position>,
    ) -> io::Result<()> {
        let current = frame.into_buffer();
        let resized = self.previous.as_ref().is_none_or(|previous| {
            previous.width() != current.width() || previous.height() != current.height()
        });
        let changed = diff(self.previous.as_ref(), &current);

        render_changed_cells(output, &changed, resized)?;
        set_cursor(output, cursor)?;
        self.previous = Some(current);
        self.needs_redraw = false;
        Ok(())
    }
}

/// Returns whether an animation tick interval has elapsed.
pub fn tick_due(now: Instant, last_tick: Instant, interval: Duration) -> bool {
    now.saturating_duration_since(last_tick) >= interval
}
