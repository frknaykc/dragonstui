use std::time::{Duration, Instant};

use dragons_tui::{Buffer, Frame, Spinner, Style, diff, tick_due};

#[test]
fn spinner_starts_advances_and_wraps() {
    let mut spinner = Spinner::new(&["a", "b", "c"]).unwrap();

    assert_eq!(spinner.current(), "a");
    spinner.advance();
    assert_eq!(spinner.current(), "b");
    spinner.advance();
    spinner.advance();
    assert_eq!(spinner.current(), "a");
    assert!(Spinner::new(&[]).is_none());
}

#[test]
fn spinner_uses_time_based_animation_updates_without_breaking_manual_advance() {
    let start = Instant::now();
    let mut spinner = Spinner::new(&["a", "b"]).unwrap();

    assert!(!spinner.update(start));
    assert!(!spinner.update(start + Duration::from_millis(99)));
    assert!(spinner.update(start + Duration::from_millis(100)));
    assert_eq!(spinner.current(), "b");
    spinner.advance();
    assert_eq!(spinner.current(), "a");
}

#[test]
fn tick_due_uses_pure_interval_calculation() {
    let start = Instant::now();
    let interval = Duration::from_millis(100);

    assert!(!tick_due(
        start + Duration::from_millis(99),
        start,
        interval
    ));
    assert!(tick_due(start + interval, start, interval));
    assert!(tick_due(
        start + Duration::from_millis(250),
        start,
        interval
    ));
}

#[test]
fn spinner_tick_changes_only_its_cell_in_the_diff() {
    let mut spinner = Spinner::new(&["⠋", "⠙"]).unwrap();
    let mut previous = Buffer::new(8, 1);
    previous.write_text(0, 0, spinner.current(), Style::new());
    spinner.advance();
    let mut current = previous.clone();
    current.write_text(0, 0, spinner.current(), Style::new());

    let changed = diff(Some(&previous), &current);

    assert_eq!(changed.len(), 1);
    assert_eq!((changed[0].x, changed[0].y), (0, 0));
    assert_eq!(changed[0].current.character, '⠙');
}

#[test]
fn initial_frame_can_be_rendered_without_a_terminal() {
    let mut runtime = dragons_tui::Runtime::new(Some(Duration::from_millis(100)));
    let mut output = Vec::new();

    assert!(runtime.needs_redraw());
    runtime.render(&mut output, Frame::new(2, 1)).unwrap();
    assert!(!runtime.needs_redraw());
    runtime.request_redraw();
    assert!(runtime.needs_redraw());
}
