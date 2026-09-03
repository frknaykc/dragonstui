//! Draw in logical Braille dots, then render the owned canvas into a frame rectangle.

use std::io;

use dragons_tui::{Canvas, Color, Frame, Rect, Style, diff, render_changed_cells};

fn main() -> io::Result<()> {
    let mut canvas = Canvas::new(28, 8);
    canvas.draw_rect(0, 0, canvas.logical_width(), canvas.logical_height());
    canvas.draw_line(
        0,
        0,
        canvas.logical_width() as i32 - 1,
        canvas.logical_height() as i32 - 1,
    );
    canvas.draw_line(
        0,
        canvas.logical_height() as i32 - 1,
        canvas.logical_width() as i32 - 1,
        0,
    );

    let mut frame = Frame::new(28, 8);
    canvas.render(
        &mut frame,
        Rect::new(0, 0, 28, 8),
        Style::new().fg(Color::rgb(255, 174, 32)),
    );
    render_changed_cells(&mut io::stdout(), &diff(None, frame.buffer()), true)
}
