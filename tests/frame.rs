use dragons_tui::{Cell, Color, Frame, Style};

#[test]
fn frame_renders_cells_into_its_buffer() {
    let mut frame = Frame::new(4, 2);
    let style = Style::new().fg(Color::Rgb {
        r: 140,
        g: 200,
        b: 255,
    });

    assert!(frame.set_cell(1, 1, Cell::new('D', style)));
    assert_eq!(frame.buffer().get(1, 1), Some(&Cell::new('D', style)));
    assert_eq!(frame.write_text(2, 0, "TUI", style), 2);
    assert_eq!(frame.buffer().get(2, 0), Some(&Cell::new('T', style)));
    assert_eq!(frame.buffer().get(3, 0), Some(&Cell::new('U', style)));
}
