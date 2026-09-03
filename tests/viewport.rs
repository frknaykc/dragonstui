use dragons_tui::{Cell, CellKind, Frame, Rect, Style, Viewport, ViewportState};

#[test]
fn viewport_state_scrolls_clamps_and_pages_within_its_valid_range() {
    let mut state = ViewportState::new();
    state.update_dimensions(100, 20);

    assert_eq!(state.offset(), 0);
    assert_eq!(state.max_scroll(), 80);
    assert!(!state.scroll_up());
    assert!(state.scroll_down());
    assert_eq!(state.offset(), 1);
    assert!(state.page_down());
    assert_eq!(state.offset(), 21);
    assert!(state.page_up());
    assert_eq!(state.offset(), 1);
    assert!(state.end());
    assert_eq!(state.offset(), 80);
    assert!(!state.scroll_down());
    assert!(state.home());
    assert_eq!(state.offset(), 0);
}

#[test]
fn viewport_state_handles_empty_short_equal_and_zero_height_viewports() {
    let mut state = ViewportState::new();

    state.update_dimensions(0, 10);
    assert_eq!(state.max_scroll(), 0);
    assert!(!state.scroll_down());
    assert!(!state.end());

    state.update_dimensions(4, 10);
    assert_eq!(state.offset(), 0);
    assert_eq!(state.max_scroll(), 0);

    state.update_dimensions(10, 10);
    assert_eq!(state.offset(), 0);
    assert_eq!(state.max_scroll(), 0);

    state.update_dimensions(10, 0);
    assert_eq!(state.max_scroll(), 10);
    assert_eq!(state.offset(), 10);
    assert!(!state.page_up());
    assert!(!state.page_down());
}

#[test]
fn viewport_state_clamps_shrinking_content_and_only_follows_growth_at_bottom() {
    let mut state = ViewportState::new();
    state.update_dimensions(100, 20);
    assert!(state.end());
    assert!(state.scroll_up());
    assert!(state.scroll_up());
    assert!(state.scroll_up());
    assert!(state.scroll_up());
    assert!(state.scroll_up());
    assert!(state.scroll_up());
    assert!(state.scroll_up());
    assert!(state.scroll_up());
    assert!(state.scroll_up());
    assert!(state.scroll_up());
    assert_eq!(state.offset(), 70);

    state.update_dimensions(20, 20);
    assert_eq!(state.offset(), 0);
    assert_eq!(state.max_scroll(), 0);

    state.update_dimensions(100, 20);
    assert_eq!(state.offset(), 80);
    assert!(state.is_at_bottom());
    state.update_dimensions(101, 20);
    assert_eq!(state.offset(), 81);
    assert!(state.is_at_bottom());

    assert!(state.home());
    for _ in 0..30 {
        assert!(state.scroll_down());
    }
    state.update_dimensions(102, 20);
    assert_eq!(state.offset(), 30);
    assert!(!state.is_at_bottom());
}

#[test]
fn viewport_renders_only_the_visible_lines_at_top_middle_and_bottom() {
    let lines = ["zero", "one", "two", "three", "four"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let viewport = Viewport::new(&lines).style(Style::new().bold());
    let mut state = ViewportState::new();

    let mut top = Frame::new(8, 4);
    viewport.render(&mut top, Rect::new(1, 1, 6, 2), &mut state);
    assert_eq!(state.offset(), 0);
    assert_eq!(
        top.buffer().get(1, 1),
        Some(&Cell::new('z', Style::new().bold()))
    );
    assert_eq!(
        top.buffer().get(1, 2),
        Some(&Cell::new('o', Style::new().bold()))
    );
    assert_eq!(top.buffer().get(1, 3), Some(&Cell::default()));

    assert!(state.scroll_down());
    let mut middle = Frame::new(8, 4);
    viewport.render(&mut middle, Rect::new(1, 1, 6, 2), &mut state);
    assert_eq!(
        middle.buffer().get(1, 1),
        Some(&Cell::new('o', Style::new().bold()))
    );
    assert_eq!(
        middle.buffer().get(1, 2),
        Some(&Cell::new('t', Style::new().bold()))
    );

    assert!(state.end());
    let mut bottom = Frame::new(8, 4);
    viewport.render(&mut bottom, Rect::new(1, 1, 6, 2), &mut state);
    assert_eq!(
        bottom.buffer().get(1, 1),
        Some(&Cell::new('t', Style::new().bold()))
    );
    assert_eq!(
        bottom.buffer().get(1, 2),
        Some(&Cell::new('f', Style::new().bold()))
    );
}

#[test]
fn viewport_clips_unicode_to_its_rect_and_ignores_zero_sized_targets() {
    let lines = ["İstanbul", "你好", "🚀", "last"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let viewport = Viewport::new(&lines);
    let mut state = ViewportState::new();
    let mut frame = Frame::new(5, 4);

    viewport.render(&mut frame, Rect::new(1, 1, 3, 2), &mut state);
    assert_eq!(frame.buffer().get(1, 1).unwrap().character, 'İ');
    assert_eq!(frame.buffer().get(0, 1), Some(&Cell::default()));

    assert!(state.scroll_down());
    let mut frame = Frame::new(5, 4);
    viewport.render(&mut frame, Rect::new(1, 1, 3, 2), &mut state);
    assert_eq!(frame.buffer().get(1, 1).unwrap().character, '你');
    assert_eq!(frame.buffer().get(1, 1).unwrap().kind, CellKind::Wide);
    assert_eq!(
        frame.buffer().get(2, 1).unwrap().kind,
        CellKind::WideContinuation
    );
    assert_eq!(frame.buffer().get(3, 1), Some(&Cell::default()));
    assert_eq!(frame.buffer().get(1, 2).unwrap().character, '🚀');

    let mut tiny = Frame::new(2, 1);
    viewport.render(&mut tiny, Rect::new(0, 0, 0, 0), &mut state);
    assert_eq!(tiny.buffer().get(0, 0), Some(&Cell::default()));
}
