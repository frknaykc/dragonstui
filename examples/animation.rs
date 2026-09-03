//! Animation is explicit: update application state, then render the current frame.

use std::{
    io,
    time::{Duration, Instant},
};

use dragons_tui::{Animation, Color, Frame, Rect, Style, Text, diff, render_changed_cells};

fn main() -> io::Result<()> {
    let start = Instant::now();
    let mut animation =
        Animation::new(["⠋", "⠙", "⠹", "⠸"]).frame_duration(Duration::from_millis(100));
    let _ = animation.update(start);
    let _changed = animation.update(start + Duration::from_millis(100));

    let mut frame = Frame::new(32, 3);
    Text::new(format!("{} Rendering", animation.current().unwrap_or(&" ")))
        .style(Style::new().fg(Color::rgb(255, 174, 32)).bold())
        .render(&mut frame, Rect::new(0, 1, 32, 1));
    render_changed_cells(&mut io::stdout(), &diff(None, frame.buffer()), true)
}
