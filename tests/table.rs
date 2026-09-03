use dragons_tui::{
    Alignment, Cell, Color, Constraint, Frame, Line, Rect, Style, Table, TableColumn, TableState,
};

#[test]
fn table_renders_a_header_and_selected_rich_row_in_constraint_columns() {
    let normal = Style::new().fg(Color::rgb(1, 2, 3));
    let selected = Style::new().fg(Color::rgb(4, 5, 6)).bg(Color::rgb(7, 8, 9));
    let table = Table::new([
        TableColumn::new(Constraint::Length(6)),
        TableColumn::new(Constraint::Fill(1)).alignment(Alignment::Right),
    ])
    .header([Line::from("NAME"), Line::from("STATUS")])
    .rows(vec![
        vec![Line::from("Codex"), Line::from("Working")],
        vec![
            Line::new([dragons_tui::Span::styled("İpek", normal)]),
            Line::from("Ready"),
        ],
    ])
    .selected_style(selected);
    let mut state = TableState::new();
    state.set_selected(1);
    let mut frame = Frame::new(16, 3);

    table.render(&mut frame, Rect::new(0, 0, 16, 3), &mut state);

    assert_eq!(
        frame.buffer().get(0, 0),
        Some(&Cell::new('N', Style::new()))
    );
    assert_eq!(
        frame.buffer().get(0, 1),
        Some(&Cell::new('C', Style::new()))
    );
    assert_eq!(
        frame.buffer().get(0, 2),
        Some(&Cell::new('İ', normal.patch(selected)))
    );
    assert_eq!(frame.buffer().get(15, 2), Some(&Cell::new('y', selected)));
}

#[test]
fn table_state_normalizes_shrinking_selection_and_scrolls_rows_with_viewport_semantics() {
    let table = Table::new([TableColumn::new(Constraint::Fill(1))]).rows(vec![
        vec![Line::from("zero")],
        vec![Line::from("one")],
        vec![Line::from("two")],
        vec![Line::from("three")],
    ]);
    let mut state = TableState::new();
    state.set_selected(9);
    let mut first = Frame::new(8, 2);

    table.render(&mut first, Rect::new(0, 0, 8, 2), &mut state);
    assert_eq!(state.selected_index(4), Some(3));
    assert_eq!(first.buffer().get(0, 0).unwrap().character, 'z');
    assert!(state.scroll_down());

    let mut scrolled = Frame::new(8, 2);
    table.render(&mut scrolled, Rect::new(0, 0, 8, 2), &mut state);
    assert_eq!(scrolled.buffer().get(0, 0).unwrap().character, 'o');
    assert_eq!(state.selected_index(1), Some(0));
}

#[test]
fn table_state_wraps_selection_without_panicking_for_empty_rows() {
    let mut state = TableState::new();

    state.next(0);
    assert_eq!(state.selected_index(0), None);
    state.next(3);
    assert_eq!(state.selected_index(3), Some(1));
    state.previous(3);
    assert_eq!(state.selected_index(3), Some(0));
    state.previous(3);
    assert_eq!(state.selected_index(3), Some(2));
}

#[test]
fn table_handles_empty_header_only_unicode_and_zero_sized_targets() {
    let table = Table::new([TableColumn::new(Constraint::Fill(1))])
        .header([Line::from("列")])
        .rows(vec![vec![Line::from("你好 🚀 İstanbul")]]);
    let mut state = TableState::new();
    let mut header = Frame::new(8, 1);
    table.render(&mut header, Rect::new(0, 0, 8, 1), &mut state);
    assert_eq!(header.buffer().get(0, 0).unwrap().character, '列');

    let mut empty = Frame::new(2, 2);
    Table::new([TableColumn::new(Constraint::Fill(1))]).render(
        &mut empty,
        Rect::new(0, 0, 0, 0),
        &mut state,
    );
    assert_eq!(empty.buffer().get(0, 0), Some(&Cell::default()));
}

#[test]
fn table_column_areas_and_clipping_remain_inside_the_parent_rect() {
    let table = Table::new([
        TableColumn::new(Constraint::Length(2)),
        TableColumn::new(Constraint::Percentage(50)),
        TableColumn::new(Constraint::Fill(1)).alignment(Alignment::Right),
    ])
    .rows(vec![vec![
        Line::from("first"),
        Line::from("second"),
        Line::from("third"),
    ]]);
    let mut state = TableState::new();
    let mut frame = Frame::new(8, 3);

    table.render(&mut frame, Rect::new(2, 1, 5, 1), &mut state);

    assert_eq!(frame.buffer().get(2, 1).unwrap().character, 'f');
    assert_eq!(frame.buffer().get(6, 1).unwrap().character, 't');
    assert_eq!(frame.buffer().get(1, 1), Some(&Cell::default()));
    assert_eq!(frame.buffer().get(7, 1), Some(&Cell::default()));
}
