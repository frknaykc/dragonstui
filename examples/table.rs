//! Tables retain rich cell content while selection and scrolling stay in caller-owned state.

use std::io;

use dragons_tui::{
    Alignment, Color, Constraint, Frame, Line, Rect, Span, Style, Table, TableColumn, TableState,
    diff, render_changed_cells,
};

fn main() -> io::Result<()> {
    let accent = Style::new().fg(Color::rgb(255, 174, 32)).bold();
    let table = Table::new([
        TableColumn::new(Constraint::Fill(2)),
        TableColumn::new(Constraint::Fill(1)).alignment(Alignment::Right),
    ])
    .header([Line::from("SERVICE"), Line::from("STATUS")])
    .rows([
        vec![
            Line::new([Span::styled("Renderer", accent)]),
            Line::from("Ready"),
        ],
        vec![Line::from("Unicode"), Line::from("Checked")],
        vec![Line::from("Canvas"), Line::from("Active")],
    ])
    .selected_style(Style::new().bg(Color::rgb(70, 20, 10)));
    let mut state = TableState::new();
    state.next(3);

    let mut frame = Frame::new(50, 7);
    table.render(&mut frame, Rect::new(0, 0, 50, 7), &mut state);
    render_changed_cells(&mut io::stdout(), &diff(None, frame.buffer()), true)
}
