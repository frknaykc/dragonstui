//! Smallest explicit DragonsTUI render: build a frame, render a primitive, and encode its diff.

use std::io;

use dragons_tui::{Color, Frame, Rect, Style, Text, diff, render_changed_cells};

fn main() -> io::Result<()> {
    let mut frame = Frame::new(40, 5);
    Text::new("Hello from DragonsTUI")
        .style(Style::new().fg(Color::rgb(255, 174, 32)).bold())
        .render(&mut frame, Rect::new(0, 0, 40, 5));

    render_changed_cells(&mut io::stdout(), &diff(None, frame.buffer()), true)
}
