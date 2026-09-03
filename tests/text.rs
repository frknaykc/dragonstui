use dragons_tui::{Alignment, Cell, Color, Frame, Rect, Style, Text};

#[test]
fn text_aligns_left_center_and_right() {
    let style = Style::new().fg(Color::Rgb { r: 1, g: 2, b: 3 });

    let mut left = Frame::new(10, 1);
    Text::new("Hi")
        .style(style)
        .render(&mut left, Rect::new(0, 0, 10, 1));
    assert_eq!(left.buffer().get(0, 0), Some(&Cell::new('H', style)));

    let mut center = Frame::new(10, 1);
    Text::new("Hi")
        .style(style)
        .alignment(Alignment::Center)
        .render(&mut center, Rect::new(0, 0, 10, 1));
    assert_eq!(center.buffer().get(4, 0), Some(&Cell::new('H', style)));

    let mut right = Frame::new(10, 1);
    Text::new("Hi")
        .style(style)
        .alignment(Alignment::Right)
        .render(&mut right, Rect::new(0, 0, 10, 1));
    assert_eq!(right.buffer().get(8, 0), Some(&Cell::new('H', style)));
}

#[test]
fn text_alignment_uses_unicode_display_width_and_clipping() {
    let mut frame = Frame::new(10, 1);
    Text::new("你好")
        .alignment(Alignment::Center)
        .render(&mut frame, Rect::new(0, 0, 10, 1));

    assert_eq!(frame.buffer().get(3, 0).unwrap().character, '你');
    assert_eq!(frame.buffer().get(5, 0).unwrap().character, '好');

    let mut clipped = Frame::new(3, 1);
    Text::new("你好").render(&mut clipped, Rect::new(0, 0, 3, 1));
    assert_eq!(clipped.buffer().get(0, 0).unwrap().character, '你');
    assert_eq!(clipped.buffer().get(2, 0), Some(&Cell::default()));
}

#[test]
fn oversized_text_does_not_wrap_its_display_width_during_alignment() {
    let content = "x".repeat(65_537);
    let mut frame = Frame::new(5, 1);

    Text::new(content)
        .alignment(Alignment::Right)
        .render(&mut frame, Rect::new(0, 0, 5, 1));

    assert_eq!(frame.buffer().get(0, 0).unwrap().character, 'x');
    assert_eq!(frame.buffer().get(4, 0).unwrap().character, 'x');
}

#[test]
fn text_does_not_render_outside_a_zero_sized_target_rect() {
    let mut frame = Frame::new(3, 1);

    Text::new("text").render(&mut frame, Rect::new(0, 0, 0, 0));

    for x in 0..3 {
        assert_eq!(frame.buffer().get(x, 0), Some(&Cell::default()));
    }
}
