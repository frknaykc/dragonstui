use dragons_tui::{Alignment, Cell, CellKind, Color, Frame, Line, Rect, Span, Style, diff};

#[test]
fn spans_and_lines_preserve_raw_empty_and_styled_content() {
    let styled = Style::new()
        .fg(Color::Rgb { r: 1, g: 2, b: 3 })
        .bg(Color::Rgb { r: 4, g: 5, b: 6 })
        .bold()
        .dim()
        .italic()
        .underline();
    let raw = Span::raw("");
    let emphasized = Span::styled("ready", styled);
    let line = Line::from([raw.clone(), emphasized.clone()]);

    assert_eq!(raw.content(), "");
    assert_eq!(raw.style(), Style::new());
    assert_eq!(emphasized.content(), "ready");
    assert_eq!(emphasized.style(), styled);
    assert_eq!(line.spans(), &[raw, emphasized]);
    assert_eq!(line.display_width(), 5);
}

#[test]
fn rich_line_alignment_uses_display_width_for_turkish_cjk_emoji_and_oversized_content() {
    let style = Style::new();
    let mut left = Frame::new(8, 1);
    Line::from([Span::styled("İ", style)]).render(&mut left, Rect::new(0, 0, 8, 1));
    assert_eq!(left.buffer().get(0, 0), Some(&Cell::new('İ', style)));

    let mut center = Frame::new(8, 1);
    Line::from([Span::styled("你", style), Span::styled("好", style)])
        .alignment(Alignment::Center)
        .render(&mut center, Rect::new(0, 0, 8, 1));
    assert_eq!(center.buffer().get(2, 0).unwrap().character, '你');
    assert_eq!(center.buffer().get(4, 0).unwrap().character, '好');

    let mut right = Frame::new(8, 1);
    Line::from([Span::styled("🚀", style)])
        .alignment(Alignment::Right)
        .render(&mut right, Rect::new(0, 0, 8, 1));
    assert_eq!(right.buffer().get(6, 0).unwrap().character, '🚀');

    let mut oversized = Frame::new(5, 1);
    Line::from([Span::styled("x".repeat(65_537), style)])
        .alignment(Alignment::Right)
        .render(&mut oversized, Rect::new(0, 0, 5, 1));
    assert_eq!(oversized.buffer().get(0, 0).unwrap().character, 'x');
    assert_eq!(oversized.buffer().get(4, 0).unwrap().character, 'x');
}

#[test]
fn rich_line_clips_at_boundaries_and_ignores_zero_or_outside_rects() {
    let first = Style::new().bold();
    let second = Style::new().italic();
    let line = Line::from([Span::styled("AB", first), Span::styled("CD", second)]);

    let mut boundary = Frame::new(4, 1);
    line.render(&mut boundary, Rect::new(0, 0, 2, 1));
    assert_eq!(boundary.buffer().get(0, 0), Some(&Cell::new('A', first)));
    assert_eq!(boundary.buffer().get(1, 0), Some(&Cell::new('B', first)));
    assert_eq!(boundary.buffer().get(2, 0), Some(&Cell::default()));

    let mut tiny = Frame::new(2, 1);
    line.render(&mut tiny, Rect::new(0, 0, 0, 1));
    line.render(&mut tiny, Rect::new(2, 0, 1, 1));
    assert_eq!(tiny.buffer().get(0, 0), Some(&Cell::default()));
    assert_eq!(tiny.buffer().get(1, 0), Some(&Cell::default()));
}

#[test]
fn rich_line_uses_the_existing_wide_cell_contract_across_span_boundaries() {
    let first = Style::new().fg(Color::Rgb { r: 1, g: 2, b: 3 });
    let second = Style::new().fg(Color::Rgb { r: 4, g: 5, b: 6 });
    let third = Style::new().fg(Color::Rgb { r: 7, g: 8, b: 9 });
    let line = Line::from([
        Span::styled("你", first),
        Span::styled("好", second),
        Span::styled("🚀", third),
    ]);
    let mut exact = Frame::new(6, 1);

    line.render(&mut exact, Rect::new(0, 0, 6, 1));

    assert_eq!(exact.buffer().get(0, 0).unwrap().kind, CellKind::Wide);
    assert_eq!(
        exact.buffer().get(1, 0).unwrap().kind,
        CellKind::WideContinuation
    );
    assert_eq!(exact.buffer().get(0, 0).unwrap().style, first);
    assert_eq!(exact.buffer().get(2, 0).unwrap().character, '好');
    assert_eq!(exact.buffer().get(2, 0).unwrap().style, second);
    assert_eq!(exact.buffer().get(4, 0).unwrap().character, '🚀');
    assert_eq!(exact.buffer().get(4, 0).unwrap().style, third);

    let mut short = Frame::new(2, 1);
    Line::from([Span::styled("A", first), Span::styled("你", second)])
        .render(&mut short, Rect::new(0, 0, 2, 1));
    assert_eq!(short.buffer().get(0, 0), Some(&Cell::new('A', first)));
    assert_eq!(short.buffer().get(1, 0), Some(&Cell::default()));
}

#[test]
fn rich_style_changes_are_visible_to_diff_but_identical_cells_are_not() {
    let red = Style::new().fg(Color::Rgb { r: 255, g: 0, b: 0 });
    let green = Style::new().fg(Color::Rgb { r: 0, g: 255, b: 0 });
    let mut previous = Frame::new(1, 1);
    let mut current = Frame::new(1, 1);
    Line::from([Span::styled("X", red)]).render(&mut previous, Rect::new(0, 0, 1, 1));
    Line::from([Span::styled("X", green)]).render(&mut current, Rect::new(0, 0, 1, 1));

    let changed = diff(Some(previous.buffer()), current.buffer());
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].current, Cell::new('X', green));

    let same = current.clone();
    assert!(diff(Some(current.buffer()), same.buffer()).is_empty());
}
