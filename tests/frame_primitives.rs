use dragons_tui::{Cell, Color, Frame, Rect, Style};

#[test]
fn frame_clips_text_to_its_rect_and_the_buffer() {
    let mut frame = Frame::new(10, 4);
    let style = Style::new();
    let rect = Rect::new(2, 1, 4, 2);

    assert_eq!(frame.write_text_in(rect, 3, 0, "AB", style), 1);
    assert_eq!(frame.buffer().get(5, 1), Some(&Cell::new('A', style)));
    assert_eq!(frame.buffer().get(6, 1), Some(&Cell::default()));
    assert_eq!(frame.write_text_in(rect, 0, 2, "outside", style), 0);

    assert_eq!(
        frame.write_text_in(Rect::new(8, 0, 5, 1), 0, 0, "ABC", style),
        2
    );
    assert_eq!(frame.buffer().get(8, 0), Some(&Cell::new('A', style)));
    assert_eq!(frame.buffer().get(9, 0), Some(&Cell::new('B', style)));
}

#[test]
fn draw_border_handles_normal_small_and_partially_out_of_bounds_rects() {
    let style = Style::new().fg(Color::Rgb { r: 1, g: 2, b: 3 }).bold();
    let mut frame = Frame::new(6, 4);

    frame.draw_border(Rect::new(1, 0, 4, 4), style);

    assert_eq!(frame.buffer().get(1, 0), Some(&Cell::new('╭', style)));
    assert_eq!(frame.buffer().get(4, 3), Some(&Cell::new('╯', style)));
    assert_eq!(frame.buffer().get(1, 1), Some(&Cell::new('│', style)));
    assert_eq!(frame.buffer().get(2, 0), Some(&Cell::new('─', style)));

    frame.draw_border(Rect::new(0, 0, 1, 1), style);
    assert_eq!(frame.buffer().get(0, 0), Some(&Cell::new('╭', style)));

    frame.draw_border(Rect::new(5, 2, 4, 3), style);
    assert_eq!(frame.buffer().get(5, 2), Some(&Cell::new('╭', style)));
}
