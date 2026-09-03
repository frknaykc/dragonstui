use dragons_tui::{Color, Frame, Rect, Sparkline, Style};

#[test]
fn sparkline_normalizes_recent_samples_into_styled_block_levels_at_target_offset() {
    let style = Style::new().fg(Color::rgb(1, 2, 3)).bold();
    let mut frame = Frame::new(8, 2);

    Sparkline::new([1.0, 2.0, 4.0, 8.0, 4.0])
        .style(style)
        .render(&mut frame, Rect::new(2, 1, 3, 1));

    let buffer = frame.buffer();
    assert_eq!(buffer.get(2, 1).unwrap().character, '▁');
    assert_eq!(buffer.get(3, 1).unwrap().character, '█');
    assert_eq!(buffer.get(4, 1).unwrap().character, '▁');
    assert_eq!(buffer.get(2, 1).unwrap().style, style);
}

#[test]
fn sparkline_uses_a_stable_middle_level_for_one_or_equal_samples_without_filling_extra_width() {
    let mut frame = Frame::new(4, 1);

    Sparkline::new([5.0, 5.0]).render(&mut frame, Rect::new(0, 0, 4, 1));

    let buffer = frame.buffer();
    assert_eq!(buffer.get(0, 0).unwrap().character, '▅');
    assert_eq!(buffer.get(1, 0).unwrap().character, '▅');
    assert_eq!(buffer.get(2, 0).unwrap().character, ' ');
}

#[test]
fn sparkline_handles_empty_negative_mixed_non_finite_and_tiny_targets_deterministically() {
    let style = Style::new().fg(Color::rgb(4, 5, 6));
    let mut frame = Frame::new(4, 1);

    Sparkline::new([])
        .style(style)
        .render(&mut frame, Rect::new(0, 0, 4, 1));
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, ' ');

    Sparkline::new([f64::NAN, -1.0, 1.0, f64::INFINITY])
        .style(style)
        .render(&mut frame, Rect::new(0, 0, 4, 1));
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, '▅');
    assert_eq!(frame.buffer().get(1, 0).unwrap().character, '▁');
    assert_eq!(frame.buffer().get(2, 0).unwrap().character, '█');
    assert_eq!(frame.buffer().get(3, 0).unwrap().character, '▅');
    assert_eq!(frame.buffer().get(0, 0).unwrap().style, style);

    Sparkline::new([f64::MIN, f64::MAX]).render(&mut frame, Rect::new(0, 0, 0, 0));
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, '▅');
}
