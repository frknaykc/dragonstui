use dragons_tui::{Alignment, Cell, Color, Frame, Line, Rect, RichText, Span, Style};

#[test]
fn line_renders_ordered_spans_with_independent_styles() {
    let name_style = Style::new()
        .fg(Color::Rgb {
            r: 20,
            g: 40,
            b: 60,
        })
        .bold();
    let status_style = Style::new()
        .fg(Color::Rgb {
            r: 70,
            g: 100,
            b: 130,
        })
        .dim();
    let line = Line::from([
        Span::styled("Codex ", name_style),
        Span::styled("● Working", status_style),
    ])
    .alignment(Alignment::Center);
    let mut frame = Frame::new(20, 1);

    line.render(&mut frame, Rect::new(0, 0, 20, 1));

    assert_eq!(frame.buffer().get(2, 0), Some(&Cell::new('C', name_style)));
    assert_eq!(
        frame.buffer().get(8, 0),
        Some(&Cell::new('●', status_style))
    );
    assert_eq!(
        frame.buffer().get(10, 0),
        Some(&Cell::new('W', status_style))
    );
}

#[test]
fn rich_text_renders_visible_lines_and_clips_vertical_overflow() {
    let style = Style::new().italic();
    let text = RichText::new([
        Line::from([Span::styled("first", style)]),
        Line::from([Span::styled("second", style)]),
        Line::from([Span::styled("third", style)]),
    ]);
    let mut frame = Frame::new(8, 3);

    text.render(&mut frame, Rect::new(1, 1, 6, 1));

    assert_eq!(frame.buffer().get(1, 1), Some(&Cell::new('f', style)));
    assert_eq!(frame.buffer().get(1, 2), Some(&Cell::default()));
}

#[test]
fn rich_text_can_apply_one_alignment_to_all_of_its_lines() {
    let style = Style::new().underline();
    let text = RichText::new([
        Line::from([Span::styled("İ", style)]),
        Line::from([Span::styled("你好", style)]),
    ])
    .alignment(Alignment::Right);
    let mut frame = Frame::new(8, 2);

    text.render(&mut frame, Rect::new(0, 0, 8, 2));

    assert_eq!(frame.buffer().get(7, 0), Some(&Cell::new('İ', style)));
    assert_eq!(frame.buffer().get(4, 1).unwrap().character, '你');
    assert_eq!(frame.buffer().get(6, 1).unwrap().character, '好');
}
