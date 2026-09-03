use dragons_tui::{Cell, Color, Frame, Gauge, ProgressBar, Rect, Style, diff};

#[test]
fn progress_bar_uses_floor_rounding_and_preserves_filled_and_unfilled_styles() {
    let filled = Style::new().fg(Color::rgb(1, 2, 3));
    let unfilled = Style::new().fg(Color::rgb(4, 5, 6));
    let mut frame = Frame::new(8, 1);

    ProgressBar::new(0.25)
        .filled_style(filled)
        .unfilled_style(unfilled)
        .render(&mut frame, Rect::new(0, 0, 8, 1));

    let buffer = frame.buffer();
    for x in 0..2 {
        assert_eq!(buffer.get(x, 0).unwrap().character, '█');
        assert_eq!(buffer.get(x, 0).unwrap().style, filled);
    }
    for x in 2..8 {
        assert_eq!(buffer.get(x, 0).unwrap().character, '░');
        assert_eq!(buffer.get(x, 0).unwrap().style, unfilled);
    }
}

#[test]
fn progress_bar_clamps_non_finite_values_and_clips_its_label_below_the_bar() {
    let style = Style::new().fg(Color::rgb(7, 8, 9));
    let mut frame = Frame::new(4, 2);

    ProgressBar::new(f64::NAN)
        .filled_style(style)
        .unfilled_style(style)
        .label("Build 0%")
        .render(&mut frame, Rect::new(0, 0, 4, 2));

    let buffer = frame.buffer();
    assert_eq!(buffer.get(0, 0).unwrap().character, '░');
    assert_eq!(buffer.get(0, 1).unwrap().character, 'B');
    assert_eq!(buffer.get(3, 1).unwrap().character, 'l');
    assert_eq!(buffer.get(0, 1).unwrap().style, style);
}

#[test]
fn gauge_clamps_its_ratio_and_reuses_the_bar_label_and_style_contract() {
    let filled = Style::new().fg(Color::rgb(10, 11, 12));
    let unfilled = Style::new().fg(Color::rgb(13, 14, 15));
    let mut frame = Frame::new(4, 2);

    Gauge::new(1.2)
        .filled_style(filled)
        .unfilled_style(unfilled)
        .label("CPU 100%")
        .render(&mut frame, Rect::new(0, 0, 4, 2));

    let buffer = frame.buffer();
    for x in 0..4 {
        assert_eq!(buffer.get(x, 0).unwrap().character, '█');
        assert_eq!(buffer.get(x, 0).unwrap().style, filled);
    }
    assert_eq!(buffer.get(0, 1).unwrap().character, 'C');
    assert_eq!(buffer.get(0, 1).unwrap().style, filled);
}

#[test]
fn bars_handle_minimum_middle_and_tiny_targets_without_mutating_outside_the_target() {
    let filled = Style::new().fg(Color::rgb(20, 21, 22));
    let unfilled = Style::new().fg(Color::rgb(23, 24, 25));
    let mut frame = Frame::new(6, 1);
    frame.set_cell(5, 0, Cell::new('X', Style::new()));

    ProgressBar::new(-1.0)
        .filled_style(filled)
        .unfilled_style(unfilled)
        .render(&mut frame, Rect::new(0, 0, 4, 1));
    assert!(
        frame
            .buffer()
            .get(0, 0)
            .is_some_and(|cell| cell.character == '░')
    );

    Gauge::new(0.5)
        .filled_style(filled)
        .unfilled_style(unfilled)
        .render(&mut frame, Rect::new(0, 0, 4, 1));
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, '█');
    assert_eq!(frame.buffer().get(1, 0).unwrap().character, '█');
    assert_eq!(frame.buffer().get(2, 0).unwrap().character, '░');

    ProgressBar::new(f64::NEG_INFINITY).render(&mut frame, Rect::new(0, 0, 4, 1));
    assert_eq!(frame.buffer().get(0, 0).unwrap().character, '░');
    ProgressBar::new(f64::INFINITY).render(&mut frame, Rect::new(0, 0, 4, 1));
    assert_eq!(frame.buffer().get(3, 0).unwrap().character, '█');

    ProgressBar::new(0.5).render(&mut frame, Rect::new(0, 0, 0, 0));
    assert_eq!(frame.buffer().get(5, 0).unwrap().character, 'X');
}

#[test]
fn progress_changes_only_the_newly_filled_cell_in_a_diff() {
    let style = Style::new().fg(Color::rgb(30, 31, 32));
    let mut previous = Frame::new(10, 1);
    let mut current = Frame::new(10, 1);

    ProgressBar::new(0.5)
        .filled_style(style)
        .unfilled_style(style)
        .render(&mut previous, Rect::new(0, 0, 10, 1));
    ProgressBar::new(0.6)
        .filled_style(style)
        .unfilled_style(style)
        .render(&mut current, Rect::new(0, 0, 10, 1));

    let changed = diff(Some(previous.buffer()), current.buffer());
    assert_eq!(changed.len(), 1);
    assert_eq!((changed[0].x, changed[0].y), (5, 0));
    assert_eq!(changed[0].current.character, '█');
}
