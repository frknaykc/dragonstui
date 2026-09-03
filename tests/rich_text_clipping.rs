use dragons_tui::{Cell, Color, Frame, Line, Rect, RichText, Span, Style};

#[test]
fn line_clips_across_span_boundaries_without_losing_the_visible_span_style() {
    let first = Style::new().fg(Color::Rgb { r: 1, g: 2, b: 3 }).bold();
    let second = Style::new().fg(Color::Rgb { r: 4, g: 5, b: 6 }).italic();
    let final_style = Style::new().underline();
    let line = Line::from([
        Span::styled("Hi", first),
        Span::styled("Dragons", second),
        Span::styled("!", final_style),
    ]);
    let mut frame = Frame::new(6, 1);

    line.render(&mut frame, Rect::new(1, 0, 5, 1));

    assert_eq!(frame.buffer().get(1, 0), Some(&Cell::new('H', first)));
    assert_eq!(frame.buffer().get(2, 0), Some(&Cell::new('i', first)));
    assert_eq!(frame.buffer().get(3, 0), Some(&Cell::new('D', second)));
    assert_eq!(frame.buffer().get(5, 0), Some(&Cell::new('a', second)));
    assert_ne!(frame.buffer().get(5, 0), Some(&Cell::new('!', final_style)));
}

#[test]
fn line_clips_inside_first_and_final_spans() {
    let first = Style::new().bold();
    let final_style = Style::new().underline();
    let mut first_clipped = Frame::new(3, 1);
    Line::from([
        Span::styled("Hello", first),
        Span::styled(" world", final_style),
    ])
    .render(&mut first_clipped, Rect::new(0, 0, 3, 1));
    assert_eq!(
        first_clipped.buffer().get(0, 0),
        Some(&Cell::new('H', first))
    );
    assert_eq!(
        first_clipped.buffer().get(2, 0),
        Some(&Cell::new('l', first))
    );

    let mut final_clipped = Frame::new(4, 1);
    Line::from([
        Span::styled("A", first),
        Span::styled("B", first),
        Span::styled("CDE", final_style),
    ])
    .render(&mut final_clipped, Rect::new(0, 0, 4, 1));
    assert_eq!(
        final_clipped.buffer().get(2, 0),
        Some(&Cell::new('C', final_style))
    );
    assert_eq!(
        final_clipped.buffer().get(3, 0),
        Some(&Cell::new('D', final_style))
    );
}

#[test]
fn rich_text_ignores_zero_height_targets() {
    let mut frame = Frame::new(2, 1);
    RichText::new([Line::from([Span::raw("text")])]).render(&mut frame, Rect::new(0, 0, 2, 0));

    assert_eq!(frame.buffer().get(0, 0), Some(&Cell::default()));
    assert_eq!(frame.buffer().get(1, 0), Some(&Cell::default()));
}
