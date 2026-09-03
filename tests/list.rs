use dragons_tui::{Cell, Color, Frame, List, ListState, Rect, Style};

#[test]
fn list_state_handles_empty_navigation_and_shrinking_items() {
    let mut state = ListState::new();

    assert_eq!(state.selected_index(0), None);
    state.next(3);
    assert_eq!(state.selected_index(3), Some(1));
    state.previous(3);
    assert_eq!(state.selected_index(3), Some(0));
    state.previous(3);
    assert_eq!(state.selected_index(3), Some(2));

    state.set_selected(9);
    assert_eq!(state.selected_index(2), Some(1));

    state.set_selected(2);
    state.next(3);
    assert_eq!(state.selected_index(3), Some(0));
}

#[test]
fn list_renders_selected_items_and_clips_rows() {
    let normal = Style::new().fg(Color::Rgb { r: 1, g: 2, b: 3 });
    let selected = Style::new().fg(Color::Rgb { r: 4, g: 5, b: 6 }).bold();
    let mut state = ListState::new();
    state.set_selected(1);
    let mut frame = Frame::new(12, 2);

    List::new(&["Codex", "Hermes", "Claude"])
        .normal_style(normal)
        .selected_style(selected)
        .render(&mut frame, Rect::new(0, 0, 12, 2), &mut state);

    assert_eq!(frame.buffer().get(0, 0), Some(&Cell::new(' ', normal)));
    assert_eq!(frame.buffer().get(0, 1), Some(&Cell::new('>', selected)));
    assert_eq!(frame.buffer().get(2, 1), Some(&Cell::new('H', selected)));
    assert_eq!(frame.buffer().get(2, 2), None);
}
