use dragons_tui::{Buffer, Cell, Frame, Runtime, Style, diff, render_changed_cells};

#[test]
fn buffer_text_operations_outside_its_bounds_are_noops() {
    let mut buffer = Buffer::new(1, 1);
    let style = Style::new().bold();

    assert_eq!(buffer.write_text(1, 0, "X", style), 0);
    assert_eq!(buffer.write_text(0, 1, "Y", style), 0);
    assert_eq!(buffer.get(0, 0), Some(&Cell::default()));
}

#[test]
fn renderer_clears_the_old_wide_continuation_after_overwrite() {
    let mut previous = Buffer::new(3, 1);
    previous.write_text(0, 0, "你", Style::new());
    let mut current = previous.clone();
    current.write_text(0, 0, "A", Style::new());
    let changed = diff(Some(&previous), &current);
    let mut output = Vec::new();

    render_changed_cells(&mut output, &changed, false).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert_eq!(output.matches('你').count(), 0);
    assert!(output.contains("\u{1b}[1;1H"));
    assert!(output.contains("A "));
    assert!(!output.contains("\u{1b}[1;2H"));
}

#[test]
fn renderer_skips_the_cursor_move_for_a_wide_continuation() {
    let mut buffer = Buffer::new(2, 1);
    buffer.write_text(0, 0, "你", Style::new());
    let changed = diff(None, &buffer);
    let mut output = Vec::new();

    render_changed_cells(&mut output, &changed, false).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert_eq!(output.matches("\u{1b}[1;1H").count(), 1);
    assert!(!output.contains("\u{1b}[1;2H"));
    assert_eq!(output.matches('你').count(), 1);
}

#[test]
fn runtime_resize_clears_and_redraws_the_entire_current_frame() {
    let mut runtime = Runtime::new(None);
    let mut output = Vec::new();
    let mut initial = Frame::new(2, 1);
    initial.set_cell(0, 0, Cell::new('A', Style::new()));
    initial.set_cell(1, 0, Cell::new('B', Style::new()));
    runtime.render(&mut output, initial).unwrap();
    output.clear();

    let mut resized = Frame::new(3, 1);
    resized.set_cell(0, 0, Cell::new('X', Style::new()));
    resized.set_cell(1, 0, Cell::new('Y', Style::new()));
    resized.set_cell(2, 0, Cell::new('Z', Style::new()));
    runtime.render(&mut output, resized).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("\u{1b}[2J"));
    assert!(output.contains("\u{1b}[1;1H\u{1b}[0mXYZ"));
    assert_eq!(output.matches('X').count(), 1);
    assert_eq!(output.matches('Y').count(), 1);
    assert_eq!(output.matches('Z').count(), 1);
}
