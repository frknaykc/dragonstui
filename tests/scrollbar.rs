use dragons_tui::{Position, Rect, Scrollbar, ScrollbarState, ViewportState};

#[test]
fn scrollbar_hides_for_empty_and_non_scrollable_viewports() {
    let track = Rect::new(9, 2, 1, 8);
    let mut viewport = ViewportState::new();
    viewport.update_dimensions(0, 8);
    assert_eq!(Scrollbar::geometry(&viewport, track), None);

    viewport.update_dimensions(8, 8);
    assert_eq!(Scrollbar::geometry(&viewport, track), None);
}

#[test]
fn scrollbar_thumb_math_is_deterministic_at_top_middle_and_bottom() {
    let track = Rect::new(9, 2, 1, 10);
    let mut viewport = ViewportState::new();
    viewport.update_dimensions(100, 20);
    let top = Scrollbar::geometry(&viewport, track).unwrap();
    assert_eq!((top.thumb.y, top.thumb.height), (2, 2));

    viewport.set_offset(40);
    let middle = Scrollbar::geometry(&viewport, track).unwrap();
    assert_eq!((middle.thumb.y, middle.thumb.height), (6, 2));

    viewport.end();
    let bottom = Scrollbar::geometry(&viewport, track).unwrap();
    assert_eq!((bottom.thumb.y, bottom.thumb.height), (10, 2));
}

#[test]
fn scrollbar_track_click_and_thumb_drag_clamp_the_shared_viewport_offset() {
    let track = Rect::new(9, 2, 1, 10);
    let mut viewport = ViewportState::new();
    viewport.update_dimensions(100, 20);
    let mut scrollbar = ScrollbarState::new();

    assert!(scrollbar.track_click(&mut viewport, track, Position { x: 9, y: 11 }));
    assert_eq!(viewport.offset(), 80);
    let geometry = Scrollbar::geometry(&viewport, track).unwrap();
    assert!(scrollbar.start_drag(geometry, Position { x: 9, y: 10 }));
    assert!(scrollbar.is_dragging());
    assert!(scrollbar.drag_to(&mut viewport, track, Position { x: 9, y: 2 }));
    assert_eq!(viewport.offset(), 0);
    assert!(scrollbar.stop_drag());
    assert!(!scrollbar.drag_to(&mut viewport, track, Position { x: 9, y: 8 }));
}

#[test]
fn scrollbar_remains_safe_for_one_or_zero_cell_tracks_and_content_shrink() {
    let mut viewport = ViewportState::new();
    viewport.update_dimensions(100, 20);
    viewport.end();
    assert!(Scrollbar::geometry(&viewport, Rect::new(0, 0, 1, 1)).is_some());
    assert_eq!(Scrollbar::geometry(&viewport, Rect::new(0, 0, 1, 0)), None);

    viewport.update_dimensions(1, 20);
    assert_eq!(viewport.offset(), 0);
    assert_eq!(Scrollbar::geometry(&viewport, Rect::new(0, 0, 1, 8)), None);
}
