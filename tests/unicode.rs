use dragons_tui::{Buffer, Cell, CellKind, Style, diff, display_width, render_changed_cells};

#[test]
fn display_width_matches_terminal_expectations_for_supported_text() {
    assert_eq!(display_width("Hello"), 5);
    assert_eq!(display_width("Merhaba dünya"), 13);
    assert_eq!(display_width("İstanbul"), 8);
    assert_eq!(display_width("你好"), 4);
    assert_eq!(display_width("日本語"), 6);
    assert_eq!(display_width("🚀"), 2);
    assert_eq!(display_width("🔥 Dragon"), 9);
}

#[test]
fn wide_characters_occupy_a_lead_and_continuation_cell() {
    let mut buffer = Buffer::new(4, 1);
    let style = Style::new();

    assert_eq!(buffer.write_text(0, 0, "你A", style), 3);
    assert_eq!(buffer.get(0, 0).unwrap().character, '你');
    assert_eq!(buffer.get(0, 0).unwrap().kind, CellKind::Wide);
    assert_eq!(buffer.get(1, 0).unwrap().kind, CellKind::WideContinuation);
    assert_eq!(buffer.get(2, 0), Some(&Cell::new('A', style)));
}

#[test]
fn wide_characters_clip_as_a_whole_at_the_right_edge() {
    let mut buffer = Buffer::new(2, 1);

    assert_eq!(buffer.write_text(1, 0, "你", Style::new()), 0);
    assert_eq!(buffer.get(1, 0), Some(&Cell::default()));
}

#[test]
fn writing_over_a_wide_character_clears_its_old_continuation() {
    let mut buffer = Buffer::new(3, 1);
    buffer.write_text(0, 0, "你", Style::new());

    buffer.write_text(0, 0, "A", Style::new());

    assert_eq!(buffer.get(0, 0), Some(&Cell::new('A', Style::new())));
    assert_eq!(buffer.get(1, 0), Some(&Cell::default()));
}

#[test]
fn diff_reports_both_cells_when_a_wide_character_is_replaced() {
    let mut previous = Buffer::new(3, 1);
    previous.write_text(0, 0, "你", Style::new());
    let mut current = previous.clone();
    current.write_text(0, 0, "A", Style::new());

    let changed = diff(Some(&previous), &current);

    assert_eq!(
        changed
            .iter()
            .map(|cell| (cell.x, cell.y))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0)]
    );
}

#[test]
fn renderer_does_not_emit_a_continuation_cell_as_a_second_character() {
    let mut buffer = Buffer::new(2, 1);
    buffer.write_text(0, 0, "你", Style::new());
    let changed = diff(None, &buffer);
    let mut output = Vec::new();

    render_changed_cells(&mut output, &changed, false).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert_eq!(output.matches('你').count(), 1);
}

#[test]
fn renderer_coalesces_a_wide_cell_and_its_adjacent_normal_cell() {
    let mut buffer = Buffer::new(3, 1);
    buffer.write_text(0, 0, "你A", Style::new());
    let changed = diff(None, &buffer);
    let mut output = Vec::new();

    render_changed_cells(&mut output, &changed, false).unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("你A"));
    assert!(!output.contains("\u{1b}[3;1H"));
}
