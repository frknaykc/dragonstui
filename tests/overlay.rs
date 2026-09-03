use dragons_tui::{
    Cell, Color, Frame, Line, Modal, Rect, Style, centered_percent_rect, centered_rect,
};

#[test]
fn centered_popup_geometry_clamps_to_the_parent_and_modal_overwrites_background() {
    assert_eq!(
        centered_rect(Rect::new(2, 3, 10, 8), 6, 4),
        Rect::new(4, 5, 6, 4)
    );
    assert_eq!(
        centered_percent_rect(Rect::new(0, 0, 5, 3), 200, 200),
        Rect::new(0, 0, 5, 3)
    );

    let base = Style::new().fg(Color::rgb(1, 2, 3));
    let border = Style::new().fg(Color::rgb(4, 5, 6));
    let mut frame = Frame::new(12, 8);
    frame.set_cell(3, 2, Cell::new('x', base));
    Modal::new("Permission", [Line::from("Execute command?")])
        .size(8, 4)
        .border_style(border)
        .title_style(border)
        .render(&mut frame, Rect::new(0, 0, 12, 8));

    assert_eq!(frame.buffer().get(2, 2), Some(&Cell::new('╭', border)));
    assert_eq!(frame.buffer().get(3, 2).unwrap().character, '─');
}
