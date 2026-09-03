use dragons_tui::{Buffer, Cell, Color, Style, diff};

#[test]
fn buffer_uses_requested_dimensions_and_blank_cells() {
    let buffer = Buffer::new(3, 2);

    assert_eq!(buffer.width(), 3);
    assert_eq!(buffer.height(), 2);
    assert_eq!(buffer.get(2, 1), Some(&Cell::default()));
}

#[test]
fn buffer_writes_and_reads_a_cell() {
    let mut buffer = Buffer::new(2, 2);
    let style = Style::new().fg(Color::Rgb { r: 1, g: 2, b: 3 }).bold();
    let cell = Cell::new('X', style);

    assert!(buffer.set(1, 0, cell));
    assert_eq!(buffer.get(1, 0), Some(&cell));
}

#[test]
fn out_of_bounds_writes_are_ignored_without_panicking() {
    let mut buffer = Buffer::new(2, 2);

    assert!(!buffer.set(2, 0, Cell::new('X', Style::new())));
    assert!(!buffer.set(0, 2, Cell::new('Y', Style::new())));
    assert_eq!(buffer.get(2, 0), None);
    assert_eq!(buffer.get(0, 2), None);
}

#[test]
fn text_writes_clip_at_the_right_edge() {
    let mut buffer = Buffer::new(4, 1);
    let style = Style::new().underline();

    assert_eq!(buffer.write_text(2, 0, "Dragons", style), 2);
    assert_eq!(buffer.get(2, 0), Some(&Cell::new('D', style)));
    assert_eq!(buffer.get(3, 0), Some(&Cell::new('r', style)));
}

#[test]
fn clear_resets_every_cell() {
    let mut buffer = Buffer::new(2, 1);
    buffer.set(0, 0, Cell::new('X', Style::new().bold()));

    buffer.clear();

    assert_eq!(buffer.get(0, 0), Some(&Cell::default()));
}

#[test]
fn identical_buffers_have_no_diff() {
    let previous = Buffer::new(2, 2);
    let current = previous.clone();

    assert!(diff(Some(&previous), &current).is_empty());
}

#[test]
fn initial_buffer_produces_changed_cells_for_every_position() {
    let current = Buffer::new(2, 2);

    let changed = diff(None, &current);

    assert_eq!(changed.len(), 4);
    assert!(changed.iter().all(|cell| cell.previous.is_none()));
    assert_eq!(
        changed
            .iter()
            .map(|cell| (cell.x, cell.y))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (0, 1), (1, 1)]
    );
}

#[test]
fn one_changed_cell_produces_one_diff_entry() {
    let previous = Buffer::new(2, 1);
    let mut current = previous.clone();
    current.set(1, 0, Cell::new('X', Style::new().bold()));

    let changed = diff(Some(&previous), &current);

    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].x, 1);
    assert_eq!(changed[0].y, 0);
    assert_eq!(changed[0].previous, Some(Cell::default()));
    assert_eq!(changed[0].current, Cell::new('X', Style::new().bold()));
}

#[test]
fn multiple_changed_cells_are_reported_in_row_order() {
    let previous = Buffer::new(2, 2);
    let mut current = previous.clone();
    current.set(0, 0, Cell::new('A', Style::new()));
    current.set(1, 1, Cell::new('B', Style::new()));

    let changed = diff(Some(&previous), &current);

    assert_eq!(changed.len(), 2);
    assert_eq!((changed[0].x, changed[0].y), (0, 0));
    assert_eq!((changed[1].x, changed[1].y), (1, 1));
}

#[test]
fn resized_buffers_redraw_every_current_cell() {
    let previous = Buffer::new(2, 2);
    let current = Buffer::new(3, 1);

    let changed = diff(Some(&previous), &current);

    assert_eq!(changed.len(), 3);
    assert!(changed.iter().all(|cell| cell.current == Cell::default()));
    assert_eq!(
        changed
            .iter()
            .map(|cell| (cell.x, cell.y))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (2, 0)]
    );
}
