use dragons_tui::{Cell, Color, Frame, Line, Rect, Span, Style};

#[test]
fn single_span_and_empty_line_are_safe_and_preserve_complete_style() {
    let style = Style::new()
        .fg(Color::Rgb {
            r: 10,
            g: 20,
            b: 30,
        })
        .bg(Color::Rgb {
            r: 40,
            g: 50,
            b: 60,
        })
        .bold()
        .dim()
        .italic()
        .underline();
    let mut frame = Frame::new(2, 1);

    Line::from([Span::styled("X", style)]).render(&mut frame, Rect::new(0, 0, 2, 1));
    Line::new(Vec::new()).render(&mut frame, Rect::new(1, 0, 1, 1));

    assert_eq!(frame.buffer().get(0, 0), Some(&Cell::new('X', style)));
    assert_eq!(frame.buffer().get(1, 0), Some(&Cell::default()));
}
