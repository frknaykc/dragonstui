//! Derive rectangles explicitly, then compose panels and text into their returned inner areas.

use std::io;

use dragons_tui::{
    Color, Constraint, Frame, Layout, Panel, Rect, Style, Text, diff, render_changed_cells,
};

fn main() -> io::Result<()> {
    let mut frame = Frame::new(60, 12);
    let root = Rect::new(0, 0, 60, 12);
    let areas = Layout::vertical(vec![Constraint::Length(3), Constraint::Fill(1)])
        .gap(1)
        .split(root);
    let title = Panel::new("Layout")
        .border_style(Style::new().fg(Color::rgb(220, 70, 10)))
        .render(&mut frame, areas[0]);
    Text::new("Application code owns these rectangles.")
        .style(Style::new().fg(Color::rgb(240, 225, 205)))
        .render(&mut frame, title);

    let body = Panel::new("Body")
        .border_style(Style::new().fg(Color::rgb(255, 174, 32)))
        .render(&mut frame, areas[1]);
    Text::new("No component tree is required.")
        .style(Style::new().fg(Color::rgb(245, 130, 20)))
        .render(&mut frame, body);

    render_changed_cells(&mut io::stdout(), &diff(None, frame.buffer()), true)
}
